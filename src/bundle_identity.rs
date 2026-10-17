//! The identity of a bundled toolchain: what a cached build or runtime object depends on, found without
//! reading the whole distribution on every build (PB03 WP-F).
//!
//! A distribution holds the launcher `tsuzuri-clang`, the LLVM Clang and Zig beside it, and about 19,000 other
//! files (C headers, libraries; 760 MB in the macOS arm64 bundle). Every cache key names all of them, which
//! cost about 8 s of CPU on every build, Clang and Zig being hashed once more for their own fields. `resolve`
//! computes the identity once and keeps it in a memo next to the caches (`<cache root>/bundle/`).
//!
//! The trust model. A memo is used again while a cheap signature of the distribution has not changed: the
//! size, modification time, status-change time and inode of the top-level files, of the files directly in
//! `bin`, `lib` and `zig`, of `manifest.json`, and of every directory up to two levels deep. A tool that is
//! replaced or edited in place, a file that is added to or removed from one of those directories, and a
//! distribution that is extracted over another one (a new `manifest.json`) all change the signature, so the
//! next build hashes the files again. What the signature does not see is a file that is edited in place
//! elsewhere (a header in `include/` or `zig/lib/libc/`, a script in `share/`) and a file that is added or
//! removed more than two directories down; delete `<cache root>/bundle/` to make the next build hash
//! everything. A memo is also not written when the newest time of the distribution is within `RACY_NS` of the
//! moment the signature was taken, because a file could change again within the clock's resolution without
//! changing its times; the next build writes it. The first build after a distribution is installed hashes all
//! of it, in parallel.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde_json::{Value, json};

use crate::cache::{Sha256, read_regular};

const FORMAT: u64 = 1;
const MEMO_LIMIT: u64 = 4 * 1024 * 1024;
/// A memo is not written while the newest time of the distribution is this close to the moment it was read (see the module documentation).
pub(crate) const RACY_NS: u128 = 2_000_000_000;
/// The most threads that hash files at once.
const MAX_HASH_THREADS: usize = 8;

/// The digest of the file at a path; `cache::file_digest` outside the tests.
pub(crate) type Hasher<'a> = &'a (dyn Fn(&Path) -> io::Result<[u8; 32]> + Sync);

/// The files of a distribution with their digests, in the order they were visited.
pub(crate) struct Inventory {
    files: Vec<(String, [u8; 32])>,
}

impl Inventory {
    /// The SHA-256 of the JSON list `[[path, digest], ...]`, which `scripts/toolchain/runtime.mjs` writes
    /// into `share/runtime/manifest.json` when it bundles the runtime.
    pub(crate) fn fingerprint(&self) -> String {
        let pairs: Vec<(&str, String)> = self
            .files
            .iter()
            .map(|(path, digest)| (path.as_str(), hex(digest)))
            .collect();
        let mut hash = Sha256::new();
        hash.update(&serde_json::to_vec(&pairs).expect("an inventory is JSON serializable"));
        hash.hex()
    }

    fn digest(&self, path: &str) -> Option<[u8; 32]> {
        self.files
            .iter()
            .find(|(file, _)| file == path)
            .map(|(_, digest)| *digest)
    }
}

/// What the inventory leaves out: the compiler, which is keyed on its own, `share/runtime`, which holds objects
/// built from the resources, and the distribution's own `manifest.json`.
fn skipped(path: &str, compiler: &str) -> bool {
    path == compiler
        || path == "share/runtime"
        || path.starts_with("share/runtime/")
        || path == "manifest.json"
}

/// A file of the distribution, as `(path from the root, path, size)`.
type Found = Vec<(String, PathBuf, u64)>;

fn walk(directory: &Path, prefix: &str, compiler: &str, found: &mut Found) -> Option<()> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().into_string().ok()?;
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if skipped(&path, compiler) {
            continue;
        }
        let file_path = entry.path();
        let metadata = fs::symlink_metadata(&file_path).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
        if metadata.is_dir() {
            walk(&file_path, &path, compiler, found)?;
        } else if metadata.is_file() {
            found.push((path, file_path, metadata.len()));
        } else {
            return None;
        }
    }
    Some(())
}

