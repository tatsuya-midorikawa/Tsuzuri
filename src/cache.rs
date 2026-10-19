use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::driver::TemporaryDirectory;
use serde_json::{Value, json};

const FORMAT: u64 = 1;
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MARKER: &[u8] = b"tsuzuri whole-build cache v1\n";
const RUNTIME_FORMAT: u64 = 2;
const UNIT_FORMAT: u64 = 1;
/// How long a unit object counts as used after a load (see `BuildCache::for_units`).
const UNIT_TOUCH_MS: u64 = 10 * 60 * 1000;
/// The shortest time between two cleanups of the unit cache.
pub(crate) const UNIT_EVICT_INTERVAL_MS: u64 = 10 * 60 * 1000;
/// The file whose time is that of the last cleanup of a cache (`BuildCache::evict_if_due`).
const EVICTED: &str = ".evicted";

pub(crate) const KEY_ENVIRONMENT: &[&str] = &[
    "TSUZURI_CLANG",
    "TSUZURI_WASM_LD",
    "TSUZURI_LLVM_LINK",
    "TSUZURI_DSYMUTIL",
    crate::driver::SYSROOT_VARIABLE,
    "PATH",
    "PATHEXT",
    "SDKROOT",
    "MACOSX_DEPLOYMENT_TARGET",
    "INCLUDE",
    "LIB",
    "LIBPATH",
    "CPATH",
    "C_INCLUDE_PATH",
    "CPLUS_INCLUDE_PATH",
    "LIBRARY_PATH",
    "SOURCE_DATE_EPOCH",
    "DEVELOPER_DIR",
    "TSUZURI_ZIG",
    "TSUZURI_LLVM_CLANG",
    crate::llvm::RANGE_FLAGS_VARIABLE,
    crate::llvm::STRINGS_VARIABLE,
];

