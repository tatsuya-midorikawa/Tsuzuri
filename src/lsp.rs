use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::check::semantic::{Reference, Role, SemanticIndex, SymbolKind};
use crate::check::{ModuleOrigin, semantic};
use crate::copies::{CopyKind, CopySite};
use crate::diagnostic::{Applicability, Diagnostic, Edit, Fix, Severity, Span, apply_edits};
use crate::driver::Project;
use crate::syntax::{CONTEXTUAL_KEYWORDS, Token, TokenKind};
use serde_json::{Value, json};

const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const MAX_HEADER: usize = 8192;
const MAX_LOCATIONS: usize = 10_000;
const MAX_COMPLETIONS: usize = 500;
const MAX_SYMBOLS: usize = 256;

const KEYWORDS: [&str; 42] = [
    "fn", "def", "rec", "and", "export", "extern", "private", "record", "union", "type", "const",
    "test", "bench", "class", "instance", "deriving", "dyn", "let", "do", "return", "yield", "for",
    "in", "to", "downto", "while", "break", "continue", "mut", "ref", "deref", "new", "as", "if",
    "then", "elif", "else", "match", "with", "when", "true", "false",
];
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
    Ok(read_frame(input)?.map(|(message, _)| message))
}

/// `read_message` with the length of the message's body.
fn read_frame(input: &mut impl BufRead) -> io::Result<Option<(Result<Value, String>, usize)>> {
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
    Ok(Some((
        serde_json::from_slice(&body).map_err(|error| error.to_string()),
        length,
    )))
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
    escape_path(&mut uri, path);
    Some(uri)
}

/// Appends `path` to a URI, escaping every byte but ASCII letters, digits and `/-._~:`.
fn escape_path(uri: &mut String, path: &str) {
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(uri, "%{byte:02X}");
        }
    }
}

/// The URI `root` followed by the components of `relative`, each escaped as `file_uri` escapes it
/// (G39 D8): the client's spelling of a file under one of its roots.
fn uri_under(root: &str, relative: &Path) -> Option<String> {
    let mut uri = root.to_owned();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return None;
        };
        if !uri.ends_with('/') {
            uri.push('/');
        }
        escape_path(&mut uri, name.to_str()?);
    }
    Some(uri)
}

/// `path` with its nearest existing ancestor directory canonicalized, as `document_path` places a
/// document (G39 spec 9): a removed file or directory stays under the directory that remains, and a
/// path that is itself a symbolic link stays where the client named it.
fn canonical_existing(path: &Path) -> Option<PathBuf> {
    let mut removed = vec![path.file_name()?];
    let mut current = path.parent()?;
    loop {
        if let Ok(canonical) = current.canonicalize() {
            return Some(
                removed
                    .iter()
                    .rev()
                    .fold(canonical, |path, name| path.join(name)),
            );
        }
        removed.push(current.file_name()?);
        current = current.parent()?;
    }
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
    /// The counts of all reported diagnostics, which a fix is verified against.
    counts: DiagnosticCounts,
    /// The implicit array and list copies that inlay hints show (A15).
    copies: Vec<CopySite>,
    /// The inferred effects of the functions with effect attributes, by the position of the
    /// declared name (A20).
    effects: BTreeMap<(usize, usize), String>,
    modules: ModuleNames,
}

struct Session {
    initialized: bool,
    shutdown: bool,
    encoding: PositionEncoding,
    /// The roots of `initialize` (`rootUri`, `workspaceFolders`), each with the URI that the client
    /// named it with (G39 D8).
    client_roots: Vec<ClientRoot>,
    buffers: BTreeMap<PathBuf, OpenBuffer>,
    projects: BTreeMap<PathBuf, ProjectState>,
    /// The last analyzed state of each root, kept while edits break analysis.
    good: BTreeMap<PathBuf, ProjectState>,
    /// The roots to analyze again, each by when.
    pending: BTreeMap<PathBuf, Instant>,
    closed: BTreeMap<PathBuf, String>,
    /// For each analyzed root, the files that its last refresh sent a non-empty list to, with the URI
    /// it sent to (G39 D5).
    published: BTreeMap<PathBuf, BTreeMap<PathBuf, String>>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            initialized: false,
            shutdown: false,
            encoding: PositionEncoding::Utf16,
            client_roots: Vec::new(),
            buffers: BTreeMap::new(),
            projects: BTreeMap::new(),
            good: BTreeMap::new(),
            pending: BTreeMap::new(),
            closed: BTreeMap::new(),
            published: BTreeMap::new(),
        }
    }
}

/// A root that the client named in `initialize`.
struct ClientRoot {
    /// The path of its URI.
    path: PathBuf,
    /// The path canonicalized when the client named it and again at each refresh (a symbolic link
    /// can be removed or pointed elsewhere), so that a refresh does not resolve it for every file it
    /// publishes; `None` when it did not resolve then.
    canonical: Option<PathBuf>,
    uri: String,
}

impl ClientRoot {
    fn new(uri: &str) -> Result<Self, (i64, String)> {
        let path = uri_path(uri).map_err(|message| invalid(&message))?;
        Ok(Self {
            canonical: path.canonicalize().ok(),
            path,
            uri: uri.to_owned(),
        })
    }
}

/// What one refresh of a root sent: the URI of each file that got a non-empty list, and every URI
/// that got a list (G39 D5).
#[derive(Default)]
struct Sent {
    nonempty: BTreeMap<PathBuf, String>,
    uris: BTreeSet<String>,
}

impl Sent {
    fn note(&mut self, path: &Path, uri: Option<String>, nonempty: bool) {
        let Some(uri) = uri else {
            return;
        };
        if nonempty {
            self.nonempty.insert(path.to_owned(), uri.clone());
        }
        self.uris.insert(uri);
    }
}

impl Session {
    /// The client's roots that exist, canonicalized, with the URI that the client named each with.
    fn canonical_roots(&self) -> impl Iterator<Item = (PathBuf, &str)> {
        self.client_roots.iter().filter_map(|root| {
            let canonical = root
                .canonical
                .clone()
                .or_else(|| root.path.canonicalize().ok())?;
            Some((canonical, root.uri.as_str()))
        })
    }

    /// The innermost client root that contains the canonical `path`, with its URI.
    fn client_root(&self, path: &Path) -> Option<(PathBuf, &str)> {
        self.canonical_roots()
            .filter(|(root, _)| path.starts_with(root))
            .max_by_key(|(root, _)| root.components().count())
    }

    fn project_root(&self, path: &Path) -> PathBuf {
        self.client_root(path)
            .map(|(root, _)| root)
            .unwrap_or_else(|| path.parent().unwrap().to_owned())
    }

    fn schedule(&mut self, path: &Path) {
        let directory = self.project_root(path);
        self.schedule_root(directory);
    }

    fn schedule_root(&mut self, directory: PathBuf) {
        if let Some(state) = self.projects.remove(&directory)
            && state.index.is_some()
        {
            self.good.insert(directory.clone(), state);
        }
        self.pending
            .insert(directory, Instant::now() + Duration::from_millis(200));
    }

