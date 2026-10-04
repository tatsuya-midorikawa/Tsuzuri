use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::check::semantic::{Reference, Role, SemanticIndex, SymbolKind};
use crate::check::{CheckedModule, ModuleOrigin, semantic};
use crate::copies::{CopyKind, CopySite};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::driver::Project;
use crate::syntax::{Token, TokenKind};
use serde_json::{Value, json};

const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const MAX_HEADER: usize = 8192;
const MAX_LOCATIONS: usize = 10_000;
const MAX_COMPLETIONS: usize = 500;
const MAX_SYMBOLS: usize = 256;

const KEYWORDS: [&str; 41] = [
    "fn", "def", "rec", "and", "export", "extern", "private", "record", "union", "type", "const",
    "test", "class", "instance", "deriving", "let", "task", "do", "return", "yield", "for", "in",
    "to", "downto", "while", "break", "continue", "mut", "ref", "deref", "new", "as", "if", "then",
    "elif", "else", "match", "with", "when", "true", "false",
];
/// Words that are keywords only in their syntax and identifiers elsewhere.
const CONTEXTUAL_KEYWORDS: [&str; 7] =
    ["finally", "is", "namespace", "of", "try", "using", "where"];
const TOKEN_TYPES: [&str; 11] = [
    "namespace",
    "struct",
    "enum",
    "enumMember",
    "property",
    "type",
    "interface",
    "method",
    "function",
    "variable",
    "parameter",
];
const TOKEN_MODIFIERS: [&str; 3] = ["declaration", "readonly", "defaultLibrary"];

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
    diagnostics: Vec<Diagnostic>,
    /// The implicit array and list copies that inlay hints show (A15).
    copies: Vec<CopySite>,
    modules: ModuleNames,
}

