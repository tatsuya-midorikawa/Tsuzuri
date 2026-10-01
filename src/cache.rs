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

fn read_regular(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
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

pub(crate) fn build_key(
    project: &crate::driver::Project,
    options: crate::driver::BuildOptions,
    action: &str,
    ir: &str,
    output: &Path,
) -> io::Result<String> {
    use crate::driver::{Emit, Target};
    let mut hash = Sha256::new();
    hash.field("format", &FORMAT.to_le_bytes());
    hash.field("compiler-version", env!("CARGO_PKG_VERSION").as_bytes());
    hash.field("compiler-binary", &file_digest(&std::env::current_exe()?)?);
    hash.field("host-os", std::env::consts::OS.as_bytes());
    hash.field("host-arch", std::env::consts::ARCH.as_bytes());
    hash.field("options", format!("{options:?}").as_bytes());
    hash.field("action", action.as_bytes());
    if cfg!(target_os = "macos") && options.debug_info && options.emit == Emit::Executable {
        let absolute = std::path::absolute(output)?;
        hash.field("debug-output-path", absolute.as_os_str().as_encoded_bytes());
    }
    hash.field("ir-and-embedded-runtime", ir.as_bytes());
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
    for variable in [
        "TSUZURI_CLANG",
        "TSUZURI_WASM_LD",
        "TSUZURI_LLVM_LINK",
        "TSUZURI_DSYMUTIL",
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
    ] {
        let value = std::env::var_os(variable);
        hash.field(
            variable,
            value.as_ref().map_or(&[], |value| value.as_encoded_bytes()),
        );
    }
    let mut tools = Vec::new();
    if !matches!(options.emit, Emit::Llvm | Emit::Header | Emit::Wgsl) {
        tools.push(("TSUZURI_CLANG", "clang"));
        if options.emit == Emit::Wasm || options.wasm_threads {
            tools.push(("TSUZURI_WASM_LD", "wasm-ld"));
        }
        if cfg!(target_os = "macos") && options.debug_info && options.target == Target::Native {
            if options.emit == Emit::Executable {
                tools.push(("TSUZURI_DSYMUTIL", "dsymutil"));
            }
            if options.emit == Emit::Object && ir.contains("@tsuzuri_task_parallel(") {
                tools.push(("TSUZURI_LLVM_LINK", "llvm-link"));
            }
        }
    }
    for (variable, fallback) in tools {
        let tool = crate::driver::resolve_tool(variable, fallback).1;
        let path = executable_path(&tool)?;
        hash.field("tool-path", path.as_os_str().as_encoded_bytes());
        hash.field("tool-binary", &file_digest(&path)?);
        let output = std::process::Command::new(&path)
            .arg("--version")
            .output()?;
        if !output.status.success() {
            return Err(invalid("tool version query failed"));
        }
        hash.field("tool-version-stdout", &output.stdout);
        hash.field("tool-version-stderr", &output.stderr);
        if variable == "TSUZURI_CLANG" && options.cpu == crate::driver::Cpu::Native {
            let output = std::process::Command::new(&path)
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
            hash.field("native-cpu-features", &output.stdout);
            hash.field("native-cpu-diagnostics", &output.stderr);
        }
    }
    Ok(hash.hex())
}

pub fn executable_path(tool: &std::ffi::OsStr) -> io::Result<PathBuf> {
    let path = Path::new(tool);
    if path.components().count() > 1 || path.is_absolute() {
        return fs::canonicalize(path);
    }
    for root in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = root.join(tool);
        if candidate.is_file() {
            return fs::canonicalize(candidate);
        }
        if cfg!(windows) && candidate.extension().is_none() {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return fs::canonicalize(candidate);
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
        })
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
                if !matches!(name.as_str(), "artifact" | "traps" | "dwarf") {
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
        metadata["last_used_unix_ms"] = json!(now_ms());
        let temporary = stage(&self.root)?;
        let staged = temporary.path.join("metadata.json");
        fs::write(&staged, serde_json::to_vec(&metadata)?)?;
        fs::rename(staged, self.root.join(key).join("metadata.json"))?;
        Ok(Some(artifact))
    }

    pub(crate) fn store(
        &self,
        key: &str,
        paths: &BTreeMap<String, PathBuf>,
        messages: &[String],
    ) -> io::Result<()> {
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
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(()),
            Err(error) => return Err(error),
        }
        let _lock = CacheLock(lock);
        if self.load(key)?.is_some() {
            return Ok(());
        }
        let temporary = stage(&self.root)?;
        let entry = temporary.path.join("entry");
        fs::create_dir(&entry)?;
        let mut remaining = MAX_ENTRY_BYTES;
        let mut files = serde_json::Map::new();
        for (name, source) in paths {
            if !matches!(name.as_str(), "artifact" | "traps" | "dwarf") {
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
        self.evict()
    }

    pub(crate) fn evict(&self) -> io::Result<()> {
        let mut entries = Vec::new();
        let mut total = 0u64;
        for entry in fs::read_dir(&self.root)?.take(4096) {
            let entry = entry?;
            let name = entry.file_name();
            let Some(key) = name.to_str().filter(|key| {
                key_name(key)
                    || key.strip_prefix(".lock-").is_some_and(key_name)
                    || key.starts_with(".tsuzuri-")
            }) else {
                continue;
            };
            let (size, used) = match self.metadata(key) {
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
                    let metadata = fs::symlink_metadata(entry.path())?;
                    let modified = metadata
                        .modified()?
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64;
                    (0, modified)
                }
            };
            total = total.saturating_add(size);
            entries.push((used, key.to_owned(), size));
        }
        entries.sort();
        let now = now_ms();
        for (used, key, size) in entries.into_iter().take(128) {
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
        let mut work = self.state;
        for (word, round) in words.into_iter().zip(ROUND) {
            let sigma =
                work[4].rotate_right(6) ^ work[4].rotate_right(11) ^ work[4].rotate_right(25);
            let choose = (work[4] & work[5]) ^ (!work[4] & work[6]);
            let first = work[7]
                .wrapping_add(sigma)
                .wrapping_add(choose)
                .wrapping_add(round)
                .wrapping_add(word);
            let sigma =
                work[0].rotate_right(2) ^ work[0].rotate_right(13) ^ work[0].rotate_right(22);
            let majority = (work[0] & work[1]) ^ (work[0] & work[2]) ^ (work[1] & work[2]);
            let second = sigma.wrapping_add(majority);
            work.rotate_right(1);
            work[4] = work[4].wrapping_add(first);
            work[0] = first.wrapping_add(second);
        }
        for (state, value) in self.state.iter_mut().zip(work) {
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
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(hash.finalize());
        }
        hash.update(&buffer[..count]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