/// Every resource of the distribution at `root` with its digest, or `None` when one cannot be read or is
/// a symbolic link or another kind of entry. The big files are hashed first, on several threads.
pub(crate) fn inventory(root: &Path, compiler: &str, hasher: Hasher<'_>) -> Option<Inventory> {
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, MAX_HASH_THREADS);
    inventory_with(root, compiler, hasher, threads)
}

fn inventory_with(
    root: &Path,
    compiler: &str,
    hasher: Hasher<'_>,
    threads: usize,
) -> Option<Inventory> {
    let mut found = Vec::new();
    walk(root, "", compiler, &mut found)?;
    let mut order: Vec<usize> = (0..found.len()).collect();
    order.sort_by_key(|&index| std::cmp::Reverse(found[index].2));
    let threads = threads.min(order.len()).max(1);
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let mut digests = vec![[0u8; 32]; found.len()];
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    while !failed.load(Ordering::Relaxed) {
                        let Some(&index) = order.get(next.fetch_add(1, Ordering::Relaxed)) else {
                            break;
                        };
                        match hasher(&found[index].1) {
                            Ok(digest) => done.push((index, digest)),
                            Err(_) => failed.store(true, Ordering::Relaxed),
                        }
                    }
                    done
                })
            })
            .collect();
        for worker in workers {
            for (index, digest) in worker.join().expect("a hash worker panicked") {
                digests[index] = digest;
            }
        }
    });
    if failed.load(Ordering::Relaxed) {
        return None;
    }
    Some(Inventory {
        files: found
            .into_iter()
            .zip(digests)
            .map(|((path, _, _), digest)| (path, digest))
            .collect(),
    })
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    text
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.is_ascii() || text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

fn digest_from(text: &Value) -> Option<[u8; 32]> {
    unhex(text.as_str()?)?.try_into().ok()
}

/// The size, the modification and status-change times and the inode of a file, or of a directory without its
/// size, as text, and the newest of the two times in nanoseconds since the epoch. Elsewhere than Unix the
/// creation time stands in for the status-change time and the inode.
#[cfg(unix)]
fn stat_text(path: &Path) -> io::Result<(String, u128)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    let length = if metadata.is_dir() { 0 } else { metadata.len() };
    let nanoseconds = |seconds: i64, nanoseconds: i64| {
        i128::from(seconds) * 1_000_000_000 + i128::from(nanoseconds)
    };
    let newest = nanoseconds(metadata.mtime(), metadata.mtime_nsec())
        .max(nanoseconds(metadata.ctime(), metadata.ctime_nsec()));
    Ok((
        format!(
            "{length}:{}.{:09}:{}.{:09}:{}",
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
            metadata.ino()
        ),
        newest.max(0) as u128,
    ))
}

#[cfg(not(unix))]
fn stat_text(path: &Path) -> io::Result<(String, u128)> {
    let metadata = fs::symlink_metadata(path)?;
    let length = if metadata.is_dir() { 0 } else { metadata.len() };
    let nanoseconds = |time: io::Result<std::time::SystemTime>| {
        time.ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos())
    };
    let modified = nanoseconds(metadata.modified());
    let created = nanoseconds(metadata.created());
    Ok((
        format!("{length}:{modified}:{created}"),
        modified.max(created),
    ))
}