pub(crate) struct ToolIdentity {
    fields: Vec<(&'static str, Vec<u8>)>,
    bundle_root: Option<PathBuf>,
    bundled_resource_fingerprint: std::sync::OnceLock<Option<String>>,
}

/// The fingerprint of every resource of the distribution at `root`, read in full; `tool_identity` finds it
/// through `bundle_identity::resolve` instead, which remembers it between builds.
fn bundled_resource_fingerprint(root: &Path) -> Option<String> {
    let compiler = format!("bin/tsuzuri{}", std::env::consts::EXE_SUFFIX);
    Some(crate::bundle_identity::inventory(root, &compiler, &file_digest)?.fingerprint())
}

impl ToolIdentity {
    fn new(fields: Vec<(&'static str, Vec<u8>)>, bundle_root: Option<PathBuf>) -> Self {
        Self {
            fields,
            bundle_root,
            bundled_resource_fingerprint: std::sync::OnceLock::new(),
        }
    }

    /// An identity whose bundled-resource fingerprint is already known: `None` when a resource is unreadable
    /// or a symbolic link, which leaves the identity without a cache key.
    fn with_fingerprint(
        fields: Vec<(&'static str, Vec<u8>)>,
        bundle_root: PathBuf,
        fingerprint: Option<String>,
    ) -> Self {
        let identity = Self::new(fields, Some(bundle_root));
        let _ = identity.bundled_resource_fingerprint.set(fingerprint);
        identity
    }

    fn hash_into(&self, hash: &mut Sha256) -> bool {
        for (name, bytes) in &self.fields {
            if *name == "bundled-resources" {
                let Some(root) = self.bundle_root.as_deref() else {
                    return false;
                };
                let fingerprint = self
                    .bundled_resource_fingerprint
                    .get_or_init(|| bundled_resource_fingerprint(root));
                let Some(fingerprint) = fingerprint else {
                    return false;
                };
                hash.field(name, fingerprint.as_bytes());
            } else {
                hash.field(name, bytes);
            }
        }
        true
    }

    pub(crate) fn matches_bundled_tool(&self, name: &str, expected: &str) -> bool {
        let suffix = std::env::consts::EXE_SUFFIX;
        let field = if name == format!("bin/tsuzuri-clang{suffix}") {
            "tool-binary"
        } else if name == format!("bin/clang{suffix}") {
            "bundled-clang-binary"
        } else if name == format!("zig/zig{suffix}") {
            "bundled-zig-binary"
        } else {
            return false;
        };
        self.fields.iter().any(|(key, bytes)| {
            *key == field
                && bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
                    == expected
        })
    }

    pub(crate) fn matches_bundled_resources(&self, expected: &str) -> bool {
        let Some(root) = self.bundle_root.as_deref() else {
            return false;
        };
        if !self
            .fields
            .iter()
            .any(|(name, _)| *name == "bundled-resources")
        {
            return false;
        }
        self.bundled_resource_fingerprint
            .get_or_init(|| bundled_resource_fingerprint(root))
            .as_deref()
            == Some(expected)
    }
}

pub(crate) fn tool_identity(
    variable: &str,
    fallback: &str,
    cpu: crate::driver::Cpu,
) -> io::Result<ToolIdentity> {
    let tool = crate::driver::resolve_tool(variable, fallback).1;
    let invoked = invoked_path(&tool)?;
    let path = real_path(&invoked)?;
    let launcher = path.file_stem().is_some_and(|name| name == "tsuzuri-clang");
    // An override points the launcher at tools outside the distribution, which a remembered identity cannot see.
    let overridden = std::env::var_os("TSUZURI_LLVM_CLANG").is_some()
        || std::env::var_os("TSUZURI_ZIG").is_some();
    let mut identity = if launcher && !overridden {
        bundled_identity(&path)?
    } else {
        read_identity(&invoked, &path, launcher)?
    };
    if variable == "TSUZURI_CLANG" && cpu == crate::driver::Cpu::Native {
        let output = std::process::Command::new(&invoked)
            .args(["-dM", "-E", "-x", "c", "-"])
            .arg(if matches!(std::env::consts::ARCH, "arm" | "aarch64") {
                "-mcpu=native"
            } else {
                "-march=native"
            })
            .stdin(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(invalid("native CPU feature query failed"));
        }
        identity.fields.push(("native-cpu-features", output.stdout));
        identity
            .fields
            .push(("native-cpu-diagnostics", output.stderr));
    }
    Ok(identity)
}

/// What `--version` of the tool printed: standard output, then standard error.
fn version_of(path: &Path) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let output = std::process::Command::new(path).arg("--version").output()?;
    if !output.status.success() {
        return Err(invalid("tool version query failed"));
    }
    Ok((output.stdout, output.stderr))
}

/// The identity of a tool that is read in full on every call, with the Clang and Zig that a bundled
/// launcher names (the overrides included) when `bundled` is set. `invoked` runs it, `path` is the file behind it.
fn read_identity(invoked: &Path, path: &Path, bundled: bool) -> io::Result<ToolIdentity> {
    let (stdout, stderr) = version_of(invoked)?;
    let mut fields = vec![
        ("tool-path", path.as_os_str().as_encoded_bytes().to_vec()),
        ("tool-binary", file_digest(path)?.to_vec()),
        ("tool-version-stdout", stdout),
        ("tool-version-stderr", stderr),
    ];
    let mut bundle_root = None;
    // The launcher alone does not identify the Clang or C SDK bundled beside it.
    if bundled {
        let root = path
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| invalid("invalid bundled compiler path"))?;
        bundle_root = Some(root.to_path_buf());
        fields.push(("bundled-resources", Vec::new()));
        let clang = std::env::var_os("TSUZURI_LLVM_CLANG").unwrap_or_else(|| {
            root.join("bin")
                .join(format!("clang{}", std::env::consts::EXE_SUFFIX))
                .into()
        });
        fields.push((
            "bundled-clang-binary",
            file_digest(&executable_path(&clang)?)?.to_vec(),
        ));
        let zig_override = std::env::var_os("TSUZURI_ZIG");
        let zig = zig_override.clone().map(PathBuf::from).unwrap_or_else(|| {
            root.join("zig")
                .join(format!("zig{}", std::env::consts::EXE_SUFFIX))
        });
        if zig_override.is_some() || zig.is_file() {
            fields.push((
                "bundled-zig-binary",
                file_digest(&executable_path(zig.as_os_str())?)?.to_vec(),
            ));
        }
    }
    Ok(ToolIdentity::new(fields, bundle_root))
}

/// The identity of the launcher `<root>/bin/tsuzuri-clang` with the Clang and Zig of its distribution, with the
/// same fields as `read_identity`. The fingerprint of the distribution's files, the tool digests and the
/// launcher's `--version` come from `bundle_identity::resolve`, which hashes the distribution once and
/// remembers the result in the cache directory, so that a build does not read 760 MB again.
fn bundled_identity(path: &Path) -> io::Result<ToolIdentity> {
    let root = path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| invalid("invalid bundled compiler path"))?;
    let suffix = std::env::consts::EXE_SUFFIX;
    let clang = root.join("bin").join(format!("clang{suffix}"));
    let zig = root.join("zig").join(format!("zig{suffix}"));
    let compiler = format!("bin/tsuzuri{suffix}");
    let request = crate::bundle_identity::Request {
        root,
        launcher: path,
        clang: &clang,
        zig: zig.is_file().then_some(zig.as_path()),
        compiler: &compiler,
    };
    // Opening the root first marks it as a cache: a nonempty directory without the marker is refused later.
    let memo = default_root()
        .filter(|root| BuildCache::open(root).is_ok())
        .map(|root| root.join("bundle"));
    let resolved = crate::bundle_identity::resolve(
        &request,
        memo.as_deref(),
        &crate::bundle_identity::Clock::system(),
        &file_digest,
        &|| version_of(path),
    )?;
    let mut fields = vec![
        ("tool-path", path.as_os_str().as_encoded_bytes().to_vec()),
        ("tool-binary", resolved.launcher.to_vec()),
        ("tool-version-stdout", resolved.version.0),
        ("tool-version-stderr", resolved.version.1),
        ("bundled-resources", Vec::new()),
        ("bundled-clang-binary", resolved.clang.to_vec()),
    ];
    if let Some(zig) = resolved.zig {
        fields.push(("bundled-zig-binary", zig.to_vec()));
    }
    Ok(ToolIdentity::with_fingerprint(
        fields,
        root.to_path_buf(),
        resolved.fingerprint,
    ))
}

pub(crate) fn runtime_key(
    unit: &str,
    source: &[u8],
    args: &[std::ffi::OsString],
    clang: &ToolIdentity,
) -> Option<String> {
    let mut hash = Sha256::new();
    hash.field("format", &RUNTIME_FORMAT.to_le_bytes());
    hash.field("kind", b"runtime-object");
    hash.field("unit", unit.as_bytes());
    hash.field("host-os", std::env::consts::OS.as_bytes());
    hash.field("host-arch", std::env::consts::ARCH.as_bytes());
    hash.field("source", source);
    for arg in args {
        hash.field("arg", arg.as_encoded_bytes());
    }
    for &variable in KEY_ENVIRONMENT {
        let value = std::env::var_os(variable);
        hash.field(
            variable,
            value.as_ref().map_or(&[], |value| value.as_encoded_bytes()),
        );
    }
    if !clang.hash_into(&mut hash) {
        return None;
    }
    Some(hash.hex())
}

/// What the object of every codegen unit of a build has in common (PB07): the Clang that compiles the units, the
/// arguments of that compile, and the environment that Clang reads. The compiler itself is not part of it: an
/// object is what that Clang makes of the text of the unit with those arguments.
pub(crate) fn unit_key_prefix(
    args: &[std::ffi::OsString],
    clang: &ToolIdentity,
) -> Option<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.field("format", &UNIT_FORMAT.to_le_bytes());
    hash.field("kind", b"codegen-unit");
    hash.field("host-os", std::env::consts::OS.as_bytes());
    hash.field("host-arch", std::env::consts::ARCH.as_bytes());
    for arg in args {
        hash.field("arg", arg.as_encoded_bytes());
    }
    for &variable in KEY_ENVIRONMENT {
        let value = std::env::var_os(variable);
        hash.field(
            variable,
            value.as_ref().map_or(&[], |value| value.as_encoded_bytes()),
        );
    }
    if !clang.hash_into(&mut hash) {
        return None;
    }
    Some(hash.finalize())
}

