use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::check::{ModuleOrigin, semantic::SemanticIndex};
use crate::diagnostic::Severity;
use crate::driver::Project;
use serde_json::{Value, json};

const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const MAX_HEADER: usize = 8192;

pub fn read_message(input: &mut impl BufRead) -> io::Result<Option<Result<Value, String>>> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = Vec::new();
        let count = input
            .by_ref()
            .take((MAX_HEADER + 1 - header_bytes) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 && header_bytes == 0 {
            return Ok(None);
        }
        header_bytes += count;
        if count == 0 || header_bytes > MAX_HEADER || !line.ends_with(b"\r\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid or oversized LSP header",
            ));
        }
        if line == b"\r\n" {
            break;
        }
        let header = std::str::from_utf8(&line[..line.len() - 2]).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid LSP header encoding")
        })?;
        let (name, value) = header
            .split_once(':')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid LSP header"))?;
        if name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() || !value.trim().bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid LSP Content-Length",
                ));
            }
            length = Some(value.trim().parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid LSP Content-Length")
            })?);
        }
    }
    let length = length
        .filter(|length| *length <= MAX_MESSAGE)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing or oversized LSP Content-Length",
            )
        })?;
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    Ok(Some(
        serde_json::from_slice(&body).map_err(|error| error.to_string()),
    ))
}

pub fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionEncoding {
    Utf8,
    Utf16,
}

#[derive(Clone, Debug)]
pub struct PositionMapper {
    starts: Vec<usize>,
    encoding: PositionEncoding,
}

impl PositionMapper {
    pub fn new(text: &str, encoding: PositionEncoding) -> Self {
        Self {
            starts: std::iter::once(0)
                .chain(
                    text.bytes()
                        .enumerate()
                        .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
                )
                .collect(),
            encoding,
        }
    }

    pub fn position(&self, text: &str, offset: usize) -> Option<Value> {
        if offset > text.len() || !text.is_char_boundary(offset) {
            return None;
        }
        let line = self.starts.partition_point(|start| *start <= offset) - 1;
        let prefix = text.get(self.starts[line]..offset)?.trim_end_matches('\r');
        let character = match self.encoding {
            PositionEncoding::Utf8 => prefix.len(),
            PositionEncoding::Utf16 => prefix.encode_utf16().count(),
        };
        Some(json!({"line": line, "character": character}))
    }

    pub fn offset(&self, text: &str, position: &Value) -> Option<usize> {
        let line = usize::try_from(position.get("line")?.as_u64()?).ok()?;
        let character = usize::try_from(position.get("character")?.as_u64()?).ok()?;
        let start = *self.starts.get(line)?;
        let end = self.starts.get(line + 1).copied().unwrap_or(text.len());
        let text = text.get(start..end)?.trim_end_matches(['\r', '\n']);
        match self.encoding {
            PositionEncoding::Utf8 => (character <= text.len() && text.is_char_boundary(character))
                .then_some(start + character),
            PositionEncoding::Utf16 => {
                let mut units = 0;
                for (offset, ch) in text.char_indices() {
                    if units == character {
                        return Some(start + offset);
                    }
                    units += ch.len_utf16();
                }
                (units == character).then_some(start + text.len())
            }
        }
    }

    pub fn range(&self, text: &str, span: crate::diagnostic::Span) -> Option<Value> {
        Some(
            json!({"start": self.position(text, span.start)?, "end": self.position(text, span.end)?}),
        )
    }
}

type RpcResult = Result<Value, (i64, String)>;

fn invalid(message: &str) -> (i64, String) {
    (-32602, message.to_owned())
}

pub fn file_uri(path: &Path) -> Option<String> {
    let path = path.to_str()?;
    #[cfg(windows)]
    let normalized = path
        .strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('\\', "/");
    #[cfg(windows)]
    let path = normalized.as_str();
    let mut uri = String::from("file://");
    if cfg!(windows) && path.as_bytes().get(1) == Some(&b':') {
        uri.push('/');
    }
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    Some(uri)
}