/// The entries that the signature checks: the directories up to two levels deep, the files at the top and
/// directly in `bin`, `lib` and `zig`, and `manifest.json`, as `(is a directory, path from the root)`.
fn signature_members(root: &Path, request: &Request<'_>) -> Option<Vec<(bool, String)>> {
    let mut members = vec![(true, String::new()), (false, "manifest.json".to_owned())];
    let visit = |directory: &Path, prefix: &str, files: bool| -> Option<Vec<(String, bool)>> {
        let mut entries: Vec<_> = fs::read_dir(directory)
            .ok()?
            .collect::<Result<_, _>>()
            .ok()?;
        entries.sort_by_key(|entry| entry.file_name());
        let mut found = Vec::new();
        for entry in entries {
            let name = entry.file_name().into_string().ok()?;
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if skipped(&path, request.compiler) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            if metadata.is_dir() {
                found.push((path, true));
            } else if files {
                found.push((path, false));
            }
        }
        Some(found)
    };
    for (path, directory) in visit(root, "", true)? {
        members.push((directory, path.clone()));
        if directory {
            let files = matches!(path.as_str(), "bin" | "lib" | "zig");
            for (inner, is_directory) in visit(&root.join(&path), &path, files)? {
                members.push((is_directory, inner));
            }
        }
    }
    for path in [Some(request.launcher), Some(request.clang), request.zig]
        .into_iter()
        .flatten()
    {
        if let Ok(relative) = path.strip_prefix(root)
            && let Some(relative) = relative_text(relative)
            && !skipped(&relative, request.compiler)
        {
            members.push((false, relative));
        }
    }
    members.sort();
    members.dedup();
    Some(members)
}

fn relative_text(path: &Path) -> Option<String> {
    path.components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

/// The stat text of every member, and the newest time among them; a missing member is `-`.
fn take_signature(root: &Path, members: &[(bool, String)]) -> Option<(Vec<[String; 3]>, u128)> {
    let mut newest = 0;
    let mut entries = Vec::with_capacity(members.len());
    for (directory, path) in members {
        let full = if path.is_empty() {
            root.to_path_buf()
        } else {
            root.join(path)
        };
        let text = match stat_text(&full) {
            Ok((text, time)) => {
                newest = newest.max(time);
                text
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !directory => "-".to_owned(),
            Err(_) => return None,
        };
        entries.push([
            if *directory { "d" } else { "f" }.to_owned(),
            path.clone(),
            text,
        ]);
    }
    Some((entries, newest))
}

/// The bundle, as `tool_identity` finds it.
pub(crate) struct Request<'a> {
    pub root: &'a Path,
    pub launcher: &'a Path,
    pub clang: &'a Path,
    pub zig: Option<&'a Path>,
    /// The compiler's path from the root, which is not a resource.
    pub compiler: &'a str,
}

/// The identity of the bundle: `fingerprint` is `None` when a resource cannot be read, and the identity then
/// has no cache key.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub fingerprint: Option<String>,
    pub launcher: [u8; 32],
    pub clang: [u8; 32],
    pub zig: Option<[u8; 32]>,
    /// What `--version` of the launcher printed: standard output, then standard error.
    pub version: (Vec<u8>, Vec<u8>),
}

pub(crate) struct Clock {
    pub now_ns: u128,
    pub racy_ns: u128,
}

impl Clock {
    pub(crate) fn system() -> Self {
        Self {
            now_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos()),
            racy_ns: RACY_NS,
        }
    }

    /// Whether a signature whose newest time is `newest_ns` may be remembered.
    fn trusts(&self, newest_ns: u128) -> bool {
        self.now_ns.saturating_sub(newest_ns) >= self.racy_ns
    }
}

fn memo_name(request: &Request<'_>) -> String {
    let mut hash = Sha256::new();
    hash.field("format", &FORMAT.to_le_bytes());
    for (name, path) in [
        ("root", Some(request.root)),
        ("launcher", Some(request.launcher)),
        ("clang", Some(request.clang)),
        ("zig", request.zig),
    ] {
        hash.field(
            name,
            path.map_or(&[][..], |path| path.as_os_str().as_encoded_bytes()),
        );
    }
    hash.field("compiler", request.compiler.as_bytes());
    format!("bundle-{}.json", hash.hex())
}