struct Session {
    initialized: bool,
    shutdown: bool,
    encoding: PositionEncoding,
    roots: Vec<PathBuf>,
    buffers: BTreeMap<PathBuf, OpenBuffer>,
    projects: BTreeMap<PathBuf, ProjectState>,
    /// The last analyzed state of each root, kept while edits break analysis.
    good: BTreeMap<PathBuf, ProjectState>,
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
            good: BTreeMap::new(),
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
        if let Some(state) = self.projects.remove(&directory)
            && state.index.is_some()
        {
            self.good.insert(directory.clone(), state);
        }
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
        let (analyzed, diagnostics) = analyze(&project);
        let (index, copies) = analyzed.map_or((None, Vec::new()), |(index, module)| {
            (Some(index), crate::copies::costly_sites(&module))
        });
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
            let messages = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.span.source.unwrap_or(project.root) == id)
                .filter_map(|diagnostic| diagnostic_json(&mappers[id], &source.text, diagnostic))
                .collect();
            self.publish(output, &source.path, messages)?;
        }
        if index.is_some() {
            self.good.remove(&directory);
        }
        let modules = ModuleNames::new(&project);
        self.projects.insert(
            directory,
            ProjectState {
                project,
                index,
                mappers,
                diagnostics,
                copies,
                modules,
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
            let legend = json!({"tokenTypes": TOKEN_TYPES, "tokenModifiers": TOKEN_MODIFIERS});
            return Ok(json!({
                "capabilities": {
                    "positionEncoding": if self.encoding == PositionEncoding::Utf8 { "utf-8" } else { "utf-16" },
                    "textDocumentSync": {"openClose": true, "change": 1},
                    "hoverProvider": true,
                    "definitionProvider": true,
                    "documentSymbolProvider": true,
                    "referencesProvider": true,
                    "documentHighlightProvider": true,
                    "renameProvider": {"prepareProvider": true},
                    "workspaceSymbolProvider": true,
                    "completionProvider": {"triggerCharacters": ["."], "resolveProvider": false},
                    "signatureHelpProvider": {"triggerCharacters": [" ", "("], "retriggerCharacters": [","]},
                    "semanticTokensProvider": {"legend": legend, "full": true, "range": false},
                    "codeActionProvider": {"codeActionKinds": ["quickfix"]},
                    "documentFormattingProvider": true,
                    "inlayHintProvider": true,
                },
                "serverInfo": {"name": "Tsuzuri", "version": env!("CARGO_PKG_VERSION")},
            }));
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
                let (directory, source_id) = self.document(params, output)?;
                let Some(source_id) = source_id else {
                    return Ok(Value::Null);
                };
                let state = &self.projects[&directory];
                let source = &state.project.sources[source_id];
                if method.ends_with("documentSymbol") {
                    let symbols: Vec<_> = state.index.iter().flat_map(|index| &index.symbols).filter(|symbol| symbol.selection.source == Some(source_id)).filter_map(|symbol| {
                        Some(json!({"name": symbol.name, "kind": symbol.kind, "range": state.mappers[source_id].range(&source.text, symbol.span)?, "selectionRange": state.mappers[source_id].range(&source.text, symbol.selection)?}))
                    }).collect();
                    return Ok(json!(symbols));
                }
                let offset = position_offset(&state.mappers[source_id], &source.text, params)?;
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
                    json!({"uri": self.uri(&target_source.path), "range": state.mappers[target_id].range(&target_source.text, target)}),
                )
            }
            "textDocument/references" | "textDocument/documentHighlight" => {
                let (directory, source_id) = self.document(params, output)?;
                let Some(source_id) = source_id else {
                    return Ok(Value::Null);
                };
                self.references(
                    &self.projects[&directory],
                    source_id,
                    params,
                    method.ends_with("documentHighlight"),
                )
            }
            "textDocument/prepareRename" | "textDocument/rename" => {
                let new_name = if method.ends_with("prepareRename") {
                    None
                } else {
                    let name = params["newName"]
                        .as_str()
                        .ok_or_else(|| invalid("newName is required"))?;
                    if !is_identifier(name) {
                        return Err(invalid(&format!(
                            "'{name}' is not a valid identifier; use letters, digits and '_' and avoid keywords"
                        )));
                    }
                    Some(name)
                };
                let (directory, source_id) = self.document(params, output)?;
                let (Some(source_id), Some(state)) = (source_id, self.projects.get(&directory))
                else {
                    return Err(failed(NO_RENAMABLE_NAME));
                };
                let source = &state.project.sources[source_id];
                let offset = position_offset(&state.mappers[source_id], &source.text, params)?;
                match new_name {
                    None => {
                        let target = rename_target(state, source_id, offset)?;
                        let index = state.index.as_ref().expect("a rename target has an index");
                        Ok(json!({
                            "range": state.mappers[source_id].range(&source.text, target.span),
                            "placeholder": index.definitions[target.definition].name,
                        }))
                    }
                    Some(name) => self.rename(&directory, source_id, offset, name),
                }
            }
            "workspace/symbol" => {
                let due: Vec<_> = self
                    .pending
                    .values()
                    .map(|(path, _)| path.clone())
                    .collect();
                for path in due {
                    self.refresh(&path, output)
                        .map_err(|error| (-32603, error.to_string()))?;
                }
                Ok(self.workspace_symbols(params["query"].as_str().unwrap_or("")))
            }
            "textDocument/completion" | "textDocument/signatureHelp" => {
                let path = document_path(params)?;
                let (directory, _) = self.document(params, output)?;
                let text = self
                    .text(&directory, &path)
                    .ok_or_else(|| invalid("document is not open"))?;
                let mapper = PositionMapper::new(text, self.encoding);
                let offset = position_offset(&mapper, text, params)?;
                let view = self.view(&directory, &path, text);
                if method.ends_with("completion") {
                    Ok(completion(view.as_ref(), text, offset))
                } else {
                    Ok(signature_help(view.as_ref(), text, offset))
                }
            }
            "textDocument/semanticTokens/full" => {
                let path = document_path(params)?;
                let (directory, _) = self.document(params, output)?;
                let text = self
                    .text(&directory, &path)
                    .ok_or_else(|| invalid("document is not open"))?;
                let data = self
                    .view(&directory, &path, text)
                    .map(|view| semantic_tokens(&view, text, self.encoding))
                    .unwrap_or_default();
                Ok(json!({"data": data}))
            }
            "textDocument/inlayHint" => {
                let path = document_path(params)?;
                let (directory, _) = self.document(params, output)?;
                let text = self
                    .text(&directory, &path)
                    .ok_or_else(|| invalid("document is not open"))?;
                match self.view(&directory, &path, text) {
                    Some(view) => copy_hints(&view, text, self.encoding, &params["range"]),
                    None => Ok(json!([])),
                }
            }
            "textDocument/codeAction" => {
                let (directory, source_id) = self.document(params, output)?;
                let Some(source_id) = source_id else {
                    return Ok(json!([]));
                };
                self.code_actions(&directory, source_id, params)
            }
            "textDocument/formatting" => {
                let path = document_path(params)?;
                let directory = self.project_root(&path);
                let Some(text) = self.text(&directory, &path) else {
                    return Ok(Value::Null);
                };
                Ok(format_document(&path, text, self.encoding))
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        }
    }

    /// Refreshes the document's project when it is stale and finds its source.
    fn document(
        &mut self,
        params: &Value,
        output: &mut impl Write,
    ) -> Result<(PathBuf, Option<usize>), (i64, String)> {
        let path = document_path(params)?;
        let directory = self.project_root(&path);
        if self.pending.contains_key(&directory) || !self.projects.contains_key(&directory) {
            self.refresh(&path, output)
                .map_err(|error| (-32603, error.to_string()))?;
        }
        let source = self.projects.get(&directory).and_then(|state| {
            state
                .project
                .sources
                .iter()
                .position(|source| source.path == path)
        });
        Ok((directory, source))
    }

    fn uri(&self, path: &Path) -> Option<String> {
        self.buffers
            .get(path)
            .map(|buffer| buffer.uri.clone())
            .or_else(|| file_uri(path))
    }

    /// The current text of a document: its open buffer or the loaded source.
    fn text(&self, directory: &Path, path: &Path) -> Option<&str> {
        self.buffers
            .get(path)
            .map(|buffer| buffer.text.as_str())
            .or_else(|| {
                let state = self.projects.get(directory)?;
                let source = state
                    .project
                    .sources
                    .iter()
                    .find(|source| source.path == path)?;
                Some(source.text.as_str())
            })
    }

    /// The current index, or the last successful one mapped onto `text`.
    fn view<'s>(&'s self, directory: &Path, path: &Path, text: &str) -> Option<View<'s>> {
        let state = self
            .projects
            .get(directory)
            .filter(|state| state.index.is_some())
            .or_else(|| self.good.get(directory))?;
        let source = state
            .project
            .sources
            .iter()
            .position(|source| source.path == path)?;
        Some(View {
            state,
            index: state.index.as_ref()?,
            source,
            map: StaleMap::new(&state.project.sources[source].text, text),
        })
    }

    fn references(
        &self,
        state: &ProjectState,
        source_id: usize,
        params: &Value,
        highlight: bool,
    ) -> RpcResult {
        let source = &state.project.sources[source_id];
        let offset = position_offset(&state.mappers[source_id], &source.text, params)?;
        let Some(index) = &state.index else {
            return Ok(Value::Null);
        };
        let Some(occurrence) = index.occurrence_at(source_id, offset) else {
            return Ok(Value::Null);
        };
        let include = !params["context"]["includeDeclaration"]
            .as_bool()
            .is_some_and(|include| !include);
        let found: Vec<_> = index
            .occurrences(occurrence.definition)
            .into_iter()
            .filter(|found| is_user(&state.project, found.span))
            .filter(|found| {
                if highlight {
                    found.span.source == Some(source_id)
                } else {
                    include || found.role != Role::Declaration
                }
            })
            .take(MAX_LOCATIONS)
            .filter_map(|found| {
                let id = found.span.source?;
                let file = &state.project.sources[id];
                let range = state.mappers[id].range(&file.text, found.span)?;
                Some(if highlight {
                    let kind = if found.role == Role::Read { 2 } else { 3 };
                    json!({"range": range, "kind": kind})
                } else {
                    json!({"uri": self.uri(&file.path), "range": range})
                })
            })
            .collect();
        Ok(if found.is_empty() {
            Value::Null
        } else {
            json!(found)
        })
    }

    fn rename(
        &self,
        directory: &Path,
        source_id: usize,
        offset: usize,
        new_name: &str,
    ) -> RpcResult {
        let state = &self.projects[directory];
        let target = rename_target(state, source_id, offset)?;
        let index = state.index.as_ref().expect("a rename target has an index");
        let definition = &index.definitions[target.definition];
        if new_name == definition.name {
            return Ok(json!({"changes": {}}));
        }
        if let Some(kind) = conflict(index, target.definition, new_name) {
            return Err(failed(format!(
                "renaming '{}' to '{new_name}' conflicts with {} '{new_name}'; choose another name",
                definition.name,
                kind.describe()
            )));
        }
        let spans: Vec<_> = index
            .occurrences(target.definition)
            .into_iter()
            .map(|occurrence| occurrence.span)
            .filter(|span| is_user(&state.project, *span))
            .collect();
        if spans.len() > MAX_LOCATIONS {
            return Err(failed(
                "rename would change more than 10000 locations; rename in smaller steps",
            ));
        }
        self.verify_edits(directory, state, &spans, new_name)
            .map_err(|detail| {
                failed(format!(
                    "rename would change the meaning of the program ({detail}); choose another name or rename manually"
                ))
            })?;
        let mut changes = serde_json::Map::new();
        for span in spans {
            let id = span.source.expect("occurrences carry their source");
            let file = &state.project.sources[id];
            let (Some(uri), Some(range)) = (
                self.uri(&file.path),
                state.mappers[id].range(&file.text, span),
            ) else {
                return Err((-32603, "cannot address a renamed location".into()));
            };
            if let Value::Array(edits) = changes.entry(uri).or_insert_with(|| json!([])) {
                edits.push(json!({"range": range, "newText": new_name}));
            }
        }
        Ok(json!({"changes": changes}))
    }

    /// Reanalyzes the project with `spans` renamed and requires the same
    /// warnings and the same binding of every name.
    fn verify_edits(
        &self,
        directory: &Path,
        state: &ProjectState,
        spans: &[Span],
        new_name: &str,
    ) -> Result<(), String> {
        let index = state
            .index
            .as_ref()
            .expect("verified edits start from an index");
        let mut overlays: BTreeMap<PathBuf, String> = self
            .buffers
            .iter()
            .map(|(path, buffer)| (path.clone(), buffer.text.clone()))
            .collect();
        let mut edited: BTreeMap<usize, Vec<Span>> = BTreeMap::new();
        for span in spans {
            edited
                .entry(span.source.expect("occurrences carry their source"))
                .or_default()
                .push(*span);
        }
        for (source, spans) in &edited {
            let file = &state.project.sources[*source];
            let mut text = file.text.clone();
            for span in spans.iter().rev() {
                text.replace_range(span.start..span.end, new_name);
            }
            overlays.insert(file.path.clone(), text);
        }
        let project = Project::load_with_overlays(directory, &overlays)
            .map_err(|error| format!("{}: {}", error.diagnostic.code, error.diagnostic.message))?;
        let (renamed, diagnostics) = analyze(&project);
        let describe =
            |diagnostic: &Diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message);
        let Some((renamed, _)) = renamed else {
            let error = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.severity == Severity::Error)
                .or(diagnostics.first());
            return Err(error.map_or_else(|| "the program no longer compiles".into(), describe));
        };
        let counts = |list: &[Diagnostic]| {
            let mut counts = BTreeMap::new();
            for diagnostic in list {
                *counts.entry(diagnostic.code).or_insert(0usize) += 1;
            }
            counts
        };
        let (before, after) = (counts(&state.diagnostics), counts(&diagnostics));
        if let Some(added) = diagnostics
            .iter()
            .find(|warning| after[warning.code] > before.get(warning.code).copied().unwrap_or(0))
        {
            return Err(describe(added));
        }
        if let Some(removed) = state.diagnostics.iter().find(|warning| {
            warning.code != "W1001"
                && after.get(warning.code).copied().unwrap_or(0) < before[warning.code]
        }) {
            return Err(describe(removed));
        }
        let moved = || "a name would refer to a different definition".to_owned();
        let new_ids: BTreeMap<&Path, usize> = project
            .sources
            .iter()
            .enumerate()
            .map(|(id, source)| (source.path.as_path(), id))
            .collect();
        let place = |span: Span| -> Option<(usize, usize)> {
            let source = span.source?;
            let id = *new_ids.get(state.project.sources.get(source)?.path.as_path())?;
            let shift: isize = edited
                .get(&source)
                .into_iter()
                .flatten()
                .filter(|edit| edit.end <= span.start)
                .map(|edit| new_name.len() as isize - (edit.end - edit.start) as isize)
                .sum();
            Some((id, span.start.checked_add_signed(shift)?))
        };
        let mut bound = BTreeMap::new();
        for occurrence in renamed.all_occurrences() {
            if let (Some(source), Some(target)) = (
                occurrence
                    .span
                    .source
                    .filter(|_| is_user(&project, occurrence.span)),
                renamed.definitions[occurrence.definition].span.source,
            ) {
                let definition = renamed.definitions[occurrence.definition].span.start;
                bound
                    .entry((source, occurrence.span.start))
                    .or_insert((target, definition));
            }
        }
        let mut seen = BTreeSet::new();
        for occurrence in index
            .all_occurrences()
            .filter(|occurrence| is_user(&state.project, occurrence.span))
        {
            let at = place(occurrence.span).ok_or_else(moved)?;
            if !seen.insert(at) {
                continue;
            }
            let definition = place(index.definitions[occurrence.definition].span);
            if bound.get(&at).copied() != definition {
                return Err(moved());
            }
        }
        if seen.len() != bound.len() {
            return Err(moved());
        }
        Ok(())
    }

    fn workspace_symbols(&self, query: &str) -> Value {
        let query = query.to_ascii_lowercase();
        let mut found = Vec::new();
        for state in self.projects.values() {
            let Some(index) = &state.index else {
                continue;
            };
            for symbol in &index.symbols {
                let Some(id) = symbol.selection.source else {
                    continue;
                };
                let file = &state.project.sources[id];
                if file.origin != ModuleOrigin::User
                    || !symbol.name.to_ascii_lowercase().contains(&query)
                {
                    continue;
                }
                let (Some(uri), Some(range)) = (
                    self.uri(&file.path),
                    state.mappers[id].range(&file.text, symbol.span),
                ) else {
                    continue;
                };
                found.push((
                    uri.clone(),
                    symbol.selection.start,
                    json!({"name": symbol.name, "kind": symbol.kind, "location": {"uri": uri, "range": range}, "containerName": file.name}),
                ));
            }
        }
        found.sort_by(|left, right| (&left.0, left.1).cmp(&(&right.0, right.1)));
        json!(
            found
                .into_iter()
                .take(MAX_SYMBOLS)
                .map(|(_, _, symbol)| symbol)
                .collect::<Vec<_>>()
        )
    }

    fn code_actions(&self, directory: &Path, source_id: usize, params: &Value) -> RpcResult {
        if params["context"]["only"]
            .as_array()
            .is_some_and(|kinds| !kinds.iter().any(|kind| kind == "quickfix"))
        {
            return Ok(json!([]));
        }
        let state = &self.projects[directory];
        let source = &state.project.sources[source_id];
        let mapper = &state.mappers[source_id];
        let bound = |name: &str| {
            mapper
                .offset(&source.text, &params["range"][name])
                .ok_or_else(|| invalid(POSITION_ERROR))
        };
        let (start, end) = (bound("start")?, bound("end")?);
        let mut actions = Vec::new();
        for diagnostic in &state.diagnostics {
            let span = diagnostic.span;
            if diagnostic.code != "W1001"
                || span.source != Some(source_id)
                || span.start > end
                || start > span.end
            {
                continue;
            }
            let Some(name) = source.text.get(span.start..span.end) else {
                continue;
            };
            let Ok(edit) = self.rename(directory, source_id, span.start, &format!("_{name}"))
            else {
                continue;
            };
            actions.push(json!({
                "title": format!("Prefix '{name}' with '_'"),
                "kind": "quickfix",
                "diagnostics": [diagnostic_json(mapper, &source.text, diagnostic)],
                "isPreferred": true,
                "edit": edit,
            }));
        }
        Ok(json!(actions))
    }
}