/// The key of the object of a unit whose text is `text`.
pub(crate) fn unit_key(prefix: &[u8; 32], text: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(prefix);
    hash.update(text.as_bytes());
    hash.hex()
}

pub(crate) struct CachedFile {
    pub bytes: Vec<u8>,
    pub mode: u32,
}

pub(crate) struct CachedArtifact {
    pub files: BTreeMap<String, CachedFile>,
    pub messages: Vec<String>,
}

pub(crate) struct BuildCache {
    root: PathBuf,
    max_bytes: u64,
    max_age_ms: u64,
    /// How many entries a cleanup looks at, and how many it removes.
    scan_entries: usize,
    remove_entries: usize,
    /// A load writes the last use of an entry again only when the one that is written is older than this.
    touch_ms: u64,
    /// A cleanup takes the time and the size of an entry from the file system, without reading its metadata.
    file_system_stats: bool,
}

struct CacheLock(PathBuf);

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn stage(root: &Path) -> io::Result<TemporaryDirectory> {
    TemporaryDirectory::new(root).map_err(|error| io::Error::other(error.message))
}

fn key_name(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn read_regular(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > limit {
        return Err(invalid("invalid cache file type or size"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("cache file exceeds its size limit"));
    }
    Ok(bytes)
}

pub(crate) fn default_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("TSUZURI_CACHE_DIR") {
        return (!root.is_empty()).then(|| PathBuf::from(root));
    }
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|root| PathBuf::from(root).join("Tsuzuri/Cache/build-cache"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|root| PathBuf::from(root).join("Library/Caches/tsuzuri/build-cache"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|root| PathBuf::from(root).join(".cache")))
            .map(|root| root.join("tsuzuri/build-cache"))
    }
}

/// The package store under the cache root, where `tsuzuri fetch` keeps each
/// downloaded git package as `git/<content sha256>/`. Eviction never touches it.
pub(crate) fn package_store() -> Option<PathBuf> {
    default_root().map(|root| root.join("packages"))
}

/// The parts of the output path that the artifact records. Other artifacts are reused for any path.
fn hash_output_path(
    hash: &mut Sha256,
    options: crate::driver::BuildOptions,
    output: &Path,
) -> io::Result<()> {
    use crate::driver::Emit;
    // The DWARF of a macOS debug executable or library depends on its path.
    if options.platform().mach_o()
        && options.debug_info
        && matches!(options.emit, Emit::Executable | Emit::Shared)
    {
        let absolute = std::path::absolute(output)?;
        hash.field("debug-output-path", absolute.as_os_str().as_encoded_bytes());
    }
    // A shared library records its file name as its install name (macOS) or soname (Linux).
    if options.emit == Emit::Shared {
        let name = output.file_name().unwrap_or_default();
        hash.field("shared-library-name", name.as_encoded_bytes());
    }
    // A Windows debug executable names its PDB, `<output>.pdb`, without a directory (G16).
    if options.platform().windows() && options.debug_info && options.emit == Emit::Executable {
        let pdb = crate::driver::pdb_path(output);
        let name = pdb.file_name().unwrap_or_default();
        hash.field("pdb-name", name.as_encoded_bytes());
    }
    Ok(())
}

/// The SHA-256 of the compiler binary that is running; a process that stays keeps the digest of the
/// file that it started from, which the path may no longer hold (see `resident::compiler_digest`).
fn compiler_digest() -> io::Result<[u8; 32]> {
    let executable = std::env::current_exe()?;
    crate::resident::compiler_digest(|| file_digest(&executable))
}