/// The identity of the bundle, from the memo in `memo_directory` while its signature still holds, and otherwise
/// by hashing the bundle (and asking `probe` for the launcher's `--version`) and remembering the result.
pub(crate) fn resolve(
    request: &Request<'_>,
    memo_directory: Option<&Path>,
    clock: &Clock,
    hasher: Hasher<'_>,
    probe: &dyn Fn() -> io::Result<(Vec<u8>, Vec<u8>)>,
) -> io::Result<Resolved> {
    let memo = memo_directory.map(|directory| directory.join(memo_name(request)));
    if let Some(memo) = &memo
        && let Some(resolved) = recall(memo, request)
    {
        return Ok(resolved);
    }
    // The signature comes first: a file that changes while the bundle is hashed must not be remembered.
    let members = signature_members(request.root, request);
    let before = members
        .as_ref()
        .and_then(|members| take_signature(request.root, members));
    let version = probe()?;
    let inventory = inventory(request.root, request.compiler, hasher);
    let digest = |path: &Path| -> io::Result<[u8; 32]> {
        if let Some(inventory) = &inventory
            && let Ok(relative) = path.strip_prefix(request.root)
            && let Some(relative) = relative_text(relative)
            && let Some(digest) = inventory.digest(&relative)
        {
            return Ok(digest);
        }
        hasher(path)
    };
    let resolved = Resolved {
        fingerprint: inventory.as_ref().map(Inventory::fingerprint),
        launcher: digest(request.launcher)?,
        clang: digest(request.clang)?,
        zig: request.zig.map(digest).transpose()?,
        version,
    };
    if let (Some(memo), Some((entries, newest)), Some(members), true) =
        (&memo, &before, &members, resolved.fingerprint.is_some())
        && clock.trusts(*newest)
        && take_signature(request.root, members).is_some_and(|after| after.0 == *entries)
    {
        remember(memo, request, &resolved, entries);
    }
    Ok(resolved)
}

fn recall(memo: &Path, request: &Request<'_>) -> Option<Resolved> {
    let value: Value = serde_json::from_slice(&read_regular(memo, MEMO_LIMIT).ok()?).ok()?;
    let text = |name: &str| value[name].as_str();
    if value["format"].as_u64() != Some(FORMAT)
        || text("root") != request.root.to_str()
        || text("launcher") != request.launcher.to_str()
        || text("clang") != request.clang.to_str()
        || value["zig"].as_str() != request.zig.and_then(Path::to_str)
        || text("compiler") != Some(request.compiler)
    {
        return None;
    }
    let signature = value["signature"].as_array()?;
    for entry in signature {
        let [kind, relative, recorded] = entry.as_array()?.as_slice() else {
            return None;
        };
        let (kind, relative, recorded) = (kind.as_str()?, relative.as_str()?, recorded.as_str()?);
        let full = if relative.is_empty() {
            request.root.to_path_buf()
        } else {
            request.root.join(relative)
        };
        let current = match stat_text(&full) {
            Ok((text, _)) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound && kind == "f" => "-".to_owned(),
            Err(_) => return None,
        };
        if current != recorded {
            return None;
        }
    }
    let version = &value["version"];
    Some(Resolved {
        fingerprint: Some(value["fingerprint"].as_str()?.to_owned()),
        launcher: digest_from(&value["digests"]["launcher"])?,
        clang: digest_from(&value["digests"]["clang"])?,
        zig: match &value["digests"]["zig"] {
            Value::Null => None,
            digest => Some(digest_from(digest)?),
        },
        version: (
            unhex(version["stdout"].as_str()?)?,
            unhex(version["stderr"].as_str()?)?,
        ),
    })
}

