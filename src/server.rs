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
//! request and closes its write side, and the server answers with one reply and closes.

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
/// How many accepted connections wait for the thread that runs the commands.
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
    /// More fields for the reply to `status` (what the process remembers).
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
pub use sockets::{control, request, serve};

#[cfg(not(unix))]
pub fn serve(
    _config: &Config,
    _handler: &mut dyn Handler,
    _log: &mut dyn Write,
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
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
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

    /// Takes the socket at `socket`: a socket that nobody answers is the leftover of a server that
    /// was killed, and goes; one that answers is a server that runs.
    fn bind(socket: &Path) -> Result<UnixListener, ServeError> {
        SocketAddr::from_pathname(socket).map_err(|error| {
            usage(format!(
                "the socket path {} is too long for this system ({error}); set TSUZURI_SERVER_DIR to a shorter directory",
                socket.display()
            ))
        })?;
        for _ in 0..2 {
            match UnixListener::bind(socket) {
                Ok(listener) => {
                    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))
                        .map_err(|error| {
                            failure(format!("cannot protect {}: {error}", socket.display()))
                        })?;
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
                    // ponytail: a server that was killed leaves its socket; nobody answers, so it is removed
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

    /// Removes the socket at the end of the server if it is still the one that this server made.
    struct Cleanup {
        socket: PathBuf,
        made: Option<(u64, u64)>,
    }

    impl Drop for Cleanup {
        fn drop(&mut self) {
            if identity(&self.socket) == self.made {
                let _ = std::fs::remove_file(&self.socket);
            }
        }
    }

    /// Accepts connections, says hello at once (so that a client that waits in the queue is not
    /// timed out by a command that runs), and queues them for the thread that runs the commands.
    fn accept_loop(
        listener: UnixListener,
        sender: SyncSender<UnixStream>,
        stop: &AtomicBool,
        busy: &AtomicU64,
        hello: &str,
    ) {
        for incoming in listener.incoming() {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let Ok(mut stream) = incoming else {
                thread::sleep(Duration::from_millis(10));
                continue;
            };
            let _ = stream.set_write_timeout(Some(RESPONSE_DEADLINE));
            if stream.write_all(hello.as_bytes()).is_err() {
                continue;
            }
            match sender.try_send(stream) {
                Ok(()) => {}
                Err(TrySendError::Full(mut stream)) => {
                    busy.fetch_add(1, Ordering::AcqRel);
                    let refused = Refused::new(
                        Refusal::Busy,
                        format!("{QUEUE} requests are waiting for the server"),
                    );
                    let _ = stream.write_all(encode_refusal(&refused).as_bytes());
                }
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
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

    /// Reads and drops, for a moment, what a client that sent too much is still sending: a socket
    /// closed with unread data resets the connection, and the client may lose the refusal.
    fn drain(stream: &UnixStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
        let mut sink = [0_u8; 65536];
        let mut reader = stream;
        let mut total = 0;
        while total < 4 * MAX_REQUEST_BYTES {
            match reader.read(&mut sink) {
                Ok(0) | Err(_) => break,
                Ok(count) => total += count,
            }
        }
    }

    /// What `status` reports of the loop.
    struct Counters {
        started: Instant,
        requests: u64,
        served: u64,
        refusals: BTreeMap<&'static str, u64>,
    }

    struct Loop<'a> {
        config: &'a Config,
        handler: &'a mut dyn Handler,
        log: &'a mut dyn Write,
        counters: Counters,
        busy: &'a AtomicU64,
        busy_logged: u64,
        compiler_sha256: String,
    }

    pub fn serve(
        config: &Config,
        handler: &mut dyn Handler,
        log: &mut dyn Write,
    ) -> Result<Stop, ServeError> {
        let socket = socket_path(&config.directory, &config.root);
        let listener = bind(&socket)?;
        let _cleanup = Cleanup {
            made: identity(&socket),
            socket: socket.clone(),
        };
        let compiler_sha256 = crate::cache::file_digest(&config.compiler).map_or_else(
            |_| "unknown".to_owned(),
            |digest| digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        );
        let _ = writeln!(
            log,
            "serve: listening on {} for {}",
            socket.display(),
            config.root.display()
        );
        let (sender, receiver) = mpsc::sync_channel(QUEUE);
        let stop = Arc::new(AtomicBool::new(false));
        let busy = Arc::new(AtomicU64::new(0));
        let hello = encode_hello(&config.root);
        let acceptor = {
            let (stop, busy) = (Arc::clone(&stop), Arc::clone(&busy));
            thread::Builder::new()
                .name("tsuzuri-serve-accept".to_owned())
                .spawn(move || accept_loop(listener, sender, &stop, &busy, &hello))
                .map_err(|error| failure(format!("cannot start the accept thread: {error}")))?
        };
        let mut running = Loop {
            config,
            handler,
            log,
            counters: Counters {
                started: Instant::now(),
                requests: 0,
                served: 0,
                refusals: BTreeMap::new(),
            },
            busy: &busy,
            busy_logged: 0,
            compiler_sha256,
        };
        let outcome = running.run(&receiver);
        // The accept thread waits in `accept`: a connection to the socket wakes it. If that
        // fails it is left to end with the process.
        stop.store(true, Ordering::Release);
        if UnixStream::connect(&socket).is_ok() {
            let _ = acceptor.join();
        }
        outcome
    }

    impl Loop<'_> {
        fn run(&mut self, receiver: &Receiver<UnixStream>) -> Result<Stop, ServeError> {
            let mut deadline = Instant::now() + self.config.idle;
            loop {
                let wait = deadline.saturating_duration_since(Instant::now());
                let stream = match receiver.recv_timeout(wait) {
                    Ok(stream) => stream,
                    Err(RecvTimeoutError::Timeout) => {
                        let _ = writeln!(
                            self.log,
                            "serve: idle for {} s; stopping",
                            self.config.idle.as_secs()
                        );
                        return Ok(Stop::Idle);
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(failure("the accept thread ended".to_owned()));
                    }
                };
                self.log_busy();
                if let Some(stop) = self.connection(stream) {
                    self.log_busy();
                    return Ok(stop);
                }
                deadline = Instant::now() + self.config.idle;
            }
        }

        fn log_busy(&mut self) {
            let busy = self.busy.load(Ordering::Acquire);
            if busy > self.busy_logged {
                let _ = writeln!(
                    self.log,
                    "serve: {} connection(s) refused: busy ({QUEUE} requests were waiting)",
                    busy - self.busy_logged
                );
                self.busy_logged = busy;
            }
        }

        /// Writes `text` to the client. A client that went away is not an error of the server.
        fn send(&mut self, stream: &mut UnixStream, text: &str) {
            if let Err(error) = stream.write_all(text.as_bytes()) {
                let _ = writeln!(
                    self.log,
                    "serve: request {}: the reply could not be written ({error})",
                    self.counters.requests
                );
            }
        }

        fn refuse(&mut self, stream: &mut UnixStream, refused: &Refused) {
            *self
                .counters
                .refusals
                .entry(refused.reason.name())
                .or_default() += 1;
            let _ = writeln!(
                self.log,
                "serve: request {} refused: {} ({})",
                self.counters.requests,
                refused.reason.name(),
                refused.message
            );
            self.send(stream, &encode_refusal(refused));
        }

        /// One request. `Some` ends the server.
        fn connection(&mut self, mut stream: UnixStream) -> Option<Stop> {
            let _ = stream.set_write_timeout(Some(RESPONSE_DEADLINE));
            let read = read_line_limited(
                &mut DeadlineReader {
                    stream: &stream,
                    deadline: Instant::now() + self.config.request_deadline,
                },
                MAX_REQUEST_BYTES,
            );
            let line = match read {
                Ok(line) => line,
                // A client that connected to see whether a server runs, or gave up.
                Err(LineError::Closed) => return None,
                Err(LineError::TooLong) => {
                    self.counters.requests += 1;
                    let refused = Refused::new(
                        Refusal::TooLarge,
                        format!("the request is longer than {MAX_REQUEST_BYTES} bytes"),
                    );
                    self.refuse(&mut stream, &refused);
                    drain(&stream);
                    return None;
                }
                Err(LineError::Io(error)) => {
                    let _ = writeln!(self.log, "serve: a request could not be read ({error})");
                    return None;
                }
            };
            self.counters.requests += 1;
            match decode_operation(&line) {
                Err(message) => {
                    self.refuse(&mut stream, &Refused::new(Refusal::Protocol, message));
                    None
                }
                Ok(Operation::Status) => {
                    let reply = self.status_reply();
                    self.send(&mut stream, &reply);
                    None
                }
                Ok(Operation::Shutdown) => {
                    self.send(
                        &mut stream,
                        &super::line(&json!({"protocol": PROTOCOL, "ok": true})),
                    );
                    let _ = writeln!(self.log, "serve: shutdown requested; stopping");
                    Some(Stop::Shutdown)
                }
                Ok(Operation::Run(request)) => self.command(&mut stream, &request),
            }
        }

        fn command(&mut self, stream: &mut UnixStream, request: &Request) -> Option<Stop> {
            let number = self.counters.requests;
            if self.handler.compiler_changed() {
                let refused = Refused::new(
                    Refusal::CompilerChanged,
                    "the compiler file was replaced since the server started",
                );
                self.refuse(stream, &refused);
                let _ = writeln!(
                    self.log,
                    "serve: the compiler binary changed; stopping (start tsuzuri serve again)"
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
                self.refuse(stream, &refused);
                return None;
            }
            match self.handler.run(request, self.log) {
                Ok(response) => {
                    let reply = encode_response(&response);
                    if reply.len() > MAX_RESPONSE_BYTES {
                        let refused = Refused::new(
                            Refusal::TooLarge,
                            format!("the output is longer than {MAX_RESPONSE_BYTES} bytes"),
                        );
                        self.refuse(stream, &refused);
                    } else {
                        self.send(stream, &reply);
                        self.counters.served += 1;
                        let _ = writeln!(
                            self.log,
                            "serve: request {number} served (exit {})",
                            response.exit
                        );
                    }
                }
                Err(refused) => self.refuse(stream, &refused),
            }
            None
        }

        fn status_reply(&self) -> String {
            let busy = self.busy.load(Ordering::Acquire);
            let mut refusals: Map<String, Value> = self
                .counters
                .refusals
                .iter()
                .map(|(name, count)| ((*name).to_owned(), json!(count)))
                .collect();
            if busy > 0 {
                refusals.insert("busy".to_owned(), json!(busy));
            }
            let refused: u64 = self.counters.refusals.values().sum::<u64>() + busy;
            let mut object = Map::new();
            for (key, value) in [
                ("protocol", json!(PROTOCOL)),
                ("ok", json!(true)),
                ("server", json!("tsuzuri")),
                ("version", json!(env!("CARGO_PKG_VERSION"))),
                ("pid", json!(std::process::id())),
                ("root", json!(self.config.root.to_string_lossy())),
                ("compiler", json!(self.config.compiler.to_string_lossy())),
                ("compiler_sha256", json!(self.compiler_sha256)),
                ("idle_seconds", json!(self.config.idle.as_secs())),
                (
                    "uptime_seconds",
                    json!(self.counters.started.elapsed().as_secs()),
                ),
                ("requests", json!(self.counters.requests)),
                ("served", json!(self.counters.served)),
                ("refused", json!(refused)),
                ("refusals", Value::Object(refusals)),
            ] {
                object.insert(key.to_owned(), value);
            }
            for (key, value) in self.handler.status() {
                object.insert(key.to_owned(), value);
            }
            super::line(&Value::Object(object))
        }
    }

    /// Connects, checks the hello, sends `operation`, and reads the reply line. `Err` says which step
    /// failed. The request ends with a newline, which is all that the server waits for, so the write
    /// side stays open (a server that has answered and closed makes `shutdown` fail on macOS).
    pub(super) fn exchange(
        socket: &Path,
        root: &Path,
        operation: &Operation,
        limit: usize,
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
        // The reply to a command comes when the commands before it are done.
        stream
            .set_read_timeout(None)
            .map_err(|error| format!("cannot wait for the reply ({error})"))?;
        stream
            .write_all(encode_operation(operation).as_bytes())
            .map_err(|error| format!("the server did not take the request ({error})"))?;
        read_line_limited(&mut &stream, limit)
            .map_err(|error| format!("the server did not reply ({error})"))
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
        paths
            .into_iter()
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
        fn start(config: Config, mut probe: Probe) -> Self {
            let socket = socket_path(&config.directory, &config.root);
            let root = config.root.clone();
            let thread = thread::spawn(move || {
                let mut log = Vec::new();
                let outcome = serve(&config, &mut probe, &mut log);
                (outcome, String::from_utf8(log).unwrap())
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
    fn an_idle_server_stops_and_removes_its_socket() {
        let scratch = Scratch::new();
        let config = config(&scratch, Duration::from_millis(300));
        let socket = socket_path(&config.directory, &config.root);
        let (outcome, log) = thread::spawn(move || {
            let mut log = Vec::new();
            let outcome = serve(&config, &mut Probe::default(), &mut log);
            (outcome, String::from_utf8(log).unwrap())
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
        let server = Server::start(
            config(&scratch, Duration::from_millis(1500)),
            Probe::default(),
        );
        for _ in 0..3 {
            thread::sleep(Duration::from_millis(600));
            assert!(
                server.ask(&Operation::Status).is_some(),
                "the server is still there"
            );
        }
        let (outcome, _) = server.thread.join().unwrap();
        assert_eq!(outcome, Ok(Stop::Idle));
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
        let again = serve(&config_for(), &mut Probe::default(), &mut Vec::new());
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
        let error = serve(&config_for(), &mut Probe::default(), &mut Vec::new()).unwrap_err();
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
        let error = serve(&long, &mut Probe::default(), &mut Vec::new()).unwrap_err();
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