const POSITION_ERROR: &str = "position is outside the document or splits a Unicode character";
const NO_RENAMABLE_NAME: &str = "no renamable name at this position; place the cursor on a name";

fn failed(message: impl Into<String>) -> (i64, String) {
    (-32803, message.into())
}

fn position_offset(
    mapper: &PositionMapper,
    text: &str,
    params: &Value,
) -> Result<usize, (i64, String)> {
    mapper
        .offset(text, &params["position"])
        .ok_or_else(|| invalid(POSITION_ERROR))
}

fn diagnostic_json(mapper: &PositionMapper, text: &str, diagnostic: &Diagnostic) -> Option<Value> {
    Some(json!({
        "range": mapper.range(text, diagnostic.span)?,
        "severity": if diagnostic.severity == Severity::Error { 1 } else { 2 },
        "code": diagnostic.code,
        "source": "tsuzuri",
        "message": diagnostic.message,
    }))
}

fn is_user(project: &Project, span: Span) -> bool {
    span.source
        .and_then(|source| project.sources.get(source))
        .is_some_and(|source| source.origin == ModuleOrigin::User)
}

/// Analyzes a project; a successful index also records `def` and `fn` heads.
fn analyze(project: &Project) -> (Option<(SemanticIndex, CheckedModule)>, Vec<Diagnostic>) {
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
            namespace: &source.namespace,
        })
        .collect();
    let mut semantic = SemanticIndex::default();
    match crate::analyze_inputs_semantic(&inputs, &mut semantic) {
        Ok(mut module) => {
            add_function_heads(&mut semantic, project);
            let warnings = std::mem::take(&mut module.warnings);
            (Some((semantic, module)), warnings)
        }
        Err(errors) => (None, errors.diagnostics),
    }
}