pub fn uri_path(uri: &str) -> Result<PathBuf, String> {
    let uri = uri
        .strip_prefix("file://")
        .ok_or("only file URIs are supported")?;
    let uri = uri
        .strip_prefix("localhost/")
        .map_or_else(|| uri.to_owned(), |path| format!("/{path}"));
    if !uri.starts_with('/') || uri.contains(['?', '#']) {
        return Err("invalid local file URI".into());
    }
    let mut decoded = Vec::new();
    let mut bytes = uri.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes
                .next()
                .and_then(|byte| char::from(byte).to_digit(16))
                .ok_or("invalid URI escape")?;
            let low = bytes
                .next()
                .and_then(|byte| char::from(byte).to_digit(16))
                .ok_or("invalid URI escape")?;
            decoded.push((high * 16 + low) as u8);
        } else {
            decoded.push(byte);
        }
    }
    let path = String::from_utf8(decoded).map_err(|_| "file URI must encode UTF-8")?;
    if path.contains('\0') {
        return Err("a file URI cannot contain NUL".into());
    }
    #[cfg(windows)]
    let path = path
        .strip_prefix('/')
        .filter(|path| {
            path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && path.as_bytes().get(1) == Some(&b':')
        })
        .unwrap_or(&path);
    Ok(PathBuf::from(path))
}

fn document_path(params: &Value) -> Result<PathBuf, (i64, String)> {
    let uri = params["textDocument"]["uri"]
        .as_str()
        .ok_or_else(|| invalid("textDocument.uri is required"))?;
    let path = uri_path(uri).map_err(|message| invalid(&message))?;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::syntax::SourceKind::from_extension)
        .is_none()
    {
        return Err(invalid("expected a .tz, .tt, or .tc document"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("document needs a parent directory"))?
        .canonicalize()
        .map_err(|_| invalid("document directory does not exist"))?;
    let name = path
        .file_name()
        .ok_or_else(|| invalid("document needs a filename"))?;
    Ok(parent.join(name))
}

struct OpenBuffer {
    uri: String,
    text: String,
    version: i64,
}

struct ProjectState {
    project: Project,
    index: Option<SemanticIndex>,
    mappers: Vec<PositionMapper>,
}

struct Session {
    initialized: bool,
    shutdown: bool,
    encoding: PositionEncoding,
    roots: Vec<PathBuf>,
    buffers: BTreeMap<PathBuf, OpenBuffer>,
    projects: BTreeMap<PathBuf, ProjectState>,
    pending: BTreeMap<PathBuf, (PathBuf, Instant)>,
    closed: BTreeMap<PathBuf, String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            initialized: false,
            shutdown: false,
            encoding: PositionEncoding::Utf16,
            roots: Vec::new(),
            buffers: BTreeMap::new(),
            projects: BTreeMap::new(),
            pending: BTreeMap::new(),
            closed: BTreeMap::new(),
        }
    }
}

