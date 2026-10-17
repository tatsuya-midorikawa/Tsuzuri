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
use std::io::{self, Read};
use std::path::{Path, PathBuf};

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
pub const REQUEST_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
/// How long a write of a reply can wait for a client that does not read.
pub const RESPONSE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);
/// How long a client waits for the hello of a server (the accept thread writes it at once).
pub const HELLO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
pub const DEFAULT_IDLE_SECONDS: u64 = 1800;
pub const MAX_IDLE_SECONDS: u64 = 86400;

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