/// Names after top-level `def`, `fn`, `and` (skipping `rec`), and a column-0 `let`.
fn function_heads(source: &str) -> Vec<(String, Span)> {
    let (tokens, _) = crate::lexer::lex_all(source);
    let mut heads = Vec::new();
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if opens(&token.kind) {
            depth += 1;
        } else if closes(&token.kind) {
            depth = depth.saturating_sub(1);
        }
        if depth > 0 {
            continue;
        }
        let introduces = match token.kind {
            TokenKind::Def | TokenKind::Fn | TokenKind::And => true,
            TokenKind::Let => {
                token.span.start == 0 || source.as_bytes()[token.span.start - 1] == b'\n'
            }
            _ => false,
        };
        if !introduces {
            continue;
        }
        let mut next = index + 1;
        if tokens
            .get(next)
            .is_some_and(|token| token.kind == TokenKind::Rec)
        {
            next += 1;
        }
        if let Some(Token {
            kind: TokenKind::Ident(name),
            span,
        }) = tokens.get(next)
        {
            heads.push((name.clone(), *span));
        }
    }
    heads
}

fn add_function_heads(index: &mut SemanticIndex, project: &Project) {
    let mut functions: BTreeMap<usize, BTreeMap<&str, usize>> = BTreeMap::new();
    for (definition, item) in index.definitions.iter().enumerate() {
        if let Some(source) = item.span.source
            && item.kind == SymbolKind::Function
            && is_user(project, item.span)
        {
            functions
                .entry(source)
                .or_default()
                .insert(item.name.as_str(), definition);
        }
    }
    for (source, names) in &functions {
        for (name, span) in function_heads(&project.sources[*source].text) {
            if let Some(&definition) = names.get(name.as_str()) {
                index.references.push(Reference {
                    span: span.in_source(*source),
                    definition,
                    role: Role::Declaration,
                });
            }
        }
    }
    let key = |reference: &Reference| {
        (
            reference.span.source,
            reference.span.start,
            reference.span.end,
        )
    };
    index.references.sort_by_key(key);
    index.references.dedup_by_key(|reference| key(reference));
}

fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 256
        && name != "_"
        && !KEYWORDS.contains(&name)
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn rename_target(
    state: &ProjectState,
    source: usize,
    offset: usize,
) -> Result<Reference, (i64, String)> {
    let index = match &state.index {
        Some(index)
            if !state
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error) =>
        {
            index
        }
        _ => {
            return Err(failed(
                "cannot rename while the project has errors; fix the errors first",
            ));
        }
    };
    let occurrence = index
        .occurrence_at(source, offset)
        .ok_or_else(|| failed(NO_RENAMABLE_NAME))?;
    let definition = &index.definitions[occurrence.definition];
    let name = &definition.name;
    if !is_user(&state.project, definition.span) {
        return Err(failed(format!(
            "cannot rename '{name}' because it is defined in the standard library"
        )));
    }
    if definition.exported {
        return Err(failed(format!(
            "cannot rename exported function '{name}' because its export name is part of the ABI; change the export manually"
        )));
    }
    use SymbolKind::*;
    if !matches!(
        definition.kind,
        Local | Parameter | Function | Const | Record | Union | Case | Field
    ) {
        return Err(failed(format!(
            "renaming {} names is not supported yet; rename '{name}' manually",
            definition.kind.describe()
        )));
    }
    Ok(occurrence)
}

/// The kind of a definition that `new_name` would collide with.
fn conflict(index: &SemanticIndex, target: usize, new_name: &str) -> Option<SymbolKind> {
    use SymbolKind::*;
    let definition = &index.definitions[target];
    let named = |item: &&semantic::Definition| item.name == new_name;
    let found = match definition.kind {
        Local | Parameter => {
            let occurrences = index.occurrences(target);
            return index
                .scopes
                .iter()
                .filter(|scope| {
                    scope.definition != target
                        && index.definitions[scope.definition].name == new_name
                })
                .find(|scope| {
                    occurrences.iter().any(|occurrence| {
                        occurrence.span.source == scope.visible.source
                            && scope.visible.start <= occurrence.span.start
                            && occurrence.span.start <= scope.visible.end
                    })
                })
                .map(|scope| index.definitions[scope.definition].kind);
        }
        Field => index
            .definitions
            .iter()
            .filter(named)
            .find(|item| item.kind == Field && item.container == definition.container),
        Record | Union => index.definitions.iter().filter(named).find(|item| {
            item.module == definition.module && matches!(item.kind, Record | Union | Alias | Class)
        }),
        _ => index.definitions.iter().filter(named).find(|item| {
            item.module == definition.module
                && matches!(item.kind, Function | Const | Case | Extern)
        }),
    };
    found.map(|item| item.kind)
}

/// Offsets of an older text carried onto a newer one through their common
/// prefix and suffix; offsets inside the edited middle have no image.
#[derive(Clone, Copy, Debug)]
struct StaleMap {
    prefix: usize,
    old_end: usize,
    new_end: usize,
}

impl StaleMap {
    fn new(old: &str, new: &str) -> Self {
        let mut prefix = old
            .bytes()
            .zip(new.bytes())
            .take_while(|(left, right)| left == right)
            .count();
        while !old.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let limit = old.len().min(new.len()) - prefix;
        let mut suffix = old
            .bytes()
            .rev()
            .zip(new.bytes().rev())
            .take(limit)
            .take_while(|(left, right)| left == right)
            .count();
        while !old.is_char_boundary(old.len() - suffix) {
            suffix -= 1;
        }
        Self {
            prefix,
            old_end: old.len() - suffix,
            new_end: new.len() - suffix,
        }
    }

    fn forward(&self, offset: usize) -> Option<usize> {
        if offset <= self.prefix {
            Some(offset)
        } else if offset >= self.old_end {
            Some(offset - self.old_end + self.new_end)
        } else {
            None
        }
    }

    fn backward(&self, offset: usize) -> Option<usize> {
        if offset <= self.prefix {
            Some(offset)
        } else if offset >= self.new_end {
            Some(offset - self.new_end + self.old_end)
        } else {
            None
        }
    }

    /// Carries a name span whose length the edit did not change.
    fn span(&self, span: Span) -> Option<Span> {
        let start = self.forward(span.start)?;
        let end = self.forward(span.end)?;
        (end - start == span.end - span.start).then_some(Span { start, end, ..span })
    }

    /// Carries a scope, widening ends inside the edit to the edited text.
    fn scope(&self, span: Span) -> (usize, usize) {
        (
            self.forward(span.start).unwrap_or(self.prefix),
            self.forward(span.end).unwrap_or(self.new_end),
        )
    }
}

struct View<'s> {
    state: &'s ProjectState,
    index: &'s SemanticIndex,
    source: usize,
    map: StaleMap,
}

impl View<'_> {
    fn module(&self) -> Option<String> {
        self.state.modules.key(self.source).map(str::to_owned)
    }

    /// The key of the module that `path` names in the current source.
    fn module_path(&self, path: &str) -> Option<&str> {
        self.state.modules.resolve(self.source, path)
    }

    /// The definition spelled by an identifier token of the current text.
    fn definition_at(&self, span: Span) -> Option<usize> {
        let start = self.map.backward(span.start)?;
        self.index
            .occurrence_at(self.source, start)
            .filter(|occurrence| occurrence.span.start == start)
            .map(|occurrence| occurrence.definition)
    }

    fn is_std(&self, definition: usize) -> bool {
        !is_user(&self.state.project, self.index.definitions[definition].span)
    }
}

