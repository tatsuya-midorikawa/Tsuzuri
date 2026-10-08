//! The frontend cache (G17): parsed programs keyed by their source text, under
//! `<cache root>/frontend/`, with a manifest of each project's module source and interface hashes.
//!
//! Each project root and analysis kind has two files. The pack `p-<project key>.tzp` holds one
//! entry per distinct source of the project's last successful parse: a `syntax_codec` encoding
//! keyed by the source text, which decodes for whichever source index the file has in the
//! current build. The manifest `m-<project key>.json` records each module's parse key and
//! interface hash, so the next analysis can tell which modules changed and whether an interface
//! did (`FrontendDelta`). One pack per project instead of one file per source keeps a warm
//! analysis at two file reads, which matters where opening a file is slow (on-access scanning
//! makes an open cost up to a millisecond, more than parsing a module). Any failure (an
//! unreadable root, a corrupt or foreign pack or entry, a failed write) is a silent miss: the
//! cache never changes the compiler's output (D8).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};

use crate::cache::{BuildCache, Sha256, read_regular};
use crate::check::ModuleOrigin;
use crate::diagnostic::Diagnostic;
use crate::syntax::{Program, SourceKind};
use crate::syntax_codec::{self, Invalid, Mode};

/// The version of the pack, entry, manifest, codec and hash rules. Bump it whenever any changes.
pub(crate) const FRONTEND_FORMAT: u32 = 1;
const PACK_MAGIC: &[u8; 8] = b"TZPACK\0\0";
const PACK_HEADER_BYTES: usize = 84;
const ENTRY_MAGIC: &[u8; 8] = b"TZFRONT\0";
const ENTRY_HEADER_BYTES: usize = 84;
const CHECKSUM_BYTES: usize = 32;
/// The largest entry; larger encodings are not stored.
pub(crate) const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// The largest pack; a project whose pack would be larger is not stored.
pub(crate) const MAX_PACK_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
/// Eviction keeps `frontend/` within this size and removes files older than `MAX_AGE_MS`.
pub(crate) const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
pub(crate) const MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// Eviction removes temporary files older than this, which a crashed process left.
const TEMPORARY_AGE_MS: u64 = 60 * 60 * 1000;
/// The files one eviction inspects and the most it removes, as `BuildCache::evict`.
const SCAN_LIMIT: usize = 4096;
const REMOVE_LIMIT: usize = 128;

static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The compiler that wrote an entry: the format, the version, and the size and modification
/// time of the running executable (D2), or `None` when they are unknown, which disables the
/// cache. Hashing the whole executable, as the build cache does, would cost more than a parse.
pub(crate) fn compiler_identity() -> Option<[u8; 32]> {
    let metadata = fs::metadata(std::env::current_exe().ok()?).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let mut hash = Sha256::new();
    hash.field("frontend-format", &FRONTEND_FORMAT.to_le_bytes());
    hash.field("compiler-version", env!("CARGO_PKG_VERSION").as_bytes());
    hash.field("compiler-length", &metadata.len().to_le_bytes());
    hash.field("compiler-modified", &modified.to_le_bytes());
    Some(hash.finalize())
}

/// The key of a parse entry: the compiler and the source text, not its path or index.
pub(crate) fn parse_key(identity: &[u8; 32], text: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.field("identity", identity);
    hash.field("source", text.as_bytes());
    hash.finalize()
}

/// The hash of what other modules can observe of `program` (D6): it ignores spans,
/// documentation, function bodies, tests, the entry code and instance method bodies.
pub(crate) fn interface_hash(
    identity: &[u8; 32],
    program: &Program,
    source: usize,
) -> Result<[u8; 32], Invalid> {
    let mut hash = Sha256::new();
    hash.field("identity", identity);
    hash.field(
        "interface",
        &syntax_codec::encode(program, source, Mode::Interface)?,
    );
    Ok(hash.finalize())
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

fn now_ms() -> u64 {
    modified_ms(std::time::SystemTime::now())
}

fn modified_ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

/// The manifest of one project root and analysis kind (`program`, `tests` or `docs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectKey {
    pub(crate) hash: [u8; 32],
    pub(crate) kind: &'static str,
}

impl ProjectKey {
    pub(crate) fn new(root: &Path, kind: &'static str) -> Option<Self> {
        let root = fs::canonicalize(root).ok()?;
        let mut hash = Sha256::new();
        hash.field("frontend-format", &FRONTEND_FORMAT.to_le_bytes());
        hash.field("project-root", root.as_os_str().as_encoded_bytes());
        hash.field("kind", kind.as_bytes());
        Some(Self {
            hash: hash.finalize(),
            kind,
        })
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FrontendStats {
    pub(crate) hits: usize,
    pub(crate) misses: usize,
    /// Entries that were read but failed validation; each is also a miss.
    pub(crate) rejected: usize,
}

pub(crate) struct Parsed {
    pub(crate) program: Program,
    pub(crate) key: [u8; 32],
    pub(crate) interface: [u8; 32],
}

/// One module of a successful parse, as the manifest records it.
pub(crate) struct ModuleRecord<'a> {
    /// The module key that the checker names the module by.
    pub(crate) name: &'a str,
    pub(crate) path: &'a str,
    pub(crate) origin: ModuleOrigin,
    pub(crate) kind: Option<SourceKind>,
    /// The module's namespace, which also changes with the package manifest (D-35).
    pub(crate) namespace: &'a str,
    /// Whether the module may hold the program's entry code (its file is `Main`).
    pub(crate) entry: bool,
    pub(crate) source: [u8; 32],
    pub(crate) interface: [u8; 32],
}

/// How a project's modules changed since the manifest of its previous analysis (D7). The
/// names are module keys in ascending order.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FrontendDelta {
    /// Whether a manifest of the same compiler existed.
    pub(crate) previous: bool,
    pub(crate) changed_sources: Vec<String>,
    pub(crate) changed_interfaces: Vec<String>,
    pub(crate) removed: Vec<String>,
}