/// Writes the memo through a temporary file, so that a build that runs at the same time reads either the old
/// memo or the new one. A failure is ignored: the next build computes the identity again.
fn remember(memo: &Path, request: &Request<'_>, resolved: &Resolved, signature: &[[String; 3]]) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let Some(directory) = memo.parent() else {
        return;
    };
    let text = |path: Option<&Path>| path.and_then(Path::to_str).map(str::to_owned);
    let value = json!({
        "format": FORMAT,
        "root": text(Some(request.root)),
        "launcher": text(Some(request.launcher)),
        "clang": text(Some(request.clang)),
        "zig": text(request.zig),
        "compiler": request.compiler,
        "fingerprint": resolved.fingerprint,
        "digests": {
            "launcher": hex(&resolved.launcher),
            "clang": hex(&resolved.clang),
            "zig": resolved.zig.as_ref().map(|digest| hex(digest)),
        },
        "version": {
            "stdout": hex(&resolved.version.0),
            "stderr": hex(&resolved.version.1),
        },
        "signature": signature,
    });
    let Ok(bytes) = serde_json::to_vec(&value) else {
        return;
    };
    let temporary = directory.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = fs::create_dir_all(directory)
        .and_then(|()| fs::write(&temporary, bytes))
        .and_then(|()| fs::rename(&temporary, memo));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::file_digest;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, UNIX_EPOCH};

    /// A distribution's root (`.0`) and, beside it, a parent (`.1`) for the memo, which is not part of the
    /// distribution.
    struct Scratch(PathBuf, PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let parent = std::env::temp_dir().join(format!(
                "tsuzuri-bundle-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let root = parent.join("dist");
            fs::create_dir_all(&root).unwrap();
            Self(fs::canonicalize(root).unwrap(), parent)
        }

        fn memo(&self) -> PathBuf {
            self.1.join("memo")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.1);
        }
    }

    const COMPILER: &str = "bin/tsuzuri";

    /// A distribution in miniature: the launcher, Clang, Zig, a library, headers at two depths, the
    /// compiler (which is not a resource) and the generated runtime directory.
    fn bundle(root: &Path) {
        for (path, text) in [
            ("bin/tsuzuri-clang", "launcher"),
            ("bin/clang", "clang"),
            ("bin/tsuzuri", "compiler"),
            ("zig/zig", "zig"),
            ("zig/lib/libc/include/stdint.h", "deep header"),
            ("lib/libllvm.dylib", "library"),
            ("include/stdint.h", "header"),
            ("share/lldb/formatter.py", "formatter"),
            ("share/runtime/manifest.json", "generated"),
            ("manifest.json", "{\"id\":\"one\"}"),
        ] {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
    }

    fn request(root: &Path) -> Request<'static> {
        Request {
            root: Box::leak(root.to_path_buf().into_boxed_path()),
            launcher: Box::leak(root.join("bin/tsuzuri-clang").into_boxed_path()),
            clang: Box::leak(root.join("bin/clang").into_boxed_path()),
            zig: Some(Box::leak(root.join("zig/zig").into_boxed_path())),
            compiler: COMPILER,
        }
    }

    /// Counts the files that are hashed and the probes that run.
    struct Counts {
        hashed: AtomicUsize,
        probed: AtomicUsize,
    }

    fn resolve_counting(
        request: &Request<'_>,
        memo: &Path,
        clock: &Clock,
    ) -> (Resolved, usize, usize) {
        let counts = Counts {
            hashed: AtomicUsize::new(0),
            probed: AtomicUsize::new(0),
        };
        let hasher = |path: &Path| {
            counts.hashed.fetch_add(1, Ordering::Relaxed);
            file_digest(path)
        };
        let probe = || {
            counts.probed.fetch_add(1, Ordering::Relaxed);
            Ok((b"clang version 21".to_vec(), b"warning".to_vec()))
        };
        let resolved = resolve(request, Some(memo), clock, &hasher, &probe).unwrap();
        (
            resolved,
            counts.hashed.load(Ordering::Relaxed),
            counts.probed.load(Ordering::Relaxed),
        )
    }

    /// Every write is remembered: the memo does not wait for the newest time to age.
    fn eager() -> Clock {
        Clock {
            now_ns: u128::MAX / 2,
            racy_ns: 0,
        }
    }

    fn sha(bytes: &[u8]) -> String {
        let mut hash = Sha256::new();
        hash.update(bytes);
        hash.hex()
    }

    /// Waits until the status of `path` differs from `before`, which a clock with a coarse tick needs.
    fn changed(path: &Path, before: &str) {
        for _ in 0..2000 {
            if stat_text(path).unwrap().0 != before {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
            let probe = path.join(".tick");
            fs::write(&probe, b"x").unwrap();
            fs::remove_file(&probe).unwrap();
        }
        panic!("the status of {} never changed", path.display());
    }

    #[test]
    fn the_fingerprint_lists_resources_in_visit_order_and_skips_the_compiler_and_runtime() {
        let scratch = Scratch::new("fingerprint");
        bundle(&scratch.0);
        let found = inventory(&scratch.0, COMPILER, &file_digest).unwrap();
        // Written out by hand: directories in name order, depth first, and none of the skipped files.
        let expected: Vec<(&str, &str)> = vec![
            ("bin/clang", "clang"),
            ("bin/tsuzuri-clang", "launcher"),
            ("include/stdint.h", "header"),
            ("lib/libllvm.dylib", "library"),
            ("share/lldb/formatter.py", "formatter"),
            ("zig/lib/libc/include/stdint.h", "deep header"),
            ("zig/zig", "zig"),
        ];
        let listing = serde_json::to_vec(
            &expected
                .iter()
                .map(|(path, text)| (*path, sha(text.as_bytes())))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(found.fingerprint(), sha(&listing));
        assert_eq!(
            found.digest("zig/zig"),
            Some(file_digest(&scratch.0.join("zig/zig")).unwrap())
        );
    }

    #[test]
    fn a_memo_that_still_matches_hashes_nothing_and_runs_no_probe() {
        let scratch = Scratch::new("hit");
        bundle(&scratch.0);
        let memo = scratch.memo();
        let request = request(&scratch.0);
        let (first, hashed, probed) = resolve_counting(&request, &memo, &eager());
        assert_eq!((hashed > 0, probed), (true, 1));
        assert!(first.fingerprint.is_some() && first.zig.is_some());
        let (second, hashed, probed) = resolve_counting(&request, &memo, &eager());
        assert_eq!((hashed, probed), (0, 0));
        assert_eq!(first, second);
        assert_eq!(
            second.version,
            (b"clang version 21".to_vec(), b"warning".to_vec())
        );
        // Without a memo directory the result is the same, and everything is computed.
        let counts = Counts {
            hashed: AtomicUsize::new(0),
            probed: AtomicUsize::new(0),
        };
        let fresh = resolve(
            &request,
            None,
            &eager(),
            &|path: &Path| {
                counts.hashed.fetch_add(1, Ordering::Relaxed);
                file_digest(path)
            },
            &|| Ok((b"clang version 21".to_vec(), b"warning".to_vec())),
        )
        .unwrap();
        assert_eq!(fresh, first);
        // Zig and Clang come from the inventory, not from a second read.
        assert_eq!(counts.hashed.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn replacing_a_tool_or_the_manifest_in_place_drops_the_memo() {
        let scratch = Scratch::new("tools");
        bundle(&scratch.0);
        let memo = scratch.memo();
        let request = request(&scratch.0);
        let (first, ..) = resolve_counting(&request, &memo, &eager());
        // Every new text has another size than the old one, so that a clock with a coarse tick still sees it.
        for (path, text) in [
            ("zig/zig", "zig, newer"),
            ("bin/clang", "other clang"),
            ("bin/tsuzuri-clang", "launcher two"),
            ("manifest.json", "{\"id\":\"two, longer\"}"),
            ("lib/libllvm.dylib", "library 2"),
        ] {
            let file = scratch.0.join(path);
            let original = fs::read(&file).unwrap();
            fs::write(&file, text).unwrap();
            let (after, hashed, probed) = resolve_counting(&request, &memo, &eager());
            assert!(
                hashed > 0 && probed == 1,
                "{path}: the memo should be dropped"
            );
            if path != "manifest.json" {
                assert_ne!(after.fingerprint, first.fingerprint, "{path}");
            }
            let (_, hashed, probed) = resolve_counting(&request, &memo, &eager());
            assert_eq!(
                (hashed, probed),
                (0, 0),
                "{path}: the new state is remembered"
            );
            fs::write(&file, original).unwrap();
            let (restored, hashed, _) = resolve_counting(&request, &memo, &eager());
            assert!(hashed > 0, "{path}: restoring the file is a change too");
            assert_eq!(restored.fingerprint, first.fingerprint, "{path}");
        }
    }

    #[test]
    fn a_file_added_or_removed_in_a_checked_directory_drops_the_memo() {
        let scratch = Scratch::new("directories");
        bundle(&scratch.0);
        let memo = scratch.memo();
        let request = request(&scratch.0);
        let (first, ..) = resolve_counting(&request, &memo, &eager());
        for directory in ["include", "bin", "zig/lib", "share/lldb", ""] {
            let path = if directory.is_empty() {
                scratch.0.clone()
            } else {
                scratch.0.join(directory)
            };
            let before = stat_text(&path).unwrap().0;
            let added = path.join("extra.h");
            fs::write(&added, b"new input").unwrap();
            changed(&path, &before);
            let (with, hashed, _) = resolve_counting(&request, &memo, &eager());
            assert!(hashed > 0, "{directory}: an added file is noticed");
            assert_ne!(with.fingerprint, first.fingerprint, "{directory}");
            let before = stat_text(&path).unwrap().0;
            fs::remove_file(&added).unwrap();
            changed(&path, &before);
            let (without, hashed, _) = resolve_counting(&request, &memo, &eager());
            assert!(hashed > 0, "{directory}: a removed file is noticed");
            assert_eq!(without.fingerprint, first.fingerprint, "{directory}");
        }
    }

    #[test]
    fn what_the_signature_does_not_see_is_found_again_only_without_the_memo() {
        // The documented limit of the trust model: a file deep in the tree rewritten in place.
        let scratch = Scratch::new("limits");
        bundle(&scratch.0);
        let memo = scratch.memo();
        let request = request(&scratch.0);
        let (first, ..) = resolve_counting(&request, &memo, &eager());
        fs::write(
            scratch.0.join("zig/lib/libc/include/stdint.h"),
            b"edited header",
        )
        .unwrap();
        let (remembered, hashed, _) = resolve_counting(&request, &memo, &eager());
        assert_eq!((hashed, &remembered), (0, &first));
        // Deleting the memo hashes everything again and sees the edit.
        fs::remove_dir_all(&memo).unwrap();
        let (fresh, hashed, _) = resolve_counting(&request, &memo, &eager());
        assert!(hashed > 0);
        assert_ne!(fresh.fingerprint, first.fingerprint);
    }

    #[test]
    fn a_memo_is_not_written_for_times_too_close_to_now_and_is_never_trusted_then() {
        let clock = Clock {
            now_ns: 10_000_000_000,
            racy_ns: RACY_NS,
        };
        assert!(clock.trusts(7_999_999_999));
        assert!(
            clock.trusts(8_000_000_000),
            "exactly the window is old enough"
        );
        assert!(!clock.trusts(8_000_000_001));
        assert!(!clock.trusts(10_000_000_000));
        assert!(
            !clock.trusts(u128::MAX),
            "a time in the future is not trusted"
        );
        let scratch = Scratch::new("racy");
        bundle(&scratch.0);
        let memo = scratch.memo();
        let request = request(&scratch.0);
        let never = Clock {
            now_ns: 0,
            racy_ns: u128::MAX,
        };
        let (_, hashed, probed) = resolve_counting(&request, &memo, &never);
        assert!(hashed > 0 && probed == 1);
        assert!(!memo.exists(), "nothing is remembered");
        let (_, hashed, probed) = resolve_counting(&request, &memo, &never);
        assert!(hashed > 0 && probed == 1);
    }

    #[test]
    fn another_bundle_launcher_or_a_damaged_memo_is_a_miss() {
        let scratch = Scratch::new("miss");
        bundle(&scratch.0);
        let memo = scratch.memo();
        let first_request = request(&scratch.0);
        let (first, ..) = resolve_counting(&first_request, &memo, &eager());
        // The same memo directory serves another root under another name.
        let other = Scratch::new("other");
        bundle(&other.0);
        fs::write(other.0.join("bin/clang"), "a different clang").unwrap();
        let other_request = request(&other.0);
        let (second, hashed, _) = resolve_counting(&other_request, &memo, &eager());
        assert!(hashed > 0);
        assert_ne!(second.fingerprint, first.fingerprint);
        assert_eq!(fs::read_dir(&memo).unwrap().count(), 2);
        // Damage, truncation and a wrong format are misses that are replaced.
        let name = memo_name(&first_request);
        for damaged in [
            b"{".to_vec(),
            Vec::new(),
            serde_json::to_vec(&json!({"format": 99})).unwrap(),
            b"[]".to_vec(),
        ] {
            fs::write(memo.join(&name), damaged).unwrap();
            let (again, hashed, probed) = resolve_counting(&first_request, &memo, &eager());
            assert!(hashed > 0 && probed == 1);
            assert_eq!(again, first);
            assert_eq!(resolve_counting(&first_request, &memo, &eager()).1, 0);
        }
        // A memo that names other digests than the files it was made for is not used for another zig.
        let mut value: Value =
            serde_json::from_slice(&fs::read(memo.join(&name)).unwrap()).unwrap();
        value["zig"] = Value::Null;
        fs::write(memo.join(&name), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(resolve_counting(&first_request, &memo, &eager()).1 > 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_bundle_with_a_symbolic_link_has_no_fingerprint_and_is_not_remembered() {
        let scratch = Scratch::new("symlink");
        bundle(&scratch.0);
        std::os::unix::fs::symlink("clang", scratch.0.join("bin/cc")).unwrap();
        let memo = scratch.memo();
        let request = request(&scratch.0);
        let (resolved, hashed, probed) = resolve_counting(&request, &memo, &eager());
        // The tools are still hashed on their own, so that a key could be made from them.
        assert_eq!((resolved.fingerprint, hashed, probed), (None, 3, 1));
        assert!(!memo.exists());
    }

    #[test]
    fn a_failing_probe_or_an_unreadable_tool_is_an_error() {
        let scratch = Scratch::new("errors");
        bundle(&scratch.0);
        let request = request(&scratch.0);
        let failed = resolve(&request, None, &eager(), &file_digest, &|| {
            Err(io::Error::other("tool version query failed"))
        });
        assert!(failed.is_err());
        fs::remove_file(scratch.0.join("bin/clang")).unwrap();
        let missing = resolve(&request, None, &eager(), &file_digest, &|| {
            Ok((Vec::new(), Vec::new()))
        });
        assert!(missing.is_err());
    }

    #[test]
    fn hashing_is_the_same_on_one_thread_and_on_many() {
        let scratch = Scratch::new("parallel");
        bundle(&scratch.0);
        for index in 0..40 {
            let file = scratch.0.join(format!("include/many/{index}.h"));
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, "x".repeat(index * 997)).unwrap();
        }
        let one = inventory_with(&scratch.0, COMPILER, &file_digest, 1).unwrap();
        let many = inventory_with(&scratch.0, COMPILER, &file_digest, 8).unwrap();
        assert_eq!(one.files, many.files);
        assert_eq!(one.files.len(), 7 + 40);
        for threads in [1, 8] {
            let failing = inventory_with(
                &scratch.0,
                COMPILER,
                &|path: &Path| {
                    if path.ends_with("7.h") {
                        Err(io::Error::other("unreadable"))
                    } else {
                        file_digest(path)
                    }
                },
                threads,
            );
            assert!(failing.is_none(), "{threads} threads");
        }
    }

    #[test]
    fn digests_are_found_by_path_from_the_root() {
        let scratch = Scratch::new("lookup");
        bundle(&scratch.0);
        let found = inventory(&scratch.0, COMPILER, &file_digest).unwrap();
        assert_eq!(
            found.digest("bin/clang"),
            Some(file_digest(&scratch.0.join("bin/clang")).unwrap())
        );
        for skipped in [
            "bin/tsuzuri",
            "manifest.json",
            "share/runtime/manifest.json",
        ] {
            assert_eq!(found.digest(skipped), None, "{skipped} is not a resource");
        }
    }
}