/// Module keys, which qualify declarations in the semantic index, by source
/// and by full name (`Namespace.File`), as the checker resolves them.
#[derive(Default)]
struct ModuleNames {
    /// Each source's key and namespace.
    sources: Vec<Option<(String, String)>>,
    full: BTreeMap<String, String>,
    /// The namespaces that each source's `using` declarations import.
    usings: Vec<Vec<String>>,
}

impl ModuleNames {
    fn new(project: &Project) -> Self {
        let headers: Vec<_> = project
            .sources
            .iter()
            .map(|source| match source.origin {
                ModuleOrigin::User => crate::declared_header(&source.text),
                ModuleOrigin::Std => (None, Vec::new()),
            })
            .collect();
        let sources: Vec<_> = project
            .sources
            .iter()
            .zip(&headers)
            .map(|(source, (declared, _))| match source.origin {
                ModuleOrigin::User => crate::module_identity(
                    source.relative_path.to_str()?,
                    declared.as_ref(),
                    &source.namespace,
                )
                .ok(),
                ModuleOrigin::Std => source
                    .path
                    .to_str()
                    .and_then(crate::stdlib::module_name)
                    .map(|name| (name.to_owned(), String::new())),
            })
            .collect();
        let full = sources
            .iter()
            .flatten()
            .map(|(key, namespace)| {
                let full = if namespace.is_empty() {
                    key.clone()
                } else {
                    format!("{namespace}.{}", key.rsplit('.').next().unwrap_or(key))
                };
                (full, key.clone())
            })
            .collect();
        let mut names = Self {
            sources,
            full,
            usings: Vec::new(),
        };
        names.usings = headers
            .iter()
            .enumerate()
            .map(|(source, (_, usings))| {
                usings
                    .iter()
                    .filter_map(|using| names.namespace_path(source, &using.text))
                    .collect()
            })
            .collect();
        names
    }

    /// The namespace that `path` names from `source`, innermost scope first.
    fn namespace_path(&self, source: usize, path: &str) -> Option<String> {
        self.scopes(source).into_iter().find_map(|scope| {
            let namespace = qualify(scope, path);
            let prefix = format!("{namespace}.");
            self.full
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(full, _)| full.starts_with(&prefix))
                .then_some(namespace)
        })
    }

    fn key(&self, source: usize) -> Option<&str> {
        self.sources
            .get(source)?
            .as_ref()
            .map(|(key, _)| key.as_str())
    }

    fn keys(&self) -> impl Iterator<Item = &str> {
        self.sources.iter().flatten().map(|(key, _)| key.as_str())
    }

    /// The namespaces that qualify names in `source`, innermost first and
    /// ending with the global namespace `""`.
    fn scopes(&self, source: usize) -> Vec<&str> {
        let mut scope = self
            .sources
            .get(source)
            .and_then(Option::as_ref)
            .map_or("", |(_, namespace)| namespace.as_str());
        let mut scopes = vec![scope];
        while !scope.is_empty() {
            scope = scope.rsplit_once('.').map_or("", |(parent, _)| parent);
            scopes.push(scope);
        }
        scopes
    }

    /// The key of the module that `path` names from `source`: each scope
    /// qualifies `path`, innermost first, with the `using` namespaces after
    /// the innermost for a module name; otherwise `path` is a key.
    fn resolve(&self, source: usize, path: &str) -> Option<&str> {
        let usings = self
            .usings
            .get(source)
            .filter(|_| !path.contains('.'))
            .map_or(&[][..], Vec::as_slice);
        for (at, scope) in self.scopes(source).into_iter().enumerate() {
            if let Some(key) = self.full.get(&qualify(scope, path)) {
                return Some(key.as_str());
            }
            if at == 0 {
                let mut imported = usings
                    .iter()
                    .filter_map(|namespace| self.full.get(&format!("{namespace}.{path}")));
                match (imported.next(), imported.next()) {
                    (Some(key), None) => return Some(key.as_str()),
                    (Some(_), Some(_)) => return None,
                    _ => {}
                }
            }
        }
        self.keys().find(|key| *key == path)
    }

    /// The next segments below `path` in `source` with their full names, as
    /// in `Sample` -> `Shape` for `Sample.Shape`, and for an empty `path` also
    /// the modules that `using` imports. A segment that names a module shows
    /// the module that `resolve` picks; any other shows the innermost
    /// namespace that holds it, so an ambiguous import alone shows nothing.
    fn children(&self, source: usize, path: &str) -> BTreeMap<String, String> {
        let mut modules = BTreeSet::new();
        if path.is_empty() {
            for namespace in self.usings.get(source).into_iter().flatten() {
                let prefix = format!("{namespace}.");
                modules.extend(
                    self.full
                        .keys()
                        .filter_map(|full| full.strip_prefix(&prefix))
                        .filter(|rest| !rest.contains('.'))
                        .map(str::to_owned),
                );
            }
        }
        let mut namespaces = BTreeMap::new();
        let mut add = |prefix: &str, name: &str| {
            let Some(rest) = name.strip_prefix(prefix) else {
                return;
            };
            if let Some((child, _)) = rest.split_once('.') {
                namespaces
                    .entry(child.to_owned())
                    .or_insert_with(|| format!("{prefix}{child}"));
            } else {
                modules.insert(rest.to_owned());
            }
        };
        for scope in self.scopes(source) {
            let prefix = if path.is_empty() {
                qualify(scope, "")
            } else {
                format!("{}.", qualify(scope, path))
            };
            for full in self.full.keys() {
                add(&prefix, full);
            }
        }
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}.")
        };
        for key in self.keys() {
            add(&prefix, key);
        }
        let mut children: BTreeMap<_, _> = modules
            .into_iter()
            .filter_map(|child| {
                let key = self.resolve(source, &qualify(path, &child))?;
                let full = self
                    .full
                    .iter()
                    .find_map(|(full, module)| (module == key).then_some(full.as_str()))
                    .unwrap_or(key);
                Some((child, full.to_owned()))
            })
            .collect();
        for (child, namespace) in namespaces {
            children.entry(child).or_insert(namespace);
        }
        children
    }

    fn is_module(&self, full: &str) -> bool {
        self.full.contains_key(full) || self.keys().any(|key| key == full)
    }
}

/// `path` in the namespace `scope`; with an empty `path`, the prefix of
/// names in `scope`.
fn qualify(scope: &str, path: &str) -> String {
    match (scope.is_empty(), path.is_empty()) {
        (true, _) => path.to_owned(),
        (false, true) => format!("{scope}."),
        (false, false) => format!("{scope}.{path}"),
    }
}

fn completion_kind(kind: SymbolKind) -> u8 {
    match kind {
        SymbolKind::Module => 9,
        SymbolKind::Record => 22,
        SymbolKind::Union => 13,
        SymbolKind::Case => 20,
        SymbolKind::Field => 5,
        SymbolKind::Alias => 7,
        SymbolKind::Class => 8,
        SymbolKind::Method => 2,
        SymbolKind::Function | SymbolKind::Extern => 3,
        SymbolKind::Const => 21,
        SymbolKind::Parameter | SymbolKind::Local => 6,
    }
}