pub(crate) fn build_key(
    project: &crate::driver::Project,
    options: crate::driver::BuildOptions,
    action: &str,
    ir: &str,
    output: &Path,
    clang: Option<&ToolIdentity>,
    profile: Option<(&str, &[u8])>,
) -> io::Result<String> {
    use crate::driver::{Emit, Target};
    let mut hash = Sha256::new();
    hash.field("format", &FORMAT.to_le_bytes());
    hash.field("compiler-version", env!("CARGO_PKG_VERSION").as_bytes());
    hash.field("compiler-binary", &compiler_digest()?);
    hash.field("host-os", std::env::consts::OS.as_bytes());
    hash.field("host-arch", std::env::consts::ARCH.as_bytes());
    hash.field("options", format!("{options:?}").as_bytes());
    hash.field("action", action.as_bytes());
    hash_output_path(&mut hash, options, output)?;
    hash.field("ir-and-embedded-runtime", ir.as_bytes());
    // The profile that Clang reads, or the directory that an instrumented program writes to (PR07). A
    // build without one adds nothing, so its key is the one that there was before.
    if let Some((tag, bytes)) = profile {
        hash.field(tag, bytes);
    }
    for source in project.sources.iter().chain(&project.manifests) {
        hash.field("path", source.path.as_os_str().as_encoded_bytes());
        hash.field(
            "relative-path",
            source.relative_path.as_os_str().as_encoded_bytes(),
        );
        hash.field("source", source.text.as_bytes());
        hash.field(
            "origin",
            format!("{:?}:{:?}", source.origin, source.package).as_bytes(),
        );
    }
    for &variable in KEY_ENVIRONMENT {
        let value = std::env::var_os(variable);
        hash.field(
            variable,
            value.as_ref().map_or(&[], |value| value.as_encoded_bytes()),
        );
    }
    // The linker that `TSUZURI_LINKER` asks for changes a native executable or shared library. Unset and `default`
    // add nothing, so their keys are the ones that were made before the variable existed.
    if options.target == Target::Native
        && matches!(options.emit, Emit::Executable | Emit::Shared)
        && let Some(linker) = std::env::var_os(crate::driver::LINKER_VARIABLE)
            .filter(|value| !value.is_empty() && value != "default")
    {
        hash.field("linker", linker.as_encoded_bytes());
    }
    // A native executable may be built from codegen units (PB07), which changes its bytes: `single`, `auto` and
    // the unset variable keep apart what they make. A build that cannot split has the same bytes whatever it says,
    // but the key has no need to know which those are.
    if options.target == Target::Native
        && options.emit == Emit::Executable
        && let Some(units) = std::env::var_os(crate::driver::CODEGEN_UNITS_VARIABLE)
    {
        hash.field("codegen-units", units.as_encoded_bytes());
    }
    let mut tools = Vec::new();
    if !(matches!(options.emit, Emit::Llvm | Emit::Header) || options.emit.is_kernel()) {
        tools.push(("TSUZURI_CLANG", "clang"));
        if options.emit == Emit::Wasm || options.wasm_threads || options.wasm_host.is_some() {
            tools.push(("TSUZURI_WASM_LD", "wasm-ld"));
        }
        // `--wasm-opt` rewrites the module with the tool that is installed, so its identity is the key's.
        if options.wasm_opt && options.emit == Emit::Wasm {
            tools.push((crate::driver::WASM_OPT_VARIABLE, "wasm-opt"));
        }
        if options.platform().mach_o() && options.debug_info && options.target == Target::Native {
            if matches!(options.emit, Emit::Executable | Emit::Shared) {
                tools.push(("TSUZURI_DSYMUTIL", "dsymutil"));
            }
            // The fast backend never gets here: it refuses a macOS debug object that needs the C runtime.
            if options.emit == Emit::Object
                && options.backend == crate::driver::Backend::Llvm
                && ir.contains("@tsuzuri_task_parallel(")
            {
                tools.push(("TSUZURI_LLVM_LINK", "llvm-link"));
            }
        }
    }
    for (variable, fallback) in tools {
        if variable == "TSUZURI_CLANG"
            && let Some(identity) = clang
        {
            identity.hash_into(&mut hash);
        } else {
            tool_identity(variable, fallback, options.cpu)?.hash_into(&mut hash);
        }
    }
    Ok(hash.hex())
}

/// `fs::canonicalize` without the Windows verbatim prefix (`\\?\`) when the plain path
/// names the same file, so users and child tools see `C:\...`.
pub(crate) fn real_path(path: &Path) -> io::Result<PathBuf> {
    let real = fs::canonicalize(path)?;
    let plain = real
        .to_str()
        .and_then(plain_path)
        .filter(|plain| fs::canonicalize(plain).is_ok_and(|again| again == real))
        .map(PathBuf::from);
    Ok(plain.unwrap_or(real))
}

