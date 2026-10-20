//! The resident build server (PB06): `tsuzuri serve DIR` keeps one process per project root, and
//! `tsuzuri check` and `tsuzuri build` ask it to run their command line (see [`request`]) over a
//! Unix domain socket instead of starting a process of their own.
//!
//! The server runs the same `cli` as a command line does, in the same working directory, with the
//! environment variables that the compiler and its tools read equal to the client's, so a request
//! that it takes prints what the command alone prints. Anything else it refuses ([`Refusal`]), and
//! the client then runs the command itself without a word: a server can make a build faster but
//! never different. Where nothing else can be said, the socket is a convenience, not a trust
//! boundary: the directory that holds it is private (mode 0700, owned by the user), which is the
//! only access control, because the peer's credentials cannot be read without `unsafe`.
//!
//! The protocol is one JSON object per line and one request per connection: the server sends a
//! hello (`protocol`, `server`, `version`, `pid`, `root`) when it accepts, the client sends a
//! request and keeps its end open until it has the reply, and the server answers with one reply
//! and closes. One thread accepts the connections and reads the requests; it answers `status` and
//! `shutdown` itself, at once, and queues a request to run for the thread that runs the commands,
//! which skips a queued request whose client has closed its end (it was interrupted while it
//! waited).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::cache::Sha256;

pub const PROTOCOL: u64 = 1;
/// The longest request line (arguments and environment): a command line is a few KiB.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
/// The longest reply line, as long as a message of the language server.
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// How many requests wait for the thread that runs the commands (the one that runs is not counted).
pub const QUEUE: usize = 16;
/// The whole time that a client has to send its request line, from the moment the server reads.
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(2);
/// How long a write of a reply can wait for a client that does not read.
pub const RESPONSE_DEADLINE: Duration = Duration::from_secs(10);
/// How long a client waits for the hello of a server (the accept thread writes it at once).
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_IDLE_SECONDS: u64 = 1800;
pub const MAX_IDLE_SECONDS: u64 = 86400;
#[cfg(unix)]
const HELLO_LIMIT: usize = 4096;
/// How long `serve --status` and `--stop` wait for a reply, which the server makes at once.
#[cfg(unix)]
const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
/// How long `serve --stop` waits for the socket of the server that it stopped to go away.
#[cfg(unix)]
const STOP_WAIT: Duration = Duration::from_secs(5);

/// Why a server will not run a request. The client runs the command itself after any of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Protocol,
    Busy,
    Root,
    Cwd,
    Env,
    Compiler,
    CompilerChanged,
    Unsupported,
    TooLarge,
    Internal,
}

impl Refusal {
    pub const ALL: [Self; 10] = [
        Self::Protocol,
        Self::Busy,
        Self::Root,
        Self::Cwd,
        Self::Env,
        Self::Compiler,
        Self::CompilerChanged,
        Self::Unsupported,
        Self::TooLarge,
        Self::Internal,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Busy => "busy",
            Self::Root => "root",
            Self::Cwd => "cwd",
            Self::Env => "env",
            Self::Compiler => "compiler",
            Self::CompilerChanged => "compiler-changed",
            Self::Unsupported => "unsupported",
            Self::TooLarge => "too-large",
            Self::Internal => "internal",
        }
    }
}

/// A refusal with the reason that goes into the server's log and the reply.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    pub reason: Refusal,
    pub message: String,
}

impl Refused {
    pub fn new(reason: Refusal, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }
}

/// A command line of a client, with what the server compares before it runs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub cwd: PathBuf,
    /// The canonical path of the compiler that the client is.
    pub compiler: PathBuf,
    /// The variables that [`is_relevant_variable`] selects, sorted by name.
    pub env: Vec<(String, String)>,
    pub args: Vec<String>,
}

/// What the command printed and how it ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub exit: u8,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Operation {
    Run(Request),
    Status,
    Shutdown,
}

/// Why a line could not be read.
#[derive(Debug)]
pub enum LineError {
    /// The connection ended before any byte.
    Closed,
    TooLong,
    Io(io::Error),
}

impl fmt::Display for LineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => formatter.write_str("the connection ended without a message"),
            Self::TooLong => formatter.write_str("the message is too long"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

/// Reads up to the first newline, or to the end of the stream, and gives back the line without the
/// newline. The rest of the connection is not read. A line longer than `limit` is an error after at
/// most `limit` plus one chunk of bytes were read.
pub fn read_line_limited(reader: &mut impl Read, limit: usize) -> Result<Vec<u8>, LineError> {
    let mut line = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(LineError::Io(error)),
        };
        let end = chunk[..count].iter().position(|byte| *byte == b'\n');
        line.extend_from_slice(&chunk[..end.unwrap_or(count)]);
        if line.len() > limit {
            return Err(LineError::TooLong);
        }
        if end.is_some() {
            return Ok(line);
        }
    }
    if line.is_empty() {
        Err(LineError::Closed)
    } else {
        Ok(line)
    }
}