fn token_type(kind: SymbolKind) -> usize {
    match kind {
        SymbolKind::Module => 0,
        SymbolKind::Record => 1,
        SymbolKind::Union => 2,
        SymbolKind::Case => 3,
        SymbolKind::Field => 4,
        SymbolKind::Alias => 5,
        SymbolKind::Class => 6,
        SymbolKind::Method => 7,
        SymbolKind::Function | SymbolKind::Extern => 8,
        SymbolKind::Const | SymbolKind::Local => 9,
        SymbolKind::Parameter => 10,
    }
}

/// Tokens of `text` without the end marker.
fn tokens(text: &str) -> Vec<Token> {
    let (mut tokens, _) = crate::lexer::lex_all(text);
    tokens.retain(|token| token.kind != TokenKind::End);
    tokens
}

fn adjacent(left: &Token, right: &Token) -> bool {
    left.span.end == right.span.start
}

/// `Ident (. Ident)*` written without spaces and ending at `tokens[last]`.
fn dotted_chain(tokens: &[Token], last: usize) -> Vec<usize> {
    let mut chain = vec![last];
    let mut at = last;
    while at >= 2
        && tokens[at - 1].kind == TokenKind::Dot
        && matches!(tokens[at - 2].kind, TokenKind::Ident(_))
        && adjacent(&tokens[at - 2], &tokens[at - 1])
        && adjacent(&tokens[at - 1], &tokens[at])
    {
        at -= 2;
        chain.insert(0, at);
    }
    chain
}

fn ident(token: &Token) -> &str {
    match &token.kind {
        TokenKind::Ident(name) => name,
        _ => "",
    }
}

fn completion(view: Option<&View<'_>>, text: &str, offset: usize) -> Value {
    let empty = json!({"isIncomplete": false, "items": []});
    let tokens = tokens(text);
    if tokens.iter().any(|token| {
        matches!(
            token.kind,
            TokenKind::String(_)
                | TokenKind::DocComment(_)
                | TokenKind::InterpolationStart(_)
                | TokenKind::InterpolationMiddle(_)
                | TokenKind::InterpolationEnd(_)
        ) && token.span.start < offset
            && offset < token.span.end
    }) {
        return empty;
    }
    let Some(view) = view else {
        return empty;
    };
    let index = view.index;
    let module = view.module().unwrap_or_default();
    let modules = &view.state.modules;
    let mut items = Completions {
        index,
        items: Vec::new(),
    };
    let mut before = tokens.partition_point(|token| token.span.end <= offset);
    if before > 0
        && matches!(tokens[before - 1].kind, TokenKind::Ident(_))
        && tokens[before - 1].span.end == offset
    {
        before -= 1;
    }
    let member = before >= 2
        && tokens[before - 1].kind == TokenKind::Dot
        && matches!(tokens[before - 2].kind, TokenKind::Ident(_))
        && adjacent(&tokens[before - 2], &tokens[before - 1])
        && tokens
            .get(before)
            .is_none_or(|token| token.span.start >= offset || adjacent(&tokens[before - 1], token));
    let top_level = |kind: SymbolKind| {
        matches!(
            kind,
            SymbolKind::Function
                | SymbolKind::Extern
                | SymbolKind::Const
                | SymbolKind::Record
                | SymbolKind::Union
                | SymbolKind::Case
                | SymbolKind::Alias
                | SymbolKind::Class
        )
    };
    if member {
        let dot = &tokens[before - 1];
        let chain = dotted_chain(&tokens, before - 2);
        let joined = chain
            .iter()
            .map(|&at| ident(&tokens[at]))
            .collect::<Vec<_>>()
            .join(".");
        let resolved = view.module_path(&joined);
        let children = modules.children(view.source, &joined);
        if resolved.is_some() || !children.is_empty() {
            if let Some(key) = resolved {
                for (definition, item) in index.definitions.iter().enumerate() {
                    if item.module == key && top_level(item.kind) && (item.public || key == module)
                    {
                        items.definition(2, definition);
                    }
                }
            }
            for (child, full) in children {
                let kind = if modules.is_module(&full) {
                    "module"
                } else {
                    "namespace"
                };
                items.add(2, &child, 9, &format!("{kind} {full}"), None);
            }
        } else {
            let last = &tokens[*chain.last().expect("a chain has a name")];
            let qualifier = chain[..chain.len() - 1]
                .iter()
                .map(|&at| ident(&tokens[at]))
                .collect::<Vec<_>>()
                .join(".");
            let owner = if qualifier.is_empty() {
                module.as_str()
            } else {
                view.module_path(&qualifier).unwrap_or(&qualifier)
            };
            let union = view
                .definition_at(last.span)
                .or_else(|| {
                    index.definitions.iter().position(|item| {
                        item.kind == SymbolKind::Union
                            && item.module == owner
                            && (item.public || owner == module)
                            && item.name == ident(last)
                    })
                })
                .filter(|&definition| index.definitions[definition].kind == SymbolKind::Union);
            let (container, kind) = if let Some(union) = union {
                (Some(union), SymbolKind::Case)
            } else {
                let dot = view.map.backward(dot.span.start);
                let record = index
                    .receivers
                    .iter()
                    .filter(|(span, _)| span.source == Some(view.source) && Some(span.end) == dot)
                    .max_by_key(|(span, _)| span.end - span.start)
                    .map(|(_, record)| *record);
                (record, SymbolKind::Field)
            };
            for (definition, item) in index.definitions.iter().enumerate() {
                if container.is_some() && item.container == container && item.kind == kind {
                    items.definition(2, definition);
                }
            }
        }
    } else {
        let mut locals: BTreeMap<&str, usize> = BTreeMap::new();
        for scope in &index.scopes {
            if scope.visible.source != Some(view.source) {
                continue;
            }
            let (start, end) = view.map.scope(scope.visible);
            let item = &index.definitions[scope.definition];
            if start <= offset && offset <= end {
                let entry = locals.entry(item.name.as_str()).or_insert(scope.definition);
                if index.definitions[*entry].span.start < item.span.start {
                    *entry = scope.definition;
                }
            }
        }
        for definition in locals.into_values() {
            items.definition(0, definition);
        }
        for (definition, item) in index.definitions.iter().enumerate() {
            if item.module == module && top_level(item.kind) && !view.is_std(definition) {
                items.definition(1, definition);
            }
        }
        for (root, full) in modules.children(view.source, "") {
            let kind = if modules.is_module(&full) {
                "module"
            } else {
                "namespace"
            };
            items.add(2, &root, 9, &format!("{kind} {full}"), None);
        }
        for keyword in KEYWORDS.iter().chain(&CONTEXTUAL_KEYWORDS) {
            items.add(3, keyword, 14, "keyword", None);
        }
    }
    let mut items = items.items;
    items.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    items.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    let incomplete = items.len() > MAX_COMPLETIONS;
    let items: Vec<_> = items
        .into_iter()
        .take(MAX_COMPLETIONS)
        .map(|(_, _, item)| item)
        .collect();
    json!({"isIncomplete": incomplete, "items": items})
}

/// Completion items with their group and label for ordering.
struct Completions<'s> {
    index: &'s SemanticIndex,
    items: Vec<(u8, String, Value)>,
}