    /// The roots that a change of the files at `uris` makes the server analyze again (G39 D7): the
    /// client root of each changed path, placed by its directory that remains, the roots of the open
    /// documents, and the roots whose diagnostics are on the client. A path outside every client root
    /// adds no root.
    ///
    /// The client roots are resolved again first (`resolve_client_roots`): a changed path resolves
    /// through a root link that points elsewhere now, and placed against the directory it pointed
    /// to, it would find no root when nothing else schedules one, as for a clean project with its
    /// documents closed (Copilot review 3 of PR #119).
    fn changed_roots<'u>(
        &mut self,
        uris: impl Iterator<Item = &'u str>,
        output: &mut impl Write,
    ) -> io::Result<BTreeSet<PathBuf>> {
        self.resolve_client_roots(None, output)?;
        let mut roots: BTreeSet<PathBuf> = uris
            .filter_map(|uri| canonical_existing(&uri_path(uri).ok()?))
            .filter_map(|path| Some(self.client_root(&path)?.0))
            .collect();
        roots.extend(self.buffers.keys().map(|path| self.project_root(path)));
        roots.extend(self.published.keys().cloned());
        Ok(roots)
    }

    /// Sends `diagnostics` for `path` and returns the URI it sent to. An analysis's non-empty list
    /// is not sent to a document that the client closed (`None`); a load error is (G39 D6).
    fn publish(
        &self,
        output: &mut impl Write,
        path: &Path,
        diagnostics: Vec<Value>,
        load_error: bool,
    ) -> io::Result<Option<String>> {
        if !load_error && self.closed.contains_key(path) && !diagnostics.is_empty() {
            return Ok(None);
        }
        let Some(uri) = self.uri(path) else {
            return Ok(None);
        };
        self.send_diagnostics(output, &uri, Some(path), diagnostics)?;
        Ok(Some(uri))
    }

    /// One `publishDiagnostics` to `uri`, with the version of `path`'s open buffer when the buffer
    /// has that URI.
    fn send_diagnostics(
        &self,
        output: &mut impl Write,
        uri: &str,
        path: Option<&Path>,
        diagnostics: Vec<Value>,
    ) -> io::Result<()> {
        let mut params = json!({"uri": uri, "diagnostics": diagnostics});
        if let Some(buffer) = path
            .and_then(|path| self.buffers.get(path))
            .filter(|buffer| buffer.uri == uri)
        {
            params["version"] = json!(buffer.version);
        }
        write_message(
            output,
            &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": params}),
        )
    }

    /// Clears what the last refresh of `directory` sent and this one did not replace: an empty list
    /// to each URI of its record that this refresh sent nothing to (a file that left the project or
    /// no longer has diagnostics, a URI that the file is no longer spelled with). This refresh's
    /// record replaces it (G39 D5).
    ///
    /// A URI that the record of another root still has keeps its list: the client replaces a URI's
    /// whole list, so an empty one would hide what the other root still reports there, such as the
    /// load error of a manifest outside both roots that both depend on (Copilot review of PR #119).
    /// It is cleared when the last root that reports it stops.
    fn retire(&mut self, output: &mut impl Write, directory: &Path, sent: Sent) -> io::Result<()> {
        for (path, uri) in self.published.remove(directory).unwrap_or_default() {
            let reported = || {
                self.published
                    .values()
                    .any(|record| record.values().any(|other| *other == uri))
            };
            if !sent.uris.contains(&uri) && !reported() {
                self.send_diagnostics(output, &uri, Some(&path), Vec::new())?;
            }
        }
        if !sent.nonempty.is_empty() {
            self.published.insert(directory.to_owned(), sent.nonempty);
        }
        Ok(())
    }

    /// The diagnostic of a project that does not load, at its span in the file's current text (an
    /// open buffer's or the disk's), or at 0:0 when the text cannot be read or has no such span.
    fn load_error(&self, error: &crate::driver::SourceError) -> Value {
        let span = error.diagnostic.span;
        let range = (span.end > 0)
            .then(|| {
                let text = match self.buffers.get(&error.path) {
                    Some(buffer) => std::borrow::Cow::Borrowed(buffer.text.as_str()),
                    None => crate::driver::read_source_text(&error.path).ok()?.into(),
                };
                PositionMapper::new(&text, self.encoding).range(&text, span)
            })
            .flatten()
            .unwrap_or_else(
                || json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}),
            );
        json!({"range": range, "severity": 1, "code": error.diagnostic.code, "source": "tsuzuri", "message": error.diagnostic.message})
    }

    /// Canonicalizes the client roots again (Copilot review 2 of PR #119): a root that no longer
    /// resolves to the directory it was canonicalized as (removed or moved away, or a symbolic link
    /// removed or pointed elsewhere) takes its new directory, which is scheduled unless it is
    /// `refreshing`. A directory that no client root resolves to any more has no project for this
    /// client: analyzed, its diagnostics would go to the URIs of a root that is gone, such as a
    /// removed link's. Its analysis is dropped and its diagnostics cleared, and it is returned.
    /// Another one that a client root still resolves to is scheduled, to be published at that root's
    /// spelling.
    ///
    /// A root that resolves again after it did not (a removed link made again) is scheduled as well
    /// (Copilot review 5 of PR #119): with its documents closed, nothing else schedules it, since its
    /// directory left the record when the root went away, and the change notification of the link
    /// itself is placed beside the directory it points to (`canonical_existing` does not follow it).
    fn resolve_client_roots(
        &mut self,
        refreshing: Option<&Path>,
        output: &mut impl Write,
    ) -> io::Result<Vec<PathBuf>> {
        let mut moved = Vec::new();
        for root in &mut self.client_roots {
            let now = root.path.canonicalize().ok();
            let before = std::mem::replace(&mut root.canonical, now.clone());
            if before != now {
                moved.push((before, now));
            }
        }
        let mut left = Vec::new();
        for (before, now) in moved {
            if let Some(now) = now.filter(|now| Some(now.as_path()) != refreshing) {
                self.schedule_root(now);
            }
            let Some(before) = before else {
                continue;
            };
            if !self.canonical_roots().any(|(root, _)| root == before) {
                left.push(before);
            } else if Some(before.as_path()) != refreshing {
                self.schedule_root(before);
            }
        }
        for gone in &left {
            self.projects.remove(gone);
            self.good.remove(gone);
            self.pending.remove(gone);
            self.retire(output, gone, Sent::default())?;
        }
        Ok(left)
    }

    /// Refreshes each root of `due` that is still pending when its turn comes. The refresh of one can
    /// drop another (`resolve_client_roots`: a directory that no client root resolves to any more),
    /// which a refresh from this list would load again, with its symbols and its diagnostics at the
    /// URIs of no remaining root (Copilot review 3 of PR #119).
    fn refresh_due(&mut self, due: Vec<PathBuf>, output: &mut impl Write) -> io::Result<()> {
        for directory in due {
            if self.pending.contains_key(&directory) {
                self.refresh(&directory, output)?;
            }
        }
        Ok(())
    }

    fn refresh(&mut self, directory: &Path, output: &mut impl Write) -> io::Result<()> {
        self.pending.remove(directory);
        let left = self.resolve_client_roots(Some(directory), output)?;
        if left.iter().any(|gone| gone == directory) {
            return Ok(());
        }
        if !directory.is_dir() {
            // The root was removed or moved away (the parent directory of a document outside every
            // client root, say). `load_for_editor` would load its parent directory, with the
            // projects beside it; the root has no project, and its diagnostics are cleared.
            self.projects.remove(directory);
            self.good.remove(directory);
            return self.retire(output, directory, Sent::default());
        }
        let overlays = self
            .buffers
            .iter()
            .map(|(path, buffer)| (path.clone(), buffer.text.clone()))
            .collect();
        let mut sent = Sent::default();
        let project = match Project::load_for_editor(directory, &overlays) {
            Ok(project) => project,
            Err(error) => {
                self.projects.remove(directory);
                // The reason is sent wherever it is, and the other files lose the diagnostics of the
                // last analysis (G39 D6).
                let uri = self.publish(output, &error.path, vec![self.load_error(&error)], true)?;
                sent.note(&error.path, uri, true);
                return self.retire(output, directory, sent);
            }
        };
        let (analyzed, diagnostics, counts) = analyze(&project);
        let mappers: Vec<_> = project
            .sources
            .iter()
            .map(|source| PositionMapper::new(&source.text, self.encoding))
            .collect();
        // A source outside the root (a path dependency beside it) is left to the client of its own
        // root, which sees its changes (G39 D9).
        for (id, source) in project.sources.iter().enumerate().filter(|(_, source)| {
            source.origin == ModuleOrigin::User && source.path.starts_with(directory)
        }) {
            let messages: Vec<Value> = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.span.source.unwrap_or(project.root) == id)
                .filter_map(|diagnostic| self.diagnostic_value(&project, &mappers, id, diagnostic))
                .collect();
            let nonempty = !messages.is_empty();
            let uri = self.publish(output, &source.path, messages, false)?;
            sent.note(&source.path, uri, nonempty);
        }
        self.retire(output, directory, sent)?;
        // PB08 O9: the inlay hints and the effects of hovers after the diagnostics are sent, the
        // empty lists of `retire` included.
        let (index, copies, effects) =
            analyzed.map_or((None, Vec::new(), BTreeMap::new()), |(index, module)| {
                (
                    Some(index),
                    module.costly_sites(),
                    inferred_effects(module.module()),
                )
            });
        if index.is_some() {
            self.good.remove(directory);
        }
        let modules = ModuleNames::new(&project);
        self.projects.insert(
            directory.to_owned(),
            ProjectState {
                project,
                index,
                mappers,
                diagnostics,
                counts,
                copies,
                effects,
                modules,
            },
        );
        Ok(())
    }

    /// Answers one message. `cancelled` says whether the client has cancelled the request; a long
    /// computation asks it between its steps (G39 段 12).
    fn handle(
        &mut self,
        method: &str,
        params: &Value,
        output: &mut impl Write,
        cancelled: &dyn Fn() -> bool,
    ) -> RpcResult {
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
                self.client_roots.push(ClientRoot::new(uri)?);
            }
            if let Some(folders) = params["workspaceFolders"].as_array() {
                for folder in folders {
                    let uri = folder["uri"]
                        .as_str()
                        .ok_or_else(|| invalid("workspace folder needs a URI"))?;
                    self.client_roots.push(ClientRoot::new(uri)?);
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
                    "completionProvider": {"triggerCharacters": [".", ":"], "resolveProvider": false},
                    "signatureHelpProvider": {"triggerCharacters": [" ", "("], "retriggerCharacters": [","]},
                    "semanticTokensProvider": {"legend": legend, "full": true, "range": false},
                    "codeActionProvider": {"codeActionKinds": ["quickfix", FIX_ALL]},
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
                let changes = params["changes"].as_array().map_or(&[][..], Vec::as_slice);
                let roots = self
                    .changed_roots(
                        changes.iter().filter_map(|change| change["uri"].as_str()),
                        output,
                    )
                    .map_err(|error| (-32603, error.to_string()))?;
                for root in roots {
                    self.schedule_root(root);
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
                    return Err(invalid(&format!(
                        "source exceeds the {}-byte limit",
                        crate::syntax::MAX_SOURCE_BYTES
                    )));
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
                self.publish(output, &path, Vec::new(), false)
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
                        Some(json!({"name": symbol.name, "detail": symbol.detail, "kind": symbol.kind, "range": state.mappers[source_id].range(&source.text, symbol.span)?, "selectionRange": state.mappers[source_id].range(&source.text, symbol.selection)?}))
                    }).collect();
                    return Ok(json!(symbols));
                }
                let offset = position_offset(&state.mappers[source_id], &source.text, params)?;
                let hovered = if method.ends_with("hover") {
                    hovered_diagnostics(state, source_id, offset)
                } else {
                    Vec::new()
                };
                let Some(entry) = state
                    .index
                    .as_ref()
                    .and_then(|index| index.at(source_id, offset))
                else {
                    // A program with errors has no index; the diagnostics under the cursor remain.
                    let Some(first) = hovered.first() else {
                        return Ok(Value::Null);
                    };
                    return Ok(json!({
                        "contents": {"kind": "markdown", "value": diagnostic_hover(&hovered)},
                        "range": state.mappers[source_id].range(&source.text, first.span),
                    }));
                };
                if method.ends_with("hover") {
                    let mut markdown = String::new();
                    if !hovered.is_empty() {
                        markdown = diagnostic_hover(&hovered);
                        markdown.push_str("\n\n---\n\n");
                    }
                    markdown.push_str(&format!("```tsuzuri\n{}\n```", entry.detail));
                    let definition = entry.target.unwrap_or(entry.span);
                    if let Some(effects) = definition
                        .source
                        .and_then(|source| state.effects.get(&(source, definition.start)))
                        .filter(|_| hovered_function(&entry.detail).is_some())
                    {
                        markdown.push_str("\n\ninferred effects: ");
                        markdown.push_str(effects);
                    }
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
                let due: Vec<_> = self.pending.keys().cloned().collect();
                self.refresh_due(due, output)
                    .map_err(|error| (-32603, error.to_string()))?;
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
                self.code_actions(&directory, source_id, params, cancelled)
            }
            "textDocument/formatting" => {
                let path = document_path(params)?;
                let directory = self.project_root(&path);
                let Some(text) = self.text(&directory, &path) else {
                    return Ok(Value::Null);
                };
                // The project's own reading of the file decides the edition; a file whose
                // project did not load is formatted as a source that no package owns.
                let edition = self
                    .projects
                    .get(&directory)
                    .or_else(|| self.good.get(&directory))
                    .and_then(|state| {
                        state
                            .project
                            .sources
                            .iter()
                            .find(|source| source.path == path)
                    })
                    .map(|source| source.edition);
                Ok(format_document(&path, text, self.encoding, edition))
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
            self.refresh(&directory, output)
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

    /// The URI of `path` for the client: its open buffer's, the one it was closed with, or the path
    /// under the client's spelling of the root that holds it; `file_uri` outside every root (G39 D8).
    fn uri(&self, path: &Path) -> Option<String> {
        self.buffers
            .get(path)
            .map(|buffer| buffer.uri.clone())
            .or_else(|| self.closed.get(path).cloned())
            .or_else(|| {
                let (root, uri) = self.client_root(path)?;
                uri_under(uri, path.strip_prefix(&root).ok()?)
            })
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
        let project = Project::load_for_editor(directory, &overlays)
            .map_err(|error| format!("{}: {}", error.diagnostic.code, error.diagnostic.message))?;
        let (renamed, diagnostics, _) = analyze(&project);
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

    /// The code actions of the requested range. `cancelled` is asked before each verification of a
    /// fix and each round of fix all; a cancelled request stops there with -32800 (G39 段 12).
    fn code_actions(
        &self,
        directory: &Path,
        source_id: usize,
        params: &Value,
        cancelled: &dyn Fn() -> bool,
    ) -> RpcResult {
        let only = params["context"]["only"].as_array();
        let asks = |kinds: &[&str]| {
            only.is_none_or(|only| {
                only.iter()
                    .any(|kind| kinds.iter().any(|asked| kind == asked))
            })
        };
        // VS Code drops the source actions of an automatic request (the light bulb, `triggerKind`
        // 2) that has no `only`, so fix all is not computed for one (G39 D11).
        let automatic = params["context"]["triggerKind"] == 2 && only.is_none();
        let state = &self.projects[directory];
        let mut actions = Vec::new();
        if asks(&["quickfix"]) {
            actions = self.quick_fixes(directory, state, source_id, params, cancelled)?;
        }
        if !automatic
            && asks(&["source", "source.fixAll", FIX_ALL])
            && let Some(action) = self.fix_all(directory, state, source_id, cancelled)?
        {
            actions.push(action);
        }
        Ok(json!(actions))
    }

    /// `source.fixAll.tsuzuri`: every `certain` fix of the document, applied by `tsuzuri fix`'s
    /// rounds to the project with the open buffers. `None` when the project has no `certain` fix,
    /// the run fails, or nothing in the document changes; -32800 when `cancelled` says so before a
    /// round. The project loads the std modules that `tsuzuri fix` and the analysis load
    /// (`load_for_editor`, PB08 O6), so the rounds see the diagnostics whose fixes the quick fixes
    /// offer.
    fn fix_all(
        &self,
        directory: &Path,
        state: &ProjectState,
        source_id: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Value>, (i64, String)> {
        // Only a `certain` fix that edits this document alone can change it: without one, skip
        // loading and analyzing the project again.
        let root = state.project.root;
        if !state.diagnostics.iter().any(|diagnostic| {
            diagnostic.fixes.iter().any(|fix| {
                fix.applicability == Applicability::Certain
                    && fix
                        .edits
                        .iter()
                        .all(|edit| edit.span.source.unwrap_or(root) == source_id)
            })
        }) {
            return Ok(None);
        }
        if cancelled() {
            return Err(request_cancelled());
        }
        let overlays: BTreeMap<PathBuf, String> = self
            .buffers
            .iter()
            .map(|(path, buffer)| (path.clone(), buffer.text.clone()))
            .collect();
        let Ok(mut project) = Project::load_for_editor(directory, &overlays) else {
            return Ok(None);
        };
        let path = &state.project.sources[source_id].path;
        let Some(index) = project
            .sources
            .iter()
            .position(|source| source.path == *path)
        else {
            return Ok(None);
        };
        let options = crate::fix::Options {
            threshold: crate::fix::Threshold::Certain,
            scope: crate::fix::Scope::File(index),
            only_codes: None,
        };
        let Ok(report) = crate::fix::run(
            &mut project,
            &options,
            &mut |project| crate::fix::analyze_project(project, false),
            cancelled,
        ) else {
            return Ok(None);
        };
        if report.stop == crate::fix::Stop::Cancelled {
            return Err(request_cancelled());
        }
        let Some(change) = report.files.iter().find(|change| change.source == index) else {
            return Ok(None);
        };
        let mapper = PositionMapper::new(&change.original, self.encoding);
        let edits: Vec<Value> = match crate::diff::line_edits(&change.original, &change.fixed) {
            Some(edits) => edits
                .into_iter()
                .map(|edit| {
                    json!({"range": mapper.range(&change.original, Span::new(edit.start, edit.end)), "newText": edit.text})
                })
                .collect(),
            None => vec![json!({
                "range": mapper.range(&change.original, Span::new(0, change.original.len())),
                "newText": change.fixed,
            })],
        };
        let Some(uri) = self.uri(path) else {
            return Ok(None);
        };
        Ok(Some(json!({
            "title": "Fix all auto-fixable problems",
            "kind": FIX_ALL,
            "edit": {"changes": {uri: edits}},
        })))
    }

    /// The quick fixes of the diagnostics in the requested range.
    fn quick_fixes(
        &self,
        directory: &Path,
        state: &ProjectState,
        source_id: usize,
        params: &Value,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Value>, (i64, String)> {
        let source = &state.project.sources[source_id];
        let mapper = &state.mappers[source_id];
        let bound = |name: &str| {
            mapper
                .offset(&source.text, &params["range"][name])
                .ok_or_else(|| invalid(POSITION_ERROR))
        };
        let (start, end) = (bound("start")?, bound("end")?);
        let mut actions = Vec::new();
        // The fixes of diagnostics, each offered only when applying it removes its diagnostic.
        for diagnostic in &state.diagnostics {
            let span = diagnostic.span;
            if diagnostic.fixes.is_empty()
                || span.source != Some(source_id)
                || span.start > end
                || start > span.end
            {
                continue;
            }
            for fix in &diagnostic.fixes {
                if cancelled() {
                    return Err(request_cancelled());
                }
                let Ok(edit) = self.fix_edit(directory, state, diagnostic, fix) else {
                    continue;
                };
                actions.push(json!({
                    "title": fix.title,
                    "kind": "quickfix",
                    "diagnostics": [self.diagnostic_value(&state.project, &state.mappers, source_id, diagnostic)],
                    "isPreferred": fix.applicability == Applicability::Certain,
                    "edit": edit,
                }));
            }
        }
        Ok(actions)
    }

    /// `diagnostic_json` plus `relatedInformation` (the related locations in the project's own
    /// files) and `data` (the kind and the fixes), each only when there is something to say.
    fn diagnostic_value(
        &self,
        project: &Project,
        mappers: &[PositionMapper],
        source_id: usize,
        diagnostic: &Diagnostic,
    ) -> Option<Value> {
        let source = &project.sources[source_id];
        let mut value = diagnostic_json(&mappers[source_id], &source.text, diagnostic)?;
        let related: Vec<Value> = diagnostic
            .related
            .iter()
            .filter_map(|related| {
                let id = related.span.source.unwrap_or(project.root);
                let file = project
                    .sources
                    .get(id)
                    .filter(|file| file.origin == ModuleOrigin::User)?;
                Some(json!({
                    "location": {
                        "uri": self.uri(&file.path)?,
                        "range": mappers[id].range(&file.text, related.span)?,
                    },
                    "message": related.message,
                }))
            })
            .collect();
        if !related.is_empty() {
            value["relatedInformation"] = json!(related);
        }
        if let Some(data) = diagnostic_data(diagnostic) {
            value["data"] = data;
        }
        Some(value)
    }

    /// The workspace edit of `fix`, when applying it to the project's files and analyzing again
    /// removes `diagnostic` (see `fix_removes`).
    fn fix_edit(
        &self,
        directory: &Path,
        state: &ProjectState,
        diagnostic: &Diagnostic,
        fix: &Fix,
    ) -> Result<Value, String> {
        let mut edited: BTreeMap<usize, Vec<&Edit>> = BTreeMap::new();
        for edit in &fix.edits {
            let id = edit.span.source.unwrap_or(state.project.root);
            if state
                .project
                .sources
                .get(id)
                .is_none_or(|file| file.origin != ModuleOrigin::User)
            {
                return Err("an edit is outside the project's own files".into());
            }
            edited.entry(id).or_default().push(edit);
        }
        let mut overlays: BTreeMap<PathBuf, String> = self
            .buffers
            .iter()
            .map(|(path, buffer)| (path.clone(), buffer.text.clone()))
            .collect();
        for (id, edits) in &edited {
            let file = &state.project.sources[*id];
            let text =
                apply_edits(&file.text, edits).ok_or("the edits overlap or are out of range")?;
            overlays.insert(file.path.clone(), text);
        }
        let project = Project::load_for_editor(directory, &overlays)
            .map_err(|error| format!("{}: {}", error.diagnostic.code, error.diagnostic.message))?;
        let (_, _, after) = analyze(&project);
        let (Some(before), Some(after)) = (&state.counts, &after) else {
            return Err("too many diagnostics to verify the fix".into());
        };
        fix_removes(diagnostic, before, after)?;
        let mut changes = serde_json::Map::new();
        for (id, edits) in &edited {
            let file = &state.project.sources[*id];
            let uri = self
                .uri(&file.path)
                .ok_or("cannot address an edited file")?;
            let list: Option<Vec<Value>> = edits
                .iter()
                .map(|edit| {
                    state.mappers[*id]
                        .range(&file.text, edit.span)
                        .map(|range| json!({"range": range, "newText": edit.text}))
                })
                .collect();
            changes.insert(uri, json!(list.ok_or("cannot address an edit")?));
        }
        Ok(json!({"changes": changes}))
    }
}

/// The code action kind of applying every `certain` fix of a document (G24).
const FIX_ALL: &str = "source.fixAll.tsuzuri";
const POSITION_ERROR: &str = "position is outside the document or splits a Unicode character";
const NO_RENAMABLE_NAME: &str = "no renamable name at this position; place the cursor on a name";

fn failed(message: impl Into<String>) -> (i64, String) {
    (-32803, message.into())
}

/// The answer to a request that the client cancelled.
fn request_cancelled() -> (i64, String) {
    (-32800, "request cancelled".into())
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

/// The `data` of a published diagnostic: its kind and the titles of its fixes.
fn diagnostic_data(diagnostic: &Diagnostic) -> Option<Value> {
    let mut data = serde_json::Map::new();
    if let Some(kind) = diagnostic.kind {
        data.insert("kind".into(), json!(kind));
    }
    if !diagnostic.fixes.is_empty() {
        let fixes: Vec<Value> = diagnostic
            .fixes
            .iter()
            .map(|fix| json!({"title": fix.title, "applicability": fix.applicability.as_str()}))
            .collect();
        data.insert("fixes".into(), json!(fixes));
    }
    (!data.is_empty()).then_some(Value::Object(data))
}

/// Whether the diagnostics `after` a fix of `diagnostic` have one fewer with its code than
/// `before` and none more of any code. Errors always count; warnings count only when
/// `diagnostic` is a warning, because the passes that follow a fixed error may report warnings
/// that were not reached before.
fn fix_removes(
    diagnostic: &Diagnostic,
    before: &BTreeMap<(Severity, &'static str), usize>,
    after: &BTreeMap<(Severity, &'static str), usize>,
) -> Result<(), String> {
    let counts = |all: &BTreeMap<(Severity, &'static str), usize>| {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for ((severity, code), count) in all {
            if *severity == Severity::Error || diagnostic.severity == Severity::Warning {
                *counts.entry(*code).or_default() += count;
            }
        }
        counts
    };
    let (before, after) = (counts(before), counts(after));
    let count =
        |counts: &BTreeMap<&'static str, usize>, code| counts.get(code).copied().unwrap_or(0);
    if count(&after, diagnostic.code) >= count(&before, diagnostic.code) {
        return Err("the diagnostic would remain".into());
    }
    if let Some((code, _)) = after.iter().find(|(code, n)| **n > count(&before, code)) {
        return Err(format!("{code} would appear"));
    }
    Ok(())
}

fn is_user(project: &Project, span: Span) -> bool {
    span.source
        .and_then(|source| project.sources.get(source))
        .is_some_and(|source| source.origin == ModuleOrigin::User)
}

/// How many diagnostics of each severity and code an analysis reported, those past the display
/// limit included; `None` when it reached the collection limit, past which nothing is counted.
type DiagnosticCounts = Option<BTreeMap<(Severity, &'static str), usize>>;

/// The most diagnostics that a hover shows.
const MAX_HOVERED_DIAGNOSTICS: usize = 3;

/// The diagnostics with a kind whose span contains `offset` of the source (an empty span: starts
/// at it), in the order they were reported.
/// The inferred effects of each function with effect attributes, the union of its instances, by
/// the position of its declared name (A20). The summaries are complete: `check` computes only the
/// effects that the attributes forbid, which would read as "none" for the others. A summary
/// leaves out the effects of the functions that the function is given and calls, which its
/// callers check (review 3 of #63), and says so.
fn inferred_effects(module: &crate::check::CheckedModule) -> BTreeMap<(usize, usize), String> {
    use crate::check::effects::{EffectSet, Options, SUPPORTED, Scope, summarize};
    if module
        .functions
        .iter()
        .all(|function| function.origin.asserts.is_empty())
    {
        return BTreeMap::new();
    }
    let Ok(summaries) = summarize(module, Options::new(Scope::Reachable, SUPPORTED)) else {
        return BTreeMap::new();
    };
    let mut effects: BTreeMap<(usize, usize), (EffectSet, bool, bool)> = BTreeMap::new();
    for (id, function) in module.functions.iter().enumerate() {
        let Some(source) = function.span.source else {
            continue;
        };
        if function.origin.asserts.is_empty() {
            continue;
        }
        let summary = summaries.summary(id);
        let entry = effects.entry((source, function.span.start)).or_default();
        entry.0 = entry.0.union(summary.effects);
        entry.1 |= summary.unknown.is_some();
        entry.2 |= summary.calls_params != 0;
    }
    effects
        .into_iter()
        .map(|(position, (set, unknown, callbacks))| {
            let names = set.names();
            let mut text = if names.is_empty() {
                "none".to_owned()
            } else {
                names.join(", ")
            };
            if callbacks {
                text.push_str(" (and the effects of the functions it is given)");
            }
            if unknown {
                text.push_str(" (and calls that the analysis cannot follow)");
            }
            (position, text)
        })
        .collect()
}

/// The declared name in a function's hover detail, `[@attribute ]... [const |pure ]def Name.f :: ...`.
fn hovered_function(detail: &str) -> Option<&str> {
    let start = detail.find("def ")? + 4;
    let rest = &detail[start..];
    Some(&rest[..rest.find(" ::")?])
}

fn hovered_diagnostics(state: &ProjectState, source_id: usize, offset: usize) -> Vec<&Diagnostic> {
    state
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            let span = diagnostic.span;
            diagnostic.kind.is_some()
                && span.source.unwrap_or(state.project.root) == source_id
                && (span.start..span.end.max(span.start + 1)).contains(&offset)
        })
        .take(MAX_HOVERED_DIAGNOSTICS)
        .collect()
}

/// `**error[E1012]** `use_after_move``, the message, and how to read the explanation, per diagnostic.
fn diagnostic_hover(diagnostics: &[&Diagnostic]) -> String {
    let blocks: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| {
            let kind = diagnostic.kind.unwrap_or_default();
            format!(
                "**{}[{}]** `{kind}`  \n{}  \nRun `tsuzuri explain {kind}` for an explanation.",
                diagnostic.severity.as_str(),
                diagnostic.code,
                diagnostic.message
            )
        })
        .collect();
    blocks.join("\n\n")
}

fn diagnostic_counts(diagnostics: &[Diagnostic]) -> BTreeMap<(Severity, &'static str), usize> {
    let mut counts = BTreeMap::new();
    for diagnostic in diagnostics {
        *counts
            .entry((diagnostic.severity, diagnostic.code))
            .or_insert(0) += 1;
    }
    counts
}

/// Analyzes a project; a successful index also records `def` and `fn` heads. Like the commands of
/// the command line, it uses the available cores, at most 8, unless `TSUZURI_THREADS` says
/// otherwise (PB08 O5); the result does not depend on the number.
fn analyze(
    project: &Project,
) -> (
    Option<(SemanticIndex, crate::copies::EditorModule)>,
    Vec<Diagnostic>,
    DiagnosticCounts,
) {
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
            edition: source.edition,
            package: project.dependency_package(source),
        })
        .collect();
    let mut semantic = SemanticIndex::default();
    match crate::analyze_inputs_for_editor(
        &inputs,
        &mut semantic,
        crate::parallel::Parallelism::command_line(),
    ) {
        Ok(mut module) => {
            add_function_heads(&mut semantic, project);
            let warnings = module.take_warnings();
            let counts = Some(diagnostic_counts(&warnings));
            (Some((semantic, module)), warnings, counts)
        }
        Err(errors) => (
            None,
            errors.diagnostics,
            (!errors.omitted_is_lower_bound).then_some(errors.counts),
        ),
    }
}

/// Names after top-level `def`, `fn`, `and` (skipping `rec`), and a column-0 `let`.
fn function_heads(source: &str, edition: crate::syntax::Edition) -> Vec<(String, Span)> {
    let (tokens, _) = crate::lexer::lex_all(source, edition);
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
        let file = &project.sources[*source];
        for (name, span) in function_heads(&file.text, file.edition) {
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

impl<'s> View<'s> {
    fn module(&self) -> Option<String> {
        self.state.modules.key(self.source).map(str::to_owned)
    }

    /// The edition that the project reads the current document with.
    fn edition(&self) -> crate::syntax::Edition {
        self.state.project.sources[self.source].edition
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

    /// The index that holds the declarations of the module `key`: the view's, or for a std module
    /// that the project does not load, the index of every std module (PB08 O6).
    fn members(&self, key: &str) -> &'s SemanticIndex {
        if self.state.modules.is_unloaded(key) {
            std_index()
        } else {
            self.index
        }
    }
}

/// Module keys, which qualify declarations in the semantic index, by source
/// and by full name (`Namespace.File`), as the checker resolves them.
#[derive(Default)]
struct ModuleNames {
    /// Each source's key and namespace, then those of the std modules that the project does not
    /// load (`unloaded_std`).
    sources: Vec<Option<(String, String)>>,
    full: BTreeMap<String, String>,
    /// The namespaces that each source's `using` declarations import.
    usings: Vec<Vec<String>>,
    /// The keys of the std modules that the project does not load (PB08 O6).
    unloaded: BTreeSet<&'static str>,
}

impl ModuleNames {
    fn new(project: &Project) -> Self {
        let headers: Vec<_> = project
            .sources
            .iter()
            .map(|source| match source.origin {
                ModuleOrigin::User => crate::declared_header(&source.text, source.edition),
                ModuleOrigin::Std => (None, Vec::new()),
            })
            .collect();
        let unloaded = unloaded_std(project);
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
                    .map(|name| (name.to_owned(), crate::stdlib::NAMESPACE.to_owned())),
            })
            .chain(
                unloaded
                    .iter()
                    .map(|name| Some(((*name).to_owned(), crate::stdlib::NAMESPACE.to_owned()))),
            )
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
            unloaded,
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

    /// The namespace that `path` (`A::B`) names from `source`, innermost scope first.
    fn namespace_path(&self, source: usize, path: &str) -> Option<String> {
        if path.contains('.') {
            return None;
        }
        let path = path.replace("::", ".");
        self.scopes(source).into_iter().find_map(|scope| {
            let namespace = qualify(scope, &path);
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

    /// The key of the module that `path` (`Name` or `A::B::Name`) names from
    /// `source`: each scope qualifies `path`, innermost first, with the
    /// `using` namespaces after the innermost for a bare module name, which
    /// may also be a key.
    fn resolve(&self, source: usize, path: &str) -> Option<&str> {
        if path.contains('.') {
            return None;
        }
        let bare = !path.contains("::");
        let path = path.replace("::", ".");
        let usings = self
            .usings
            .get(source)
            .filter(|_| bare)
            .map_or(&[][..], Vec::as_slice);
        for (at, scope) in self.scopes(source).into_iter().enumerate() {
            if let Some(key) = self.full.get(&qualify(scope, &path)) {
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
        self.keys().find(|key| bare && *key == path)
    }

    /// The next segments below the namespace `path` (`A::B`) in `source` with
    /// their dotted full names, as in `Sample` -> `Shape` for
    /// `Sample::Shape`, and for an empty `path` also the modules that `using`
    /// imports. A segment that names a module shows the module that `resolve`
    /// picks; any other shows the innermost namespace that holds it, so an
    /// ambiguous import alone shows nothing.
    fn children(&self, source: usize, path: &str) -> BTreeMap<String, String> {
        if path.contains('.') {
            return BTreeMap::new();
        }
        let dotted = path.replace("::", ".");
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
            modules.extend(
                self.keys()
                    .filter(|key| !key.contains('.'))
                    .map(str::to_owned),
            );
        }
        let mut namespaces = BTreeMap::new();
        for scope in self.scopes(source) {
            let prefix = if path.is_empty() {
                qualify(scope, "")
            } else {
                format!("{}.", qualify(scope, &dotted))
            };
            for full in self.full.keys() {
                let Some(rest) = full.strip_prefix(&prefix) else {
                    continue;
                };
                if let Some((child, _)) = rest.split_once('.') {
                    namespaces
                        .entry(child.to_owned())
                        .or_insert_with(|| format!("{prefix}{child}"));
                } else {
                    modules.insert(rest.to_owned());
                }
            }
        }
        let mut children: BTreeMap<_, _> = modules
            .into_iter()
            .filter_map(|child| {
                let reference = if path.is_empty() {
                    child.clone()
                } else {
                    format!("{path}::{child}")
                };
                let key = self.resolve(source, &reference)?;
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

    /// Whether `key` names a std module that the project does not load (PB08 O6).
    fn is_unloaded(&self, key: &str) -> bool {
        self.unloaded.contains(key)
    }
}

/// The std modules of `stdlib::SOURCES` that `project` does not load, in their order: the opt-in
/// modules that its sources do not name (PB08 O6). Module names and paths still offer them, as
/// when the language server loaded every std module; their declarations come from `std_index`.
fn unloaded_std(project: &Project) -> BTreeSet<&'static str> {
    let loaded: BTreeSet<&str> = project
        .sources
        .iter()
        .filter(|source| source.origin == ModuleOrigin::Std)
        .filter_map(|source| source.path.to_str().and_then(crate::stdlib::module_name))
        .collect();
    crate::stdlib::SOURCES
        .iter()
        .filter_map(|(path, _)| crate::stdlib::module_name(path))
        .filter(|name| !loaded.contains(name))
        .collect()
}

/// The semantic index of a program that loads every std module, made once per process the first
/// time a request needs the declarations of a std module that its project does not load (PB08 O6):
/// the members that completion offers after `Json.` before a source names `Json`, and the
/// signature of `Json.parse` while the first edit that names it does not parse. The declarations
/// of a std module do not depend on the program, so they are those of the project's index when
/// it loaded every std module.
fn std_index() -> &'static SemanticIndex {
    static INDEX: std::sync::OnceLock<SemanticIndex> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let inputs: Vec<_> = std::iter::once(crate::SourceInput {
            path: "Main.tz",
            text: "0\n",
            origin: ModuleOrigin::User,
            namespace: "",
            edition: crate::syntax::Edition::LATEST,
            package: None,
        })
        .chain(
            crate::stdlib::SOURCES
                .iter()
                .map(|(path, text)| crate::SourceInput {
                    path,
                    text,
                    origin: ModuleOrigin::Std,
                    namespace: "",
                    edition: crate::syntax::Edition::LATEST,
                    package: None,
                }),
        )
        .collect();
        let mut index = SemanticIndex::default();
        // Every std module checks (`stdlib::tests::every_std_module_type_checks_when_named`); were
        // it not so, the requests would find no declarations instead of stopping the server.
        match crate::analyze_inputs_for_editor(
            &inputs,
            &mut index,
            crate::parallel::Parallelism::command_line(),
        ) {
            Ok(_) => index,
            Err(_) => SemanticIndex::default(),
        }
    })
}

/// The completion detail of a module or namespace by its dotted full name,
/// as in `module Demo::Shapes::Circle`.
fn module_detail(modules: &ModuleNames, full: &str) -> String {
    let kind = if modules.is_module(full) {
        "module"
    } else {
        "namespace"
    };
    format!("{kind} {}", full.replace('.', "::"))
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

/// Tokens of `text`, read with `edition`, without the end marker.
fn tokens(text: &str, edition: crate::syntax::Edition) -> Vec<Token> {
    let (mut tokens, _) = crate::lexer::lex_all(text, edition);
    tokens.retain(|token| token.kind != TokenKind::End);
    tokens
}

fn adjacent(left: &Token, right: &Token) -> bool {
    left.span.end == right.span.start
}

/// Whether `tokens[at]` joins two segments of a path: `.`, a namespace `::`,
/// or a `::` whose path is still being written, but not the `::` that
/// annotates a declaration's type, as in `def f::T`.
fn path_separator(tokens: &[Token], at: usize) -> bool {
    match tokens[at].kind {
        TokenKind::Dot | TokenKind::PathSep => true,
        TokenKind::DoubleColon => {
            !(at >= 2
                && matches!(
                    tokens[at - 2].kind,
                    TokenKind::Def | TokenKind::Rec | TokenKind::And
                ))
        }
        _ => false,
    }
}

/// `Ident ((. | ::) Ident)*` written without spaces and ending at `tokens[last]`.
fn path_chain(tokens: &[Token], last: usize) -> Vec<usize> {
    let mut chain = vec![last];
    let mut at = last;
    while at >= 2
        && path_separator(tokens, at - 1)
        && matches!(tokens[at - 2].kind, TokenKind::Ident(_))
        && adjacent(&tokens[at - 2], &tokens[at - 1])
        && adjacent(&tokens[at - 1], &tokens[at])
    {
        at -= 2;
        chain.insert(0, at);
    }
    chain
}

/// The segments of a `path_chain` joined by the separators written between them.
fn path_text(tokens: &[Token], chain: &[usize]) -> String {
    let mut text = String::new();
    for (position, &at) in chain.iter().enumerate() {
        if position > 0 {
            text.push_str(if tokens[at - 1].kind == TokenKind::Dot {
                "."
            } else {
                "::"
            });
        }
        text.push_str(ident(&tokens[at]));
    }
    text
}

fn ident(token: &Token) -> &str {
    match &token.kind {
        TokenKind::Ident(name) => name,
        TokenKind::Yield => "yield",
        TokenKind::Union => "union",
        TokenKind::New => "new",
        _ => "",
    }
}

fn name_token(tokens: &[Token], at: usize) -> bool {
    matches!(tokens[at].kind, TokenKind::Ident(_))
        || (at > 0
            && tokens[at - 1].kind == TokenKind::Dot
            && matches!(
                tokens[at].kind,
                TokenKind::Yield | TokenKind::Union | TokenKind::New
            ))
}

fn completion(view: Option<&View<'_>>, text: &str, offset: usize) -> Value {
    let empty = json!({"isIncomplete": false, "items": []});
    let Some(view) = view else {
        return empty;
    };
    let tokens = tokens(text, view.edition());
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
    let index = view.index;
    let module = view.module().unwrap_or_default();
    let modules = &view.state.modules;
    let mut items = Completions {
        index,
        items: Vec::new(),
    };
    let mut before = tokens.partition_point(|token| token.span.end <= offset);
    if before > 0 && name_token(&tokens, before - 1) && tokens[before - 1].span.end == offset {
        before -= 1;
    }
    let separator = (before >= 2)
        .then(|| &tokens[before - 1])
        .filter(|separator| {
            path_separator(&tokens, before - 1)
                && matches!(tokens[before - 2].kind, TokenKind::Ident(_))
                && adjacent(&tokens[before - 2], separator)
                && tokens
                    .get(before)
                    .is_none_or(|token| token.span.start >= offset || adjacent(separator, token))
        });
    // Namespaces hold modules and namespaces, which follow `::`.
    let namespace = separator
        .filter(|separator| separator.kind != TokenKind::Dot)
        .map(|_| {
            modules.children(
                view.source,
                &path_text(&tokens, &path_chain(&tokens, before - 2)),
            )
        })
        .filter(|children| !children.is_empty());
    if namespace.is_none()
        && before > 0
        && tokens[before - 1].span.end == offset
        && matches!(
            tokens[before - 1].kind,
            TokenKind::Colon | TokenKind::DoubleColon
        )
    {
        // A type annotation, field, or cons that a typed ':' starts.
        return empty;
    }
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
    if let Some(children) = namespace {
        for (child, full) in children {
            items.add(2, &child, 9, &module_detail(modules, &full), None);
        }
    } else if let Some(dot) = separator.filter(|separator| separator.kind == TokenKind::Dot) {
        let chain = path_chain(&tokens, before - 2);
        if let Some(key) = view.module_path(&path_text(&tokens, &chain)) {
            // A type named after its module is the module path itself, not a member.
            let stem = key.rsplit('.').next().unwrap_or(key);
            let members = view.members(key);
            items.index = members;
            for (definition, item) in members.definitions.iter().enumerate() {
                if item.module == key
                    && top_level(item.kind)
                    && (item.public || key == module)
                    && !(matches!(
                        item.kind,
                        SymbolKind::Record | SymbolKind::Union | SymbolKind::Alias
                    ) && item.name == stem)
                {
                    items.definition(2, definition);
                }
            }
        } else {
            let last = &tokens[*chain.last().expect("a chain has a name")];
            let qualifier = path_text(&tokens, &chain[..chain.len() - 1]);
            let owner = if qualifier.is_empty() {
                module.as_str()
            } else {
                view.module_path(&qualifier).unwrap_or(&qualifier)
            };
            // A definition at the name, or else a union of the owner, whose cases follow the dot.
            let union = match view.definition_at(last.span) {
                Some(definition) => Some((index, definition)),
                None => {
                    let members = view.members(owner);
                    members
                        .definitions
                        .iter()
                        .position(|item| {
                            item.kind == SymbolKind::Union
                                && item.module == owner
                                && (item.public || owner == module)
                                && item.name == ident(last)
                        })
                        .map(|definition| (members, definition))
                }
            }
            .filter(|(members, definition)| {
                members.definitions[*definition].kind == SymbolKind::Union
            });
            let (members, container, kind) = if let Some((members, union)) = union {
                (members, Some(union), SymbolKind::Case)
            } else {
                let dot = view.map.backward(dot.span.start);
                let record = index
                    .receivers
                    .iter()
                    .filter(|(span, _)| span.source == Some(view.source) && Some(span.end) == dot)
                    .max_by_key(|(span, _)| span.end - span.start)
                    .map(|(_, record)| *record);
                (index, record, SymbolKind::Field)
            };
            items.index = members;
            for (definition, item) in members.definitions.iter().enumerate() {
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
            items.add(2, &root, 9, &module_detail(modules, &full), None);
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
            | TokenKind::PathSep
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
        if matches!(token.kind, TokenKind::Dot | TokenKind::PathSep)
            && end == Some(token.span.start)
        {
            dotted = true;
            continue;
        }
        if dotted && name_token(tokens, index) && adjacent(&tokens[index - 1], token) {
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

/// The calls around `offset`, innermost first: the token index of each called name and its active argument.
/// A second call is the `name(` call that an earlier line opened around the cursor's line, which applies when
/// the head that the cursor's line writes names no function.
fn call_context(tokens: &[Token], text: &str, offset: usize) -> Vec<(usize, usize)> {
    let line_start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let mut end = tokens.partition_point(|token| token.span.start < offset);
    let mut in_progress = false;
    loop {
        let (mut depth, mut start, mut open) = (0usize, 0, None);
        // After the first bracket at depth 0 that closes an earlier line: where the cursor's line starts.
        let mut boundary = None;
        let mut index = end;
        while index > 0 {
            index -= 1;
            let token = &tokens[index];
            if closes(&token.kind) {
                if depth == 0 && token.span.start < line_start {
                    boundary.get_or_insert(index + 1);
                }
                depth += 1;
            } else if opens(&token.kind) && depth > 0 {
                depth -= 1;
            } else if token.kind == TokenKind::LeftParen {
                open = Some(index);
                start = index + 1;
                break;
            } else if depth == 0
                && (token.span.start < line_start
                    || !(is_term(&token.kind)
                        || name_token(tokens, index)
                        || is_prefix(tokens, index)))
            {
                start = index + 1;
                break;
            }
        }
        let compatible = |open: usize| {
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
            (open - 1, commas)
        };
        let mut call = open.filter(|&open| {
            open > 0 && name_token(tokens, open - 1) && adjacent(&tokens[open - 1], &tokens[open])
        });
        // The brackets that close an earlier line are not arguments of the call of the cursor's line, which
        // starts at `boundary`. A `(` that the walk reached has only such brackets after it, so it contains
        // the line: a tuple or a group adds no call, and a `name(` call is the call when the line writes no
        // head of its own before the cursor, and the fallback when that head names no function.
        let mut outer = None;
        if let Some(boundary) = boundary
            && (call.is_none()
                || (boundary < end && matches!(tokens[boundary].kind, TokenKind::Ident(_))))
        {
            outer = call.take().map(compatible);
            open = None;
            start = boundary;
        }
        if let Some(open) = call {
            return vec![compatible(open)];
        }
        if start < end && matches!(tokens[start].kind, TokenKind::Ident(_)) {
            let mut head = start;
            while head + 2 < end
                && matches!(tokens[head + 1].kind, TokenKind::Dot | TokenKind::PathSep)
                && name_token(tokens, head + 2)
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
            return std::iter::once((head, active)).chain(outer).collect();
        }
        // An empty `(` is an argument still being written to the enclosing call.
        let Some(open) = open.filter(|_| start == end) else {
            return Vec::new();
        };
        in_progress = true;
        end = open;
    }
}

fn signature_help(view: Option<&View<'_>>, text: &str, offset: usize) -> Value {
    let Some(view) = view else {
        return Value::Null;
    };
    let tokens = tokens(text, view.edition());
    call_context(&tokens, text, offset)
        .into_iter()
        .find_map(|(head, active)| signature(view, &tokens, head, active))
        .unwrap_or(Value::Null)
}

/// The signature help of the call whose name is `tokens[head]`, or `None` when the name is no function.
fn signature(view: &View<'_>, tokens: &[Token], head: usize, active: usize) -> Option<Value> {
    let index = view.index;
    let chain = path_chain(tokens, head);
    let module = view.module().unwrap_or_default();
    let owner = if chain.len() > 1 {
        let qualifier = path_text(tokens, &chain[..chain.len() - 1]);
        view.module_path(&qualifier)
            .map_or(qualifier.clone(), str::to_owned)
    } else {
        module.clone()
    };
    let callable = |kind: SymbolKind| matches!(kind, SymbolKind::Function | SymbolKind::Extern);
    // A definition at the name, or else a function of the owner (PB08 O6: of the index of every
    // std module when the project does not load the owner).
    let item = match view.definition_at(tokens[head].span) {
        Some(definition) => Some(&index.definitions[definition]),
        None => view.members(&owner).definitions.iter().find(|item| {
            callable(item.kind)
                && item.module == owner
                && (item.public || owner == module)
                && item.name == ident(&tokens[head])
        }),
    }
    // A member of an Objective-C class (M02) is a method that carries the signature of its send;
    // a method of a type class has none.
    .filter(|item| {
        callable(item.kind) || (item.kind == SymbolKind::Method && !item.result.is_empty())
    })?;
    let parameters = if item.parameters.is_empty() {
        "()".to_owned()
    } else {
        item.parameters
            .iter()
            .map(|parameter| format!("({parameter})"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Some(json!({
        "signatures": [{
            "label": format!("{} {parameters} -> {}", item.name, item.result),
            "parameters": item.parameters.iter().map(|parameter| json!({"label": parameter})).collect::<Vec<_>>(),
        }],
        "activeSignature": 0,
        "activeParameter": active,
    }))
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
    let lexed = tokens(text, view.edition());
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
        for segment in path_chain(&lexed, at).into_iter().rev().skip(1) {
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
            if lexed
                .get(at)
                .is_none_or(|separator| separator.kind != TokenKind::PathSep)
            {
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

fn format_document(
    path: &Path,
    text: &str,
    encoding: PositionEncoding,
    edition: Option<crate::syntax::Edition>,
) -> Value {
    let kind = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::syntax::SourceKind::from_extension)
        .unwrap_or(crate::syntax::SourceKind::Code);
    let path = path.to_string_lossy();
    let formatted = match edition {
        Some(edition) => crate::formatter::format_source_with_edition(&path, text, kind, edition),
        None => crate::formatter::format_source(&path, text, kind),
    };
    match formatted {
        Ok(result) if result.changed => {
            let mapper = PositionMapper::new(text, encoding);
            json!([{"range": mapper.range(text, Span::new(0, text.len())), "newText": result.formatted}])
        }
        Ok(_) => json!([]),
        Err(_) => Value::Null,
    }
}

/// The most that the messages the reader holds before the main loop has handled them may count
/// (their bodies, and `MESSAGE_COST` each): four messages of the largest size (G39). At the limit the
/// reader waits, so the queue and the cancelled ids stay bounded while the main loop computes.
const MAX_QUEUED_BYTES: usize = 4 * MAX_MESSAGE;
/// What each message counts against `MAX_QUEUED_BYTES` besides its body, for its entry in the
/// channel and the queue. By their bodies alone, empty or tiny messages would count next to nothing,
/// and a client could pile them up without bound (Copilot review of PR #119); with it, the reader
/// waits after about 65,536 messages, however small.
const MESSAGE_COST: usize = 1024;
/// How many ids the main loop keeps of the cancellations that no queued request had when they
/// arrived (one may precede its request), and of the requests it answered last (a later
/// cancellation of one is dropped).
const MAX_REMEMBERED_IDS: usize = 1024;
/// The longest id (its JSON text) that the main loop keeps among those. An id may be as long as a
/// message, and the ids stay after their messages are handled, so the count alone would let 1024
/// long ones hold gigabytes (Copilot review 2 of PR #119). A cancellation of a longer id that no
/// queued request has is dropped at once, as is a late one of its answered request; one that
/// precedes its request has no effect.
const MAX_REMEMBERED_ID: usize = 256;
const TEXT_SYNC: [&str; 3] = [
    "textDocument/didOpen",
    "textDocument/didChange",
    "textDocument/didClose",
];

enum InputEvent {
    /// A message and what it counts against `MAX_QUEUED_BYTES`: the length of its body and
    /// `MESSAGE_COST`.
    Message(Result<Value, String>, usize),
    Failure(String),
    End,
}

/// What the reader thread shares with the main loop (G39 段 11).
#[derive(Default)]
struct Inbox {
    state: Mutex<InboxState>,
    handled: Condvar,
}

#[derive(Default)]
struct InboxState {
    /// What the messages that the reader has read and the main loop has not handled count: their
    /// bodies and `MESSAGE_COST` each.
    bytes: usize,
    /// The main loop has returned.
    closed: bool,
    /// For each id (`Value::to_string`), how many of the `$/cancelRequest` messages that cancel it
    /// the reader has read and the main loop has not let go of (Copilot review 3 of PR #119): one
    /// still on its way to the queue, one of `Queue::unmatched`, or one of `Queue::held`. A request
    /// is cancelled while its id has one. The reader records a cancellation when it reads it, so
    /// that it takes effect at once, also for the request being computed; the main loop lets go of
    /// exactly the ones it disposes of. Were the ids a set, letting go of one copy would forget
    /// another that the reader read before the main loop queued it.
    cancelled: BTreeMap<String, usize>,
}

impl Inbox {
    /// Waits while the unhandled messages count more than `MAX_QUEUED_BYTES`; `false` once the main
    /// loop has returned.
    fn wait_for_room(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        while state.bytes > MAX_QUEUED_BYTES && !state.closed {
            let Ok(next) = self.handled.wait(state) else {
                return false;
            };
            state = next;
        }
        !state.closed
    }

    /// Counts a message that the reader read; a cancellation takes effect at once, also for the
    /// request that the main loop is computing.
    fn received(&self, message: &Result<Value, String>, bytes: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.bytes += bytes;
            if let Some(id) = message.as_ref().ok().and_then(cancelled_id) {
                *state.cancelled.entry(id).or_default() += 1;
            }
        }
    }

    /// Lets go of `count` cancellations of `id` that the main loop has disposed of.
    fn release(&self, id: &str, count: usize) {
        if count == 0 {
            return;
        }
        if let Ok(mut state) = self.state.lock()
            && let Some(held) = state.cancelled.get_mut(id)
        {
            *held = held.saturating_sub(count);
            if *held == 0 {
                state.cancelled.remove(id);
            }
        }
    }

    fn handled(&self, bytes: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.bytes = state.bytes.saturating_sub(bytes);
        }
        self.handled.notify_all();
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
        self.handled.notify_all();
    }

    fn is_cancelled(&self, id: &str) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.cancelled.contains_key(id))
    }
}

/// The id that a `$/cancelRequest` cancels, as a key of `InboxState::cancelled`.
fn cancelled_id(message: &Value) -> Option<String> {
    if message["method"] != "$/cancelRequest" {
        return None;
    }
    id_key(message["params"].get("id")?)
}

/// Whether a JSON-RPC id is valid: a string or an integer.
fn valid_id(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64()
}

/// The key of a valid JSON-RPC id (`Value::to_string`).
fn id_key(id: &Value) -> Option<String> {
    valid_id(id).then(|| id.to_string())
}

/// The method of a message whose envelope is valid: `jsonrpc` is `"2.0"`, `method` is a string, and
/// the id, if there is one, is a string or an integer. The main loop answers any other message with
/// -32600.
fn envelope_method(message: &Value) -> Option<&str> {
    (message.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && message.get("id").is_none_or(valid_id))
    .then(|| message.get("method").and_then(Value::as_str))
    .flatten()
}

/// A message that the main loop took from the reader, with the document that it syncs or asks
/// about.
struct Queued {
    event: InputEvent,
    document: Option<PathBuf>,
}

impl Queued {
    fn new(event: InputEvent) -> Self {
        let document = match &event {
            InputEvent::Message(Ok(message), _)
                if message["method"]
                    .as_str()
                    .is_some_and(|method| method.starts_with("textDocument/")) =>
            {
                document_path(&message["params"]).ok()
            }
            _ => None,
        };
        Self { event, document }
    }

    fn message(&self) -> Option<&Value> {
        match &self.event {
            InputEvent::Message(Ok(message), _) => Some(message),
            _ => None,
        }
    }

    fn method(&self) -> &str {
        self.message()
            .and_then(|message| message["method"].as_str())
            .unwrap_or("")
    }

    /// The key of the message's id, when it is a string or an integer. The main loop answers every
    /// message that has one through `Queue::answer`, whatever it does with the message, also when it
    /// rejects its envelope (Copilot review 4 of PR #119).
    fn id_key(&self) -> Option<String> {
        id_key(self.message()?.get("id")?)
    }

    /// The key of a request's id: of a message with a valid id that is not a text sync, whatever its
    /// envelope.
    fn request_id(&self) -> Option<String> {
        self.id_key()
            .filter(|_| !TEXT_SYNC.contains(&self.method()))
    }

    /// The document that this message changes or closes.
    fn modified(&self) -> Option<&Path> {
        matches!(
            self.method(),
            "textDocument/didChange" | "textDocument/didClose"
        )
        .then_some(self.document.as_deref())
        .flatten()
    }

    /// What the message counts against `MAX_QUEUED_BYTES`.
    fn bytes(&self) -> usize {
        match self.event {
            InputEvent::Message(_, bytes) => bytes,
            _ => 0,
        }
    }
}

/// The messages that the main loop has taken from the reader and not handled, in the order they
/// arrived, with the ids that it keeps for cancellations (G39 spec 12, 13).
///
/// Each cancellation that the reader recorded in `Inbox` is let go of there exactly when the queue
/// disposes of it: when it is dropped as it arrives, pushed out of `unmatched`, or replaced there by
/// a later one of the same id, and when the request that `held` keeps it for is answered. So `Inbox`
/// counts, for each id, the cancellations on their way to the queue, its entry of `unmatched`, and
/// its count in `held` (Copilot review 3 of PR #119). The main loop answers every message with a
/// valid id that it takes, also one whose envelope it rejects (`Queued::id_key`), so `held` keeps
/// ids only for the queued requests and the message being answered (Copilot review 4).
struct Queue {
    items: VecDeque<Queued>,
    /// How many queued requests have each id.
    requests: BTreeMap<String, usize>,
    /// How many queued text syncs change or close each document.
    modifications: BTreeMap<PathBuf, usize>,
    /// The cancellations that no queued request had, oldest first, each id once.
    unmatched: VecDeque<String>,
    /// How many cancellations belong to the queued requests (or the one being computed) of each id,
    /// to be let go of when it is answered.
    held: BTreeMap<String, usize>,
    /// The requests answered last, oldest first, and the same as a set.
    answered: VecDeque<String>,
    recent: BTreeSet<String>,
    /// How many ids `unmatched` and `answered` keep: `MAX_REMEMBERED_IDS`, fewer in tests.
    remembered: usize,
}

impl Default for Queue {
    fn default() -> Self {
        Self::remembering(MAX_REMEMBERED_IDS)
    }
}

impl Queue {
    fn remembering(remembered: usize) -> Self {
        Self {
            items: VecDeque::new(),
            requests: BTreeMap::new(),
            modifications: BTreeMap::new(),
            unmatched: VecDeque::new(),
            held: BTreeMap::new(),
            answered: VecDeque::new(),
            recent: BTreeSet::new(),
            remembered,
        }
    }

    /// Takes a message from the reader. A cancellation of a queued request is kept for it; one of an
    /// answered request is dropped; any other with an id of at most `MAX_REMEMBERED_ID` bytes is kept
    /// for a request that may follow, the latest `remembered` of them (a repeated one at its latest
    /// place), and belongs to that request once it arrives (later unknown ids no longer push it
    /// out); any other is dropped. A request with the id of an answered one (ids need be unique only
    /// among outstanding requests) makes the server forget the answered one, so that the new
    /// request's cancellation is not dropped as a late one.
    fn push(&mut self, event: InputEvent, inbox: &Inbox) {
        let item = Queued::new(event);
        if let Some(id) = item.message().and_then(cancelled_id) {
            if self.requests.contains_key(&id) {
                *self.held.entry(id).or_default() += 1;
            } else if self.recent.contains(&id) || id.len() > MAX_REMEMBERED_ID {
                inbox.release(&id, 1);
            } else {
                // Once, at the place of the latest (Copilot review 2 of PR #119): the older copy is
                // let go of, and the newer one keeps the id cancelled.
                if let Some(at) = self.unmatched.iter().position(|unmatched| *unmatched == id) {
                    self.unmatched.remove(at);
                    inbox.release(&id, 1);
                }
                self.unmatched.push_back(id);
                if self.unmatched.len() > self.remembered
                    && let Some(oldest) = self.unmatched.pop_front()
                {
                    inbox.release(&oldest, 1);
                }
            }
        }
        if let Some(id) = item.request_id() {
            if self.recent.remove(&id) {
                self.answered.retain(|answered| *answered != id);
            }
            // A cancellation that arrived before the request is now the queued request's: dropping
            // it as the oldest unmatched one would forget it (Copilot review of PR #119).
            if let Some(at) = self.unmatched.iter().position(|unmatched| *unmatched == id) {
                self.unmatched.remove(at);
                *self.held.entry(id.clone()).or_default() += 1;
            }
            *self.requests.entry(id).or_default() += 1;
        }
        if let Some(document) = item.modified() {
            *self.modifications.entry(document.to_owned()).or_default() += 1;
        }
        self.items.push_back(item);
    }

    fn pop(&mut self, inbox: &Inbox) -> Option<Queued> {
        let item = self.items.pop_front()?;
        inbox.handled(item.bytes());
        if let Some(id) = item.request_id() {
            decrement(&mut self.requests, &id);
        }
        if let Some(document) = item.modified() {
            decrement(&mut self.modifications, document);
        }
        Some(item)
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Records that the request `id` is answered (unless the id is longer than `MAX_REMEMBERED_ID`)
    /// and lets go of the cancellations kept for it; whether it had one. One that the reader has
    /// read and the queue has not taken yet stays recorded until the queue drops it as a late one.
    fn answer(&mut self, id: &str, inbox: &Inbox) -> bool {
        if id.len() <= MAX_REMEMBERED_ID && self.recent.insert(id.to_owned()) {
            self.answered.push_back(id.to_owned());
            if self.answered.len() > self.remembered
                && let Some(oldest) = self.answered.pop_front()
            {
                self.recent.remove(&oldest);
            }
        }
        let cancelled = inbox.is_cancelled(id);
        let mut held = self.held.remove(id).unwrap_or(0);
        // A message with an id that is not a request (a text sync) does not take the cancellation
        // that preceded it from `unmatched`; once it is answered, that one goes as well.
        if let Some(at) = self.unmatched.iter().position(|unmatched| unmatched == id) {
            self.unmatched.remove(at);
            held += 1;
        }
        inbox.release(id, held);
        cancelled
    }

    /// Whether a later text sync changes or closes `document`: a request about it would answer for a
    /// text that the client has replaced (G39 D10).
    fn modified_later(&self, document: &Path) -> bool {
        self.modifications.contains_key(document)
    }
}

fn decrement<K: Ord + ?Sized + ToOwned>(counts: &mut BTreeMap<K::Owned, usize>, key: &K)
where
    K::Owned: Ord + std::borrow::Borrow<K>,
{
    if let Some(count) = counts.get_mut(key) {
        *count -= 1;
        if *count == 0 {
            counts.remove(key);
        }
    }
}

/// The reader thread: reads messages while the unhandled ones stay within `MAX_QUEUED_BYTES` and
/// passes them on, through the end of the input or its first failure.
fn read_input(input: impl Read, sender: &mpsc::Sender<InputEvent>, inbox: &Inbox) {
    let mut input = io::BufReader::new(input);
    while inbox.wait_for_room() {
        let event = match read_frame(&mut input) {
            Ok(Some((message, bytes))) => {
                let cost = bytes + MESSAGE_COST;
                inbox.received(&message, cost);
                InputEvent::Message(message, cost)
            }
            Ok(None) => InputEvent::End,
            Err(error) => InputEvent::Failure(error.to_string()),
        };
        let last = !matches!(event, InputEvent::Message(..));
        if sender.send(event).is_err() || last {
            return;
        }
    }
}

/// Lets the reader stop waiting for room once the main loop has returned.
struct Closing<'a>(&'a Inbox);

impl Drop for Closing<'_> {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub fn serve(
    input: impl Read + Send + 'static,
    mut output: impl Write,
    mut stderr: impl Write,
) -> i32 {
    let (sender, receiver) = mpsc::channel();
    let inbox = Arc::new(Inbox::default());
    let reader = Arc::clone(&inbox);
    std::thread::spawn(move || read_input(input, &sender, &reader));
    let _closing = Closing(&inbox);
    let mut session = Session::default();
    // The sources that the project loads again at every change are read from memory while their
    // `stat` says that they are the same (PB06).
    let mut resident = crate::resident::ResidentState::new();
    let _resident = crate::resident::Scope::enter(&mut resident);
    let mut queue = Queue::default();
    loop {
        if queue.is_empty() {
            let event = if let Some(deadline) = session.pending.values().copied().min() {
                receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            } else {
                receiver
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
            };
            match event {
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let due: Vec<_> = session
                        .pending
                        .iter()
                        .filter(|(_, deadline)| **deadline <= Instant::now())
                        .map(|(directory, _)| directory.clone())
                        .collect();
                    if let Err(error) = session.refresh_due(due, &mut output) {
                        let _ = writeln!(stderr, "LSP output failed: {error}");
                        return 1;
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return i32::from(!session.shutdown);
                }
                Ok(event) => queue.push(event, &inbox),
            }
        }
        // Every message that has arrived joins the queue before the next one is handled, so that a
        // cancellation or a newer text is seen before the request starts (G39 spec 12, 13).
        while let Ok(event) = receiver.try_recv() {
            queue.push(event, &inbox);
        }
        let Some(item) = queue.pop(&inbox) else {
            continue;
        };
        let request = item.request_id().is_some();
        // Every message with a valid id is answered through `Queue::answer` below, whichever way it
        // takes, so that the cancellations kept for its id are let go of.
        let key = item.id_key();
        let Queued { event, document } = item;
        let message = match event {
            InputEvent::End => return i32::from(!session.shutdown),
            InputEvent::Failure(message) => {
                let _ = write_message(
                    &mut output,
                    &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": message}}),
                );
                return 1;
            }
            InputEvent::Message(Err(message), _) => {
                if write_message(&mut output, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": message}})).is_err() { return 1; }
                continue;
            }
            InputEvent::Message(Ok(message), _) => message,
        };
        let Some(method) = envelope_method(&message) else {
            // The queue took a valid id as a request's whatever the envelope, and kept the
            // cancellations of that id for it; unanswered, they would stay after the queue is empty
            // and cancel the next request with the id (Copilot review 4 of PR #119).
            if let Some(key) = &key {
                queue.answer(key, &inbox);
            }
            if write_message(&mut output, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "invalid JSON-RPC request"}})).is_err() { return 1; }
            continue;
        };
        let id = message.get("id").cloned();
        if method == "exit" {
            return i32::from(!session.shutdown);
        }
        // A text sync is applied and checked in order, also when a later one of its document
        // replaces it: skipped, it would be lost if the later one were rejected, and the changes of
        // other documents would meet the limit of the open buffers beside a text that the client
        // has replaced (Copilot review 2 of PR #119). Applying it only schedules its root, and
        // nothing is analyzed before a computed request or an empty queue, so a replaced text is
        // not analyzed (G39 spec 12).
        let cancelled = || key.as_deref().is_some_and(|key| inbox.is_cancelled(key));
        let result = if cancelled() {
            Err(request_cancelled())
        } else if request
            && document
                .as_deref()
                .is_some_and(|document| queue.modified_later(document))
        {
            // The client has sent a newer text of the document; it drops this answer (G39 D10).
            Err((-32801, "content modified".into()))
        } else {
            session.handle(
                method,
                message.get("params").unwrap_or(&Value::Null),
                &mut output,
                &cancelled,
            )
        };
        let result = match &key {
            Some(key) if queue.answer(key, &inbox) => Err(request_cancelled()),
            _ => result,
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
    use crate::diagnostic::DiagnosticKind;
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
    fn a_fix_must_remove_its_diagnostic_and_add_nothing() {
        let error = |kind: DiagnosticKind| Diagnostic::of(kind, "m", Span::new(0, 0));
        let warning = error;
        let removes = |diagnostic: &Diagnostic, before: &[Diagnostic], after: &[Diagnostic]| {
            fix_removes(
                diagnostic,
                &diagnostic_counts(before),
                &diagnostic_counts(after),
            )
        };
        // A fixed error may uncover warnings of later passes.
        assert!(
            removes(
                &error(DiagnosticKind::NonExhaustiveMatch),
                &[error(DiagnosticKind::NonExhaustiveMatch)],
                &[warning(DiagnosticKind::UnusedLocal)]
            )
            .is_ok()
        );
        assert!(
            removes(
                &error(DiagnosticKind::NonExhaustiveMatch),
                &[error(DiagnosticKind::NonExhaustiveMatch)],
                &[error(DiagnosticKind::NonExhaustiveMatch)]
            )
            .is_err()
        );
        assert!(
            removes(
                &error(DiagnosticKind::NonExhaustiveMatch),
                &[error(DiagnosticKind::NonExhaustiveMatch)],
                &[error(DiagnosticKind::TypeMismatch)]
            )
            .is_err()
        );
        assert!(
            removes(
                &warning(DiagnosticKind::UnusedLocal),
                &[warning(DiagnosticKind::UnusedLocal)],
                &[]
            )
            .is_ok()
        );
        assert!(
            removes(
                &warning(DiagnosticKind::UnusedLocal),
                &[warning(DiagnosticKind::UnusedLocal)],
                &[warning(DiagnosticKind::UnusedLocal)]
            )
            .is_err()
        );
        assert!(
            removes(
                &warning(DiagnosticKind::UnusedLocal),
                &[warning(DiagnosticKind::UnusedLocal)],
                &[warning(DiagnosticKind::DeprecatedUse)]
            )
            .is_err()
        );
        assert!(
            removes(
                &warning(DiagnosticKind::UnusedLocal),
                &[warning(DiagnosticKind::UnusedLocal)],
                &[error(DiagnosticKind::TypeMismatch)]
            )
            .is_err()
        );
        // The counts include the diagnostics past the display limit: 51 to 50 is one fewer.
        let many = |count: usize| vec![error(DiagnosticKind::NonExhaustiveMatch); count];
        assert!(
            removes(
                &error(DiagnosticKind::NonExhaustiveMatch),
                &many(51),
                &many(50)
            )
            .is_ok()
        );
        assert!(
            removes(
                &error(DiagnosticKind::NonExhaustiveMatch),
                &many(51),
                &many(51)
            )
            .is_err()
        );
    }

    /// G39 段 12: a code action asks for cancellation before its first verification and stops there
    /// with -32800, without verifying a fix or starting fix all's rounds.
    #[test]
    fn a_cancelled_code_action_stops_before_its_first_verification() {
        let directory = std::env::temp_dir().join(format!(
            "tsuzuri-lsp-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        let text = "fn warn() -> i64 { let spare = 1; 2 }\n";
        std::fs::write(directory.join("Warn.tz"), text).unwrap();
        let uri = file_uri(&directory.join("Warn.tz")).unwrap();
        let never = || false;
        let mut session = Session::default();
        let mut output = Vec::new();
        session
            .handle(
                "initialize",
                &json!({"rootUri": file_uri(&directory), "capabilities": {}}),
                &mut output,
                &never,
            )
            .unwrap();
        session
            .handle(
                "textDocument/didOpen",
                &json!({"textDocument": {"uri": uri, "languageId": "tsuzuri", "version": 1, "text": text}}),
                &mut output,
                &never,
            )
            .unwrap();
        let params = json!({"textDocument": {"uri": uri}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 37}}, "context": {"diagnostics": []}});
        let (root, source) = session.document(&params, &mut output).unwrap();
        let source = source.unwrap();
        // The fix of `spare` is in the range and `certain`: without a cancellation, both a quick fix
        // and fix all would be verified.
        let actions = session
            .code_actions(&root, source, &params, &never)
            .unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 2, "{actions}");
        let calls = std::cell::Cell::new(0);
        let cancelled = || {
            calls.set(calls.get() + 1);
            true
        };
        let result = session.code_actions(&root, source, &params, &cancelled);
        assert_eq!(result.unwrap_err().0, -32800);
        assert_eq!(calls.get(), 1);
        // Without a quick fix in the range, fix all asks before its first round.
        let elsewhere = json!({"textDocument": {"uri": uri}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 2}}, "context": {"diagnostics": []}});
        calls.set(0);
        let result = session.code_actions(&root, source, &elsewhere, &cancelled);
        assert_eq!(result.unwrap_err().0, -32800);
        assert_eq!(calls.get(), 1);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    /// Copilot review of PR #119: a cancellation that arrives before its request belongs to the
    /// request once the request is queued, so the cancellations of unknown ids that follow cannot
    /// push it out of the ones kept for a request that may follow.
    #[test]
    fn a_cancellation_before_its_request_survives_later_unknown_ones() {
        let inbox = Inbox::default();
        let mut queue = Queue::default();
        // As the reader does: count and record a message, then pass it on.
        let mut arrive = |message: Value| {
            let message = Ok(message);
            inbox.received(&message, 1);
            queue.push(InputEvent::Message(message, 1), &inbox);
        };
        arrive(json!({"jsonrpc": "2.0", "method": "$/cancelRequest", "params": {"id": 5}}));
        arrive(
            json!({"jsonrpc": "2.0", "id": 5, "method": "workspace/symbol", "params": {"query": ""}}),
        );
        for ghost in 0..=MAX_REMEMBERED_IDS {
            arrive(
                json!({"jsonrpc": "2.0", "method": "$/cancelRequest", "params": {"id": format!("ghost-{ghost}")}}),
            );
        }
        assert!(inbox.is_cancelled("5"));
    }

    /// Passes `message` to the queue as the reader thread does: counts it and records a
    /// cancellation, then sends it on.
    fn arrive(inbox: &Inbox, queue: &mut Queue, message: Value) {
        let message = Ok(message);
        inbox.received(&message, 1);
        queue.push(InputEvent::Message(message, 1), inbox);
    }

    fn cancel(id: Value) -> Value {
        json!({"jsonrpc": "2.0", "method": "$/cancelRequest", "params": {"id": id}})
    }

    fn symbol_request(id: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": "workspace/symbol", "params": {"query": ""}})
    }

    /// Copilot review 2 of PR #119: a cancellation that arrives more than once before its request is
    /// kept once, at the place of the latest one, so the cancellations of unknown ids that follow
    /// push it out no sooner than they would push out that latest one.
    #[test]
    fn a_repeated_cancellation_before_its_request_is_kept_at_its_latest_place() {
        // Twice in a row, then unknown ids that fill the rest of the room.
        let (inbox, mut queue) = (Inbox::default(), Queue::default());
        arrive(&inbox, &mut queue, cancel(json!(5)));
        arrive(&inbox, &mut queue, cancel(json!(5)));
        for ghost in 1..MAX_REMEMBERED_IDS {
            arrive(&inbox, &mut queue, cancel(json!(format!("ghost-{ghost}"))));
        }
        arrive(&inbox, &mut queue, symbol_request(json!(5)));
        assert!(inbox.is_cancelled("5"));
        // Once, unknown ids that fill the room, again, and one more unknown id: the second takes the
        // newest place, so the unknown id that pushes out the first place does not forget it.
        let (inbox, mut queue) = (Inbox::default(), Queue::default());
        arrive(&inbox, &mut queue, cancel(json!(6)));
        for ghost in 1..MAX_REMEMBERED_IDS {
            arrive(&inbox, &mut queue, cancel(json!(format!("ghost-{ghost}"))));
        }
        arrive(&inbox, &mut queue, cancel(json!(6)));
        arrive(&inbox, &mut queue, cancel(json!("ghost-last")));
        arrive(&inbox, &mut queue, symbol_request(json!(6)));
        assert!(inbox.is_cancelled("6"));
    }

    /// Copilot review 3 of PR #119: the reader records each cancellation when it reads it, before
    /// the main loop queues it. Here it reads a whole burst first: a cancellation of 5, cancellations
    /// of unknown ids that push the first out of the ones kept, the cancellation of 5 again, and the
    /// request 5. Pushing out the first must not forget the second, which the reader recorded too.
    #[test]
    fn a_cancellation_read_again_outlives_its_pushed_out_copy() {
        let (inbox, mut queue) = (Inbox::default(), Queue::default());
        let burst: Vec<Value> = std::iter::once(cancel(json!(5)))
            .chain((0..MAX_REMEMBERED_IDS).map(|ghost| cancel(json!(format!("ghost-{ghost}")))))
            .chain([cancel(json!(5)), symbol_request(json!(5))])
            .collect();
        for message in &burst {
            inbox.received(&Ok(message.clone()), 1);
        }
        for message in burst {
            queue.push(InputEvent::Message(Ok(message), 1), &inbox);
        }
        assert!(inbox.is_cancelled("5"));
    }

    /// Copilot review 4 of PR #119: a message with a valid id but an invalid envelope (no `jsonrpc`,
    /// another version, no `method`, a `method` that is not a string) is queued as a request, so a
    /// cancellation of its id that precedes or follows it is kept for it. Answering it with -32600
    /// lets go of that cancellation: kept, it would never be let go of (one id more for each such
    /// message, after the queue is empty), and it cancelled the next valid request with the same id.
    #[test]
    fn a_malformed_request_lets_go_of_its_cancellations() {
        let messages = [
            cancel(json!(5)),
            json!({"id": 5, "method": "workspace/symbol", "params": {"query": ""}}),
            symbol_request(json!(5)),
            json!({"jsonrpc": "2.0", "id": 6, "result": null}),
            cancel(json!(6)),
            symbol_request(json!(6)),
            json!({"jsonrpc": "2.0", "id": "seven", "method": 7}),
            cancel(json!("seven")),
            symbol_request(json!("seven")),
            cancel(json!(8)),
            json!({"jsonrpc": "1.0", "id": 8, "method": "workspace/symbol"}),
            symbol_request(json!(8)),
        ];
        let mut input = Vec::new();
        for message in &messages {
            write_message(&mut input, message).unwrap();
        }
        let (mut output, mut stderr) = (Vec::new(), Vec::new());
        serve(Cursor::new(input), &mut output, &mut stderr);
        let mut output = Cursor::new(output);
        let mut answers = BTreeMap::new();
        let mut invalid = 0;
        while let Some(message) = read_message(&mut output).unwrap() {
            let message = message.unwrap();
            match &message["id"] {
                Value::Null if message["error"]["code"] == -32600 => invalid += 1,
                Value::Null => {}
                id => {
                    answers.insert(id.to_string(), message["error"]["code"].clone());
                }
            }
        }
        assert_eq!(invalid, 4, "{answers:?}");
        // Before `initialize`, a request that is not cancelled is answered with -32002.
        for id in ["5", "6", "\"seven\"", "8"] {
            assert_eq!(answers.get(id), Some(&json!(-32002)), "{id}: {answers:?}");
        }
    }

    /// The reference for `cancellations_agree_with_a_reference_model_in_any_interleaving`: the main
    /// loop alone keeps the cancellations, by the rules of `Queue::push` (G39 spec 13 and the
    /// deviations 1 and 11), and the request being computed is cancelled as well by one that the
    /// reader has read and the queue has not taken yet (it can only follow the request). A message
    /// with a valid id that is not a text sync is a request whatever its envelope, and answering a
    /// message with a valid id (a rejected one and a text sync too) lets go of every cancellation
    /// kept for that id (Copilot review 4 of PR #119).
    struct CancelModel {
        remembered: usize,
        /// The cancellations kept for a request that may follow, oldest first, each id once.
        unmatched: VecDeque<String>,
        /// How many queued requests have each id.
        queued: BTreeMap<String, usize>,
        /// The ids of the queued requests (and of the one being computed) that a cancellation was
        /// kept for.
        cancelled: BTreeSet<String>,
        /// The requests answered last, oldest first.
        answered: VecDeque<String>,
    }

    impl CancelModel {
        fn cancel(&mut self, id: &str) {
            if self.queued.contains_key(id) {
                self.cancelled.insert(id.to_owned());
            } else if id.len() <= MAX_REMEMBERED_ID
                && !self.answered.iter().any(|answered| answered == id)
            {
                self.unmatched.retain(|unmatched| unmatched != id);
                self.unmatched.push_back(id.to_owned());
                if self.unmatched.len() > self.remembered {
                    self.unmatched.pop_front();
                }
            }
        }

        fn request(&mut self, id: &str) {
            self.answered.retain(|answered| answered != id);
            if self.unmatched.iter().any(|unmatched| unmatched == id) {
                self.unmatched.retain(|unmatched| unmatched != id);
                self.cancelled.insert(id.to_owned());
            }
            *self.queued.entry(id.to_owned()).or_default() += 1;
        }

        /// Whether the message `id` that was taken is cancelled; `arriving` says whether the reader
        /// has read a cancellation of `id` that the queue has not taken.
        fn answer(&mut self, id: &str, arriving: bool) -> bool {
            let cancelled = self.cancelled.remove(id) || arriving;
            self.unmatched.retain(|unmatched| unmatched != id);
            if id.len() <= MAX_REMEMBERED_ID && !self.answered.iter().any(|answered| answered == id)
            {
                self.answered.push_back(id.to_owned());
                if self.answered.len() > self.remembered {
                    self.answered.pop_front();
                }
            }
            cancelled
        }
    }

    /// The messages of `cancellations_agree_with_a_reference_model_in_any_interleaving`, one for
    /// each way through `serve` that a message with an id or a cancellation can take.
    #[derive(Clone, Copy, Debug)]
    enum Kind {
        /// `$/cancelRequest`.
        Cancel,
        /// `$/cancelRequest` without `jsonrpc`: rejected with -32600, it cancels all the same.
        BareCancel,
        /// `$/cancelRequest` with an id of its own: it cancels, and it is computed as a request.
        CancelWithId,
        /// A request: computed and answered.
        Request,
        /// Requests rejected with -32600 and answered: without `jsonrpc`, a response (no
        /// `method`), and a request whose `method` is a number.
        NoVersion,
        NoMethod,
        NumericMethod,
        /// A text sync with an id: applied and answered, not queued as a request.
        SyncWithId,
        /// Messages without an id to answer: a notification, a request whose id is `null`
        /// (rejected with -32600), and one that is not JSON (-32700).
        Notification,
        NullId,
        NotJson,
    }

    impl Kind {
        const ALL: [Kind; 11] = [
            Kind::Cancel,
            Kind::BareCancel,
            Kind::CancelWithId,
            Kind::Request,
            Kind::NoVersion,
            Kind::NoMethod,
            Kind::NumericMethod,
            Kind::SyncWithId,
            Kind::Notification,
            Kind::NullId,
            Kind::NotJson,
        ];

        fn cancels(self) -> bool {
            matches!(self, Kind::Cancel | Kind::BareCancel | Kind::CancelWithId)
        }

        /// Whether the queue takes the message's own id as a request's.
        fn is_request(self) -> bool {
            matches!(
                self,
                Kind::CancelWithId
                    | Kind::Request
                    | Kind::NoVersion
                    | Kind::NoMethod
                    | Kind::NumericMethod
            )
        }

        /// Whether the main loop computes the message as a request, rather than answering it at
        /// once.
        fn is_computed(self) -> bool {
            matches!(self, Kind::CancelWithId | Kind::Request)
        }

        /// Whether the message has a valid id of its own, which the main loop answers.
        fn is_answered(self) -> bool {
            self.is_request() || matches!(self, Kind::SyncWithId)
        }
    }

    /// Copilot reviews 3 and 4 of PR #119: the reader records each cancellation in `Inbox` when it
    /// reads it and sends it on after; the main loop queues all that has been sent, takes one
    /// message, and answers it as `serve` does (it computes a valid request first; it answers a
    /// message with a valid id that it rejects or that is a text sync at once), while the reader
    /// goes on. In any interleaving of those steps, each computed request is cancelled exactly when
    /// `CancelModel` says so, and after every step `Inbox` counts, for each id, the cancellations on
    /// their way to the queue, its entry of `unmatched` and its count in `held`; `held` keeps
    /// nothing but the ids of queued requests and of the message being answered; and the ids kept
    /// for cancellations and answers are the model's. Once all is answered, the queue keeps nothing
    /// but those bounded ids. Short memories (one or two ids), two short ids and one longer than
    /// `MAX_REMEMBERED_ID` make pushing out, repeating, reusing and dropping ids occur within a few
    /// messages. The interleavings are drawn from a fixed seed.
    #[test]
    fn cancellations_agree_with_a_reference_model_in_any_interleaving() {
        let ids = [json!(1), json!("two"), json!("3".repeat(MAX_REMEMBERED_ID))];
        let keys: Vec<String> = ids.iter().map(Value::to_string).collect();
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut random = move |below: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % below as u64) as usize
        };
        for run in 0..40_000 {
            let remembered = 1 + random(2);
            let reader_odds = 1 + random(3);
            // Each message: its kind, the id that it cancels or has, and the id of its own of a
            // `CancelWithId`. Cancellations and requests are as frequent as the other kinds together.
            let stream: Vec<(Kind, usize, usize)> = (0..1 + random(8))
                .map(|_| {
                    let kind = if random(2) == 0 {
                        [Kind::Cancel, Kind::Request][random(2)]
                    } else {
                        Kind::ALL[random(Kind::ALL.len())]
                    };
                    (kind, random(ids.len()), random(ids.len()))
                })
                .collect();
            let message = |at: usize| -> Result<Value, String> {
                let (kind, id, own) = stream[at];
                let (id, own) = (ids[id].clone(), ids[own].clone());
                Ok(match kind {
                    Kind::Cancel => cancel(id),
                    Kind::BareCancel => json!({"method": "$/cancelRequest", "params": {"id": id}}),
                    Kind::CancelWithId => {
                        json!({"jsonrpc": "2.0", "id": own, "method": "$/cancelRequest", "params": {"id": id}})
                    }
                    Kind::Request => symbol_request(id),
                    Kind::NoVersion => {
                        json!({"id": id, "method": "workspace/symbol", "params": {"query": ""}})
                    }
                    Kind::NoMethod => json!({"jsonrpc": "2.0", "id": id, "result": null}),
                    Kind::NumericMethod => json!({"jsonrpc": "2.0", "id": id, "method": 7}),
                    Kind::SyncWithId => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "method": "textDocument/didChange",
                        "params": {"textDocument": {"uri": "file:///w/A.tz", "version": 2}, "contentChanges": []},
                    }),
                    Kind::Notification => json!({"jsonrpc": "2.0", "method": "initialized"}),
                    Kind::NullId => json!({"jsonrpc": "2.0", "id": null, "method": "shutdown"}),
                    Kind::NotJson => return Err("expected value at line 1 column 1".into()),
                })
            };
            let kind_at = |at: usize| stream[at].0;
            // The key of the id that the message `at` cancels, and of its own id.
            let cancelled_key = |at: usize| {
                let (kind, id, _) = stream[at];
                kind.cancels().then(|| keys[id].clone())
            };
            let own_key = |at: usize| {
                let (kind, id, own) = stream[at];
                match kind {
                    Kind::CancelWithId => Some(keys[own].clone()),
                    kind if kind.is_answered() => Some(keys[id].clone()),
                    _ => None,
                }
            };
            let (inbox, mut queue) = (Inbox::default(), Queue::remembering(remembered));
            let mut model = CancelModel {
                remembered,
                unmatched: VecDeque::new(),
                queued: BTreeMap::new(),
                cancelled: BTreeSet::new(),
                answered: VecDeque::new(),
            };
            // The reader has recorded the first `read` messages and sent the first `sent`; the main
            // loop has queued the first `queued` and taken the first `taken`. Its step: queue what
            // was sent, take one message (`drained`), answer the message with an id that it took
            // (`answering`: its id, whether it was cancelled when taken, whether it is computed).
            let (mut read, mut sent, mut queued, mut taken) = (0, 0, 0, 0);
            let mut drained = false;
            let mut answering: Option<(String, bool, bool)> = None;
            let mut trace = Vec::new();
            // The cancellations of `id` that the reader has recorded and the queue has not taken.
            let on_the_way = |id: &str, queued: usize, read: usize| {
                (queued..read)
                    .filter(|&at| cancelled_key(at).as_deref() == Some(id))
                    .count()
            };
            loop {
                let reader = sent < read || read < stream.len();
                let main = answering.is_some() || drained || !queue.is_empty() || queued < sent;
                if !reader && !main {
                    break;
                }
                if reader && (!main || random(4) < reader_odds) {
                    if sent < read {
                        trace.push(format!("send {sent}"));
                        sent += 1;
                    } else {
                        trace.push(format!("read {read}"));
                        inbox.received(&message(read), 1);
                        read += 1;
                    }
                } else if let Some((id, when_taken, computed)) = answering.take() {
                    let cancelled = queue.answer(&id, &inbox) || when_taken;
                    let arriving = on_the_way(&id, queued, read) > 0;
                    trace.push(format!("answer {id}: cancelled {cancelled}"));
                    let expected = model.answer(&id, arriving);
                    if computed {
                        assert_eq!(cancelled, expected, "run {run}, {stream:?}, {trace:?}");
                    }
                } else if drained {
                    drained = false;
                    let item = queue.pop(&inbox).expect("a drained queue has a message");
                    let kind = kind_at(taken);
                    // As `serve` decides: a valid request is computed, and any other message with a
                    // valid id is answered at once.
                    let computed = item.request_id().is_some()
                        && item.message().and_then(envelope_method).is_some();
                    assert_eq!(item.id_key(), own_key(taken), "run {run}, {trace:?}");
                    assert_eq!(computed, kind.is_computed(), "run {run}, {kind:?}");
                    if kind.is_request() {
                        decrement(&mut model.queued, own_key(taken).unwrap().as_str());
                    }
                    if let Some(id) = item.id_key() {
                        trace.push(format!("take {kind:?} {id}"));
                        answering = Some((id.clone(), inbox.is_cancelled(&id), computed));
                    } else {
                        trace.push(format!("take {kind:?}"));
                    }
                    taken += 1;
                } else {
                    trace.push(format!("queue {queued}..{sent}"));
                    for at in queued..sent {
                        queue.push(InputEvent::Message(message(at), 1), &inbox);
                        if let Some(id) = cancelled_key(at) {
                            model.cancel(&id);
                        }
                        if kind_at(at).is_request() {
                            model.request(&own_key(at).unwrap());
                        }
                    }
                    queued = sent;
                    drained = true;
                }
                let counted = inbox.state.lock().unwrap().cancelled.clone();
                for id in &keys {
                    let kept = on_the_way(id, queued, read)
                        + usize::from(queue.unmatched.contains(id))
                        + queue.held.get(id).copied().unwrap_or(0);
                    let count = counted.get(id).copied().unwrap_or(0);
                    assert_eq!(count, kept, "run {run}, {id}, {stream:?}, {trace:?}");
                }
                for id in queue.held.keys() {
                    assert!(
                        queue.requests.contains_key(id)
                            || answering
                                .as_ref()
                                .is_some_and(|(answering, ..)| answering == id),
                        "run {run}, {id} is held for no request, {stream:?}, {trace:?}"
                    );
                }
                assert_eq!(queue.unmatched, model.unmatched, "run {run}, {trace:?}");
                assert_eq!(queue.answered, model.answered, "run {run}, {trace:?}");
            }
            // Drained and answered: nothing is kept for a request, and every id still counted is a
            // cancellation kept for a request that may follow.
            assert!(
                queue.held.is_empty() && queue.requests.is_empty(),
                "run {run}"
            );
            assert!(
                model.cancelled.is_empty() && model.queued.is_empty(),
                "run {run}"
            );
            let state = inbox.state.lock().unwrap();
            assert_eq!(state.bytes, 0, "run {run}");
            let unmatched: BTreeMap<String, usize> =
                queue.unmatched.iter().map(|id| (id.clone(), 1)).collect();
            assert_eq!(
                state.cancelled, unmatched,
                "run {run}, {stream:?}, {trace:?}"
            );
            assert_eq!(queue.recent.len(), queue.answered.len(), "run {run}");
        }
    }

    /// Copilot review 2 of PR #119: an id may be as long as a message, so the ids that the main loop
    /// keeps after their requests are answered, or for a request that may follow, are bounded by
    /// their length as well as by their count: long ones are not kept at all. Short ones still are.
    #[test]
    fn long_ids_are_not_remembered() {
        let (inbox, mut queue) = (Inbox::default(), Queue::default());
        let long = |n: usize| json!(format!("{n:0>64000}"));
        for n in 0..64 {
            // As the main loop does: take the request, then answer it.
            arrive(&inbox, &mut queue, symbol_request(long(n)));
            let id = queue.pop(&inbox).unwrap().request_id().unwrap();
            queue.answer(&id, &inbox);
            // A late cancellation of it, and one of an id that no request has.
            arrive(&inbox, &mut queue, cancel(long(n)));
            arrive(&inbox, &mut queue, cancel(long(1000 + n)));
            while queue.pop(&inbox).is_some() {}
        }
        let kept: usize = queue
            .answered
            .iter()
            .chain(&queue.recent)
            .chain(&queue.unmatched)
            .chain(inbox.state.lock().unwrap().cancelled.keys())
            .map(String::len)
            .sum();
        assert_eq!(kept, 0);
        // A late cancellation of an answered request with a short id is dropped, and a cancellation
        // that precedes its request with a short id belongs to it.
        arrive(&inbox, &mut queue, symbol_request(json!(7)));
        let id = queue.pop(&inbox).unwrap().request_id().unwrap();
        queue.answer(&id, &inbox);
        arrive(&inbox, &mut queue, cancel(json!(7)));
        queue.pop(&inbox);
        assert!(!inbox.is_cancelled("7"));
        assert!(queue.recent.contains("7"));
        arrive(&inbox, &mut queue, cancel(json!(8)));
        arrive(&inbox, &mut queue, symbol_request(json!(8)));
        assert!(inbox.is_cancelled("8"));
    }

    /// Copilot review of PR #119: an empty message costs nothing by its body, but it counts against
    /// the reader's limit all the same, so a stream of them makes the reader wait as larger ones do.
    #[test]
    fn empty_messages_count_against_the_readers_limit() {
        let frames = 100_000;
        let input = Cursor::new(b"Content-Length: 0\r\n\r\n".repeat(frames));
        let inbox = Arc::new(Inbox::default());
        let (sender, receiver) = mpsc::channel();
        let reader = {
            let inbox = Arc::clone(&inbox);
            std::thread::spawn(move || read_input(input, &sender, &inbox))
        };
        let mut read = 0;
        while inbox.state.lock().unwrap().bytes <= MAX_QUEUED_BYTES {
            match receiver.recv() {
                Ok(InputEvent::Message(Err(_), _)) => read += 1,
                Ok(InputEvent::Message(Ok(message), _)) => panic!("{message}"),
                Ok(InputEvent::Failure(failure)) => panic!("{failure}"),
                Ok(InputEvent::End) | Err(_) => {
                    panic!("the reader read all {read} empty messages without waiting")
                }
            }
        }
        // The reader waits for room before it reads the next message; closing ends its wait.
        inbox.close();
        reader.join().unwrap();
        let rest: Vec<_> = receiver.try_iter().collect();
        assert!(
            rest.iter()
                .all(|event| matches!(event, InputEvent::Message(Err(_), _)))
        );
        assert!(read + rest.len() < frames, "{}", read + rest.len());
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
        let heads: Vec<_> = function_heads(source, crate::syntax::Edition::LATEST)
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
            let (tokens, diagnostics) =
                crate::lexer::lex_all(keyword, crate::syntax::Edition::LATEST);
            assert!(diagnostics.is_empty());
            assert_eq!(tokens.len(), 2, "{keyword}");
            assert!(!matches!(tokens[0].kind, TokenKind::Ident(_)), "{keyword}");
        }
        let source = include_str!("lexer.rs");
        let body = &source[source.find("fn identifier").unwrap()..];
        let body = &body[..body.find("name => TokenKind::Ident").unwrap()];
        assert_eq!(body.matches("=> TokenKind::").count(), KEYWORDS.len());
    }

    /// PB08 O5: the language server analyzes with the threads of the command line (the available
    /// cores, at most 8, unless `TSUZURI_THREADS` says otherwise), so its stages run on workers.
    #[test]
    fn analysis_uses_the_threads_of_the_command_line() {
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-lsp-threads-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Main.tz"),
            "let values = [1, 2]\nArray.length (ref values)\n",
        )
        .unwrap();
        let project = Project::load_with_overlays(&root, &BTreeMap::new());
        let before = crate::parallel::PARALLEL_STAGES.get();
        let analyzed = project.map(|project| analyze(&project).0.is_some());
        let stages = crate::parallel::PARALLEL_STAGES.get() - before;
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(analyzed.ok(), Some(true));
        if crate::parallel::Parallelism::command_line().workers(usize::MAX, 1) > 1 {
            assert!(stages > 0, "{stages}");
        } else {
            assert_eq!(stages, 0);
        }
    }

    /// PB08 O6: the language server loads the std modules that `check` loads, `sources_for` of the
    /// project's sources with the open buffers in place of their files: the base modules for a
    /// project that names no opt-in module, and `Json` too once an unsaved buffer names it.
    #[test]
    fn refresh_loads_the_std_modules_that_check_loads() {
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-lsp-opt-in-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let saved = "let answer = 42\nanswer\n";
        std::fs::write(root.join("Main.tz"), saved).unwrap();
        std::fs::write(root.join("Other.tz"), "def other :: i64 = 1\n").unwrap();
        let path = root.join("Main.tz");
        let mut session = Session::default();
        session
            .client_roots
            .push(ClientRoot::new(&file_uri(&root).unwrap()).unwrap());
        let loaded = |session: &mut Session| {
            session.refresh(&root, &mut Vec::new()).unwrap();
            session.projects[&root]
                .project
                .sources
                .iter()
                .filter(|source| source.origin == ModuleOrigin::Std)
                .map(|source| source.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        let expected = |texts: &[&str]| {
            crate::stdlib::sources_for(texts.iter().copied())
                .iter()
                .map(|(path, _)| (*path).to_owned())
                .collect::<Vec<_>>()
        };
        let base = loaded(&mut session);
        let edited = "let value = Json.Null\nlet answer = 42\nanswer\n";
        session.buffers.insert(
            path.clone(),
            OpenBuffer {
                uri: file_uri(&path).unwrap(),
                text: edited.to_owned(),
                version: 2,
            },
        );
        let named = loaded(&mut session);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(base, expected(&[saved, "def other :: i64 = 1\n"]));
        assert_eq!(
            base.len(),
            crate::stdlib::SOURCES.len() - crate::stdlib::OPT_IN.len()
        );
        assert_eq!(named, expected(&[edited, "def other :: i64 = 1\n"]));
        assert!(named.iter().any(|path| path == "std/Json.tz"), "{named:?}");
        assert_eq!(named.len(), base.len() + 1);
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