fn text<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{key}` is missing or not a string"))
}

fn strings(value: &Value, key: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("`{key}` is not an array"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("`{key}` holds a value that is not a string"))
        })
        .collect()
}

/// Reads a request line. The error is the text of the `protocol` refusal.
pub fn decode_operation(line: &[u8]) -> Result<Operation, String> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|error| format!("the request is not valid JSON ({error})"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "the request is not a JSON object".to_owned())?;
    match object.get("protocol").and_then(Value::as_u64) {
        Some(PROTOCOL) => {}
        Some(other) => {
            return Err(format!(
                "protocol {other} is not supported; this server speaks protocol {PROTOCOL}"
            ));
        }
        None => return Err("`protocol` is missing or not a number".to_owned()),
    }
    match text(object, "op")? {
        "status" => Ok(Operation::Status),
        "shutdown" => Ok(Operation::Shutdown),
        "run" => {
            let env = object
                .get("env")
                .and_then(Value::as_array)
                .ok_or_else(|| "`env` is missing or not an array".to_owned())?
                .iter()
                .map(|pair| match pair.as_array().map(Vec::as_slice) {
                    Some([Value::String(name), Value::String(value)]) => {
                        Ok((name.clone(), value.clone()))
                    }
                    _ => Err("`env` holds an entry that is not a pair of strings".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let args = strings(
                object
                    .get("args")
                    .ok_or_else(|| "`args` is missing".to_owned())?,
                "args",
            )?;
            Ok(Operation::Run(Request {
                cwd: PathBuf::from(text(object, "cwd")?),
                compiler: PathBuf::from(text(object, "compiler")?),
                env,
                args,
            }))
        }
        other => Err(format!("unknown operation {other:?}")),
    }
}

pub fn encode_operation(operation: &Operation) -> String {
    let value = match operation {
        Operation::Status => json!({"protocol": PROTOCOL, "op": "status"}),
        Operation::Shutdown => json!({"protocol": PROTOCOL, "op": "shutdown"}),
        Operation::Run(request) => json!({
            "protocol": PROTOCOL,
            "op": "run",
            "cwd": request.cwd.to_string_lossy(),
            "compiler": request.compiler.to_string_lossy(),
            "env": request.env.iter().map(|(name, value)| json!([name, value])).collect::<Vec<_>>(),
            "args": request.args,
        }),
    };
    line(&value)
}

fn line(value: &Value) -> String {
    let mut text = serde_json::to_string(value).expect("a JSON value is serializable");
    text.push('\n');
    text
}

pub fn encode_response(response: &Response) -> String {
    line(&json!({
        "protocol": PROTOCOL,
        "ok": true,
        "exit": response.exit,
        "stdout": response.stdout,
        "stderr": response.stderr,
    }))
}

pub fn encode_refusal(refused: &Refused) -> String {
    line(&json!({
        "protocol": PROTOCOL,
        "ok": false,
        "error": refused.reason.name(),
        "message": refused.message,
    }))
}

/// The reply to `run`: `None` for a refusal and for anything that is not a reply of this protocol.
pub fn decode_response(line: &[u8]) -> Option<Response> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let object = value.as_object()?;
    if object.get("protocol")?.as_u64()? != PROTOCOL || object.get("ok")?.as_bool() != Some(true) {
        return None;
    }
    Some(Response {
        exit: u8::try_from(object.get("exit")?.as_u64()?).ok()?,
        stdout: object.get("stdout")?.as_str()?.to_owned(),
        stderr: object.get("stderr")?.as_str()?.to_owned(),
    })
}

/// The reply to `status` or `shutdown`: the object when `ok` is true.
pub fn decode_reply(line: &[u8]) -> Option<Map<String, Value>> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let object = value.as_object()?;
    if object.get("protocol")?.as_u64()? != PROTOCOL || object.get("ok")?.as_bool() != Some(true) {
        return None;
    }
    Some(object.clone())
}

pub fn encode_hello(root: &Path) -> String {
    line(&json!({
        "protocol": PROTOCOL,
        "server": "tsuzuri",
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "root": root.to_string_lossy(),
    }))
}

/// Whether `line` is the hello of a server of this protocol for the project at `root`.
pub fn check_hello(line: &[u8], root: &Path) -> bool {
    let Ok(Value::Object(object)) = serde_json::from_slice::<Value>(line) else {
        return false;
    };
    object.get("protocol").and_then(Value::as_u64) == Some(PROTOCOL)
        && object.get("server").and_then(Value::as_str) == Some("tsuzuri")
        && object.get("root").and_then(Value::as_str) == root.to_str()
}

/// The prefixes and names of the variables, besides `cache::KEY_ENVIRONMENT`, that the compiler or
/// the tools that it starts read: `TSUZURI_*` is every setting of the compiler (the ones that later
/// tickets add too), the others choose the cache and the temporary directories, the locale of the
/// messages of the tools, and the SDK, linker, and runtime of Clang, `ld`, and Zig.
const VARIABLE_PREFIXES: &[&str] = &[
    "TSUZURI_", "LD_", "DYLD_", "CLANG_", "CCC_", "ZIG_", "LC_", "LLVM_", "NIX_", "RC_",
];
const VARIABLE_SUFFIXES: &[&str] = &["_DEPLOYMENT_TARGET"];
const VARIABLES: &[&str] = &[
    "HOME",
    "XDG_CACHE_HOME",
    "LOCALAPPDATA",
    "SystemRoot",
    "TMPDIR",
    "TEMP",
    "TMP",
    "TOOLCHAINS",
    "LANG",
    "LANGUAGE",
    "COMPILER_PATH",
    "GCC_EXEC_PREFIX",
    "OBJC_INCLUDE_PATH",
    "ZERO_AR_DATE",
];

/// Whether a server compares the variable `name` with the client's before it runs a request.
/// The other variables of a shell (a terminal's session, the editor, the prompt) never change
/// what a build prints, and would make a server useless in a second terminal.
pub fn is_relevant_variable(name: &str) -> bool {
    crate::cache::KEY_ENVIRONMENT.contains(&name)
        || VARIABLES.contains(&name)
        || VARIABLE_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        || VARIABLE_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

/// The variables of `variables` that [`is_relevant_variable`] selects, sorted by name. `Err` names
/// the variable whose name or value is not UTF-8: the protocol cannot carry it.
pub fn relevant_environment(
    variables: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<Vec<(String, String)>, OsString> {
    let mut selected = BTreeMap::new();
    for (name, value) in variables {
        let Some(utf8_name) = name.to_str() else {
            // A name that is not UTF-8 cannot be one of the names that are compared.
            continue;
        };
        if !is_relevant_variable(utf8_name) {
            continue;
        }
        let Ok(value) = value.into_string() else {
            return Err(name);
        };
        selected.insert(utf8_name.to_owned(), value);
    }
    Ok(selected.into_iter().collect())
}

/// The variables of this process's environment that [`is_relevant_variable`] selects (see
/// [`relevant_environment`]).
pub fn current_environment() -> Result<Vec<(String, String)>, OsString> {
    relevant_environment(std::env::vars_os())
}

/// The first variable that is not the same in `server` and `request` (a variable that only one of
/// them has is different too), with the text of the refusal.
pub fn environment_difference(
    server: &[(String, String)],
    request: &[(String, String)],
) -> Option<String> {
    let server: BTreeMap<_, _> = server.iter().map(|(n, v)| (n.as_str(), v)).collect();
    let request: BTreeMap<_, _> = request.iter().map(|(n, v)| (n.as_str(), v)).collect();
    server
        .keys()
        .chain(request.keys())
        .find(|name| server.get(*name) != request.get(*name))
        .map(|name| format!("{name} differs from the server environment"))
}

/// Why there is no directory for the sockets.
#[derive(Debug, PartialEq, Eq)]
pub enum DirectoryError {
    /// `TSUZURI_SERVER_DIR` is empty: servers are turned off, as `TSUZURI_CACHE_DIR=` turns off the caches.
    Disabled,
    Relative(PathBuf),
    /// No variable names a place (`HOME` is unset), or the platform has no server.
    Unavailable,
}

impl fmt::Display for DirectoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => {
                formatter.write_str("the build server is turned off (TSUZURI_SERVER_DIR is empty)")
            }
            Self::Relative(path) => write!(
                formatter,
                "TSUZURI_SERVER_DIR must be an absolute path, not {}",
                path.display()
            ),
            Self::Unavailable => formatter.write_str(
                "there is no directory for the build server's sockets; set TSUZURI_SERVER_DIR",
            ),
        }
    }
}

/// The directory of the sockets: `TSUZURI_SERVER_DIR`, else under the user's cache (macOS) or
/// runtime (Linux) directory.
pub fn server_directory() -> Result<PathBuf, DirectoryError> {
    let directory = match std::env::var_os("TSUZURI_SERVER_DIR") {
        Some(value) if value.is_empty() => return Err(DirectoryError::Disabled),
        Some(value) => PathBuf::from(value),
        None => default_directory().ok_or(DirectoryError::Unavailable)?,
    };
    if directory.is_absolute() {
        Ok(directory)
    } else {
        Err(DirectoryError::Relative(directory))
    }
}

#[cfg(target_os = "macos")]
fn default_directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches/tsuzuri/server"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_directory() -> Option<PathBuf> {
    let present = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    present("XDG_RUNTIME_DIR")
        .map(|runtime| PathBuf::from(runtime).join("tsuzuri"))
        .or_else(|| {
            present("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| present("HOME").map(|home| PathBuf::from(home).join(".cache")))
                .map(|cache| cache.join("tsuzuri/server"))
        })
}

#[cfg(not(unix))]
fn default_directory() -> Option<PathBuf> {
    None
}

/// The canonical directory of the project that `input` names, by the rules of `Project::load`: a
/// directory is the root itself, a file's parent is (a bare file name's is the working directory).
/// `None` for an input that does not exist, is a symbolic link (`E1011`), or cannot be resolved.
pub fn project_root(input: &Path) -> Option<PathBuf> {
    let metadata = std::fs::symlink_metadata(input).ok()?;
    if metadata.is_symlink() {
        return None;
    }
    let directory = if metadata.is_dir() {
        input
    } else {
        input
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    };
    std::fs::canonicalize(directory).ok()
}

/// The socket of the server for `root` in `directory`: the first 16 hex digits of the SHA-256 of
/// the root's path name it (a short name leaves room in the 104 bytes that a socket path can have
/// on macOS), and the server's hello names the root, so a collision never connects two projects.
pub fn socket_path(directory: &Path, root: &Path) -> PathBuf {
    let mut hash = Sha256::new();
    hash.update(root.as_os_str().as_encoded_bytes());
    let digest = hash.finalize();
    let name: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    directory.join(format!("{name}.sock"))
}

/// Makes sure that `directory` exists and only its owner can use it: it is created with mode 0700;
/// one that exists must be a directory (not a symbolic link) without access for the group and
/// others, and owned by the user that runs this process.
#[cfg(unix)]
pub fn ensure_private_directory(directory: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    let describe =
        |what: &str, error: io::Error| format!("{what} {}: {error}", directory.display());
    match std::fs::symlink_metadata(directory) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)
                .map_err(|error| describe("cannot create", error))?;
        }
        Err(error) => return Err(describe("cannot inspect", error)),
    }
    let metadata =
        std::fs::symlink_metadata(directory).map_err(|error| describe("cannot inspect", error))?;
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{} must be a directory, not a file or a symbolic link",
            directory.display()
        ));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(format!(
            "{} is open to other users (mode {mode:03o}); run chmod 700 on it",
            directory.display()
        ));
    }
    // A file that this process creates is owned by it, which tells whose the directory is.
    let probe = directory.join(format!(".owner-{}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&probe)
        .map_err(|error| describe("cannot write in", error))?;
    let owner = file.metadata().map(|metadata| metadata.uid());
    drop(file);
    let _ = std::fs::remove_file(&probe);
    if owner.map_err(|error| describe("cannot inspect", error))? != metadata.uid() {
        return Err(format!("{} belongs to another user", directory.display()));
    }
    Ok(())
}

/// Whether the directory of the sockets looks private, with one `lstat`: for a client, which
/// connects only when it does. (A directory that other users can enter is refused; one that only
/// another user can enter cannot be entered by this process, and the connection fails.)
#[cfg(unix)]
pub fn is_private_directory(directory: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::symlink_metadata(directory).is_ok_and(|metadata| {
        metadata.is_dir() && !metadata.is_symlink() && metadata.permissions().mode() & 0o077 == 0
    })
}

#[cfg(not(unix))]
pub fn ensure_private_directory(_directory: &Path) -> Result<(), String> {
    Err(NO_SOCKETS.to_owned())
}

#[cfg(not(unix))]
pub fn is_private_directory(_directory: &Path) -> bool {
    false
}

/// What the program that hosts a server provides: the commands themselves live in the binary.
pub trait Handler {
    /// Runs the command line of `request`, which the loop has found to come from this compiler, in
    /// this working directory, with the same environment. It refuses what it does not serve; what
    /// it wants in the server's log it writes to `log`.
    fn run(&mut self, request: &Request, log: &mut dyn Write) -> Result<Response, Refused>;
    /// Whether the compiler file that started the server was replaced since.
    fn compiler_changed(&self) -> bool;
    /// More fields for the reply to `status` (what the process remembers). The server asks after
    /// every request, so that `status` can answer while the next one runs.
    fn status(&self) -> Vec<(&'static str, Value)>;
}

/// What a server needs to know about the process that it is.
pub struct Config {
    /// The private directory of the sockets (see [`ensure_private_directory`]).
    pub directory: PathBuf,
    /// The canonical root of the project that it serves.
    pub root: PathBuf,
    /// How long it waits for a request before it stops.
    pub idle: Duration,
    /// The canonical path of the compiler that it is, which a request has to name.
    pub compiler: PathBuf,
    /// The working directory that it started in, which a request has to be made in.
    pub cwd: PathBuf,
    /// The variables of its environment that [`is_relevant_variable`] selects.
    pub environment: Vec<(String, String)>,
    /// The whole time that a client has to send its request line, from when the server reads it.
    pub request_deadline: Duration,
}

impl Config {
    pub fn new(
        directory: PathBuf,
        root: PathBuf,
        idle: Duration,
        compiler: PathBuf,
        cwd: PathBuf,
        environment: Vec<(String, String)>,
    ) -> Self {
        Self {
            directory,
            root,
            idle,
            compiler,
            cwd,
            environment,
            request_deadline: REQUEST_DEADLINE,
        }
    }
}

/// Why a server ended without an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// No request for the idle time.
    Idle,
    /// `shutdown`, from `tsuzuri serve --stop`.
    Shutdown,
    /// The compiler file was replaced: a server of the old compiler must not go on.
    CompilerChanged,
}

/// A server that could not start or went on no longer: `code` is `E2000` for a setting that the
/// user has to change, `E2001` for the file system.
#[derive(Debug, PartialEq, Eq)]
pub struct ServeError {
    pub code: &'static str,
    pub message: String,
}

#[cfg(not(unix))]
const NO_SOCKETS: &str = "the build server needs Unix domain sockets; use tsuzuri watch, or run check and build without a server";

#[cfg(unix)]
pub use sockets::{control, request};

/// Serves the project of `config` until it has been idle for `config.idle`, `shutdown` is asked,
/// or the compiler file is replaced. What the server wants in its log, and what the handler writes
/// to the `log` that it is given, are whole lines that a thread of their own writes to `log`, one
/// write for each. A writer that blocks (a standard error that nobody reads, a terminal that was
/// stopped) holds that thread and nothing else: no request, `status` or `shutdown` waits for it. A
/// line that does not fit in the queue (256 lines) is dropped and counted (`log_dropped` of
/// `status`, and a line `serve: N log lines dropped` where the writer takes it), and the end of a
/// server waits at most a second for the lines that are queued.
#[cfg(unix)]
pub fn serve(
    config: &Config,
    handler: &mut dyn Handler,
    log: impl Write + Send + 'static,
) -> Result<Stop, ServeError> {
    sockets::serve(config, handler, Box::new(log))
}

#[cfg(not(unix))]
pub fn serve(
    _config: &Config,
    _handler: &mut dyn Handler,
    _log: impl Write + Send + 'static,
) -> Result<Stop, ServeError> {
    Err(ServeError {
        code: "E2000",
        message: NO_SOCKETS.to_owned(),
    })
}

/// Runs the command line `args` of the project at `input` in a server, if one is running for it
/// and takes the request: `None` means that the caller runs the command itself. Nothing is
/// reported for why, so that the output of a command never depends on whether a server exists.
#[cfg(not(unix))]
pub fn request(_input: &Path, _args: &[OsString]) -> Option<Response> {
    None
}

/// Sends `status` or `shutdown` to the server of the project at `input`.
#[cfg(not(unix))]
pub fn control(_input: &Path, _operation: &Operation) -> Result<Map<String, Value>, String> {
    Err(NO_SOCKETS.to_owned())
}

#[cfg(unix)]
mod sockets {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs::{File, OpenOptions, TryLockError};
    use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::thread;
    use std::time::Instant;

    fn usage(message: String) -> ServeError {
        ServeError {
            code: "E2000",
            message,
        }
    }

    fn failure(message: String) -> ServeError {
        ServeError {
            code: "E2001",
            message,
        }
    }

    /// The identity of the file at `path`, which tells a socket that this process made from one that
    /// replaced it.
    fn identity(path: &Path) -> Option<(u64, u64)> {
        std::fs::symlink_metadata(path)
            .ok()
            .map(|metadata| (metadata.dev(), metadata.ino()))
    }

    /// Whether a socket can have the path `socket` (`sun_path` holds 104 bytes on macOS).
    fn check_path(socket: &Path) -> Result<(), ServeError> {
        SocketAddr::from_pathname(socket).map(drop).map_err(|error| {
            usage(format!(
                "the socket path {} is too long for this system ({error}); set TSUZURI_SERVER_DIR to a shorter directory",
                socket.display()
            ))
        })
    }

    /// The lock that a server holds for as long as it lives, in a file beside its socket. The
    /// system gives a lock up when its process ends, however it ends, so a lock that is held is a
    /// server that runs, and what a killed server left is free to take. It makes two servers that
    /// start together one: without it both could remove the same stale socket and bind, and one of
    /// them would end up with a socket that nobody can reach. `None` where the file system has no
    /// locks; `bind` then finds out what it can.
    // `File::try_lock` is stable since Rust 1.89, newer than the `rust-version` of the package (which
    // let chains, stable since 1.88, have passed already), and `std` has no other lock.
    #[allow(clippy::incompatible_msrv)]
    pub(super) fn take_lock(socket: &Path) -> Result<Option<File>, ServeError> {
        let path = socket.with_extension("lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| {
                failure(format!("cannot open the lock {}: {error}", path.display()))
            })?;
        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(TryLockError::WouldBlock) => Err(failure(format!(
                "a build server is already running for this project ({})",
                socket.display()
            ))),
            Err(TryLockError::Error(_)) => Ok(None),
        }
    }

    /// Takes the socket at `socket`: a socket that nobody answers is the leftover of a server that
    /// was killed, and goes; one that answers is a server that runs.
    fn bind(socket: &Path) -> Result<UnixListener, ServeError> {
        for _ in 0..2 {
            match UnixListener::bind(socket) {
                Ok(listener) => {
                    // Only this user can enter the directory, so nobody can connect before this.
                    if let Err(error) =
                        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))
                    {
                        let _ = std::fs::remove_file(socket);
                        return Err(failure(format!(
                            "cannot protect {}: {error}",
                            socket.display()
                        )));
                    }
                    return Ok(listener);
                }
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => {}
                Err(error) => {
                    return Err(failure(format!(
                        "cannot create the socket {}: {error}",
                        socket.display()
                    )));
                }
            }
            let metadata = std::fs::symlink_metadata(socket).map_err(|error| {
                failure(format!("cannot inspect {}: {error}", socket.display()))
            })?;
            if !metadata.file_type().is_socket() {
                return Err(failure(format!(
                    "{} exists and is not a socket",
                    socket.display()
                )));
            }
            match UnixStream::connect(socket) {
                Ok(_) => {
                    return Err(failure(format!(
                        "a build server is already running for this project ({})",
                        socket.display()
                    )));
                }
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    // A server that was killed leaves its socket; nobody answers, so it is removed.
                    let _ = std::fs::remove_file(socket);
                }
                Err(error) => {
                    return Err(failure(format!(
                        "cannot use the socket {}: {error}",
                        socket.display()
                    )));
                }
            }
        }
        Err(failure(format!(
            "cannot take the socket {}: another server took it first",
            socket.display()
        )))
    }

    /// Removes the socket at the end of the server if it is still the one that this server made, and
    /// then lets go of the lock (the field is dropped after `drop` has run).
    struct Cleanup {
        socket: PathBuf,
        made: Option<(u64, u64)>,
        _lock: Option<File>,
    }

    impl Drop for Cleanup {
        fn drop(&mut self) {
            if identity(&self.socket) == self.made {
                let _ = std::fs::remove_file(&self.socket);
            }
        }
    }

    fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// How many lines of the log wait for the thread that writes them. A write to the standard
    /// error can block for ever (a pipe that nobody reads, a terminal that was stopped), and nothing
    /// that serves a client may wait for it: a line that does not fit is dropped and counted.
    pub(super) const LOG_QUEUE: usize = 256;
    /// The longest line of the log, so that what a client sends cannot make the queue large.
    pub(super) const LOG_LINE: usize = 4096;
    /// How long the end of a server waits for the thread of the log to write what is queued.
    const LOG_FLUSH: Duration = Duration::from_secs(1);

    enum Entry {
        /// A line, and how many lines were dropped just before it.
        Line { missed: u64, text: String },
        /// The server has ended: write what is before this, and stop.
        Close,
    }

    #[derive(Default)]
    struct LogCounts {
        /// Dropped since the last line that was queued: the next one carries the number.
        missed: AtomicU64,
        /// Dropped altogether, for `status`.
        total: AtomicU64,
    }

    /// The server's log, for the threads that serve. It never blocks.
    #[derive(Clone)]
    struct Log {
        sender: SyncSender<Entry>,
        counts: Arc<LogCounts>,
    }

    impl Log {
        /// Queues one line, or drops it when the queue is full.
        fn line(&self, text: impl Into<String>) {
            let text = clip(text.into());
            let missed = self.counts.missed.swap(0, Ordering::AcqRel);
            if self.sender.try_send(Entry::Line { missed, text }).is_err() {
                self.counts.missed.fetch_add(missed + 1, Ordering::AcqRel);
                self.counts.total.fetch_add(1, Ordering::AcqRel);
            }
        }

        fn dropped(&self) -> u64 {
            self.counts.total.load(Ordering::Acquire)
        }
    }

    /// `text` as one line of at most [`LOG_LINE`] bytes. A newline in it (a client sends the paths
    /// and names that a line quotes) is written as `\n`, so that it cannot make a line of its own.
    fn clip(text: String) -> String {
        let mut text = if text.contains(['\n', '\r']) {
            text.replace('\n', "\\n").replace('\r', "\\r")
        } else {
            text
        };
        if text.len() > LOG_LINE {
            let mut end = LOG_LINE;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str("...");
        }
        text
    }

    /// The thread that writes the log, and what the end of a server needs of it.
    struct LogThread {
        log: Log,
        done: Option<Receiver<()>>,
    }

    impl LogThread {
        fn start(writer: Box<dyn Write + Send>) -> io::Result<Self> {
            let (sender, receiver) = mpsc::sync_channel(LOG_QUEUE);
            let (finished, done) = mpsc::channel();
            let counts = Arc::new(LogCounts::default());
            let written = Arc::clone(&counts);
            // It is not joined: a writer that is stuck must not keep the process.
            thread::Builder::new()
                .name("tsuzuri-serve-log".to_owned())
                .spawn(move || {
                    write_log(writer, &receiver, &written);
                    let _ = finished.send(());
                })?;
            Ok(Self {
                log: Log { sender, counts },
                done: Some(done),
            })
        }

        /// Asks the thread to write what is queued and stop, and waits at most `wait` for it. A
        /// queue that is full (a writer that is behind, or stuck) takes the request when it has room.
        fn finish(&mut self, wait: Duration) {
            let Some(done) = self.done.take() else {
                return;
            };
            let deadline = Instant::now() + wait;
            loop {
                match self.log.sender.try_send(Entry::Close) {
                    Ok(()) => break,
                    Err(TrySendError::Disconnected(_)) => return,
                    Err(TrySendError::Full(_)) => {
                        if Instant::now() >= deadline {
                            return;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            }
            let _ = done.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        }
    }

    impl Drop for LogThread {
        fn drop(&mut self) {
            self.finish(LOG_FLUSH);
        }
    }

    /// Writes the log: each line in one write, and the number of the lines that were dropped before
    /// one as a line of its own, in the order that all of it was queued in. Ends at `Close`, or when
    /// nothing can send any more.
    fn write_log(
        mut writer: Box<dyn Write + Send>,
        receiver: &Receiver<Entry>,
        counts: &LogCounts,
    ) {
        let mut put = |text: &str| {
            let mut line = Vec::with_capacity(text.len() + 1);
            line.extend_from_slice(text.as_bytes());
            line.push(b'\n');
            let _ = writer.write_all(&line);
            let _ = writer.flush();
        };
        while let Ok(Entry::Line { missed, text }) = receiver.recv() {
            if missed > 0 {
                put(&format!("serve: {missed} log lines dropped"));
            }
            put(&text);
        }
        let missed = counts.missed.swap(0, Ordering::AcqRel);
        if missed > 0 {
            put(&format!("serve: {missed} log lines dropped"));
        }
    }

    /// What a handler writes to the `log` that it is given: its lines, whole, for the log of the server.
    struct HandlerLog<'a> {
        log: &'a Log,
        pending: Vec<u8>,
    }

    impl<'a> HandlerLog<'a> {
        fn new(log: &'a Log) -> Self {
            Self {
                log,
                pending: Vec::new(),
            }
        }

        fn line(&self, bytes: &[u8]) {
            self.log.line(String::from_utf8_lossy(bytes).into_owned());
        }
    }

    impl Write for HandlerLog<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.pending.extend_from_slice(bytes);
            while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let rest = self.pending.split_off(end + 1);
                let line = std::mem::replace(&mut self.pending, rest);
                self.line(&line[..end]);
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Drop for HandlerLog<'_> {
        fn drop(&mut self) {
            if !self.pending.is_empty() {
                self.line(&self.pending);
            }
        }
    }

    /// What `status` reports, and what the accept thread and the thread that runs the commands
    /// both change.
    struct Counters {
        started: Instant,
        requests: u64,
        served: u64,
        /// Queued requests whose client had gone when their turn came.
        abandoned: u64,
        refusals: BTreeMap<&'static str, u64>,
        /// The request that runs, and since when.
        running: Option<(u64, Instant)>,
        /// The last connection, or the end of the last request: the idle time counts from here.
        activity: Instant,
        /// What the handler said about the process after the last request that ended.
        handler: Vec<(&'static str, Value)>,
    }

    /// What the two threads of a server share. The accept thread answers a `status` or a
    /// `shutdown` itself, so that they do not wait for the commands that run.
    struct Shared {
        counters: Mutex<Counters>,
        /// Requests that wait for the thread that runs the commands (the one that runs is not here).
        queued: AtomicUsize,
        /// Connections that were refused because the queue was full.
        busy: AtomicU64,
        /// `shutdown` was asked: the server ends when the request that runs ends.
        shutdown: AtomicBool,
        /// The server has ended: the accept thread ends at its next connection.
        stop: AtomicBool,
    }

    impl Shared {
        fn new(handler: Vec<(&'static str, Value)>) -> Self {
            let now = Instant::now();
            Self {
                counters: Mutex::new(Counters {
                    started: now,
                    requests: 0,
                    served: 0,
                    abandoned: 0,
                    refusals: BTreeMap::new(),
                    running: None,
                    activity: now,
                    handler,
                }),
                queued: AtomicUsize::new(0),
                busy: AtomicU64::new(0),
                shutdown: AtomicBool::new(false),
                stop: AtomicBool::new(false),
            }
        }

        fn counters(&self) -> MutexGuard<'_, Counters> {
            locked(&self.counters)
        }

        /// The number of the request that has just been read.
        fn next_request(&self) -> u64 {
            let mut counters = self.counters();
            counters.requests += 1;
            counters.requests
        }

        /// Counts a refusal, and gives the line for the log.
        fn refused(&self, number: u64, refused: &Refused) -> String {
            *self
                .counters()
                .refusals
                .entry(refused.reason.name())
                .or_default() += 1;
            format!(
                "serve: request {number} refused: {} ({})",
                refused.reason.name(),
                refused.message
            )
        }

        /// The reply to `status` (the fields that the handler reports are those of the last request
        /// that ended).
        fn status(&self, info: &Acceptor) -> String {
            let busy = self.busy.load(Ordering::Acquire);
            let counters = self.counters();
            let mut refusals: Map<String, Value> = counters
                .refusals
                .iter()
                .map(|(name, count)| ((*name).to_owned(), json!(count)))
                .collect();
            if busy > 0 {
                refusals.insert("busy".to_owned(), json!(busy));
            }
            let refused: u64 = counters.refusals.values().sum::<u64>() + busy;
            let mut object = Map::new();
            for (key, value) in [
                ("protocol", json!(PROTOCOL)),
                ("ok", json!(true)),
                ("server", json!("tsuzuri")),
                ("version", json!(env!("CARGO_PKG_VERSION"))),
                ("pid", json!(std::process::id())),
                ("root", json!(info.root)),
                ("compiler", json!(info.compiler)),
                ("compiler_sha256", json!(info.compiler_sha256)),
                ("idle_seconds", json!(info.idle.as_secs())),
                (
                    "uptime_seconds",
                    json!(counters.started.elapsed().as_secs()),
                ),
                ("requests", json!(counters.requests)),
                ("served", json!(counters.served)),
                ("abandoned", json!(counters.abandoned)),
                ("refused", json!(refused)),
                ("refusals", Value::Object(refusals)),
                ("running", running_json(counters.running)),
                ("queued", json!(self.queued.load(Ordering::Acquire))),
                ("log_dropped", json!(info.log.dropped())),
            ] {
                object.insert(key.to_owned(), value);
            }
            for (key, value) in &counters.handler {
                object.insert((*key).to_owned(), value.clone());
            }
            super::line(&Value::Object(object))
        }
    }

    /// The request that runs, for a reply: its number and for how long, or `null`.
    fn running_json(running: Option<(u64, Instant)>) -> Value {
        running.map_or(
            Value::Null,
            |(request, since)| json!({"request": request, "seconds": since.elapsed().as_secs()}),
        )
    }

    /// A request for the thread that runs the commands.
    struct Job {
        stream: UnixStream,
        request: Request,
        number: u64,
    }

    /// What the accept thread passes to the thread that runs the commands.
    enum Message {
        Run(Job),
        /// `shutdown` was asked.
        Shutdown,
    }

    /// What the accept thread knows of the server, so that it can answer without the other thread.
    struct Acceptor {
        hello: String,
        root: String,
        compiler: String,
        compiler_sha256: String,
        idle: Duration,
        request_deadline: Duration,
        log: Log,
    }

    /// Accepts connections, says hello at once (so that a client that waits in the queue is not
    /// timed out by a command that runs), reads the request, and answers what does not need the
    /// thread that runs the commands; a request to run goes into the queue for that thread.
    fn accept_loop(
        listener: UnixListener,
        sender: &Sender<Message>,
        shared: &Shared,
        info: &Acceptor,
    ) {
        for incoming in listener.incoming() {
            if shared.stop.load(Ordering::Acquire) {
                break;
            }
            let Ok(mut stream) = incoming else {
                thread::sleep(Duration::from_millis(10));
                continue;
            };
            shared.counters().activity = Instant::now();
            let _ = stream.set_write_timeout(Some(RESPONSE_DEADLINE));
            if stream.write_all(info.hello.as_bytes()).is_err() {
                continue;
            }
            if !receive(stream, sender, shared, info) {
                break;
            }
        }
    }

    /// Writes `text` to the client of request `number`. A client that went away is not an error of
    /// the server.
    fn reply(stream: &mut UnixStream, text: &str, number: u64, log: &Log) {
        if let Err(error) = stream.write_all(text.as_bytes()) {
            log.line(format!(
                "serve: request {number}: the reply could not be written ({error})"
            ));
        }
    }

    /// Reads one request. `false` when the thread that runs the commands has ended.
    fn receive(
        mut stream: UnixStream,
        sender: &Sender<Message>,
        shared: &Shared,
        info: &Acceptor,
    ) -> bool {
        let read = read_line_limited(
            &mut DeadlineReader {
                stream: &stream,
                deadline: Instant::now() + info.request_deadline,
            },
            MAX_REQUEST_BYTES,
        );
        let refuse = |stream: &mut UnixStream, number: u64, refused: &Refused| {
            info.log.line(shared.refused(number, refused));
            reply(stream, &encode_refusal(refused), number, &info.log);
        };
        let line = match read {
            Ok(line) => line,
            // A client that connected to see whether a server runs, or gave up.
            Err(LineError::Closed) => return true,
            Err(LineError::TooLong) => {
                let refused = Refused::new(
                    Refusal::TooLarge,
                    format!("the request is longer than {MAX_REQUEST_BYTES} bytes"),
                );
                refuse(&mut stream, shared.next_request(), &refused);
                drain(&stream);
                return true;
            }
            Err(LineError::Io(error)) => {
                info.log
                    .line(format!("serve: a request could not be read ({error})"));
                return true;
            }
        };
        let number = shared.next_request();
        match decode_operation(&line) {
            Err(message) => refuse(
                &mut stream,
                number,
                &Refused::new(Refusal::Protocol, message),
            ),
            Ok(Operation::Status) => {
                reply(&mut stream, &shared.status(info), number, &info.log);
            }
            Ok(Operation::Shutdown) => {
                shared.shutdown.store(true, Ordering::Release);
                let running = running_json(shared.counters().running);
                let text = super::line(
                    &json!({"protocol": PROTOCOL, "ok": true, "pid": std::process::id(), "running": running}),
                );
                reply(&mut stream, &text, number, &info.log);
                return sender.send(Message::Shutdown).is_ok();
            }
            Ok(Operation::Run(request)) => {
                if shared.queued.load(Ordering::Acquire) >= QUEUE {
                    shared.busy.fetch_add(1, Ordering::AcqRel);
                    let refused = Refused::new(
                        Refusal::Busy,
                        format!("{QUEUE} requests are waiting for the server"),
                    );
                    reply(&mut stream, &encode_refusal(&refused), number, &info.log);
                } else {
                    shared.queued.fetch_add(1, Ordering::AcqRel);
                    let job = Job {
                        stream,
                        request,
                        number,
                    };
                    return sender.send(Message::Run(job)).is_ok();
                }
            }
        }
        true
    }

    /// Whether the client has closed its end of the connection. A client keeps its end open until
    /// it has read the reply, so a request that waited and finds it closed was left by a client that
    /// has gone (it was interrupted while it waited), and nobody would read what it makes.
    fn client_gone(stream: &UnixStream) -> bool {
        if stream.set_nonblocking(true).is_err() {
            return false;
        }
        let mut byte = [0_u8; 1];
        let mut reader = stream;
        let gone = match reader.read(&mut byte) {
            Ok(0) => true,
            Ok(_) => false,
            Err(error) => !matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ),
        };
        let _ = stream.set_nonblocking(false);
        gone
    }

    /// A reader of the request line that gives up when the client is slow: the deadline is for the
    /// whole line, not for each read.
    struct DeadlineReader<'a> {
        stream: &'a UnixStream,
        deadline: Instant,
    }

    impl Read for DeadlineReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let left = self.deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the request did not arrive in time",
                ));
            }
            match self.stream.set_read_timeout(Some(left)) {
                Ok(()) => {}
                // macOS refuses the option on a socket whose peer has closed; the read that
                // follows returns what the peer sent, and then ends.
                Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
                Err(error) => return Err(error),
            }
            let mut stream = self.stream;
            match stream.read(buffer) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the request did not arrive in time",
                )),
                other => other,
            }
        }
    }

    /// Reads and drops, for a moment (a second at most), what a client that sent too much is still
    /// sending: a socket closed with unread data resets the connection, and the client may lose the
    /// refusal.
    fn drain(stream: &UnixStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
        let mut sink = [0_u8; 65536];
        let mut reader = stream;
        let mut total = 0;
        let started = Instant::now();
        while total < 4 * MAX_REQUEST_BYTES && started.elapsed() < Duration::from_secs(1) {
            match reader.read(&mut sink) {
                Ok(0) | Err(_) => break,
                Ok(count) => total += count,
            }
        }
    }

    /// The thread that runs the commands.
    struct Loop<'a> {
        config: &'a Config,
        handler: &'a mut dyn Handler,
        log: &'a Log,
        shared: &'a Shared,
        busy_logged: u64,
    }

    pub fn serve(
        config: &Config,
        handler: &mut dyn Handler,
        writer: Box<dyn Write + Send>,
    ) -> Result<Stop, ServeError> {
        let socket = socket_path(&config.directory, &config.root);
        check_path(&socket)?;
        let lock = take_lock(&socket)?;
        let listener = bind(&socket)?;
        let made = identity(&socket);
        let locks = lock.is_some();
        let cleanup = Cleanup {
            made,
            socket: socket.clone(),
            _lock: lock,
        };
        let compiler_sha256 = crate::cache::file_digest(&config.compiler).map_or_else(
            |_| "unknown".to_owned(),
            |digest| digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        );
        let mut logger = LogThread::start(writer)
            .map_err(|error| failure(format!("cannot start the thread of the log: {error}")))?;
        let log = logger.log.clone();
        log.line(format!(
            "serve: listening on {} for {}",
            socket.display(),
            config.root.display()
        ));
        if !locks {
            log.line(format!(
                "serve: the file system of {} has no locks; two servers that start together could both run",
                config.directory.display()
            ));
        }
        let (sender, receiver) = mpsc::channel();
        let shared = Arc::new(Shared::new(handler.status()));
        let info = Acceptor {
            hello: encode_hello(&config.root),
            root: config.root.to_string_lossy().into_owned(),
            compiler: config.compiler.to_string_lossy().into_owned(),
            compiler_sha256,
            idle: config.idle,
            request_deadline: config.request_deadline,
            log: log.clone(),
        };
        let acceptor = {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("tsuzuri-serve-accept".to_owned())
                .spawn(move || accept_loop(listener, &sender, &shared, &info))
                .map_err(|error| failure(format!("cannot start the accept thread: {error}")))?
        };
        let mut running = Loop {
            config,
            handler,
            log: &log,
            shared: &shared,
            busy_logged: 0,
        };
        let outcome = running.run(&receiver);
        // The accept thread waits in `accept`: a connection to the socket wakes it. If the socket
        // is not this server's any more, a connection would wake somebody else's: the thread is left
        // to end with the process.
        shared.stop.store(true, Ordering::Release);
        if identity(&socket) == made && UnixStream::connect(&socket).is_ok() {
            let _ = acceptor.join();
        }
        // The socket and the lock go first, so that nobody who waits for the server to stop waits
        // for its log, which a writer that is stuck can hold for as long as it likes.
        drop(cleanup);
        logger.finish(LOG_FLUSH);
        outcome
    }

    impl Loop<'_> {
        fn run(&mut self, receiver: &Receiver<Message>) -> Result<Stop, ServeError> {
            self.messages(receiver)
        }

        fn messages(&mut self, receiver: &Receiver<Message>) -> Result<Stop, ServeError> {
            loop {
                if self.shared.shutdown.load(Ordering::Acquire) {
                    return Ok(self.shutdown());
                }
                let activity = self.shared.counters().activity;
                let wait = (activity + self.config.idle).saturating_duration_since(Instant::now());
                match receiver.recv_timeout(wait) {
                    Ok(Message::Shutdown) => return Ok(self.shutdown()),
                    Ok(Message::Run(job)) => {
                        self.shared.queued.fetch_sub(1, Ordering::AcqRel);
                        self.log_busy();
                        let stop = self.job(job);
                        self.log_busy();
                        if let Some(stop) = stop {
                            return Ok(stop);
                        }
                        self.shared.counters().activity = Instant::now();
                    }
                    // A connection since then moved the end of the idle time.
                    Err(RecvTimeoutError::Timeout) => {
                        let activity = self.shared.counters().activity;
                        if activity + self.config.idle <= Instant::now() {
                            self.log.line(format!(
                                "serve: idle for {} s; stopping",
                                self.config.idle.as_secs()
                            ));
                            return Ok(Stop::Idle);
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(failure("the accept thread ended".to_owned()));
                    }
                }
            }
        }

        /// `shutdown` was asked: the request that ran has ended, and the ones that wait do not run.
        fn shutdown(&mut self) -> Stop {
            self.log.line("serve: shutdown requested; stopping");
            let waiting = self.shared.queued.load(Ordering::Acquire);
            if waiting > 0 {
                self.log.line(format!(
                    "serve: {waiting} waiting request(s) dropped; their clients run the command themselves"
                ));
            }
            Stop::Shutdown
        }

        fn log_busy(&mut self) {
            let busy = self.shared.busy.load(Ordering::Acquire);
            if busy > self.busy_logged {
                self.log.line(format!(
                    "serve: {} connection(s) refused: busy ({QUEUE} requests were waiting)",
                    busy - self.busy_logged
                ));
                self.busy_logged = busy;
            }
        }

        /// Writes `text` to the client. A client that went away is not an error of the server.
        fn send(&mut self, stream: &mut UnixStream, text: &str, number: u64) {
            reply(stream, text, number, self.log);
        }

        fn refuse(&mut self, stream: &mut UnixStream, number: u64, refused: &Refused) {
            self.log.line(self.shared.refused(number, refused));
            self.send(stream, &encode_refusal(refused), number);
        }

        /// One request that was queued. `Some` ends the server.
        fn job(&mut self, job: Job) -> Option<Stop> {
            let Job {
                mut stream,
                request,
                number,
            } = job;
            if client_gone(&stream) {
                self.shared.counters().abandoned += 1;
                self.log.line(format!(
                    "serve: request {number} skipped: its client went away while it waited"
                ));
                return None;
            }
            self.command(&mut stream, &request, number)
        }

        fn command(
            &mut self,
            stream: &mut UnixStream,
            request: &Request,
            number: u64,
        ) -> Option<Stop> {
            if self.handler.compiler_changed() {
                let refused = Refused::new(
                    Refusal::CompilerChanged,
                    "the compiler file was replaced since the server started",
                );
                self.refuse(stream, number, &refused);
                self.log.line(
                    "serve: the compiler binary changed; stopping (start tsuzuri serve again)",
                );
                return Some(Stop::CompilerChanged);
            }
            let mismatch = if request.compiler != self.config.compiler {
                Some(Refused::new(
                    Refusal::Compiler,
                    format!(
                        "the client is {} and the server is {}",
                        request.compiler.display(),
                        self.config.compiler.display()
                    ),
                ))
            } else if request.cwd != self.config.cwd {
                Some(Refused::new(
                    Refusal::Cwd,
                    format!(
                        "the client works in {} and the server in {}",
                        request.cwd.display(),
                        self.config.cwd.display()
                    ),
                ))
            } else {
                environment_difference(&self.config.environment, &request.env)
                    .map(|message| Refused::new(Refusal::Env, message))
            };
            if let Some(refused) = mismatch {
                self.refuse(stream, number, &refused);
                return None;
            }
            self.shared.counters().running = Some((number, Instant::now()));
            let mut handler_log = HandlerLog::new(self.log);
            let result = self.handler.run(request, &mut handler_log);
            drop(handler_log);
            // What `status` says is up to date when the client has its reply, so it is written
            // before the reply is sent.
            let remembered = self.handler.status();
            {
                let mut counters = self.shared.counters();
                counters.running = None;
                counters.handler = remembered;
            }
            match result {
                Ok(response) => {
                    let reply = encode_response(&response);
                    if reply.len() > MAX_RESPONSE_BYTES {
                        let refused = Refused::new(
                            Refusal::TooLarge,
                            format!("the output is longer than {MAX_RESPONSE_BYTES} bytes"),
                        );
                        self.refuse(stream, number, &refused);
                    } else {
                        self.shared.counters().served += 1;
                        self.send(stream, &reply, number);
                        self.log.line(format!(
                            "serve: request {number} served (exit {})",
                            response.exit
                        ));
                    }
                }
                Err(refused) => self.refuse(stream, number, &refused),
            }
            None
        }
    }

    /// Connects, checks the hello, sends `operation`, and reads the reply line. `Err` says which step
    /// failed. The request ends with a newline, which is all that the server waits for, and the
    /// client keeps its end of the connection open until it has the reply: a server takes a request
    /// whose client has closed its end for one that was abandoned while it waited (and a server that
    /// has answered and closed makes `shutdown` of the write side fail on macOS). A `status` and a
    /// `shutdown` are answered at once, so they wait [`CONTROL_TIMEOUT`] for the reply; a command
    /// waits for the commands before it, and for itself, as long as that takes.
    pub(super) fn exchange(
        socket: &Path,
        root: &Path,
        operation: &Operation,
        limit: usize,
    ) -> Result<Vec<u8>, String> {
        exchange_within(socket, root, operation, limit, CONTROL_TIMEOUT)
    }

    pub(super) fn exchange_within(
        socket: &Path,
        root: &Path,
        operation: &Operation,
        limit: usize,
        control_timeout: Duration,
    ) -> Result<Vec<u8>, String> {
        let mut stream = UnixStream::connect(socket)
            .map_err(|error| format!("cannot connect to {} ({error})", socket.display()))?;
        stream
            .set_read_timeout(Some(HELLO_TIMEOUT))
            .map_err(|error| format!("cannot wait for the hello ({error})"))?;
        let hello = read_line_limited(&mut &stream, HELLO_LIMIT)
            .map_err(|error| format!("the server did not say hello ({error})"))?;
        if !check_hello(&hello, root) {
            return Err("the hello is not that of a build server for this project".to_owned());
        }
        let wait = match operation {
            Operation::Run(_) => None,
            Operation::Status | Operation::Shutdown => Some(control_timeout),
        };
        stream
            .set_read_timeout(wait)
            .map_err(|error| format!("cannot wait for the reply ({error})"))?;
        stream
            .write_all(encode_operation(operation).as_bytes())
            .map_err(|error| format!("the server did not take the request ({error})"))?;
        read_line_limited(&mut &stream, limit).map_err(|error| match error {
            LineError::Io(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                format!(
                    "the server did not reply within {} s",
                    control_timeout.as_secs_f32()
                )
            }
            other => format!("the server did not reply ({other})"),
        })
    }

    /// The socket of the server for the project at `input`, if there is one to try: the cheapest
    /// checks come first, since this runs at the start of every `check` and `build`.
    fn find(input: &Path) -> Option<(PathBuf, PathBuf)> {
        let directory = server_directory().ok()?;
        if !is_private_directory(&directory) {
            return None;
        }
        let root = project_root(input)?;
        let socket = socket_path(&directory, &root);
        std::fs::symlink_metadata(&socket)
            .ok()?
            .file_type()
            .is_socket()
            .then_some((socket, root))
    }

    pub fn request(input: &Path, args: &[OsString]) -> Option<Response> {
        let (socket, root) = find(input)?;
        let request = Request {
            cwd: std::env::current_dir().ok()?,
            compiler: std::fs::canonicalize(std::env::current_exe().ok()?).ok()?,
            env: current_environment().ok()?,
            args: args
                .iter()
                .map(|argument| argument.to_str().map(str::to_owned))
                .collect::<Option<_>>()?,
        };
        // The protocol carries paths as text.
        request.cwd.to_str()?;
        request.compiler.to_str()?;
        let reply = exchange(&socket, &root, &Operation::Run(request), MAX_RESPONSE_BYTES).ok()?;
        decode_response(&reply)
    }

    pub fn control(input: &Path, operation: &Operation) -> Result<Map<String, Value>, String> {
        let directory = server_directory().map_err(|error| error.to_string())?;
        let root = project_root(input).ok_or_else(|| {
            format!(
                "cannot find the project of {}: it must be an existing project directory or file, not a symbolic link",
                input.display()
            )
        })?;
        let socket = socket_path(&directory, &root);
        let none = |why: &str| format!("no build server is running for {}{why}", root.display());
        if !is_private_directory(&directory)
            || !std::fs::symlink_metadata(&socket).is_ok_and(|m| m.file_type().is_socket())
        {
            return Err(none(""));
        }
        let reply = exchange(&socket, &root, operation, MAX_RESPONSE_BYTES)
            .map_err(|why| none(&format!(" ({why})")))?;
        let reply = decode_reply(&reply).ok_or_else(|| {
            format!(
                "the build server for {} did not accept the request",
                root.display()
            )
        })?;
        if *operation == Operation::Shutdown {
            // A server that runs a request stops when that request ends, which can be long: the
            // reply says so, and there is no point in waiting for the socket to go.
            if let Some(running) = reply.get("running").filter(|running| !running.is_null()) {
                let field = |name: &str| running.get(name).and_then(Value::as_u64).unwrap_or(0);
                return Err(format!(
                    "the build server for {} (pid {}) is running request {} (for {} s) and stops when it ends",
                    root.display(),
                    reply.get("pid").and_then(Value::as_u64).unwrap_or(0),
                    field("request"),
                    field("seconds")
                ));
            }
            let started = Instant::now();
            while std::fs::symlink_metadata(&socket).is_ok() {
                if started.elapsed() > STOP_WAIT {
                    return Err(format!(
                        "the build server for {} did not stop within {} s",
                        root.display(),
                        STOP_WAIT.as_secs()
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::TemporaryDirectory;
    use std::collections::BTreeSet;
    use std::io::Cursor;

    fn request() -> Request {
        Request {
            cwd: PathBuf::from("/work"),
            compiler: PathBuf::from("/work/target/release/tsuzuri"),
            env: vec![
                ("HOME".to_owned(), "/Users/me".to_owned()),
                ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            ],
            args: vec!["check".to_owned(), "app".to_owned()],
        }
    }

    #[test]
    fn protocol_roundtrips_and_rejects_invalid_messages() {
        for operation in [
            Operation::Run(request()),
            Operation::Status,
            Operation::Shutdown,
        ] {
            let encoded = encode_operation(&operation);
            assert!(encoded.ends_with('\n'));
            assert_eq!(
                decode_operation(encoded.trim_end().as_bytes()).unwrap(),
                operation
            );
        }
        let response = Response {
            exit: 1,
            stdout: String::new(),
            stderr: "error: \"quoted\"\n  1 | 日本語\n".to_owned(),
        };
        let encoded = encode_response(&response);
        assert_eq!(encoded.matches('\n').count(), 1, "one message is one line");
        assert_eq!(
            decode_response(encoded.trim_end().as_bytes()),
            Some(response)
        );
        let refusal = encode_refusal(&Refused::new(Refusal::Env, "PATH differs"));
        assert_eq!(decode_response(refusal.trim_end().as_bytes()), None);
        assert!(refusal.contains("\"error\":\"env\""));

        let run = |extra: &str| {
            format!(
                "{{\"protocol\":1,\"op\":\"run\",\"cwd\":\"/w\",\"compiler\":\"/c\",\"env\":[],\"args\":[\"check\"]{extra}}}"
            )
        };
        assert!(decode_operation(run("").as_bytes()).is_ok());
        assert!(
            decode_operation(run(",\"future\":[1,2]").as_bytes()).is_ok(),
            "a field that this protocol does not know is ignored"
        );
        for broken in [
            "",
            "not json",
            "[]",
            "\"run\"",
            "{}",
            "{\"protocol\":2,\"op\":\"status\"}",
            "{\"protocol\":\"1\",\"op\":\"status\"}",
            "{\"protocol\":1}",
            "{\"protocol\":1,\"op\":\"restart\"}",
            "{\"protocol\":1,\"op\":3}",
            "{\"protocol\":1,\"op\":\"run\",\"cwd\":\"/w\",\"compiler\":\"/c\",\"env\":[]}",
            "{\"protocol\":1,\"op\":\"run\",\"cwd\":\"/w\",\"compiler\":\"/c\",\"env\":[],\"args\":\"check\"}",
            "{\"protocol\":1,\"op\":\"run\",\"cwd\":\"/w\",\"compiler\":\"/c\",\"env\":[],\"args\":[1]}",
            "{\"protocol\":1,\"op\":\"run\",\"cwd\":\"/w\",\"compiler\":\"/c\",\"env\":[[\"A\"]],\"args\":[]}",
            "{\"protocol\":1,\"op\":\"run\",\"cwd\":\"/w\",\"compiler\":\"/c\",\"env\":[[\"A\",1]],\"args\":[]}",
            "{\"protocol\":1,\"op\":\"run\",\"cwd\":\"/w\",\"compiler\":\"/c\",\"env\":{\"A\":\"B\"},\"args\":[]}",
            "{\"protocol\":1,\"op\":\"run\",\"compiler\":\"/c\",\"env\":[],\"args\":[]}",
            "{\"protocol\":1,\"op\":\"run\",\"cwd\":7,\"compiler\":\"/c\",\"env\":[],\"args\":[]}",
        ] {
            assert!(decode_operation(broken.as_bytes()).is_err(), "{broken}");
        }
        assert!(decode_operation(&[0xff, 0xfe]).is_err());
        let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(decode_operation(deep.as_bytes()).is_err());

        for broken in [
            "",
            "{}",
            "{\"protocol\":1,\"ok\":true}",
            "{\"protocol\":2,\"ok\":true,\"exit\":0,\"stdout\":\"\",\"stderr\":\"\"}",
            "{\"protocol\":1,\"ok\":true,\"exit\":256,\"stdout\":\"\",\"stderr\":\"\"}",
            "{\"protocol\":1,\"ok\":true,\"exit\":-1,\"stdout\":\"\",\"stderr\":\"\"}",
            "{\"protocol\":1,\"ok\":true,\"exit\":0,\"stdout\":1,\"stderr\":\"\"}",
            "{\"protocol\":1,\"ok\":false,\"exit\":0,\"stdout\":\"\",\"stderr\":\"\"}",
        ] {
            assert_eq!(decode_response(broken.as_bytes()), None, "{broken}");
        }
    }

    #[test]
    fn the_hello_names_the_protocol_and_the_root() {
        let hello = encode_hello(Path::new("/work/app"));
        let hello = hello.trim_end().as_bytes();
        assert!(check_hello(hello, Path::new("/work/app")));
        assert!(!check_hello(hello, Path::new("/work/other")));
        assert!(!check_hello(b"", Path::new("/work/app")));
        assert!(!check_hello(
            b"{\"protocol\":2,\"server\":\"tsuzuri\",\"root\":\"/work/app\"}",
            Path::new("/work/app")
        ));
        assert!(!check_hello(
            b"{\"protocol\":1,\"server\":\"other\",\"root\":\"/work/app\"}",
            Path::new("/work/app")
        ));
    }

    #[test]
    fn lines_are_read_up_to_the_limit() {
        let read = |input: &[u8], limit| read_line_limited(&mut Cursor::new(input.to_vec()), limit);
        assert_eq!(read(b"abc\nrest", 16).unwrap(), b"abc");
        assert_eq!(
            read(b"abc", 16).unwrap(),
            b"abc",
            "the end of the stream ends a line"
        );
        assert_eq!(
            read(b"abcd\n", 4).unwrap(),
            b"abcd",
            "the limit is the line without its newline"
        );
        assert!(matches!(read(b"abcde\n", 4), Err(LineError::TooLong)));
        assert!(matches!(read(b"abcde", 4), Err(LineError::TooLong)));
        assert!(matches!(read(b"", 4), Err(LineError::Closed)));
        assert_eq!(read(b"\n", 4).unwrap(), b"", "an empty line is a line");
        let long = vec![b'x'; 3 * 8192];
        assert!(matches!(read(&long, 8192), Err(LineError::TooLong)));
        let mut reader = Cursor::new(vec![b'y'; 1 << 20]);
        assert!(matches!(
            read_line_limited(&mut reader, 100),
            Err(LineError::TooLong)
        ));
        assert!(
            reader.position() <= 8192,
            "a line past the limit stops the reading at one chunk"
        );
    }

    #[test]
    fn socket_path_is_short_stable_and_root_specific() {
        let directory = Path::new("/cache/server");
        let first = socket_path(directory, Path::new("/work/app"));
        assert_eq!(first, socket_path(directory, Path::new("/work/app")));
        assert_ne!(first, socket_path(directory, Path::new("/work/other")));
        assert_eq!(first.parent(), Some(directory));
        let name = first.file_name().unwrap().to_str().unwrap();
        let (stem, extension) = name.split_once('.').unwrap();
        assert_eq!(extension, "sock");
        assert_eq!(stem.len(), 16);
        assert!(
            stem.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert_eq!(
            socket_path(directory, Path::new("/work/app")),
            Path::new("/cache/server/70467eff2e0a2364.sock"),
            "the first 16 hex digits of the SHA-256 of the path"
        );
    }

    #[test]
    fn the_environment_comparison_looks_only_at_the_variables_that_matter() {
        let relevant = relevant_environment(
            [
                ("PATH", "/usr/bin"),
                ("HOME", "/Users/me"),
                ("TSUZURI_THREADS", "2"),
                ("TSUZURI_FUTURE_SETTING", "x"),
                ("MACOSX_DEPLOYMENT_TARGET", "14.0"),
                ("DYLD_LIBRARY_PATH", "/lib"),
                ("LC_ALL", "C"),
                ("TERM_SESSION_ID", "w0t0p0"),
                ("SHLVL", "2"),
                ("_", "/usr/bin/env"),
                ("OLDPWD", "/"),
                ("EDITOR", "vi"),
            ]
            .map(|(name, value)| (OsString::from(name), OsString::from(value))),
        )
        .unwrap();
        let names: Vec<_> = relevant.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "DYLD_LIBRARY_PATH",
                "HOME",
                "LC_ALL",
                "MACOSX_DEPLOYMENT_TARGET",
                "PATH",
                "TSUZURI_FUTURE_SETTING",
                "TSUZURI_THREADS"
            ],
            "a variable that a later ticket adds under TSUZURI_ is compared without a change here"
        );

        assert_eq!(environment_difference(&relevant, &relevant), None);
        let mut other = relevant.clone();
        other[4].1 = "/usr/local/bin".to_owned();
        assert_eq!(
            environment_difference(&relevant, &other).as_deref(),
            Some("PATH differs from the server environment")
        );
        other = relevant.clone();
        other.push(("TSUZURI_NEW".to_owned(), "1".to_owned()));
        assert_eq!(
            environment_difference(&relevant, &other).as_deref(),
            Some("TSUZURI_NEW differs from the server environment")
        );
        other = relevant.clone();
        other.remove(1);
        assert_eq!(
            environment_difference(&relevant, &other).as_deref(),
            Some("HOME differs from the server environment")
        );
        assert_eq!(environment_difference(&[], &[]), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_variable_that_is_not_utf8_cannot_be_sent() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![0x66, 0xff]);
        let result = relevant_environment([(OsString::from("PATH"), bad)]);
        assert_eq!(result, Err(OsString::from("PATH")));
        let ignored =
            relevant_environment([(OsString::from("EDITOR"), OsString::from_vec(vec![0xff]))])
                .unwrap();
        assert!(
            ignored.is_empty(),
            "only the variables that are compared have to be UTF-8"
        );
    }

    #[test]
    fn project_root_matches_project_load() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let canonical = std::fs::canonicalize(&temporary.path).unwrap();
        let app = temporary.path.join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(app.join("Main.tz"), "42\n").unwrap();
        assert_eq!(project_root(&app), Some(canonical.join("app")));
        assert_eq!(
            project_root(&app.join("Main.tz")),
            Some(canonical.join("app"))
        );
        assert_eq!(project_root(&temporary.path.join("missing")), None);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&app, temporary.path.join("link")).unwrap();
            assert_eq!(
                project_root(&temporary.path.join("link")),
                None,
                "Project::load refuses a link"
            );
        }
        // A bare file name is in the working directory, which is the process's.
        let here = std::fs::canonicalize(".").unwrap();
        assert_eq!(project_root(Path::new("Cargo.toml")), Some(here));
    }

    #[cfg(unix)]
    #[test]
    fn private_directory_rejects_shared_or_linked_directories() {
        use std::os::unix::fs::PermissionsExt;
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let fresh = temporary.path.join("a").join("b");
        ensure_private_directory(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o700);
        assert!(is_private_directory(&fresh));
        assert_eq!(
            std::fs::read_dir(&fresh).unwrap().count(),
            0,
            "the probe is removed"
        );
        ensure_private_directory(&fresh).unwrap();

        let shared = temporary.path.join("shared");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = ensure_private_directory(&shared).unwrap_err();
        assert!(error.contains("open to other users (mode 755)"), "{error}");
        assert!(!is_private_directory(&shared));
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o710)).unwrap();
        assert!(
            ensure_private_directory(&shared).is_err(),
            "the group's access is access"
        );

        let link = temporary.path.join("link");
        std::os::unix::fs::symlink(&fresh, &link).unwrap();
        let error = ensure_private_directory(&link).unwrap_err();
        assert!(error.contains("not a file or a symbolic link"), "{error}");
        assert!(!is_private_directory(&link));

        let file = temporary.path.join("file");
        std::fs::write(&file, "").unwrap();
        assert!(ensure_private_directory(&file).is_err());
        assert!(!is_private_directory(&file));
        assert!(!is_private_directory(&temporary.path.join("missing")));
    }

    fn sources() -> Vec<(String, String)> {
        fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    found.push(path);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut paths = Vec::new();
        walk(&root, &mut paths);
        paths.sort();
        // The other executables of the package, such as the test helper of the fast backend, are
        // processes of their own: a server never runs them.
        let helpers = root.join("bin");
        paths
            .into_iter()
            .filter(|path| !path.starts_with(&helpers))
            .map(|path| {
                let name = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).unwrap();
                // Only what the compiler runs: the module of tests is the end of a file.
                let code = text.split("#[cfg(test)]\nmod ").next().unwrap().to_owned();
                (name, code)
            })
            .collect()
    }

    /// The arguments of every call of `callee` in `text`: a string literal's contents, or the
    /// text of a path.
    fn first_arguments<'a>(text: &'a str, callee: &str) -> Vec<(bool, &'a str)> {
        let mut found = Vec::new();
        let mut rest = text;
        while let Some(index) = rest.find(callee) {
            rest = &rest[index + callee.len()..];
            let argument = rest.trim_start();
            if let Some(literal) = argument.strip_prefix('"') {
                found.push((true, &literal[..literal.find('"').unwrap()]));
            } else {
                let end = argument
                    .find(|character: char| {
                        !(character.is_alphanumeric() || character == '_' || character == ':')
                    })
                    .unwrap_or(argument.len());
                found.push((false, &argument[..end]));
            }
        }
        found
    }

    fn constants(sources: &[(String, String)]) -> BTreeMap<String, String> {
        let mut found = BTreeMap::new();
        for (_, text) in sources {
            for line in text.lines() {
                let line = line.trim();
                let Some(index) = line.find("const ") else {
                    continue;
                };
                let rest = &line[index + "const ".len()..];
                let Some((name, value)) = rest.split_once(": &str = \"") else {
                    continue;
                };
                if let Some((value, _)) = value.split_once('"') {
                    found.insert(name.to_owned(), value.to_owned());
                }
            }
        }
        found
    }

    /// What a server runs writes to the streams that it is given, never to the process's own: a
    /// write that bypasses them would reach the server's log instead of the client. These are the
    /// places that do (none of them is on the way of `check` or `build`, except the two that the
    /// server collects: the warning of a temporary directory that cannot be removed, `resident::warn`,
    /// and the report of `TSUZURI_TIME_PASSES`, `timings`). A new one has to be looked at.
    #[test]
    fn only_known_code_writes_to_the_streams_of_the_process() {
        let mut found = BTreeMap::new();
        for (file, text) in sources() {
            if file == "main.rs" || file == "server.rs" {
                continue;
            }
            let lines = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .filter(|line| {
                    ["println!(", "print!(", "io::stderr()", "io::stdout()"]
                        .iter()
                        .any(|site| line.contains(site))
                })
                .count();
            if lines > 0 {
                found.insert(file, lines);
            }
        }
        let expected: BTreeMap<String, usize> = [
            // `run` relays the standard error of the program that it started.
            ("driver.rs", 2),
            ("resident.rs", 1),
            ("timings.rs", 1),
        ]
        .into_iter()
        .map(|(file, count)| (file.to_owned(), count))
        .collect();
        assert_eq!(found, expected);
    }

    /// Every variable that the code reads has to be in the set that a server compares, or a
    /// request could run under settings that the client does not have. A new `TSUZURI_` variable
    /// needs nothing; any other name has to be added to `VARIABLES` (or a prefix) on purpose.
    #[test]
    fn every_environment_variable_that_the_compiler_reads_is_compared() {
        let sources = sources();
        let constants = constants(&sources);
        let mut names = BTreeSet::new();
        let mut dynamic = Vec::new();
        let mut whole = Vec::new();
        for (file, text) in &sources {
            if file == "server.rs" {
                continue;
            }
            for callee in ["var_os(", "env::var(", "resolve_tool("] {
                for (literal, argument) in first_arguments(text, callee) {
                    if literal {
                        names.insert(argument.to_owned());
                    } else if let Some(value) = constants.get(argument.rsplit("::").next().unwrap())
                    {
                        names.insert(value.clone());
                    } else {
                        dynamic.push((file.as_str(), callee, argument));
                    }
                }
            }
            for callee in ["vars_os(", "env::vars("] {
                if text.contains(callee) {
                    whole.push(file.as_str());
                }
            }
        }
        assert!(
            names.contains("TSUZURI_CACHE_DIR")
                && names.contains("TSUZURI_THREADS")
                && names.contains("PATH"),
            "the scan finds the variables: {names:?}"
        );
        // Reads that arrived with other tickets: through a constant (`profile.rs`), through
        // `resolve_tool` in a subdirectory (`driver/lto.rs`) and by a literal (`profile.rs`).
        assert!(
            sources.iter().any(|(file, _)| file.contains('/'))
                && names.contains("TSUZURI_LLVM_PROFDATA")
                && names.contains("TSUZURI_LLVM_LINK")
                && names.contains("LLVM_PROFILE_FILE"),
            "the scan reaches the subdirectories and the constants: {names:?}"
        );
        let missing: Vec<_> = names
            .iter()
            .filter(|name| !is_relevant_variable(name))
            .collect();
        assert!(
            missing.is_empty(),
            "variables that the compiler reads and a server does not compare: {missing:?}"
        );
        // The reads whose name is not a literal: each loops over a list that is checked below.
        let mut dynamic: Vec<_> = dynamic
            .into_iter()
            .filter(|(_, callee, _)| *callee != "resolve_tool(")
            .collect();
        dynamic.sort();
        assert_eq!(
            dynamic,
            [
                // The loops over `KEY_ENVIRONMENT`: the key of a build, of a tool, and of the units of a build.
                ("cache.rs", "var_os(", "variable"),
                ("cache.rs", "var_os(", "variable"),
                ("cache.rs", "var_os(", "variable"),
                ("driver.rs", "var_os(", "variable"),
                ("runtime_cache.rs", "var_os(", "name"),
                ("runtime_cache.rs", "var_os(", "name"),
            ],
            "a read of a variable by a name that is not a literal needs a look: is every name it can take compared?"
        );
        // The loops over `KEY_ENVIRONMENT` in `cache.rs` read the names that it lists.
        assert!(
            crate::cache::KEY_ENVIRONMENT
                .iter()
                .all(|name| is_relevant_variable(name))
        );
        // The runtime cache reads the names that its file lists.
        let runtime = &sources
            .iter()
            .find(|(file, _)| file == "runtime_cache.rs")
            .unwrap()
            .1;
        let start = runtime.find("PREBUILT_ENVIRONMENT: &[&str] = &[").unwrap();
        let list = &runtime[start..start + runtime[start..].find("];").unwrap()];
        for name in list.split('"').skip(1).step_by(2) {
            assert!(is_relevant_variable(name), "{name}");
        }
        // The whole environment is read where a child process is made to forget the `GIT_*` ones.
        assert_eq!(
            whole,
            ["fetch.rs"],
            "a read of the whole environment needs a look"
        );
    }
}

#[cfg(all(test, unix))]
mod serve_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::thread;
    use std::time::Instant;

    /// A directory with a name short enough for a socket path (104 bytes on macOS).
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "tz{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            ensure_private_directory(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    type Gate = Arc<(Mutex<bool>, Condvar)>;

    #[derive(Clone, Default)]
    struct Probe {
        seen: Arc<Mutex<Vec<Vec<String>>>>,
        changed: Arc<AtomicBool>,
        gate: Option<Gate>,
    }

    impl Probe {
        fn gated() -> (Self, Gate) {
            let gate: Gate = Arc::new((Mutex::new(false), Condvar::new()));
            (
                Self {
                    gate: Some(Arc::clone(&gate)),
                    ..Self::default()
                },
                gate,
            )
        }
    }

    fn open(gate: &Gate) {
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
    }

    /// A log that the test reads: what the server wrote to it, as text.
    #[derive(Clone, Default)]
    struct SharedLog(Arc<Mutex<Vec<u8>>>);

    impl SharedLog {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedLog {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A log that keeps each write that it is given, and that blocks in a write while its gate is
    /// closed (as a pipe that nobody reads does).
    #[derive(Clone)]
    struct StuckLog {
        writes: Arc<Mutex<Vec<Vec<u8>>>>,
        gate: Gate,
    }

    impl StuckLog {
        fn new(open: bool) -> Self {
            Self {
                writes: Arc::default(),
                gate: Arc::new((Mutex::new(open), Condvar::new())),
            }
        }

        fn open(&self) {
            open(&self.gate);
        }

        /// What was written, one write at a time, each of which has to be one whole line.
        fn lines(&self) -> Vec<String> {
            self.writes
                .lock()
                .unwrap()
                .iter()
                .map(|bytes| {
                    let text = String::from_utf8(bytes.clone()).unwrap();
                    assert!(
                        text.ends_with('\n') && text.matches('\n').count() == 1,
                        "a write is one line: {text:?}"
                    );
                    text.trim_end().to_owned()
                })
                .collect()
        }
    }

    impl Write for StuckLog {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut opened = self.gate.0.lock().unwrap();
            while !*opened {
                opened = self.gate.1.wait(opened).unwrap();
            }
            drop(opened);
            self.writes.lock().unwrap().push(bytes.to_vec());
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Handler for Probe {
        fn run(&mut self, request: &Request, log: &mut dyn Write) -> Result<Response, Refused> {
            self.seen.lock().unwrap().push(request.args.clone());
            if let Some(gate) = &self.gate {
                let mut opened = gate.0.lock().unwrap();
                while !*opened {
                    opened = gate.1.wait(opened).unwrap();
                }
            }
            let _ = writeln!(log, "handler: {:?}", request.args);
            match request.args.first().map(String::as_str) {
                Some("refuse") => Err(Refused::new(
                    Refusal::Unsupported,
                    "only check and build are served",
                )),
                Some("big") => Ok(Response {
                    exit: 0,
                    stdout: "x".repeat(MAX_RESPONSE_BYTES),
                    stderr: String::new(),
                }),
                _ => Ok(Response {
                    exit: 3,
                    stdout: "out\n".to_owned(),
                    stderr: format!("args: {:?}\n", request.args),
                }),
            }
        }

        fn compiler_changed(&self) -> bool {
            self.changed.load(Ordering::Acquire)
        }

        fn status(&self) -> Vec<(&'static str, Value)> {
            vec![("resident_bytes", json!(7))]
        }
    }

    fn config(scratch: &Scratch, idle: Duration) -> Config {
        let mut config = Config::new(
            scratch.0.clone(),
            PathBuf::from("/work/app"),
            idle,
            PathBuf::from("/work/tsuzuri"),
            PathBuf::from("/work"),
            vec![("PATH".to_owned(), "/usr/bin".to_owned())],
        );
        config.request_deadline = Duration::from_millis(300);
        config
    }

    fn matching() -> Request {
        Request {
            cwd: PathBuf::from("/work"),
            compiler: PathBuf::from("/work/tsuzuri"),
            env: vec![("PATH".to_owned(), "/usr/bin".to_owned())],
            args: vec!["check".to_owned(), "app".to_owned()],
        }
    }

    struct Server {
        socket: PathBuf,
        root: PathBuf,
        thread: thread::JoinHandle<(Result<Stop, ServeError>, String)>,
    }

    impl Server {
        fn start(config: Config, probe: Probe) -> Self {
            let log = SharedLog::default();
            Self::start_logging(config, probe, log.clone(), move || log.text())
        }

        /// Starts a server that writes its log to `writer`; `text` gives what the thread hands back
        /// when the server has ended.
        fn start_logging(
            config: Config,
            mut probe: Probe,
            writer: impl Write + Send + 'static,
            text: impl FnOnce() -> String + Send + 'static,
        ) -> Self {
            let socket = socket_path(&config.directory, &config.root);
            let root = config.root.clone();
            let thread = thread::spawn(move || {
                let outcome = serve(&config, &mut probe, writer);
                (outcome, text())
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            while UnixStream::connect(&socket).is_err() {
                assert!(Instant::now() < deadline, "the server did not start");
                thread::sleep(Duration::from_millis(5));
            }
            Self {
                socket,
                root,
                thread,
            }
        }

        fn ask(&self, operation: &Operation) -> Option<Vec<u8>> {
            exchange_line(&self.socket, &self.root, operation)
        }

        fn status(&self) -> Map<String, Value> {
            decode_reply(&self.ask(&Operation::Status).unwrap()).unwrap()
        }

        /// Sends `bytes` after the hello and returns the reply line.
        fn raw(&self, bytes: &[u8]) -> String {
            let mut stream = UnixStream::connect(&self.socket).unwrap();
            let mut hello = String::new();
            read_hello(&mut stream, &mut hello);
            let _ = stream.write_all(bytes);
            let _ = stream.shutdown(std::net::Shutdown::Write);
            let mut reply = String::new();
            let _ = stream.read_to_string(&mut reply);
            reply
        }

        fn stop(self) -> (Result<Stop, ServeError>, String) {
            let reply = self.ask(&Operation::Shutdown).unwrap();
            assert!(decode_reply(&reply).is_some());
            self.thread.join().unwrap()
        }
    }

    fn exchange_line(socket: &Path, root: &Path, operation: &Operation) -> Option<Vec<u8>> {
        sockets::exchange(socket, root, operation, MAX_RESPONSE_BYTES).ok()
    }

    fn read_hello(stream: &mut UnixStream, into: &mut String) {
        let mut byte = [0_u8; 1];
        while stream.read_exact(&mut byte).is_ok() {
            if byte[0] == b'\n' {
                break;
            }
            into.push(char::from(byte[0]));
        }
    }

    fn reason(reply: &str) -> String {
        let value: Value = serde_json::from_str(reply).unwrap();
        assert_eq!(value["ok"], json!(false), "{reply}");
        value["error"].as_str().unwrap().to_owned()
    }

    #[test]
    fn a_server_runs_requests_and_counts_them() {
        let scratch = Scratch::new();
        let probe = Probe::default();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), probe.clone());
        let reply = server.ask(&Operation::Run(matching())).unwrap();
        assert_eq!(
            decode_response(&reply),
            Some(Response {
                exit: 3,
                stdout: "out\n".to_owned(),
                stderr: "args: [\"check\", \"app\"]\n".to_owned(),
            })
        );
        assert_eq!(*probe.seen.lock().unwrap(), [vec!["check", "app"]]);
        let status = server.status();
        assert_eq!(status["server"], json!("tsuzuri"));
        assert_eq!(status["root"], json!("/work/app"));
        assert_eq!(status["compiler"], json!("/work/tsuzuri"));
        assert_eq!(
            status["requests"],
            json!(2),
            "the status request counts too"
        );
        assert_eq!(status["served"], json!(1));
        assert_eq!(status["refused"], json!(0));
        assert_eq!(status["log_dropped"], json!(0));
        assert_eq!(
            status["resident_bytes"],
            json!(7),
            "what the handler reports"
        );
        assert!(status["pid"].as_u64().unwrap() > 0);
        let mode = std::fs::metadata(&server.socket)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "only the owner can use the socket");
        let socket = server.socket.clone();
        let (outcome, log) = server.stop();
        assert_eq!(outcome, Ok(Stop::Shutdown));
        assert!(!socket.exists(), "the socket is removed");
        for expected in [
            "serve: listening on ",
            "handler: [\"check\", \"app\"]",
            "serve: request 1 served (exit 3)",
            "serve: shutdown requested; stopping",
        ] {
            assert!(log.contains(expected), "{expected} in {log}");
        }
    }

    #[test]
    fn a_request_that_differs_is_refused_before_the_handler_sees_it() {
        let scratch = Scratch::new();
        let probe = Probe::default();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), probe.clone());
        let mut other_compiler = matching();
        other_compiler.compiler = PathBuf::from("/other/tsuzuri");
        let mut other_directory = matching();
        other_directory.cwd = PathBuf::from("/work/app");
        let mut other_path = matching();
        other_path.env[0].1 = "/usr/local/bin".to_owned();
        let mut extra = matching();
        extra
            .env
            .push(("TSUZURI_THREADS".to_owned(), "1".to_owned()));
        let mut missing = matching();
        missing.env.clear();
        for (request, expected) in [
            (other_compiler, "compiler"),
            (other_directory, "cwd"),
            (other_path, "env"),
            (extra, "env"),
            (missing, "env"),
        ] {
            let reply = String::from_utf8(server.ask(&Operation::Run(request)).unwrap()).unwrap();
            assert_eq!(reason(&reply), expected, "{reply}");
        }
        assert!(probe.seen.lock().unwrap().is_empty());
        let reply = String::from_utf8(
            server
                .ask(&Operation::Run(Request {
                    args: vec!["refuse".to_owned()],
                    ..matching()
                }))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            reason(&reply),
            "unsupported",
            "a refusal of the handler is sent as it is"
        );
        let status = server.status();
        assert_eq!(status["refused"], json!(6));
        assert_eq!(
            status["refusals"],
            json!({"compiler": 1, "cwd": 1, "env": 3, "unsupported": 1})
        );
        assert_eq!(status["served"], json!(0));
        let (outcome, log) = server.stop();
        assert_eq!(outcome, Ok(Stop::Shutdown));
        assert!(
            log.contains("request 3 refused: env (PATH differs from the server environment)"),
            "{log}"
        );
    }

    #[test]
    fn protocol_violations_are_refused_and_the_server_goes_on() {
        let scratch = Scratch::new();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), Probe::default());
        assert_eq!(reason(&server.raw(b"not json\n")), "protocol");
        assert_eq!(
            reason(&server.raw(b"{\"protocol\":2,\"op\":\"status\"}\n")),
            "protocol"
        );
        assert_eq!(
            reason(&server.raw(b"{\"protocol\":1,\"op\":\"restart\"}\n")),
            "protocol"
        );
        let long = vec![b'x'; MAX_REQUEST_BYTES + 1];
        assert_eq!(reason(&server.raw(&long)), "too-large");
        assert_eq!(
            server.raw(b""),
            "",
            "a connection that sends nothing gets nothing"
        );
        let mut padded = long;
        padded.push(b'\n');
        assert_eq!(reason(&server.raw(&padded)), "too-large");
        let reply = server.ask(&Operation::Run(matching())).unwrap();
        assert!(
            decode_response(&reply).is_some(),
            "the server answers the next request"
        );
        let status = server.status();
        assert_eq!(status["refusals"], json!({"protocol": 3, "too-large": 2}));
        let _ = server.stop();
    }

    #[test]
    fn a_slow_client_is_cut_off() {
        let scratch = Scratch::new();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), Probe::default());
        let mut slow = UnixStream::connect(&server.socket).unwrap();
        let mut hello = String::new();
        read_hello(&mut slow, &mut hello);
        slow.write_all(b"{\"protocol\":1,").unwrap();
        let started = Instant::now();
        let mut rest = String::new();
        let _ = slow.read_to_string(&mut rest);
        assert!(rest.is_empty());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the deadline of 300 ms cuts it off"
        );
        let reply = server.ask(&Operation::Run(matching())).unwrap();
        assert!(decode_response(&reply).is_some());
        let (_, log) = server.stop();
        assert!(log.contains("a request could not be read"), "{log}");
    }

    /// Connects as a client does: reads the hello and sends `operation`, but does not wait. (The
    /// write of a client that was refused at once fails, which a client takes as a refusal too.)
    fn begin(server: &Server, operation: &Operation) -> UnixStream {
        let mut stream = UnixStream::connect(&server.socket).unwrap();
        let mut hello = String::new();
        read_hello(&mut stream, &mut hello);
        let _ = stream.write_all(encode_operation(operation).as_bytes());
        stream
    }

    #[test]
    fn requests_beyond_the_queue_are_refused_as_busy() {
        let scratch = Scratch::new();
        let (probe, gate) = Probe::gated();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), probe.clone());
        let run = Operation::Run(matching());
        // One request runs and waits at the gate, and QUEUE more wait behind it.
        let mut waiting = vec![begin(&server, &run)];
        let started = Instant::now();
        while probe.seen.lock().unwrap().is_empty() {
            assert!(started.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(5));
        }
        // The accept thread takes the connections one at a time, so after the hello of one the ones
        // before it are queued.
        for _ in 0..QUEUE {
            waiting.push(begin(&server, &run));
        }
        let mut overflow = begin(&server, &run);
        let mut refusal = String::new();
        let _ = overflow.read_to_string(&mut refusal);
        assert_eq!(reason(&refusal), "busy");
        open(&gate);
        for mut stream in waiting {
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            assert!(
                decode_response(reply.trim_end().as_bytes()).is_some(),
                "{reply}"
            );
        }
        let status = server.status();
        assert_eq!(status["refusals"], json!({"busy": 1}));
        assert_eq!(status["served"], json!(1 + QUEUE));
        let (_, log) = server.stop();
        assert!(log.contains("connection(s) refused: busy"), "{log}");
    }

    #[test]
    fn a_log_that_blocks_holds_nothing_but_its_own_thread() {
        let scratch = Scratch::new();
        let stuck = StuckLog::new(false);
        let probe = Probe::default();
        let server = Server::start_logging(
            config(&scratch, Duration::from_secs(60)),
            probe.clone(),
            stuck.clone(),
            String::new,
        );
        // Every refusal is a line of the log, and the writer is stuck in the first: far more lines
        // than the queue holds. Every client has its answer.
        let sent = 2 * sockets::LOG_QUEUE;
        for _ in 0..sent {
            assert_eq!(reason(&server.raw(b"not json\n")), "protocol");
        }
        // A command is run, and `status` says what was dropped.
        let reply = server.ask(&Operation::Run(matching())).unwrap();
        assert!(decode_response(&reply).is_some());
        let status = server.status();
        assert_eq!(status["served"], json!(1));
        let dropped = status["log_dropped"].as_u64().unwrap();
        assert!(
            dropped >= (sent - sockets::LOG_QUEUE) as u64,
            "{dropped} lines were dropped"
        );
        assert!(stuck.lines().is_empty(), "the writer has not been let go");
        // And the server stops with its log still stuck, a second at most after the stop.
        assert!(server.ask(&Operation::Shutdown).is_some());
        let stopped = Instant::now();
        while !server.thread.is_finished() {
            assert!(
                stopped.elapsed() < Duration::from_secs(10),
                "the server did not stop while its log was stuck"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let (outcome, _) = server.thread.join().unwrap();
        assert_eq!(outcome, Ok(Stop::Shutdown));
        // The writer is let go: the lines that were queued come out whole and in order, and the
        // lines that were dropped are counted, in a line where they were dropped and at the end.
        stuck.open();
        let started = Instant::now();
        let lines = loop {
            let lines = stuck.lines();
            if lines
                .last()
                .is_some_and(|line| line.contains("log lines dropped"))
            {
                break lines;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the lines were not written: {lines:?}"
            );
            thread::sleep(Duration::from_millis(5));
        };
        assert!(lines[0].starts_with("serve: listening on "), "{lines:?}");
        let mut previous = 0;
        let mut counted = 0;
        for line in &lines {
            assert!(line.starts_with("serve: "), "{line}");
            if let Some(number) = line.strip_prefix("serve: request ") {
                let number: u64 = number.split(' ').next().unwrap().parse().unwrap();
                assert!(number > previous, "in order: {line}");
                previous = number;
            }
            if let Some(count) = line
                .strip_prefix("serve: ")
                .and_then(|rest| rest.strip_suffix(" log lines dropped"))
            {
                counted += count.parse::<u64>().unwrap();
            }
        }
        assert!(counted >= dropped, "{counted} counted, {dropped} dropped");
        assert!(
            lines.len() <= sockets::LOG_QUEUE + 3,
            "{} lines",
            lines.len()
        );
    }

    #[test]
    fn every_line_of_the_log_is_one_write_and_a_line_of_its_own() {
        let scratch = Scratch::new();
        let log = StuckLog::new(true);
        let probe = Probe::default();
        let server = Server::start_logging(
            config(&scratch, Duration::from_secs(60)),
            probe.clone(),
            log.clone(),
            String::new,
        );
        assert!(decode_response(&server.ask(&Operation::Run(matching())).unwrap()).is_some());
        assert_eq!(reason(&server.raw(b"not json\n")), "protocol");
        // What a client sends goes into the lines that quote it: not as a line of its own, and not
        // beyond the length that a line has.
        let mut forged = matching();
        forged.cwd = PathBuf::from(format!(
            "/a\nserve: request 99 served (exit 0)\r{}",
            "y".repeat(10_000)
        ));
        let reply = String::from_utf8(server.ask(&Operation::Run(forged)).unwrap()).unwrap();
        assert_eq!(reason(&reply), "cwd");
        assert!(server.ask(&Operation::Shutdown).is_some());
        let (outcome, _) = server.thread.join().unwrap();
        assert_eq!(outcome, Ok(Stop::Shutdown));
        let lines = log.lines();
        assert!(lines[0].starts_with("serve: listening on "), "{lines:?}");
        // The handler writes with `writeln!`, in pieces; the server writes it as the line that it is.
        assert!(
            lines.contains(&"handler: [\"check\", \"app\"]".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line == "serve: request 1 served (exit 3)"),
            "{lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("request 99 served") && !line.contains("refused")),
            "a line of a client's own: {lines:?}"
        );
        let refused = lines
            .iter()
            .find(|line| line.starts_with("serve: request 3 refused: cwd"))
            .unwrap();
        assert!(
            refused.contains("\\n") && refused.contains("\\r"),
            "{refused}"
        );
        assert!(
            refused.len() <= sockets::LOG_LINE + 3,
            "{} bytes",
            refused.len()
        );
        assert!(refused.ends_with("..."));
    }

    #[test]
    fn an_idle_server_stops_and_removes_its_socket() {
        let scratch = Scratch::new();
        let config = config(&scratch, Duration::from_millis(300));
        let socket = socket_path(&config.directory, &config.root);
        let (outcome, log) = thread::spawn(move || {
            let log = SharedLog::default();
            let outcome = serve(&config, &mut Probe::default(), log.clone());
            (outcome, log.text())
        })
        .join()
        .unwrap();
        assert_eq!(outcome, Ok(Stop::Idle));
        assert!(!socket.exists());
        assert!(log.contains("serve: listening on "), "{log}");
        assert!(log.contains("serve: idle for 0 s; stopping"), "{log}");
    }

    #[test]
    fn a_request_resets_the_idle_time() {
        let scratch = Scratch::new();
        // Three pauses of 1 s add up to the idle time of 3 s, and each stays 2 s short of it: only
        // the reset makes the server last through all of them, and a slow runner has room.
        let server = Server::start(config(&scratch, Duration::from_secs(3)), Probe::default());
        for _ in 0..3 {
            thread::sleep(Duration::from_secs(1));
            assert!(
                server.ask(&Operation::Status).is_some(),
                "the server is still there"
            );
        }
        let (outcome, _) = server.thread.join().unwrap();
        assert_eq!(outcome, Ok(Stop::Idle));
    }

    /// Waits until the handler has been given `count` requests.
    fn wait_until_run(probe: &Probe, count: usize) {
        let started = Instant::now();
        while probe.seen.lock().unwrap().len() < count {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no request ran"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn check(name: &str) -> Operation {
        Operation::Run(Request {
            args: vec!["check".to_owned(), name.to_owned()],
            ..matching()
        })
    }

    #[test]
    fn status_and_shutdown_answer_while_a_request_runs_and_the_server_ends_after_it() {
        let scratch = Scratch::new();
        let (probe, gate) = Probe::gated();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), probe.clone());
        // One request runs and waits at the gate, and two wait behind it.
        let mut running = begin(&server, &check("first"));
        wait_until_run(&probe, 1);
        let waiting = [
            begin(&server, &check("second")),
            begin(&server, &check("third")),
        ];
        // A `status` is answered at once (a client that is not answered fails after 10 s), and says
        // what runs and what waits.
        let status = server.status();
        assert_eq!(status["running"]["request"], json!(1));
        assert_eq!(status["queued"], json!(2));
        assert_eq!(status["served"], json!(0));
        assert_eq!(
            status["resident_bytes"],
            json!(7),
            "what the handler said after the last request"
        );
        // So is a `shutdown`: the reply says that a request runs, and the server goes on until it ends.
        let reply = decode_reply(&server.ask(&Operation::Shutdown).unwrap()).unwrap();
        assert_eq!(reply["running"]["request"], json!(1));
        assert!(reply["pid"].as_u64().unwrap() > 0);
        assert!(
            !server.thread.is_finished(),
            "the request that runs is not cut off"
        );
        open(&gate);
        let mut text = String::new();
        running.read_to_string(&mut text).unwrap();
        assert!(
            decode_response(text.trim_end().as_bytes()).is_some(),
            "{text}"
        );
        // The requests that waited do not run: their clients see the connection close.
        for mut stream in waiting {
            let mut text = String::new();
            let _ = stream.read_to_string(&mut text);
            assert_eq!(text, "");
        }
        let (outcome, log) = server.thread.join().unwrap();
        assert_eq!(outcome, Ok(Stop::Shutdown));
        assert_eq!(*probe.seen.lock().unwrap(), [vec!["check", "first"]]);
        assert!(log.contains("serve: shutdown requested; stopping"), "{log}");
        assert!(log.contains("serve: 2 waiting request(s) dropped"), "{log}");
    }

    #[test]
    fn a_queued_request_whose_client_went_away_is_not_run() {
        let scratch = Scratch::new();
        let (probe, gate) = Probe::gated();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), probe.clone());
        let mut first = begin(&server, &check("first"));
        wait_until_run(&probe, 1);
        let gone = begin(&server, &check("gone"));
        let mut live = begin(&server, &check("live"));
        // The second client is interrupted while it waits.
        drop(gone);
        assert_eq!(server.status()["queued"], json!(2));
        open(&gate);
        for stream in [&mut first, &mut live] {
            let mut text = String::new();
            stream.read_to_string(&mut text).unwrap();
            assert!(
                decode_response(text.trim_end().as_bytes()).is_some(),
                "{text}"
            );
        }
        let status = server.status();
        assert_eq!(status["abandoned"], json!(1));
        assert_eq!(status["served"], json!(2));
        assert_eq!(
            *probe.seen.lock().unwrap(),
            [vec!["check", "first"], vec!["check", "live"]],
            "the request of the client that went away did not run"
        );
        let (_, log) = server.stop();
        assert!(
            log.contains("serve: request 2 skipped: its client went away while it waited"),
            "{log}"
        );
    }

    #[test]
    fn a_client_waits_for_the_reply_to_a_status_only_so_long() {
        let scratch = Scratch::new();
        let root = Path::new("/work/app");
        let hello = encode_hello(root);
        let ok = "{\"protocol\":1,\"ok\":true,\"exit\":0,\"stdout\":\"\",\"stderr\":\"\"}\n";
        let slow = |reply: &'static str| {
            let hello = hello.clone();
            move |mut stream: UnixStream| {
                let _ = stream.write_all(hello.as_bytes());
                let mut request = String::new();
                let _ = std::io::BufRead::read_line(
                    &mut std::io::BufReader::new(&stream),
                    &mut request,
                );
                thread::sleep(Duration::from_millis(600));
                let _ = stream.write_all(reply.as_bytes());
            }
        };
        let within = |operation: &Operation, script: Box<dyn FnOnce(UnixStream) + Send>| {
            let (socket, handle) = fake_server(&scratch, script);
            let reply = sockets::exchange_within(
                &socket,
                root,
                operation,
                MAX_RESPONSE_BYTES,
                Duration::from_millis(200),
            );
            handle.join().unwrap();
            std::fs::remove_file(&socket).unwrap();
            reply
        };
        // A status that is not answered in time is an error, which names the time.
        let error = within(&Operation::Status, Box::new(slow(ok))).unwrap_err();
        assert!(error.contains("did not reply within 0.2 s"), "{error}");
        // A command is waited for as long as it takes.
        let reply = within(&Operation::Run(matching()), Box::new(slow(ok))).unwrap();
        assert!(decode_response(&reply).is_some());
    }

    #[test]
    fn servers_that_start_together_are_one_server() {
        let scratch = Scratch::new();
        let root = PathBuf::from("/work/app");
        let socket = socket_path(&scratch.0, &root);
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let starts: Vec<_> = (0..4)
            .map(|_| {
                let config = config(&scratch, Duration::from_secs(60));
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    serve(&config, &mut Probe::default(), Vec::new())
                })
            })
            .collect();
        // Three of them end at once; the fourth runs, and its socket is the one that is there.
        let started = Instant::now();
        while starts.iter().filter(|start| start.is_finished()).count() < 3 {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "three servers should have ended"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let status = exchange_line(&socket, &root, &Operation::Status);
        assert!(status.is_some(), "the server that runs can be reached");
        assert!(exchange_line(&socket, &root, &Operation::Shutdown).is_some());
        let outcomes: Vec<_> = starts
            .into_iter()
            .map(|start| start.join().unwrap())
            .collect();
        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.is_ok()).count(),
            1,
            "{outcomes:?}"
        );
        for error in outcomes.iter().filter_map(|outcome| outcome.as_ref().err()) {
            assert_eq!(error.code, "E2001");
            assert!(
                error.message.contains("a build server is already running"),
                "{}",
                error.message
            );
        }
        assert!(!socket.exists(), "the server that ended removed its socket");
    }

    #[test]
    fn a_held_lock_is_a_running_server_and_a_free_one_is_not() {
        let scratch = Scratch::new();
        let config_for = || config(&scratch, Duration::from_secs(60));
        let socket = socket_path(&scratch.0, &config_for().root);
        // What a server that was killed leaves is a lock file that nobody holds.
        std::fs::write(socket.with_extension("lock"), "").unwrap();
        let server = Server::start(config_for(), Probe::default());
        let _ = server.stop();
        assert!(
            socket.with_extension("lock").exists(),
            "the file stays; only the hold ends"
        );
        // A server that holds it, and has not made its socket yet, is not run over.
        let held = sockets::take_lock(&socket).unwrap();
        assert!(held.is_some());
        let error = serve(&config_for(), &mut Probe::default(), Vec::new()).unwrap_err();
        assert!(
            error.message.contains("a build server is already running"),
            "{}",
            error.message
        );
        assert!(!socket.exists(), "the socket was not touched");
        drop(held);
        let server = Server::start(config_for(), Probe::default());
        let _ = server.stop();
    }

    #[test]
    fn a_replaced_compiler_ends_the_server() {
        let scratch = Scratch::new();
        let probe = Probe::default();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), probe.clone());
        assert!(decode_response(&server.ask(&Operation::Run(matching())).unwrap()).is_some());
        probe.changed.store(true, Ordering::Release);
        let reply = String::from_utf8(server.ask(&Operation::Run(matching())).unwrap()).unwrap();
        assert_eq!(reason(&reply), "compiler-changed");
        let socket = server.socket.clone();
        let (outcome, log) = server.thread.join().unwrap();
        assert_eq!(outcome, Ok(Stop::CompilerChanged));
        assert!(!socket.exists());
        assert!(
            log.contains(
                "serve: the compiler binary changed; stopping (start tsuzuri serve again)"
            ),
            "{log}"
        );
        assert_eq!(probe.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_output_beyond_the_limit_is_refused() {
        let scratch = Scratch::new();
        let server = Server::start(config(&scratch, Duration::from_secs(60)), Probe::default());
        let reply = String::from_utf8(
            server
                .ask(&Operation::Run(Request {
                    args: vec!["big".to_owned()],
                    ..matching()
                }))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(reason(&reply), "too-large");
        let _ = server.stop();
    }

    #[test]
    fn a_stale_socket_is_taken_and_a_running_server_is_not() {
        let scratch = Scratch::new();
        let config_for = || config(&scratch, Duration::from_secs(60));
        let socket = socket_path(&scratch.0, &config_for().root);
        // The leftover of a server that was killed: the file, and nobody that answers.
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        let server = Server::start(config_for(), Probe::default());
        let again = serve(&config_for(), &mut Probe::default(), Vec::new());
        let error = again.unwrap_err();
        assert_eq!(error.code, "E2001");
        assert!(
            error.message.contains("a build server is already running"),
            "{}",
            error.message
        );
        assert!(
            server.ask(&Operation::Status).is_some(),
            "the first server is not disturbed"
        );
        let _ = server.stop();

        std::fs::write(&socket, "not a socket").unwrap();
        let error = serve(&config_for(), &mut Probe::default(), Vec::new()).unwrap_err();
        assert!(
            error.message.contains("exists and is not a socket"),
            "{}",
            error.message
        );
        assert_eq!(
            std::fs::read_to_string(&socket).unwrap(),
            "not a socket",
            "nothing is deleted"
        );
    }

    #[test]
    fn a_socket_path_beyond_the_system_limit_is_a_usage_error() {
        let mut long = config(&Scratch::new(), Duration::from_secs(1));
        long.directory = PathBuf::from("/").join("d".repeat(200));
        let error = serve(&long, &mut Probe::default(), Vec::new()).unwrap_err();
        assert_eq!(error.code, "E2000");
        assert!(error.message.contains("too long"), "{}", error.message);
    }

    /// Runs `script` once for the first connection to a socket in `scratch`.
    fn fake_server(
        scratch: &Scratch,
        script: impl FnOnce(UnixStream) + Send + 'static,
    ) -> (PathBuf, thread::JoinHandle<()>) {
        let socket = scratch.0.join("fake.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            script(stream);
        });
        (socket, handle)
    }

    #[test]
    fn a_client_gives_up_on_a_server_that_is_not_one() {
        let root = Path::new("/work/app");
        let operation = Operation::Run(matching());
        let scratch = Scratch::new();
        assert!(exchange_line(&scratch.0.join("none.sock"), root, &operation).is_none());

        let hello = encode_hello(root);
        let ok = "{\"protocol\":1,\"ok\":true,\"exit\":0,\"stdout\":\"\",\"stderr\":\"\"}\n";
        type Script = Box<dyn FnOnce(UnixStream) + Send>;
        let scripts: Vec<(&str, Script, bool)> = vec![
            ("closes at once", Box::new(drop), false),
            (
                "garbage hello",
                Box::new(|mut s| {
                    let _ = s.write_all(b"hello there\n");
                }),
                false,
            ),
            (
                "protocol 2",
                Box::new(|mut s| {
                    let _ = s.write_all(
                        b"{\"protocol\":2,\"server\":\"tsuzuri\",\"root\":\"/work/app\"}\n",
                    );
                }),
                false,
            ),
            (
                "another root",
                Box::new(|mut s| {
                    let _ = s.write_all(encode_hello(Path::new("/work/other")).as_bytes());
                }),
                false,
            ),
            (
                "no reply",
                Box::new({
                    let hello = hello.clone();
                    move |mut s| {
                        let _ = s.write_all(hello.as_bytes());
                    }
                }),
                false,
            ),
            (
                "refusal",
                Box::new({
                    let hello = hello.clone();
                    move |mut s| {
                        let _ = s.write_all(hello.as_bytes());
                        let _ = s.write_all(
                            encode_refusal(&Refused::new(Refusal::Busy, "full")).as_bytes(),
                        );
                    }
                }),
                false,
            ),
            (
                "an answer",
                Box::new({
                    let hello = hello.clone();
                    move |mut s| {
                        let _ = s.write_all(hello.as_bytes());
                        let mut request = String::new();
                        let _ = std::io::BufRead::read_line(
                            &mut std::io::BufReader::new(&s),
                            &mut request,
                        );
                        assert!(request.contains("\"op\":\"run\""));
                        let _ = s.write_all(ok.as_bytes());
                    }
                }),
                true,
            ),
        ];
        for (name, script, answers) in scripts {
            let (socket, handle) = fake_server(&scratch, script);
            let reply = exchange_line(&socket, root, &operation);
            handle.join().unwrap();
            std::fs::remove_file(&socket).unwrap();
            let response = reply.as_deref().and_then(decode_response);
            assert_eq!(response.is_some(), answers, "{name}: {reply:?}");
        }
    }
}