impl Completions<'_> {
    fn add(&mut self, group: u8, label: &str, kind: u8, detail: &str, definition: Option<usize>) {
        let mut item = json!({"label": label, "kind": kind, "detail": detail, "sortText": format!("{group}_{label}")});
        if let Some(doc) = definition
            .and_then(|definition| self.index.doc_for(self.index.definitions[definition].span))
        {
            item["documentation"] = json!({"kind": "markdown", "value": doc});
        }
        self.items.push((group, label.to_owned(), item));
    }

    fn definition(&mut self, group: u8, definition: usize) {
        let index = self.index;
        let item = &index.definitions[definition];
        self.add(
            group,
            &item.name,
            completion_kind(item.kind),
            &item.detail,
            Some(definition),
        );
    }
}

fn is_prefix(tokens: &[Token], index: usize) -> bool {
    match tokens[index].kind {
        TokenKind::Ref | TokenKind::Mut | TokenKind::Deref => true,
        TokenKind::Minus
        | TokenKind::Plus
        | TokenKind::Star
        | TokenKind::DoubleStar
        | TokenKind::Ampersand
        | TokenKind::Bang
        | TokenKind::Tilde
        | TokenKind::TripleTilde => tokens
            .get(index + 1)
            .is_some_and(|next| adjacent(&tokens[index], next)),
        _ => false,
    }
}

fn is_term(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident(_)
            | TokenKind::TypeVariable(_)
            | TokenKind::Integer(_)
            | TokenKind::BigInteger(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::ByteString(_)
            | TokenKind::ScalarString(_)
            | TokenKind::Char(_)
            | TokenKind::Utf8Char(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Dot
    )
}

fn opens(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::LeftParen
            | TokenKind::LeftBracket
            | TokenKind::LeftBrace
            | TokenKind::LeftList
            | TokenKind::InterpolationStart(_)
    )
}

fn closes(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
            | TokenKind::RightList
            | TokenKind::InterpolationEnd(_)
    )
}

/// Counts the argument terms written after a call head and returns where the last one ends.
fn count_terms(tokens: &[Token]) -> (usize, Option<usize>) {
    let (mut count, mut depth, mut prefix, mut dotted) = (0, 0usize, false, false);
    let mut end = None;
    for (index, token) in tokens.iter().enumerate() {
        if depth > 0 {
            if opens(&token.kind) {
                depth += 1;
            } else if closes(&token.kind) {
                depth -= 1;
                if depth == 0 {
                    end = Some(token.span.end);
                }
            }
            continue;
        }
        if token.kind == TokenKind::Dot && end == Some(token.span.start) {
            dotted = true;
            continue;
        }
        if dotted
            && matches!(token.kind, TokenKind::Ident(_))
            && adjacent(&tokens[index - 1], token)
        {
            dotted = false;
            end = Some(token.span.end);
            continue;
        }
        dotted = false;
        if !prefix {
            count += 1;
        }
        end = None;
        if is_prefix(tokens, index) {
            prefix = true;
        } else if opens(&token.kind) {
            prefix = false;
            depth = 1;
        } else {
            prefix = false;
            end = Some(token.span.end);
        }
    }
    (count, end)
}

/// The token index of the called name and the active argument around `offset`.
fn call_context(tokens: &[Token], text: &str, offset: usize) -> Option<(usize, usize)> {
    let line_start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let mut end = tokens.partition_point(|token| token.span.start < offset);
    let mut in_progress = false;
    loop {
        let (mut depth, mut start, mut open) = (0usize, 0, None);
        let mut index = end;
        while index > 0 {
            index -= 1;
            let token = &tokens[index];
            if closes(&token.kind) {
                depth += 1;
            } else if opens(&token.kind) && depth > 0 {
                depth -= 1;
            } else if token.kind == TokenKind::LeftParen {
                open = Some(index);
                start = index + 1;
                break;
            } else if depth == 0
                && (token.span.start < line_start
                    || !(is_term(&token.kind) || is_prefix(tokens, index)))
            {
                start = index + 1;
                break;
            }
        }
        if let Some(open) = open
            && open > 0
            && matches!(tokens[open - 1].kind, TokenKind::Ident(_))
            && adjacent(&tokens[open - 1], &tokens[open])
        {
            let mut depth = 0usize;
            let mut commas = 0;
            for token in &tokens[open + 1..end] {
                if opens(&token.kind) {
                    depth += 1;
                } else if closes(&token.kind) {
                    depth = depth.saturating_sub(1);
                } else if depth == 0 && token.kind == TokenKind::Comma {
                    commas += 1;
                }
            }
            return Some((open - 1, commas));
        }
        if start < end && matches!(tokens[start].kind, TokenKind::Ident(_)) {
            let mut head = start;
            while head + 2 < end
                && tokens[head + 1].kind == TokenKind::Dot
                && matches!(tokens[head + 2].kind, TokenKind::Ident(_))
                && adjacent(&tokens[head], &tokens[head + 1])
                && adjacent(&tokens[head + 1], &tokens[head + 2])
            {
                head += 2;
            }
            let (count, last) = count_terms(&tokens[head + 1..end]);
            let active = if !in_progress && last == Some(offset) {
                count.saturating_sub(1)
            } else {
                count
            };
            return Some((head, active));
        }
        // An empty `(` is an argument still being written to the enclosing call.
        let open = open.filter(|_| start == end)?;
        in_progress = true;
        end = open;
    }
}