impl FrontendDelta {
    /// Whether the checking results of `module` may differ from the previous analysis. Any
    /// module can name any other qualified and instances are global, so every module depends
    /// on every interface; narrowing that by recorded uses is left to Phase 2 (D10).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the reuse rule for PB06 and PB07; G17 D10 deferred reuse"
        )
    )]
    pub(crate) fn must_recheck(&self, module: &str) -> bool {
        !self.previous
            || !self.changed_interfaces.is_empty()
            || !self.removed.is_empty()
            || self
                .changed_sources
                .binary_search_by(|name| name.as_str().cmp(module))
                .is_ok()
    }
}

/// A module as one manifest records it.
#[derive(Debug, PartialEq, Eq)]
struct ManifestModule {
    path: String,
    origin: &'static str,
    kind: Option<&'static str>,
    namespace: String,
    entry: bool,
    source: String,
    interface: String,
}

fn origin_name(origin: ModuleOrigin) -> &'static str {
    match origin {
        ModuleOrigin::User => "user",
        ModuleOrigin::Std => "std",
    }
}

fn kind_name(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Code => "tz",
        SourceKind::TypeClass => "tt",
        SourceKind::Computation => "tc",
    }
}

impl ManifestModule {
    fn new(record: &ModuleRecord<'_>) -> Self {
        Self {
            path: record.path.to_owned(),
            origin: origin_name(record.origin),
            kind: record.kind.map(kind_name),
            namespace: record.namespace.to_owned(),
            entry: record.entry,
            source: hex(&record.source),
            interface: hex(&record.interface),
        }
    }

    fn read(value: &Value) -> Option<(String, Self)> {
        let text = |field: &str| value[field].as_str().map(str::to_owned);
        let origin = match value["origin"].as_str()? {
            "user" => "user",
            "std" => "std",
            _ => return None,
        };
        let kind = match &value["kind"] {
            Value::Null => None,
            Value::String(kind) => Some(
                [
                    SourceKind::Code,
                    SourceKind::TypeClass,
                    SourceKind::Computation,
                ]
                .into_iter()
                .map(kind_name)
                .find(|name| name == kind)?,
            ),
            _ => return None,
        };
        Some((
            text("name")?,
            Self {
                path: text("path")?,
                origin,
                kind,
                namespace: text("namespace")?,
                entry: value["entry"].as_bool()?,
                source: text("source")?,
                interface: text("interface")?,
            },
        ))
    }
}

/// Compares the modules of the previous analysis with the current ones (D7).
fn compare(
    previous: Option<&BTreeMap<String, ManifestModule>>,
    current: &BTreeMap<String, ManifestModule>,
) -> FrontendDelta {
    let Some(previous) = previous else {
        let names: Vec<_> = current.keys().cloned().collect();
        return FrontendDelta {
            previous: false,
            changed_sources: names.clone(),
            changed_interfaces: names,
            removed: Vec::new(),
        };
    };
    let mut delta = FrontendDelta {
        previous: true,
        ..FrontendDelta::default()
    };
    for (name, module) in current {
        let Some(before) = previous.get(name) else {
            delta.changed_sources.push(name.clone());
            delta.changed_interfaces.push(name.clone());
            continue;
        };
        // The path alone changes nothing that the checker sees of the module.
        if before.source != module.source {
            delta.changed_sources.push(name.clone());
        }
        let face = |module: &ManifestModule| {
            (
                module.origin,
                module.kind,
                module.namespace.clone(),
                module.entry,
                module.interface.clone(),
            )
        };
        if face(before) != face(module) {
            delta.changed_interfaces.push(name.clone());
        }
    }
    delta.removed = previous
        .keys()
        .filter(|name| !current.contains_key(*name))
        .cloned()
        .collect();
    delta
}

/// Where an entry of this analysis comes from.
enum Stored {
    /// A byte range of the previous pack.
    Packed(Range<usize>),
    /// A new encoding.
    Fresh(Vec<u8>),
}

pub(crate) struct FrontendCache {
    /// `<root>/frontend`, canonicalized.
    directory: PathBuf,
    identity: [u8; 32],
    project: ProjectKey,
    /// The pack of the previous analysis, empty when there is none or it is not valid.
    pack: Vec<u8>,
    /// The entries of `pack` by parse key.
    packed: BTreeMap<[u8; 32], Range<usize>>,
    /// The entries of this analysis, in the order of their first parse.
    entries: Vec<([u8; 32], Stored)>,
    kept: BTreeSet<[u8; 32]>,
    pub(crate) stats: FrontendStats,
}