impl Session {
    fn project_root(&self, path: &Path) -> PathBuf {
        self.roots
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.components().count())
            .unwrap_or_else(|| path.parent().unwrap().to_owned())
    }

    fn schedule(&mut self, path: &Path) {
        let directory = self.project_root(path);
        self.projects.remove(&directory);
        self.pending.insert(
            directory,
            (path.to_owned(), Instant::now() + Duration::from_millis(200)),
        );
    }

    fn publish(
        &self,
        output: &mut impl Write,
        path: &Path,
        diagnostics: Vec<Value>,
    ) -> io::Result<()> {
        if self.closed.contains_key(path) && !diagnostics.is_empty() {
            return Ok(());
        }
        let uri = self
            .buffers
            .get(path)
            .map(|buffer| buffer.uri.clone())
            .or_else(|| self.closed.get(path).cloned())
            .or_else(|| file_uri(path));
        let mut params = json!({"uri": uri, "diagnostics": diagnostics});
        if let Some(buffer) = self.buffers.get(path) {
            params["version"] = json!(buffer.version);
        }
        write_message(
            output,
            &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": params}),
        )
    }

    fn refresh(&mut self, path: &Path, output: &mut impl Write) -> io::Result<()> {
        let directory = self.project_root(path);
        self.pending.remove(&directory);
        let overlays = self
            .buffers
            .iter()
            .map(|(path, buffer)| (path.clone(), buffer.text.clone()))
            .collect();
        let project = match Project::load_with_overlays(&directory, &overlays) {
            Ok(project) => project,
            Err(error) => {
                self.projects.remove(&directory);
                self.publish(output, &error.path, vec![json!({"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}, "severity": 1, "code": error.diagnostic.code, "source": "tsuzuri", "message": error.diagnostic.message})])?;
                return Ok(());
            }
        };
        let inputs: Vec<_> = project
            .sources
            .iter()
            .map(|source| crate::SourceInput {
                path: if source.origin == ModuleOrigin::User {
                    source.relative_path.to_str().unwrap()
                } else {
                    source.path.to_str().unwrap()
                },
                text: &source.text,
                origin: source.origin,
            })
            .collect();
        let mut semantic = SemanticIndex::default();
        let analyzed = crate::analyze_inputs_indexed_all(&inputs, Some(&mut semantic));
        let (index, diagnostics) = match analyzed {
            Ok(module) => (Some(semantic), module.warnings),
            Err(errors) => (None, errors.diagnostics),
        };
        let mappers: Vec<_> = project
            .sources
            .iter()
            .map(|source| PositionMapper::new(&source.text, self.encoding))
            .collect();
        for (id, source) in project
            .sources
            .iter()
            .enumerate()
            .filter(|(_, source)| source.origin == ModuleOrigin::User)
        {
            let messages = diagnostics.iter().filter(|diagnostic| diagnostic.span.source.unwrap_or(project.root) == id).filter_map(|diagnostic| {
                Some(json!({"range": mappers[id].range(&source.text, diagnostic.span)?, "severity": if diagnostic.severity == Severity::Error { 1 } else { 2 }, "code": diagnostic.code, "source": "tsuzuri", "message": diagnostic.message}))
            }).collect();
            self.publish(output, &source.path, messages)?;
        }
        self.projects.insert(
            directory,
            ProjectState {
                project,
                index,
                mappers,
            },
        );
        Ok(())
    }

    fn handle(&mut self, method: &str, params: &Value, output: &mut impl Write) -> RpcResult {
        if method == "initialize" {
            if self.initialized {
                return Err((-32600, "server is already initialized".into()));
            }
            if params["capabilities"]["general"]["positionEncodings"]
                .as_array()
                .is_some_and(|encodings| encodings.iter().any(|encoding| encoding == "utf-8"))
            {
                self.encoding = PositionEncoding::Utf8;
            }
            if let Some(uri) = params["rootUri"].as_str() {
                self.roots
                    .push(uri_path(uri).map_err(|message| invalid(&message))?);
            }
            if let Some(folders) = params["workspaceFolders"].as_array() {
                for folder in folders {
                    self.roots.push(
                        uri_path(
                            folder["uri"]
                                .as_str()
                                .ok_or_else(|| invalid("workspace folder needs a URI"))?,
                        )
                        .map_err(|message| invalid(&message))?,
                    );
                }
            }
            self.initialized = true;
            return Ok(
                json!({"capabilities": {"positionEncoding": if self.encoding == PositionEncoding::Utf8 { "utf-8" } else { "utf-16" }, "textDocumentSync": {"openClose": true, "change": 1}, "hoverProvider": true, "definitionProvider": true, "documentSymbolProvider": true}, "serverInfo": {"name": "Tsuzuri", "version": env!("CARGO_PKG_VERSION")}}),
            );
        }
        if !self.initialized {
            return Err((-32002, "server is not initialized".into()));
        }
        if self.shutdown {
            return Err((-32600, "server has shut down".into()));
        }
        match method {
            "initialized" | "$/cancelRequest" => Ok(Value::Null),
            "workspace/didChangeWatchedFiles" => {
                let paths: Vec<_> = self.buffers.keys().cloned().collect();
                for path in paths {
                    self.schedule(&path);
                }
                Ok(Value::Null)
            }
            "shutdown" => {
                self.shutdown = true;
                self.pending.clear();
                Ok(Value::Null)
            }
            "textDocument/didOpen" | "textDocument/didChange" => {
                let path = document_path(params)?;
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .ok_or_else(|| invalid("textDocument.uri is required"))?;
                let version = params["textDocument"]["version"]
                    .as_i64()
                    .ok_or_else(|| invalid("document version is required"))?;
                let text = if method.ends_with("didOpen") {
                    if self.buffers.contains_key(&path) {
                        return Err(invalid("document is already open"));
                    }
                    params["textDocument"]["text"]
                        .as_str()
                        .ok_or_else(|| invalid("document text is required"))?
                } else {
                    let buffer = self
                        .buffers
                        .get(&path)
                        .ok_or_else(|| invalid("document is not open"))?;
                    if version <= buffer.version {
                        return Err(invalid("document versions must increase"));
                    }
                    let changes = params["contentChanges"]
                        .as_array()
                        .ok_or_else(|| invalid("contentChanges is required"))?;
                    if changes.len() != 1 || changes[0].get("range").is_some() {
                        return Err(invalid("only full-document synchronization is supported"));
                    }
                    changes[0]["text"]
                        .as_str()
                        .ok_or_else(|| invalid("replacement text is required"))?
                };
                if text.len() > crate::syntax::MAX_SOURCE_BYTES {
                    return Err(invalid("source exceeds the 1 MiB limit"));
                }
                let used: usize = self
                    .buffers
                    .iter()
                    .filter(|(other, _)| **other != path)
                    .map(|(_, buffer)| buffer.text.len())
                    .sum();
                if used + text.len() > 32 * 1024 * 1024
                    || (!self.buffers.contains_key(&path) && self.buffers.len() >= 1024)
                {
                    return Err(invalid("open buffers exceed the server resource limit"));
                }
                self.buffers.insert(
                    path.clone(),
                    OpenBuffer {
                        uri: uri.to_owned(),
                        text: text.to_owned(),
                        version,
                    },
                );
                self.closed.remove(&path);
                self.schedule(&path);
                Ok(Value::Null)
            }
            "textDocument/didClose" => {
                let path = document_path(params)?;
                self.buffers.remove(&path);
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .ok_or_else(|| invalid("textDocument.uri is required"))?;
                self.closed.insert(path.clone(), uri.to_owned());
                self.schedule(&path);
                self.publish(output, &path, Vec::new())
                    .map_err(|error| (-32603, error.to_string()))?;
                Ok(Value::Null)
            }
            "textDocument/hover" | "textDocument/definition" | "textDocument/documentSymbol" => {
                let path = document_path(params)?;
                let directory = self.project_root(&path);
                if self.pending.contains_key(&directory) || !self.projects.contains_key(&directory)
                {
                    self.refresh(&path, output)
                        .map_err(|error| (-32603, error.to_string()))?;
                }
                let Some(state) = self.projects.get(&directory) else {
                    return Ok(Value::Null);
                };
                let Some(source_id) = state
                    .project
                    .sources
                    .iter()
                    .position(|source| source.path == path)
                else {
                    return Ok(Value::Null);
                };
                let source = &state.project.sources[source_id];
                if method.ends_with("documentSymbol") {
                    let symbols: Vec<_> = state.index.iter().flat_map(|index| &index.symbols).filter(|symbol| symbol.selection.source == Some(source_id)).filter_map(|symbol| {
                        Some(json!({"name": symbol.name, "kind": symbol.kind, "range": state.mappers[source_id].range(&source.text, symbol.span)?, "selectionRange": state.mappers[source_id].range(&source.text, symbol.selection)?}))
                    }).collect();
                    return Ok(json!(symbols));
                }
                let offset = state.mappers[source_id]
                    .offset(&source.text, &params["position"])
                    .ok_or_else(|| {
                        invalid("position is outside the document or splits a Unicode character")
                    })?;
                let Some(entry) = state
                    .index
                    .as_ref()
                    .and_then(|index| index.at(source_id, offset))
                else {
                    return Ok(Value::Null);
                };
                if method.ends_with("hover") {
                    let mut markdown = format!("```tsuzuri\n{}\n```", entry.detail);
                    if let Some(doc) = state
                        .index
                        .as_ref()
                        .and_then(|index| index.doc_for(entry.target.unwrap_or(entry.span)))
                    {
                        markdown.push_str("\n\n");
                        markdown.push_str(doc);
                    }
                    return Ok(
                        json!({"contents": {"kind": "markdown", "value": markdown}, "range": state.mappers[source_id].range(&source.text, entry.span)}),
                    );
                }
                let Some(target) = entry.target else {
                    return Ok(Value::Null);
                };
                let target_id = target.source.unwrap_or(source_id);
                let Some(target_source) = state
                    .project
                    .sources
                    .get(target_id)
                    .filter(|source| source.origin == ModuleOrigin::User)
                else {
                    return Ok(Value::Null);
                };
                Ok(
                    json!({"uri": self.buffers.get(&target_source.path).map(|buffer| buffer.uri.clone()).or_else(|| file_uri(&target_source.path)), "range": state.mappers[target_id].range(&target_source.text, target)}),
                )
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        }
    }
}

enum InputEvent {
    Message(Result<Value, String>),
    Failure(String),
    End,
}

pub fn serve(
    input: impl Read + Send + 'static,
    mut output: impl Write,
    mut stderr: impl Write,
) -> i32 {
    let (sender, receiver) = mpsc::sync_channel(8);
    let cancelled = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let reader_cancelled = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        let mut input = io::BufReader::new(input);
        loop {
            match read_message(&mut input) {
                Ok(Some(message)) => {
                    if let Ok(message) = &message
                        && message["method"] == "$/cancelRequest"
                        && let Some(id) = message["params"].get("id")
                        && (id.is_string() || id.is_i64() || id.is_u64())
                        && let Ok(mut pending) = reader_cancelled.lock()
                        && pending.len() < 1024
                    {
                        pending.insert(id.to_string());
                    }
                    if sender.send(InputEvent::Message(message)).is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = sender.send(InputEvent::End);
                    break;
                }
                Err(error) => {
                    let _ = sender.send(InputEvent::Failure(error.to_string()));
                    break;
                }
            }
        }
    });
    let mut session = Session::default();
    loop {
        let event = if let Some(deadline) = session
            .pending
            .values()
            .map(|(_, deadline)| *deadline)
            .min()
        {
            receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        } else {
            receiver
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        let message = match event {
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let due: Vec<_> = session
                    .pending
                    .values()
                    .filter(|(_, deadline)| *deadline <= Instant::now())
                    .map(|(path, _)| path.clone())
                    .collect();
                for path in due {
                    if let Err(error) = session.refresh(&path, &mut output) {
                        let _ = writeln!(stderr, "LSP output failed: {error}");
                        return 1;
                    }
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) | Ok(InputEvent::End) => {
                return i32::from(!session.shutdown);
            }
            Ok(InputEvent::Failure(message)) => {
                let _ = write_message(
                    &mut output,
                    &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": message}}),
                );
                return 1;
            }
            Ok(InputEvent::Message(Err(message))) => {
                if write_message(&mut output, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": message}})).is_err() { return 1; }
                continue;
            }
            Ok(InputEvent::Message(Ok(message))) => message,
        };
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str);
        if message.get("jsonrpc") != Some(&json!("2.0"))
            || method.is_none()
            || id
                .as_ref()
                .is_some_and(|id| !id.is_string() && !id.is_i64() && !id.is_u64())
        {
            if write_message(&mut output, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "invalid JSON-RPC request"}})).is_err() { return 1; }
            continue;
        }
        let method = method.unwrap();
        if method == "exit" {
            return i32::from(!session.shutdown);
        }
        let is_cancelled = || {
            id.as_ref().is_some_and(|id| {
                cancelled
                    .lock()
                    .is_ok_and(|mut pending| pending.remove(&id.to_string()))
            })
        };
        let result = if is_cancelled() {
            Err((-32800, "request cancelled".into()))
        } else {
            session.handle(
                method,
                message.get("params").unwrap_or(&Value::Null),
                &mut output,
            )
        };
        let result = if is_cancelled() {
            Err((-32800, "request cancelled".into()))
        } else {
            result
        };
        if let Some(id) = id {
            let response = match result {
                Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
                Err((code, message)) => {
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
                }
            };
            if write_message(&mut output, &response).is_err() {
                return 1;
            }
        } else if let Err((code, message)) = result
            && code != -32601
            && write_message(&mut output, &json!({"jsonrpc": "2.0", "method": "window/logMessage", "params": {"type": 1, "message": message}})).is_err()
        {
            return 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn framing_roundtrips_and_rejects_invalid_inputs() {
        let message = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"text": "\u{65e5}\u{1f600}"}});
        let mut bytes = Vec::new();
        write_message(&mut bytes, &message).unwrap();
        let mut input = Cursor::new(bytes);
        assert_eq!(read_message(&mut input).unwrap().unwrap().unwrap(), message);
        assert!(read_message(&mut input).unwrap().is_none());
        for header in [
            "Content-Length: -1\r\n\r\n",
            "Content-Length: 16777217\r\n\r\n",
            "Content-Length: 1\r\nContent-Length: 1\r\n\r\n0",
            "Content-Type: json\r\n\r\n",
        ] {
            assert!(read_message(&mut Cursor::new(header)).is_err());
        }
        assert!(
            read_message(&mut Cursor::new("Content-Length: 1\r\n\r\n{"))
                .unwrap()
                .unwrap()
                .is_err()
        );
        let deep = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        let framed = format!("Content-Length: {}\r\n\r\n{deep}", deep.len());
        assert!(
            read_message(&mut Cursor::new(framed))
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn positions_preserve_utf8_utf16_crlf_and_eof() {
        let text = "a\u{65e5}\u{1f600}\r\nz\n";
        for (encoding, character) in [(PositionEncoding::Utf8, 8), (PositionEncoding::Utf16, 4)] {
            let mapper = PositionMapper::new(text, encoding);
            assert_eq!(
                mapper.position(text, 8),
                Some(json!({"line": 0, "character": character}))
            );
            assert_eq!(
                mapper.offset(text, &json!({"line": 0, "character": character})),
                Some(8)
            );
            assert_eq!(
                mapper.position(text, text.len()),
                Some(json!({"line": 2, "character": 0}))
            );
            assert_eq!(
                mapper.offset(text, &json!({"line": 3, "character": 0})),
                None
            );
            assert_eq!(
                mapper.offset(text, &json!({"line": 1, "character": 2})),
                None
            );
        }
        let mapper = PositionMapper::new(text, PositionEncoding::Utf16);
        assert_eq!(
            mapper.offset(text, &json!({"line": 0, "character": 3})),
            None
        );
        assert_eq!(mapper.position(text, 2), None);
    }
}