fn signature_help(view: Option<&View<'_>>, text: &str, offset: usize) -> Value {
    let Some(view) = view else {
        return Value::Null;
    };
    let tokens = tokens(text);
    let Some((head, active)) = call_context(&tokens, text, offset) else {
        return Value::Null;
    };
    let index = view.index;
    let chain = dotted_chain(&tokens, head);
    let module = view.module().unwrap_or_default();
    let owner = if chain.len() > 1 {
        let qualifier = chain[..chain.len() - 1]
            .iter()
            .map(|&at| ident(&tokens[at]))
            .collect::<Vec<_>>()
            .join(".");
        view.module_path(&qualifier)
            .map_or(qualifier.clone(), str::to_owned)
    } else {
        module.clone()
    };
    let callable = |kind: SymbolKind| matches!(kind, SymbolKind::Function | SymbolKind::Extern);
    let definition = view.definition_at(tokens[head].span).or_else(|| {
        index.definitions.iter().position(|item| {
            callable(item.kind)
                && item.module == owner
                && (item.public || owner == module)
                && item.name == ident(&tokens[head])
        })
    });
    let Some(item) = definition
        .map(|definition| &index.definitions[definition])
        .filter(|item| callable(item.kind))
    else {
        return Value::Null;
    };
    let parameters = if item.parameters.is_empty() {
        "()".to_owned()
    } else {
        item.parameters
            .iter()
            .map(|parameter| format!("({parameter})"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    json!({
        "signatures": [{
            "label": format!("{} {parameters} -> {}", item.name, item.result),
            "parameters": item.parameters.iter().map(|parameter| json!({"label": parameter})).collect::<Vec<_>>(),
        }],
        "activeSignature": 0,
        "activeParameter": active,
    })
}

fn semantic_tokens(view: &View<'_>, text: &str, encoding: PositionEncoding) -> Vec<u32> {
    let index = view.index;
    // Start -> (end, token type, modifiers, whether a module path may qualify it).
    let mut found: BTreeMap<usize, (usize, usize, u32, bool)> = BTreeMap::new();
    for occurrence in index.all_occurrences() {
        if occurrence.span.source != Some(view.source) {
            continue;
        }
        let Some(span) = view.map.span(occurrence.span) else {
            continue;
        };
        let item = &index.definitions[occurrence.definition];
        let modifiers = u32::from(occurrence.role == Role::Declaration)
            | if item.kind == SymbolKind::Const { 2 } else { 0 }
            | if view.is_std(occurrence.definition) {
                4
            } else {
                0
            };
        let qualifiable = !matches!(
            item.kind,
            SymbolKind::Field | SymbolKind::Local | SymbolKind::Parameter
        );
        found.entry(span.start).or_insert((
            span.end,
            token_type(item.kind),
            modifiers,
            qualifiable,
        ));
    }
    let lexed = tokens(text);
    let positions: BTreeMap<usize, usize> = lexed
        .iter()
        .enumerate()
        .map(|(at, token)| (token.span.start, at))
        .collect();
    let mut namespaces = Vec::new();
    for (&start, &(_, _, _, qualifiable)) in &found {
        let Some(&at) = positions.get(&start).filter(|_| qualifiable) else {
            continue;
        };
        for segment in dotted_chain(&lexed, at).into_iter().rev().skip(1) {
            if !found.contains_key(&lexed[segment].span.start) {
                namespaces.push(lexed[segment].span);
            }
        }
    }
    for span in namespaces {
        found.insert(span.start, (span.end, 0, 0, false));
    }
    // The paths of the leading `namespace` and `using` declarations name namespaces.
    let mut at = 0;
    while lexed
        .get(at)
        .is_some_and(|token| matches!(ident(token), "namespace" | "using"))
    {
        at += 1;
        while let Some(segment) = lexed
            .get(at)
            .filter(|token| matches!(token.kind, TokenKind::Ident(_)))
        {
            found
                .entry(segment.span.start)
                .or_insert((segment.span.end, 0, 0, false));
            at += 1;
            if lexed.get(at).is_none_or(|dot| dot.kind != TokenKind::Dot) {
                break;
            }
            at += 1;
        }
    }
    let mapper = PositionMapper::new(text, encoding);
    let (mut data, mut line, mut column) = (Vec::new(), 0, 0);
    for (start, (end, kind, modifiers, _)) in found {
        let (Some(from), Some(to)) = (mapper.position(text, start), mapper.position(text, end))
        else {
            continue;
        };
        let at_line = from["line"].as_u64().unwrap_or(0) as u32;
        let at_column = from["character"].as_u64().unwrap_or(0) as u32;
        let length = to["character"].as_u64().unwrap_or(0) as u32 - at_column;
        let delta_column = if at_line == line {
            at_column - column
        } else {
            at_column
        };
        data.extend([at_line - line, delta_column, length, kind as u32, modifiers]);
        (line, column) = (at_line, at_column);
    }
    data
}

/// An inlay hint after each implicit array or list copy that ends in `range` (A15).
fn copy_hints(view: &View<'_>, text: &str, encoding: PositionEncoding, range: &Value) -> RpcResult {
    let mapper = PositionMapper::new(text, encoding);
    let bound = |name: &str| {
        mapper
            .offset(text, &range[name])
            .ok_or_else(|| invalid(POSITION_ERROR))
    };
    let (start, end) = (bound("start")?, bound("end")?);
    let hints: Vec<_> = view
        .state
        .copies
        .iter()
        .filter(|site| site.span.source == Some(view.source))
        .filter_map(|site| {
            let span = view.map.span(site.span)?;
            let kind = match site.kind {
                CopyKind::Local => "local",
                CopyKind::Field => "field",
                CopyKind::Element => "element",
                CopyKind::Tail => "tail",
                CopyKind::Payload => "payload",
                CopyKind::Dereference => "dereference",
                CopyKind::Temporary => "temporary",
            };
            (start..=end).contains(&span.end).then(|| {
                Some(json!({
                    "position": mapper.position(text, span.end)?,
                    "label": format!("copy ({kind})"),
                    "tooltip": crate::copies::message(&site.ty),
                    "paddingLeft": true,
                }))
            })?
        })
        .collect();
    Ok(json!(hints))
}

fn format_document(path: &Path, text: &str, encoding: PositionEncoding) -> Value {
    let kind = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::syntax::SourceKind::from_extension)
        .unwrap_or(crate::syntax::SourceKind::Code);
    match crate::formatter::format_source(&path.to_string_lossy(), text, kind) {
        Ok(result) if result.changed => {
            let mapper = PositionMapper::new(text, encoding);
            json!([{"range": mapper.range(text, Span::new(0, text.len())), "newText": result.formatted}])
        }
        Ok(_) => json!([]),
        Err(_) => Value::Null,
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

    #[test]
    fn function_heads_find_def_fn_and_and() {
        let source = "def rec even :: i64 -> bool\nand odd :: i64 -> bool = \\n -> n == 1\nfn even n = n == 0\nlet twice = \\x -> x\n  let inner = 1\nclass Twice<'a> {\n    def twice :: 'a -> 'a\n}\ninstance Twice<i64> {\n    fn twice n = n\n    and odd n = n\n}\nfn (";
        let heads: Vec<_> = function_heads(source)
            .into_iter()
            .map(|(name, span)| (name, span.start))
            .collect();
        let at = |needle: &str| source.find(needle).unwrap();
        assert_eq!(
            heads,
            [
                ("even".to_owned(), at("even ::")),
                ("odd".to_owned(), at("odd")),
                ("even".to_owned(), at("even n")),
                ("twice".to_owned(), at("twice")),
            ]
        );
    }

    #[test]
    fn keywords_match_the_lexer() {
        for keyword in KEYWORDS {
            let (tokens, diagnostics) = crate::lexer::lex_all(keyword);
            assert!(diagnostics.is_empty());
            assert_eq!(tokens.len(), 2, "{keyword}");
            assert!(!matches!(tokens[0].kind, TokenKind::Ident(_)), "{keyword}");
        }
        let source = include_str!("lexer.rs");
        let body = &source[source.find("fn identifier").unwrap()..];
        let body = &body[..body.find("name => TokenKind::Ident").unwrap()];
        assert_eq!(body.matches("=> TokenKind::").count(), KEYWORDS.len());
    }

    #[test]
    fn stale_map_keeps_prefix_and_shifts_suffix() {
        let map = StaleMap::new("ab\u{65e5}cd", "ab\u{65e5}XYZcd");
        for offset in 0..=5 {
            assert_eq!(map.forward(offset), Some(offset));
        }
        assert_eq!(map.forward(6), Some(9));
        assert_eq!(map.forward(7), Some(10));
        assert_eq!(map.backward(9), Some(6));
        assert_eq!(map.backward(7), None);
        let map = StaleMap::new("ab\u{65e5}cd", "abXcd");
        assert_eq!(map.forward(2), Some(2));
        assert_eq!(map.forward(3), None);
        assert_eq!(map.forward(4), None);
        assert_eq!(map.forward(5), Some(3));
        assert_eq!(map.span(Span::new(5, 7)), Some(Span::new(3, 5)));
        assert_eq!(map.span(Span::new(1, 4)), None);
        let same = StaleMap::new("same", "same");
        assert_eq!(same.forward(4), Some(4));
        assert_eq!(same.backward(2), Some(2));
    }
}