impl FrontendCache {
    /// Opens the cache of `project` under the build cache `root`, whose marker
    /// `BuildCache::open` checks or creates, and reads the project's pack. Returns `None` when
    /// the root, `frontend/` or the compiler identity is unusable.
    pub(crate) fn open(root: &Path, project: ProjectKey) -> Option<Self> {
        Self::open_with(root, project, compiler_identity()?)
    }

    fn open_with(root: &Path, project: ProjectKey, identity: [u8; 32]) -> Option<Self> {
        BuildCache::open(root).ok()?;
        let directory = root.join("frontend");
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        if let Err(error) = builder.create(&directory)
            && error.kind() != io::ErrorKind::AlreadyExists
        {
            return None;
        }
        let metadata = fs::symlink_metadata(&directory).ok()?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return None;
        }
        let mut cache = Self {
            directory: fs::canonicalize(&directory).ok()?,
            identity,
            project,
            pack: Vec::new(),
            packed: BTreeMap::new(),
            entries: Vec::new(),
            kept: BTreeSet::new(),
            stats: FrontendStats::default(),
        };
        // A pack of another compiler is a plain miss; a broken one counts as rejected.
        if let Ok(pack) = read_regular(&cache.pack_path(), MAX_PACK_BYTES)
            && pack.get(12..44) == Some(&identity[..])
        {
            match index_pack(&pack, &project) {
                Some(packed) => {
                    cache.pack = pack;
                    cache.packed = packed;
                }
                None => cache.stats.rejected += 1,
            }
        }
        Some(cache)
    }

    fn pack_path(&self) -> PathBuf {
        self.directory
            .join(format!("p-{}.tzp", hex(&self.project.hash)))
    }

    fn manifest_path(&self) -> PathBuf {
        self.directory
            .join(format!("m-{}.json", hex(&self.project.hash)))
    }

    /// Parses `text` as source `source`, or decodes its entry from the project's pack. The
    /// program and the diagnostics are those of `parser::parse_with_source_all`.
    pub(crate) fn parse(&mut self, text: &str, source: usize) -> Result<Parsed, Vec<Diagnostic>> {
        let key = parse_key(&self.identity, text);
        if let Some(range) = self.packed.get(&key).cloned() {
            match load(&self.pack[range.clone()], &key, source) {
                Some(parsed) => {
                    self.stats.hits += 1;
                    self.keep(key, Stored::Packed(range));
                    return Ok(parsed);
                }
                None => {
                    self.stats.rejected += 1;
                    self.packed.remove(&key);
                }
            }
        }
        self.stats.misses += 1;
        let program = crate::parser::parse_with_source_all(text, source)?;
        let encoded = syntax_codec::encode(&program, source, Mode::Full)
            .and_then(|full| Ok((full, interface_hash(&self.identity, &program, source)?)));
        let Ok((full, interface)) = encoded else {
            // Without an interface hash, the parse key stands in: it changes with any edit.
            return Ok(Parsed {
                program,
                key,
                interface: key,
            });
        };
        let entry = entry_bytes(&key, &interface, &full);
        if entry.len() as u64 <= MAX_ENTRY_BYTES {
            self.keep(key, Stored::Fresh(entry));
        }
        Ok(Parsed {
            program,
            key,
            interface,
        })
    }

    fn keep(&mut self, key: [u8; 32], stored: Stored) {
        if self.kept.insert(key) {
            self.entries.push((key, stored));
        }
    }

    /// Records the modules of a successful parse of every source: writes the project's pack
    /// when its entries changed and its manifest when the modules did, and returns how they
    /// changed since the previous manifest. Writing evicts old files.
    pub(crate) fn finish(&mut self, modules: &[ModuleRecord<'_>]) -> FrontendDelta {
        let mut current = BTreeMap::new();
        for module in modules {
            current
                .entry(module.name.to_owned())
                .or_insert_with(|| ManifestModule::new(module));
        }
        let path = self.manifest_path();
        let before = read_regular(&path, MAX_MANIFEST_BYTES).ok();
        let bytes = self.manifest_bytes(&current);
        let mut wrote = false;
        // The manifest is canonical, so an identical one means that nothing changed.
        let delta = if before.as_deref() == Some(bytes.as_slice()) {
            FrontendDelta {
                previous: true,
                ..FrontendDelta::default()
            }
        } else {
            if bytes.len() as u64 <= MAX_MANIFEST_BYTES {
                wrote |= self.write(&path, &bytes).is_ok();
            }
            let previous = before
                .as_deref()
                .and_then(|bytes| self.read_manifest(bytes));
            compare(previous.as_ref(), &current)
        };
        // Each previous entry is used at most once, so the pack is unchanged exactly when every
        // entry of this analysis came from it and it holds no other.
        let unchanged = self.entries.len() == self.packed.len()
            && self
                .entries
                .iter()
                .all(|(_, stored)| matches!(stored, Stored::Packed(_)));
        if !unchanged {
            let pack = self.pack_bytes();
            if pack.len() as u64 <= MAX_PACK_BYTES {
                wrote |= self.write(&self.pack_path(), &pack).is_ok();
            }
        }
        if wrote {
            let _ = self.evict();
        }
        // The rest of the analysis needs neither the old pack nor the new entries.
        self.pack = Vec::new();
        self.packed = BTreeMap::new();
        self.entries = Vec::new();
        self.kept = BTreeSet::new();
        delta
    }

    fn pack_bytes(&self) -> Vec<u8> {
        let mut pack = Vec::new();
        pack.extend_from_slice(PACK_MAGIC);
        pack.extend_from_slice(&FRONTEND_FORMAT.to_le_bytes());
        pack.extend_from_slice(&self.identity);
        pack.extend_from_slice(&self.project.hash);
        pack.extend_from_slice(&(self.entries.len() as u64).to_le_bytes());
        for (_, stored) in &self.entries {
            pack.extend_from_slice(match stored {
                Stored::Packed(range) => &self.pack[range.clone()],
                Stored::Fresh(entry) => entry,
            });
        }
        pack
    }

    fn manifest_bytes(&self, modules: &BTreeMap<String, ManifestModule>) -> Vec<u8> {
        let modules: Vec<_> = modules
            .iter()
            .map(|(name, module)| {
                json!({
                    "name": name,
                    "path": module.path,
                    "origin": module.origin,
                    "kind": module.kind,
                    "namespace": module.namespace,
                    "entry": module.entry,
                    "source": module.source,
                    "interface": module.interface,
                })
            })
            .collect();
        let manifest = json!({
            "format": FRONTEND_FORMAT,
            "compiler": hex(&self.identity),
            "kind": self.project.kind,
            "modules": modules,
        });
        let mut bytes = serde_json::to_vec(&manifest).expect("a manifest serializes");
        bytes.push(b'\n');
        bytes
    }

    /// The modules of a manifest that this compiler wrote for the project, or `None`.
    fn read_manifest(&self, bytes: &[u8]) -> Option<BTreeMap<String, ManifestModule>> {
        let value: Value = serde_json::from_slice(bytes).ok()?;
        if value["format"].as_u64() != Some(u64::from(FRONTEND_FORMAT))
            || value["compiler"].as_str() != Some(hex(&self.identity).as_str())
            || value["kind"].as_str() != Some(self.project.kind)
        {
            return None;
        }
        let mut modules = BTreeMap::new();
        for module in value["modules"].as_array()? {
            let (name, module) = ManifestModule::read(module)?;
            modules.entry(name).or_insert(module);
        }
        Some(modules)
    }

    /// Writes `bytes` to a temporary file and renames it over `path`, so readers see the old
    /// or the new file and never a partial one.
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let temporary = self.directory.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?
                .write_all(bytes)?;
            fs::rename(&temporary, path)
        })();
        let _ = fs::remove_file(&temporary);
        result
    }

    pub(crate) fn evict(&self) -> io::Result<()> {
        self.evict_with(MAX_TOTAL_BYTES, MAX_AGE_MS, now_ms())
    }

    /// Removes the oldest packs and manifests (by modification time) while `frontend/` is over
    /// `max_total` bytes or they are older than `max_age_ms`, and stale temporary files. Other
    /// names are left alone.
    fn evict_with(&self, max_total: u64, max_age_ms: u64, now: u64) -> io::Result<()> {
        let mut files = Vec::new();
        let mut total = 0u64;
        let mut removed = 0;
        for entry in fs::read_dir(&self.directory)?.take(SCAN_LIMIT) {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let temporary = name.starts_with(".tmp-");
            if !temporary && !owned_name(name) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.is_dir() {
                continue;
            }
            let modified = metadata.modified().map_or(0, modified_ms);
            if temporary {
                if now.saturating_sub(modified) > TEMPORARY_AGE_MS && removed < REMOVE_LIMIT {
                    remove(&entry.path())?;
                    removed += 1;
                }
                continue;
            }
            total = total.saturating_add(metadata.len());
            files.push((modified, name.to_owned(), metadata.len()));
        }
        files.sort();
        for (modified, name, size) in files {
            if removed == REMOVE_LIMIT
                || (total <= max_total && now.saturating_sub(modified) <= max_age_ms)
            {
                break;
            }
            remove(&self.directory.join(name))?;
            total = total.saturating_sub(size);
            removed += 1;
        }
        Ok(())
    }
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// `p-<64 hex>.tzp` or `m-<64 hex>.json`.
fn owned_name(name: &str) -> bool {
    let key = name
        .strip_prefix("p-")
        .and_then(|rest| rest.strip_suffix(".tzp"))
        .or_else(|| {
            name.strip_prefix("m-")
                .and_then(|rest| rest.strip_suffix(".json"))
        });
    key.is_some_and(|key| {
        key.len() == 64
            && key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn le_u64(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

/// The entries of a pack for `project` by parse key, or `None` when the pack is not valid.
/// Entries are checked one by one when they are used.
fn index_pack(pack: &[u8], project: &ProjectKey) -> Option<BTreeMap<[u8; 32], Range<usize>>> {
    let header = pack.get(..PACK_HEADER_BYTES)?;
    if &header[..8] != PACK_MAGIC
        || header[8..12] != FRONTEND_FORMAT.to_le_bytes()
        || header[44..76] != project.hash[..]
    {
        return None;
    }
    let count = le_u64(&header[76..84])?;
    let mut entries = BTreeMap::new();
    let mut at = PACK_HEADER_BYTES;
    for _ in 0..count {
        let header = pack.get(at..at.checked_add(ENTRY_HEADER_BYTES)?)?;
        let payload = usize::try_from(le_u64(&header[76..84])?).ok()?;
        let end = at
            .checked_add(ENTRY_HEADER_BYTES)?
            .checked_add(payload)?
            .checked_add(CHECKSUM_BYTES)?;
        if end > pack.len() {
            return None;
        }
        let key: [u8; 32] = header[12..44].try_into().ok()?;
        entries.insert(key, at..end);
        at = end;
    }
    (at == pack.len()).then_some(entries)
}

fn entry_bytes(key: &[u8; 32], interface: &[u8; 32], payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(ENTRY_HEADER_BYTES + payload.len() + CHECKSUM_BYTES);
    bytes.extend_from_slice(ENTRY_MAGIC);
    bytes.extend_from_slice(&FRONTEND_FORMAT.to_le_bytes());
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(interface);
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(payload);
    let mut hash = Sha256::new();
    hash.update(&bytes);
    bytes.extend_from_slice(&hash.finalize());
    bytes
}

/// The program of a valid entry for `key`, decoded as source `source`.
fn load(bytes: &[u8], key: &[u8; 32], source: usize) -> Option<Parsed> {
    let body_length = bytes.len().checked_sub(CHECKSUM_BYTES)?;
    if body_length < ENTRY_HEADER_BYTES {
        return None;
    }
    let (body, checksum) = bytes.split_at(body_length);
    let mut hash = Sha256::new();
    hash.update(body);
    if hash.finalize() != checksum
        || &body[..8] != ENTRY_MAGIC
        || body[8..12] != FRONTEND_FORMAT.to_le_bytes()
        || body[12..44] != key[..]
        || body[76..84] != ((body_length - ENTRY_HEADER_BYTES) as u64).to_le_bytes()
    {
        return None;
    }
    Some(Parsed {
        program: syntax_codec::decode(&body[ENTRY_HEADER_BYTES..], source).ok()?,
        key: *key,
        interface: body[44..76].try_into().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::TemporaryDirectory;
    use crate::parser::parse_with_source_all;

    const IDENTITY: [u8; 32] = [7; 32];
    const SOURCE: &str = "/// Doubles.\ndef double :: i64 -> i64 = \\x -> x * 2\n\ndouble 21\n";
    const OTHER: &str = "def other :: i64 = 2\n";

    /// A fresh cache root under the system temporary directory.
    fn root() -> (TemporaryDirectory, PathBuf) {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let root = temporary.path.join("cache");
        (temporary, root)
    }

    fn project(root: &Path, kind: &'static str) -> ProjectKey {
        ProjectKey::new(root.parent().unwrap(), kind).unwrap()
    }

    fn open(root: &Path, identity: [u8; 32]) -> FrontendCache {
        FrontendCache::open_with(root, project(root, "program"), identity).unwrap()
    }

    fn files(directory: &Path, prefix: &str) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.starts_with(prefix))
            .collect();
        names.sort();
        names
    }

    fn stats(hits: usize, misses: usize, rejected: usize) -> FrontendStats {
        FrontendStats {
            hits,
            misses,
            rejected,
        }
    }

    /// One analysis of `sources` as modules `M0`, `M1`, ...: parses each and finishes.
    fn analyze(cache: &mut FrontendCache, sources: &[&str]) -> (Vec<Parsed>, FrontendDelta) {
        let parsed: Vec<_> = sources
            .iter()
            .enumerate()
            .map(|(index, text)| cache.parse(text, index).unwrap())
            .collect();
        let names: Vec<_> = (0..sources.len())
            .map(|index| format!("M{index}"))
            .collect();
        let records: Vec<_> = parsed
            .iter()
            .zip(&names)
            .map(|(parsed, name)| ModuleRecord {
                name,
                path: name,
                origin: ModuleOrigin::User,
                kind: Some(SourceKind::Code),
                namespace: "",
                entry: false,
                source: parsed.key,
                interface: parsed.interface,
            })
            .collect();
        let delta = cache.finish(&records);
        (parsed, delta)
    }

    fn pack_entries(cache: &FrontendCache) -> usize {
        let pack = fs::read(cache.pack_path()).unwrap();
        index_pack(&pack, &cache.project).unwrap().len()
    }

    fn age(path: &Path) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(UNIX_EPOCH)
            .unwrap();
    }

    fn modified(path: &Path) -> std::time::SystemTime {
        fs::metadata(path).unwrap().modified().unwrap()
    }

    #[test]
    fn stores_then_hits() {
        let (_temporary, root) = root();
        let mut cache = open(&root, IDENTITY);
        let (first, delta) = analyze(&mut cache, &[SOURCE, OTHER]);
        assert_eq!(cache.stats, stats(0, 2, 0));
        assert!(!delta.previous);
        assert_eq!(files(&cache.directory, "p-").len(), 1);
        assert_eq!(pack_entries(&cache), 2);
        // Another process, with the source at another index as after adding a file.
        let mut again = open(&root, IDENTITY);
        let second = again.parse(SOURCE, 5).unwrap();
        assert_eq!(again.stats, stats(1, 0, 0));
        let expected = parse_with_source_all(SOURCE, 5).unwrap();
        assert_eq!(format!("{:?}", second.program), format!("{expected:?}"));
        assert_eq!(
            (first[0].key, first[0].interface),
            (second.key, second.interface)
        );
        assert_eq!(first[0].key, parse_key(&IDENTITY, SOURCE));
        assert_eq!(
            first[0].interface,
            interface_hash(&IDENTITY, &expected, 5).unwrap()
        );
        // An unchanged analysis rewrites neither file.
        age(&cache.pack_path());
        age(&cache.manifest_path());
        let mut unchanged = open(&root, IDENTITY);
        let (_, delta) = analyze(&mut unchanged, &[SOURCE, OTHER]);
        assert_eq!(unchanged.stats, stats(2, 0, 0));
        assert_eq!(delta, self::delta(true, &[], &[], &[]));
        assert_eq!(modified(&cache.pack_path()), UNIX_EPOCH);
        assert_eq!(modified(&cache.manifest_path()), UNIX_EPOCH);
        // Each kind of analysis has a pack of its own.
        let mut tests = FrontendCache::open_with(&root, project(&root, "tests"), IDENTITY).unwrap();
        tests.parse(SOURCE, 0).unwrap();
        assert_eq!(tests.stats, stats(0, 1, 0));
    }

    #[test]
    fn packs_follow_the_sources() {
        let (_temporary, root) = root();
        let mut cache = open(&root, IDENTITY);
        // Two files with the same text share an entry.
        analyze(&mut cache, &[SOURCE, OTHER, SOURCE]);
        assert_eq!(pack_entries(&cache), 2);
        // A removed source leaves the pack; a changed one replaces its entry.
        let mut next = open(&root, IDENTITY);
        let changed = OTHER.replace('2', "3");
        let (_, delta) = analyze(&mut next, &[&changed]);
        assert_eq!(next.stats, stats(0, 1, 0));
        assert_eq!(pack_entries(&next), 1);
        assert_eq!(delta, self::delta(true, &["M0"], &["M0"], &["M1", "M2"]));
        let mut last = open(&root, IDENTITY);
        analyze(&mut last, &[&changed]);
        assert_eq!(last.stats, stats(1, 0, 0));
    }

    #[test]
    fn corrupt_entries_are_misses_and_replaced() {
        let (_temporary, root) = root();
        analyze(&mut open(&root, IDENTITY), &[SOURCE]);
        let path = open(&root, IDENTITY).pack_path();
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, &bytes).unwrap();
        let mut cache = open(&root, IDENTITY);
        let (parsed, _) = analyze(&mut cache, &[SOURCE]);
        assert_eq!(cache.stats, stats(0, 1, 1));
        let expected = parse_with_source_all(SOURCE, 0).unwrap();
        assert_eq!(format!("{:?}", parsed[0].program), format!("{expected:?}"));
        let mut repaired = open(&root, IDENTITY);
        repaired.parse(SOURCE, 0).unwrap();
        assert_eq!(repaired.stats, stats(1, 0, 0));
        // A truncated or empty pack is rejected as a whole.
        let bytes = fs::read(&path).unwrap();
        for length in [bytes.len() - 1, bytes.len() / 2, 0] {
            fs::write(&path, &bytes[..length]).unwrap();
            let mut cache = open(&root, IDENTITY);
            cache.parse(SOURCE, 0).unwrap();
            assert_eq!(
                cache.stats,
                stats(0, 1, usize::from(length != 0)),
                "{length}"
            );
        }
    }

    #[test]
    fn parse_errors_are_not_stored() {
        let (_temporary, root) = root();
        let mut cache = open(&root, IDENTITY);
        let source = "def broken :: = \n\ndef also :: i64 -> = 1\n";
        let Err(errors) = cache.parse(source, 4) else {
            panic!("the source does not parse");
        };
        assert_eq!(errors, parse_with_source_all(source, 4).unwrap_err());
        assert!(cache.entries.is_empty());
        analyze(&mut cache, &[OTHER]);
        assert_eq!(pack_entries(&cache), 1);
    }

    /// Rewrites the only entry of the pack with `edit` applied to its header and a fresh
    /// checksum.
    fn rewrite_entry(root: &Path, edit: impl FnOnce(&mut [u8])) {
        let path = open(root, IDENTITY).pack_path();
        let mut bytes = fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - CHECKSUM_BYTES);
        edit(&mut bytes[PACK_HEADER_BYTES..]);
        let mut hash = Sha256::new();
        hash.update(&bytes[PACK_HEADER_BYTES..]);
        bytes.extend_from_slice(&hash.finalize());
        fs::write(&path, bytes).unwrap();
    }

    #[test]
    fn other_format_or_identity_is_a_miss() {
        let (_temporary, root) = root();
        analyze(&mut open(&root, IDENTITY), &[SOURCE]);
        rewrite_entry(&root, |entry| {
            entry[8..12].copy_from_slice(&2u32.to_le_bytes())
        });
        let mut cache = open(&root, IDENTITY);
        analyze(&mut cache, &[SOURCE]);
        assert_eq!(cache.stats, stats(0, 1, 1));
        // An entry for another source text under the key of this one is rejected too.
        rewrite_entry(&root, |entry| entry[44] ^= 1);
        let mut cache = open(&root, IDENTITY);
        cache.parse(SOURCE, 0).unwrap();
        assert_eq!(cache.stats, stats(1, 0, 0), "the interface hash is data");
        rewrite_entry(&root, |entry| entry[12] ^= 1);
        let mut cache = open(&root, IDENTITY);
        analyze(&mut cache, &[SOURCE]);
        assert_eq!(cache.stats, stats(0, 1, 0), "the key no longer matches");
        // A pack of another format, and one of another compiler.
        let path = cache.pack_path();
        let mut bytes = fs::read(&path).unwrap();
        bytes[8] = 2;
        fs::write(&path, &bytes).unwrap();
        let mut cache = open(&root, IDENTITY);
        cache.parse(SOURCE, 0).unwrap();
        assert_eq!(cache.stats, stats(0, 1, 1));
        bytes[8] = 1;
        fs::write(&path, &bytes).unwrap();
        let mut other = open(&root, [8; 32]);
        analyze(&mut other, &[SOURCE]);
        assert_eq!(other.stats, stats(0, 1, 0));
        let mut back = open(&root, IDENTITY);
        back.parse(SOURCE, 0).unwrap();
        assert_eq!(back.stats, stats(0, 1, 0));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_pack_is_a_miss() {
        let (temporary, root) = root();
        analyze(&mut open(&root, IDENTITY), &[SOURCE]);
        let path = open(&root, IDENTITY).pack_path();
        let elsewhere = temporary.path.join("elsewhere.tzp");
        fs::rename(&path, &elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        let mut cache = open(&root, IDENTITY);
        analyze(&mut cache, &[SOURCE]);
        assert_eq!(cache.stats, stats(0, 1, 0));
        // The miss replaced the link with a regular pack.
        assert!(fs::symlink_metadata(&path).unwrap().is_file());
        let mut again = open(&root, IDENTITY);
        again.parse(SOURCE, 0).unwrap();
        assert_eq!(again.stats, stats(1, 0, 0));
    }

    #[test]
    fn shares_root_with_build_cache() {
        let (_temporary, root) = root();
        let build = BuildCache::open(&root).unwrap();
        analyze(&mut open(&root, IDENTITY), &[SOURCE]);
        // The build cache's eviction skips the directory however old it is (Windows does not
        // open directories as files, so it stays new there).
        if let Ok(directory) = fs::File::open(root.join("frontend")) {
            let _ = directory.set_modified(
                std::time::SystemTime::now() - std::time::Duration::from_secs(31 * 24 * 60 * 60),
            );
        }
        build.evict().unwrap();
        assert_eq!(files(&root.join("frontend"), "p-").len(), 1);
        BuildCache::open(&root).unwrap();
        let mut cache = open(&root, IDENTITY);
        cache.parse(SOURCE, 0).unwrap();
        assert_eq!(cache.stats, stats(1, 0, 0));
        // A root that is not a cache is left alone.
        let (_other, unrelated) = self::root();
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("keep"), "user file").unwrap();
        assert!(
            FrontendCache::open_with(&unrelated, project(&unrelated, "program"), IDENTITY)
                .is_none()
        );
        assert!(!unrelated.join("frontend").exists());
    }

    fn module(name: &str, source: u8, interface: u8) -> ManifestModule {
        ManifestModule {
            path: format!("{name}.tz"),
            origin: "user",
            kind: Some("tz"),
            namespace: String::new(),
            entry: name == "Main",
            source: hex(&[source; 32]),
            interface: hex(&[interface; 32]),
        }
    }

    fn manifest(modules: Vec<(&str, ManifestModule)>) -> BTreeMap<String, ManifestModule> {
        modules
            .into_iter()
            .map(|(name, module)| (name.to_owned(), module))
            .collect()
    }

    fn delta(
        previous: bool,
        sources: &[&str],
        interfaces: &[&str],
        removed: &[&str],
    ) -> FrontendDelta {
        let names = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
        FrontendDelta {
            previous,
            changed_sources: names(sources),
            changed_interfaces: names(interfaces),
            removed: names(removed),
        }
    }

    #[test]
    fn delta_classifies_changes() {
        let before = manifest(vec![
            ("A", module("A", 1, 1)),
            ("B", module("B", 2, 2)),
            ("Main", module("Main", 3, 3)),
        ]);
        let unchanged = |edit: &dyn Fn(&mut BTreeMap<String, ManifestModule>)| {
            let mut after = manifest(vec![
                ("A", module("A", 1, 1)),
                ("B", module("B", 2, 2)),
                ("Main", module("Main", 3, 3)),
            ]);
            edit(&mut after);
            compare(Some(&before), &after)
        };
        // A body, comment or whitespace edit.
        assert_eq!(
            unchanged(&|after| after.get_mut("A").unwrap().source = hex(&[9; 32])),
            delta(true, &["A"], &[], &[])
        );
        // A header edit.
        assert_eq!(
            unchanged(&|after| *after.get_mut("A").unwrap() = module("A", 9, 9)),
            delta(true, &["A"], &["A"], &[])
        );
        // A renamed file with the same content.
        assert_eq!(
            unchanged(&|after| {
                after.remove("A");
                after.insert("C".into(), module("A", 1, 1));
            }),
            delta(true, &["C"], &["C"], &["A"])
        );
        // Another extension: the same content is a hit, but the kind changes.
        assert_eq!(
            unchanged(&|after| {
                let a = after.get_mut("A").unwrap();
                a.path = "A.tt".into();
                a.kind = Some("tt");
            }),
            delta(true, &[], &["A"], &[])
        );
        // Another compiler's manifest is not read, so there is no previous one.
        let after = unchanged(&|_| {});
        assert_eq!(after, delta(true, &[], &[], &[]));
        assert_eq!(
            compare(None, &before),
            delta(false, &["A", "B", "Main"], &["A", "B", "Main"], &[])
        );
        // An added file moves the others' source indices but changes only itself.
        assert_eq!(
            unchanged(&|after| {
                after.insert("AA".into(), module("AA", 5, 5));
            }),
            delta(true, &["AA"], &["AA"], &[])
        );
        // The namespace and the entry flag belong to the interface.
        assert_eq!(
            unchanged(&|after| after.get_mut("B").unwrap().namespace = "Lib".into()),
            delta(true, &[], &["B"], &[])
        );
        assert_eq!(
            unchanged(&|after| after.get_mut("Main").unwrap().entry = false),
            delta(true, &[], &["Main"], &[])
        );
    }

    #[test]
    fn must_recheck_follows_phase_one_rule() {
        let body = delta(true, &["A"], &[], &[]);
        assert!(body.must_recheck("A"));
        assert!(!body.must_recheck("B"));
        for delta in [
            delta(true, &["A"], &["A"], &[]),
            delta(true, &[], &[], &["C"]),
            delta(false, &["A", "B"], &["A", "B"], &[]),
        ] {
            assert!(
                delta.must_recheck("A") && delta.must_recheck("B"),
                "{delta:?}"
            );
        }
        assert!(!delta(true, &[], &[], &[]).must_recheck("A"));
    }

    #[test]
    fn finish_records_the_manifest() {
        let (_temporary, root) = root();
        let mut cache = open(&root, IDENTITY);
        let parsed = cache.parse(SOURCE, 0).unwrap();
        let record = |interface| ModuleRecord {
            name: "Main",
            path: "Main.tz",
            origin: ModuleOrigin::User,
            kind: Some(SourceKind::Code),
            namespace: "",
            entry: true,
            source: parsed.key,
            interface,
        };
        assert_eq!(
            cache.finish(&[record(parsed.interface)]),
            delta(false, &["Main"], &["Main"], &[])
        );
        let manifest = cache.manifest_path();
        let written = fs::read(&manifest).unwrap();
        let value: Value = serde_json::from_slice(&written).unwrap();
        assert_eq!(value["kind"], "program");
        assert_eq!(value["compiler"], hex(&IDENTITY));
        assert_eq!(value["modules"][0]["name"], "Main");
        assert_eq!(value["modules"][0]["source"], hex(&parsed.key));
        assert_eq!(value["modules"][0]["interface"], hex(&parsed.interface));
        assert_eq!(
            cache.finish(&[record(parsed.interface)]),
            delta(true, &[], &[], &[])
        );
        assert_eq!(fs::read(&manifest).unwrap(), written);
        assert_eq!(
            cache.finish(&[record([0; 32])]),
            delta(true, &[], &["Main"], &[])
        );
        // Another compiler does not read this compiler's manifest.
        let mut other = open(&root, [8; 32]);
        assert!(!other.finish(&[record([0; 32])]).previous);
        assert!(files(&cache.directory, ".tmp-").is_empty());
    }

    #[test]
    fn evicts_oldest_over_budget_and_stale_temporaries() {
        let (_temporary, root) = root();
        let cache = open(&root, IDENTITY);
        let now = now_ms();
        let hour = 60 * 60 * 1000;
        let file = |name: &str, bytes: usize, age_ms: u64| {
            let path = cache.directory.join(name);
            fs::write(&path, vec![0; bytes]).unwrap();
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(UNIX_EPOCH + std::time::Duration::from_millis(now - age_ms))
                .unwrap();
        };
        let entry = |byte: u8| format!("p-{}.tzp", hex(&[byte; 32]));
        file(&entry(1), 100, 5 * hour);
        file(&entry(2), 100, 4 * hour);
        file(&entry(3), 100, 3 * hour);
        file(&format!("m-{}.json", hex(&[4; 32])), 100, 2 * hour);
        file(".tmp-1-1", 10, 2 * hour);
        file(".tmp-1-2", 10, 0);
        file("keep.txt", 1000, 100 * hour);
        cache.evict_with(250, MAX_AGE_MS, now).unwrap();
        let left = files(&cache.directory, "");
        assert_eq!(
            left,
            [
                ".tmp-1-2".to_owned(),
                "keep.txt".to_owned(),
                format!("m-{}.json", hex(&[4; 32])),
                entry(3),
            ]
        );
        // Old entries go even under the size budget.
        cache.evict_with(u64::MAX, hour, now).unwrap();
        assert_eq!(files(&cache.directory, ""), [".tmp-1-2", "keep.txt"]);
    }
}