/// The path that a verbatim path stands for: `\\?\C:\dir` is `C:\dir`, and `\\?\UNC\server\share\dir`
/// is `\\server\share\dir`. Any other path has no verbatim prefix to remove.
fn plain_path(verbatim: &str) -> Option<String> {
    let rest = verbatim.strip_prefix(r"\\?\")?;
    Some(match rest.strip_prefix(r"UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => rest.to_owned(),
    })
}

pub fn executable_path(tool: &std::ffi::OsStr) -> io::Result<PathBuf> {
    real_path(&invoked_path(tool)?)
}

/// The path that runs `tool`: the tool itself, or its first match on `PATH`, absolute but not resolved through
/// links. A multi-call binary chooses its behavior by the name it is called by, and Homebrew's `wasm-ld` is a
/// link to `lld`, which answers neither `--version` nor a link under its own name.
pub(crate) fn invoked_path(tool: &std::ffi::OsStr) -> io::Result<PathBuf> {
    invoked_path_in(tool, &std::env::var_os("PATH").unwrap_or_default())
}

fn invoked_path_in(tool: &std::ffi::OsStr, search: &std::ffi::OsStr) -> io::Result<PathBuf> {
    let path = Path::new(tool);
    if path.components().count() > 1 || path.is_absolute() {
        return std::path::absolute(path);
    }
    for root in std::env::split_paths(search) {
        let candidate = root.join(tool);
        if candidate.is_file() {
            return std::path::absolute(candidate);
        }
        if cfg!(windows) && candidate.extension().is_none() {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return std::path::absolute(candidate);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "compiler tool was not found on PATH",
    ))
}

impl BuildCache {
    pub(crate) fn open(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(invalid("cache root must be a real directory"));
        }
        let marker = root.join(".tsuzuri-cache");
        if !marker.exists() {
            if fs::read_dir(root)?.next().is_some() {
                return Err(invalid(
                    "existing nonempty directory is not a Tsuzuri cache",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
            }
            use std::io::Write;
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)
            {
                Ok(mut file) => file.write_all(MARKER)?,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        if read_regular(&marker, 64)? != MARKER {
            return Err(invalid("cache ownership marker is invalid"));
        }
        Ok(Self {
            root: fs::canonicalize(root)?,
            max_bytes: 2 * 1024 * 1024 * 1024,
            max_age_ms: 30 * 24 * 60 * 60 * 1000,
            scan_entries: 4096,
            remove_entries: 128,
            touch_ms: 0,
            file_system_stats: false,
        })
    }

    /// The same cache for the objects of codegen units (PB07), of which a build reads and writes thousands: the
    /// last use of an entry is kept to ten minutes, a cleanup looks at 32,768 entries without reading their
    /// metadata, and `evict_if_due` runs it at most once in ten minutes.
    pub(crate) fn for_units(self) -> Self {
        Self {
            scan_entries: 32768,
            remove_entries: 1024,
            touch_ms: UNIT_TOUCH_MS,
            file_system_stats: true,
            ..self
        }
    }

    fn metadata(&self, key: &str) -> io::Result<Value> {
        if !key_name(key) {
            return Err(invalid("invalid build cache key"));
        }
        let directory = self.root.join(key);
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(invalid("invalid cache entry directory"));
        }
        let bytes = read_regular(&directory.join("metadata.json"), 1024 * 1024)?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid cache metadata"))?;
        if value["format"].as_u64() != Some(FORMAT) || value["key"].as_str() != Some(key) {
            return Err(invalid("incompatible cache metadata"));
        }
        Ok(value)
    }

    pub(crate) fn load(&self, key: &str) -> io::Result<Option<CachedArtifact>> {
        let mut metadata = match self.metadata(key) {
            Ok(value) => value,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidData
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let load = || -> io::Result<CachedArtifact> {
            let files = metadata["files"]
                .as_object()
                .ok_or_else(|| invalid("missing cache files"))?;
            if !files.contains_key("artifact") || files.len() > 3 {
                return Err(invalid("invalid cache artifact set"));
            }
            let mut remaining = MAX_ENTRY_BYTES;
            let mut result = BTreeMap::new();
            for (name, fields) in files {
                if !matches!(
                    name.as_str(),
                    "artifact" | "traps" | "dwarf" | "pdb" | "profile"
                ) {
                    return Err(invalid("invalid cache artifact name"));
                }
                let size = fields["size"]
                    .as_u64()
                    .ok_or_else(|| invalid("missing cache size"))?;
                if size > remaining {
                    return Err(invalid("cache artifact is too large"));
                }
                remaining -= size;
                let bytes = read_regular(&self.root.join(key).join(name), size)?;
                let mut hash = Sha256::new();
                hash.update(&bytes);
                if bytes.len() as u64 != size
                    || fields["sha256"].as_str() != Some(hash.hex().as_str())
                {
                    return Err(invalid("cache artifact checksum mismatch"));
                }
                let mode = fields["mode"]
                    .as_u64()
                    .filter(|mode| *mode <= 0o777)
                    .ok_or_else(|| invalid("invalid cache file permissions"))?
                    as u32;
                result.insert(name.clone(), CachedFile { bytes, mode });
            }
            let messages = metadata["messages"]
                .as_array()
                .ok_or_else(|| invalid("missing cached diagnostics"))?
                .iter()
                .map(|message| {
                    message
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("invalid cached diagnostic"))
                })
                .collect::<io::Result<_>>()?;
            Ok(CachedArtifact {
                files: result,
                messages,
            })
        };
        let artifact = match load() {
            Ok(value) => value,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidData
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let now = now_ms();
        let used = metadata["last_used_unix_ms"].as_u64().unwrap_or(0);
        if self.touch_ms == 0 || now.saturating_sub(used) >= self.touch_ms {
            metadata["last_used_unix_ms"] = json!(now);
            let temporary = stage(&self.root)?;
            let staged = temporary.path.join("metadata.json");
            fs::write(&staged, serde_json::to_vec(&metadata)?)?;
            fs::rename(staged, self.root.join(key).join("metadata.json"))?;
        }
        Ok(Some(artifact))
    }

    pub(crate) fn store(
        &self,
        key: &str,
        paths: &BTreeMap<String, PathBuf>,
        messages: &[String],
    ) -> io::Result<()> {
        if self.store_entry(key, paths, messages)? {
            self.evict()
        } else {
            Ok(())
        }
    }

    /// [`store`](Self::store) without the cleanup, which a caller that stores many entries runs once at the end.
    /// `false` when another process holds the entry or it is there already.
    pub(crate) fn store_entry(
        &self,
        key: &str,
        paths: &BTreeMap<String, PathBuf>,
        messages: &[String],
    ) -> io::Result<bool> {
        if !key_name(key) {
            return Err(invalid("invalid build cache key"));
        }
        let lock = self.root.join(format!(".lock-{key}"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(error) => return Err(error),
        }
        let _lock = CacheLock(lock);
        if self.load(key)?.is_some() {
            return Ok(false);
        }
        let temporary = stage(&self.root)?;
        let entry = temporary.path.join("entry");
        fs::create_dir(&entry)?;
        let mut remaining = MAX_ENTRY_BYTES;
        let mut files = serde_json::Map::new();
        for (name, source) in paths {
            if !matches!(
                name.as_str(),
                "artifact" | "traps" | "dwarf" | "pdb" | "profile"
            ) {
                return Err(invalid("invalid cache artifact name"));
            }
            let bytes = read_regular(source, remaining)?;
            remaining -= bytes.len() as u64;
            let mut hash = Sha256::new();
            hash.update(&bytes);
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                fs::metadata(source)?.permissions().mode() & 0o777
            };
            #[cfg(not(unix))]
            let mode = 0;
            files.insert(
                name.clone(),
                json!({ "size": bytes.len(), "sha256": hash.hex(), "mode": mode }),
            );
            fs::write(entry.join(name), bytes)?;
        }
        let metadata = json!({ "format": FORMAT, "key": key, "files": files, "messages": messages, "created_unix_ms": now_ms(), "last_used_unix_ms": now_ms() });
        let bytes = serde_json::to_vec(&metadata)?;
        if bytes.len() > 1024 * 1024 {
            return Err(invalid("cache metadata exceeds 1 MiB"));
        }
        fs::write(entry.join("metadata.json"), bytes)?;
        let destination = self.root.join(key);
        if destination.exists() && self.load(key)?.is_none() {
            match fs::rename(&destination, temporary.path.join("previous")) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        match fs::rename(entry, &destination) {
            Ok(()) => {}
            Err(_) if self.load(key)?.is_some() => {}
            Err(error) => return Err(error),
        }
        Ok(true)
    }

    /// [`evict`](Self::evict), unless a cleanup of this cache ran less than `interval_ms` ago.
    pub(crate) fn evict_if_due(&self, interval_ms: u64) -> io::Result<()> {
        let marker = self.root.join(EVICTED);
        let due = fs::metadata(&marker)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_none_or(|age| age.as_millis() >= u128::from(interval_ms));
        if !due {
            return Ok(());
        }
        fs::write(&marker, b"")?;
        self.evict()
    }

    pub(crate) fn evict(&self) -> io::Result<()> {
        let mut entries = Vec::new();
        let mut total = 0u64;
        for entry in fs::read_dir(&self.root)?.take(self.scan_entries) {
            let entry = entry?;
            let name = entry.file_name();
            let Some(key) = name.to_str().filter(|key| {
                key_name(key)
                    || key.strip_prefix(".lock-").is_some_and(key_name)
                    // Staging directories, but never the ownership marker.
                    || key.starts_with(".tsuzuri-") && *key != ".tsuzuri-cache"
            }) else {
                continue;
            };
            let from_file_system = |entry: &fs::DirEntry| -> io::Result<(u64, u64)> {
                let metadata = fs::symlink_metadata(entry.path())?;
                let modified = metadata
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64;
                let size = if metadata.is_dir() && key_name(key) {
                    fs::metadata(entry.path().join("artifact")).map_or(0, |artifact| artifact.len())
                } else {
                    0
                };
                Ok((size, modified))
            };
            let (size, used) = if self.file_system_stats {
                from_file_system(&entry)?
            } else {
                match self.metadata(key) {
                    Ok(metadata) => (
                        metadata["files"]
                            .as_object()
                            .map(|files| {
                                files
                                    .values()
                                    .filter_map(|file| file["size"].as_u64())
                                    .fold(0u64, u64::saturating_add)
                            })
                            .unwrap_or(0),
                        metadata["last_used_unix_ms"].as_u64().unwrap_or(0),
                    ),
                    Err(_) => {
                        let (_, modified) = from_file_system(&entry)?;
                        (0, modified)
                    }
                }
            };
            total = total.saturating_add(size);
            entries.push((used, key.to_owned(), size));
        }
        entries.sort();
        let now = now_ms();
        for (used, key, size) in entries.into_iter().take(self.remove_entries) {
            if total <= self.max_bytes && now.saturating_sub(used) <= self.max_age_ms {
                break;
            }
            if !key_name(&key) && now.saturating_sub(used) <= self.max_age_ms {
                continue;
            }
            let temporary = stage(&self.root)?;
            match fs::rename(self.root.join(key), temporary.path.join("expired")) {
                Ok(()) => total = total.saturating_sub(size),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

const ROUND: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffered: 0,
            length: 0,
        }
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        if self.buffered != 0 {
            let count = bytes.len().min(64 - self.buffered);
            self.buffer[self.buffered..self.buffered + count].copy_from_slice(&bytes[..count]);
            self.buffered += count;
            bytes = &bytes[count..];
            if self.buffered != 64 {
                return;
            }
            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
        }
        while bytes.len() >= 64 {
            self.compress(bytes[..64].try_into().unwrap());
            bytes = &bytes[64..];
        }
        self.buffer[..bytes.len()].copy_from_slice(bytes);
        self.buffered = bytes.len();
    }

    pub fn field(&mut self, tag: &str, bytes: &[u8]) {
        self.update(&(tag.len() as u64).to_le_bytes());
        self.update(tag.as_bytes());
        self.update(&(bytes.len() as u64).to_le_bytes());
        self.update(bytes);
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut words = [0u32; 64];
        for (index, bytes) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(bytes.try_into().unwrap());
        }
        for index in 16..64 {
            let low = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let high = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(low)
                .wrapping_add(words[index - 7])
                .wrapping_add(high);
        }
        // The eight working variables stay in registers instead of an array rotated each round.
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for (word, round) in words.into_iter().zip(ROUND) {
            let sigma = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let first = h
                .wrapping_add(sigma)
                .wrapping_add(choose)
                .wrapping_add(round)
                .wrapping_add(word);
            let sigma = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let second = sigma.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }
        for (state, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state = state.wrapping_add(value);
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        self.update(&[0x80]);
        let padding = if self.buffered <= 56 {
            56 - self.buffered
        } else {
            120 - self.buffered
        };
        self.update(&[0; 64][..padding]);
        self.update(&bits.to_be_bytes());
        let mut output = [0; 32];
        for (word, bytes) in self.state.iter().zip(output.chunks_exact_mut(4)) {
            bytes.copy_from_slice(&word.to_be_bytes());
        }
        output
    }

    pub fn hex(self) -> String {
        self.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

pub fn file_digest(path: &Path) -> io::Result<[u8; 32]> {
    // A resident process remembers a digest while the stamp of its file is the same, and then does not open the file.
    let started = crate::resident::clock();
    let stamp = started
        .and_then(|_| std::fs::metadata(path).ok())
        .map(|metadata| crate::resident::FileStamp::from_metadata(&metadata));
    if let Some(stamp) = &stamp
        && let Some(digest) = crate::resident::cached_digest(path, stamp)
    {
        return Ok(digest);
    }
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let digest = loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break hash.finalize();
        }
        hash.update(&buffer[..count]);
    };
    if let (Some(started), Some(stamp)) = (started, stamp) {
        crate::resident::remember_digest(path, stamp, started, digest);
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_that_stays_keys_its_builds_by_the_compiler_that_it_started_from() {
        use crate::resident::{ResidentState, Scope};
        // The digest that was taken before the file was replaced is the one that goes into a key.
        let mut state = ResidentState::pinned_to([9; 32]).with_compiler_digest([7; 32]);
        {
            let _scope = Scope::enter(&mut state);
            assert_eq!(compiler_digest().unwrap(), [7; 32]);
        }
        // A process whose file was replaced before the first key has no digest of its image, and
        // the file at the path is not hashed in its place.
        let mut state = ResidentState::pinned_to([9; 32]);
        let _scope = Scope::enter(&mut state);
        let error = compiler_digest().unwrap_err();
        assert!(error.to_string().contains("was replaced"), "{error}");
    }

    #[test]
    fn plain_path_removes_the_verbatim_prefix_of_drive_and_share_paths() {
        assert_eq!(
            plain_path(r"\\?\C:\work\tmp").as_deref(),
            Some(r"C:\work\tmp")
        );
        assert_eq!(
            plain_path(r"\\?\UNC\server\share\tmp").as_deref(),
            Some(r"\\server\share\tmp")
        );
        assert_eq!(plain_path(r"C:\work\tmp"), None);
        assert_eq!(plain_path(r"\\server\share\tmp"), None);
        assert_eq!(plain_path("/work/tmp"), None);
    }

    /// The administrative share of the system drive is a UNC path that every Windows machine can reach;
    /// a machine that does not offer it skips the check.
    #[cfg(windows)]
    #[test]
    fn real_path_names_a_share_without_the_verbatim_prefix() {
        let Some(system) = std::env::var_os("SystemRoot") else {
            return;
        };
        let system = system.to_string_lossy().into_owned();
        let Some((drive, rest)) = system.split_once(":\\") else {
            return;
        };
        let share = format!(r"\\localhost\{drive}$\{rest}");
        let Ok(canonical) = fs::canonicalize(&share) else {
            return;
        };
        assert!(
            canonical.to_string_lossy().starts_with(r"\\?\UNC\"),
            "{canonical:?}"
        );
        assert_eq!(
            real_path(Path::new(&share)).unwrap().to_string_lossy(),
            share
        );
    }

    #[test]
    fn cache_checks_content_partial_entries_races_and_eviction() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let root = temporary.path.join("cache");
        let cache = BuildCache::open(&root).unwrap();
        let source = temporary.path.join("source");
        fs::write(&source, b"artifact").unwrap();
        let mut hash = Sha256::new();
        hash.field("input", b"first");
        let key = hash.hex();
        let paths = BTreeMap::from([("artifact".into(), source)]);
        cache.store(&key, &paths, &["message".into()]).unwrap();
        let loaded = cache.load(&key).unwrap().unwrap();
        assert_eq!(loaded.files["artifact"].bytes, b"artifact");
        assert_eq!(loaded.messages, ["message"]);
        fs::write(root.join(&key).join("artifact"), b"corrupt!").unwrap();
        assert!(cache.load(&key).unwrap().is_none());
        fs::remove_file(root.join(&key).join("metadata.json")).unwrap();
        assert!(cache.load(&key).unwrap().is_none());
        std::thread::scope(|scope| {
            let mut threads = Vec::new();
            for _ in 0..8 {
                threads.push(scope.spawn(|| cache.store(&key, &paths, &[])));
            }
            for thread in threads {
                thread.join().unwrap().unwrap();
            }
        });
        assert_eq!(
            cache.load(&key).unwrap().unwrap().files["artifact"].bytes,
            b"artifact"
        );
        let tiny = BuildCache {
            max_bytes: 0,
            ..cache
        };
        tiny.evict().unwrap();
        assert!(tiny.load(&key).unwrap().is_none());
        fs::write(root.join("do-not-delete"), b"user file").unwrap();
        tiny.evict().unwrap();
        assert!(root.join("do-not-delete").exists());
    }

    #[test]
    fn sidecars_round_trip_and_unknown_names_are_refused() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let cache = BuildCache::open(&temporary.path.join("cache")).unwrap();
        let file = |name: &str| {
            let path = temporary.path.join(name);
            fs::write(&path, name).unwrap();
            path
        };
        // A Windows `-g` build caches its PDB beside the executable (G16), as macOS caches DWARF, and an
        // instrumented WebAssembly module caches its profile writer (PR07).
        for sidecars in [["traps", "dwarf"], ["traps", "pdb"], ["traps", "profile"]] {
            let mut hash = Sha256::new();
            hash.field("sidecars", sidecars.join(",").as_bytes());
            let key = hash.hex();
            let mut paths = BTreeMap::from([("artifact".to_owned(), file("artifact"))]);
            for name in sidecars {
                paths.insert(name.to_owned(), file(name));
            }
            cache.store(&key, &paths, &[]).unwrap();
            let loaded = cache.load(&key).unwrap().unwrap();
            assert!(loaded.files.keys().eq(paths.keys()));
            for name in sidecars {
                assert_eq!(loaded.files[name].bytes, name.as_bytes());
            }
        }
        let paths = BTreeMap::from([
            ("artifact".to_owned(), file("artifact")),
            ("other".to_owned(), file("other")),
        ]);
        let error = cache.store(&"0".repeat(64), &paths, &[]).unwrap_err();
        assert_eq!(error.to_string(), "invalid cache artifact name");
    }

    #[test]
    fn eviction_keeps_the_marker_and_the_package_store() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let root = temporary.path.join("cache");
        let cache = BuildCache::open(&root).unwrap();
        fs::create_dir_all(root.join("packages/git/package")).unwrap();
        // The marker is written once, so it is older than the age limit after a month.
        fs::OpenOptions::new()
            .write(true)
            .open(root.join(".tsuzuri-cache"))
            .unwrap()
            .set_modified(SystemTime::now() - std::time::Duration::from_secs(31 * 24 * 60 * 60))
            .unwrap();
        cache.evict().unwrap();
        assert!(root.join(".tsuzuri-cache").is_file());
        assert!(root.join("packages/git/package").is_dir());
        BuildCache::open(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_tool_runs_under_the_name_it_is_found_by_not_the_file_behind_a_link() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let bin = fs::canonicalize(&temporary.path).unwrap();
        fs::write(bin.join("lld"), b"multi-call").unwrap();
        std::os::unix::fs::symlink("lld", bin.join("wasm-ld")).unwrap();
        let search = std::env::join_paths([bin.join("missing"), bin.clone()]).unwrap();
        let invoked = invoked_path_in("wasm-ld".as_ref(), &search).unwrap();
        assert_eq!(invoked, bin.join("wasm-ld"));
        assert_eq!(real_path(&invoked).unwrap(), bin.join("lld"));
        // A path is kept as it is, and a missing tool is an error.
        assert_eq!(
            invoked_path_in(bin.join("wasm-ld").as_os_str(), "".as_ref()).unwrap(),
            bin.join("wasm-ld")
        );
        assert!(invoked_path_in("nothing-here".as_ref(), &search).is_err());
    }

    #[test]
    fn shared_library_keys_follow_the_file_name() {
        use crate::driver::{BuildOptions, Emit};
        let key = |emit, output: &str| {
            let mut hash = Sha256::new();
            let options = BuildOptions {
                emit,
                ..BuildOptions::default()
            };
            hash_output_path(&mut hash, options, Path::new(output)).unwrap();
            hash.hex()
        };
        // The install name and the soname are the file name, so another name needs another build.
        assert_ne!(
            key(Emit::Shared, "one/libfoo.dylib"),
            key(Emit::Shared, "one/libbar.dylib")
        );
        assert_eq!(
            key(Emit::Shared, "one/libfoo.dylib"),
            key(Emit::Shared, "two/libfoo.dylib")
        );
        assert_eq!(
            key(Emit::Object, "one/foo.o"),
            key(Emit::Object, "two/bar.o")
        );
        assert_eq!(
            key(Emit::Executable, "one/foo"),
            key(Emit::Executable, "two/bar")
        );
    }

    #[test]
    fn sha256_nist_vectors_and_streaming_boundaries() {
        for (source, expected) in [
            (
                "".to_owned(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc".to_owned(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".to_owned(),
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
            (
                "a".repeat(1000000),
                "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
            ),
        ] {
            for chunk in [1, 55, 56, 63, 64, 65, 4096] {
                let mut hash = Sha256::new();
                for part in source.as_bytes().chunks(chunk) {
                    hash.update(part);
                }
                assert_eq!(hash.hex(), expected, "chunk {chunk}");
            }
        }
        let mut first = Sha256::new();
        first.field("source", b"ab");
        first.field("source", b"c");
        let mut second = Sha256::new();
        second.field("source", b"a");
        second.field("source", b"bc");
        assert_ne!(first.finalize(), second.finalize());
    }
    #[test]
    fn runtime_keys_follow_contents_flags_and_tool_identity() {
        let identity = ToolIdentity::new(
            vec![
                ("tool-binary", vec![1]),
                ("native-cpu-features", b"cpu-a".to_vec()),
            ],
            None,
        );
        let args = vec!["-O0".into(), "-fPIC".into()];
        let key = runtime_key("numeric", b"source", &args, &identity).unwrap();
        assert_eq!(
            key,
            runtime_key("numeric", b"source", &args, &identity).unwrap()
        );
        assert_ne!(
            key,
            runtime_key("math", b"source", &args, &identity).unwrap()
        );
        assert_ne!(
            key,
            runtime_key("numeric", b"changed", &args, &identity).unwrap()
        );
        for changed in [
            vec!["-O3".into(), "-fPIC".into()],
            vec!["-O0".into(), "--target=wasm32-unknown-unknown".into()],
            vec!["-O0".into(), "-msimd128".into()],
            vec!["-fPIC".into(), "-O0".into()],
            vec!["-O0".into(), "--sysroot=other".into()],
        ] {
            assert_ne!(
                key,
                runtime_key("numeric", b"source", &changed, &identity).unwrap()
            );
        }
        for identity in [
            ToolIdentity::new(
                vec![
                    ("tool-binary", vec![2]),
                    ("native-cpu-features", b"cpu-a".to_vec()),
                ],
                None,
            ),
            ToolIdentity::new(
                vec![
                    ("tool-binary", vec![1]),
                    ("native-cpu-features", b"cpu-b".to_vec()),
                ],
                None,
            ),
        ] {
            assert_ne!(
                key,
                runtime_key("numeric", b"source", &args, &identity).unwrap()
            );
        }
    }

    #[test]
    fn bundled_resource_identity_hashes_unlisted_files_and_excludes_generated_files() {
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-resource-fingerprint-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sdk_file = root.join("include/stdint.h");
        let compiler = root
            .join("bin")
            .join(format!("tsuzuri{}", std::env::consts::EXE_SUFFIX));
        let runtime_manifest = root.join("share/runtime/manifest.json");
        fs::create_dir_all(sdk_file.parent().unwrap()).unwrap();
        fs::create_dir_all(compiler.parent().unwrap()).unwrap();
        fs::create_dir_all(runtime_manifest.parent().unwrap()).unwrap();
        fs::write(&sdk_file, b"old sdk").unwrap();
        fs::write(&compiler, b"old compiler").unwrap();
        fs::write(&runtime_manifest, b"old runtime").unwrap();
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&json!({
                "files": [
                    {"path": format!("bin/tsuzuri{}", std::env::consts::EXE_SUFFIX)},
                    {"path": "include/stdint.h"},
                    {"path": "share/runtime/manifest.json"}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let identity = || {
            ToolIdentity::new(
                vec![("tool-binary", vec![1]), ("bundled-resources", Vec::new())],
                Some(root.clone()),
            )
        };
        let fingerprint = bundled_resource_fingerprint(&root).unwrap();
        let initial = identity();
        assert!(initial.matches_bundled_resources(&fingerprint));
        let args = ["-O0".into()];
        let key = runtime_key("native-c", b"source", &args, &initial).unwrap();

        fs::write(&compiler, b"new compiler").unwrap();
        fs::write(&runtime_manifest, b"new runtime").unwrap();
        assert!(identity().matches_bundled_resources(&fingerprint));

        let unlisted_file = root.join("include/extra.h");
        fs::write(&unlisted_file, b"new sdk input").unwrap();
        let added_resource = identity();
        assert!(!added_resource.matches_bundled_resources(&fingerprint));
        assert_ne!(
            key,
            runtime_key("native-c", b"source", &args, &added_resource).unwrap()
        );
        fs::remove_file(unlisted_file).unwrap();

        fs::write(&sdk_file, b"new sdk").unwrap();
        let changed_resource = identity();
        assert!(!changed_resource.matches_bundled_resources(&fingerprint));
        assert_ne!(
            key,
            runtime_key("native-c", b"source", &args, &changed_resource).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
