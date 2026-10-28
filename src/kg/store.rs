//! Reading a knowledge graph (G27 Phase 2, D-60): `KgData` holds the rows of a graph's files and
//! `KgIndex`, built once when the graph is read: every endpoint numbered in the byte order of its
//! ID, and for each relation the edges out of and into each endpoint. Queries, cards, `impact`,
//! diagrams and MCP read graphs through it and never touch the disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde_json::Value;

use super::algo::Digraph;
use super::manifest::{self, Manifest, ModuleEntry};
use super::write::{Files, MANIFEST};
use crate::diagnostic::Diagnostic;

/// The most bytes of one graph file that a reader reads.
const FILE_LIMIT: u64 = 256 << 20;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: String,
    pub conv: String,
}

/// A node (a row of `nodes.jsonl`).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub module: String,
    pub name: String,
    pub signature: String,
    pub params: Vec<Param>,
    pub result: Option<String>,
    pub public: bool,
    pub export: bool,
    pub pure: bool,
    pub doc: Option<String>,
    pub deprecated: Option<String>,
    pub origin: String,
    pub span: Span,
    pub raw: Value,
}

/// An edge (a row of `edges.jsonl`).
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub rel: String,
    pub from: String,
    pub to: String,
    pub args: Option<Vec<String>>,
    pub span: Span,
    pub raw: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEntry {
    pub id: String,
    pub kind: String,
    pub module: String,
    pub name: String,
    pub index: usize,
    pub span: Span,
}

/// A trap, allocation or copy site of a facts row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub kind: String,
    pub proven: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectSite {
    pub effect: String,
    pub span: Span,
    pub via: Vec<String>,
}

/// A row of `facts/<Module>.jsonl`.
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    pub id: String,
    pub effects: Vec<String>,
    pub effect_sites: Vec<EffectSite>,
    pub effects_complete: bool,
    pub copy_sites: Vec<Site>,
    pub alloc_sites: Vec<Site>,
    pub alloc_complete: bool,
    pub trap_sites: Vec<Site>,
    pub recursion: String,
    pub specializations: u64,
    pub raw: Value,
}

/// What the manifest says of the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub schema: String,
    pub compiler: String,
    pub project: String,
    pub version: Option<String>,
    pub edition: String,
    pub complete: bool,
    pub source_hash: String,
    pub warnings: usize,
    pub modules: Vec<ModuleEntry>,
}

impl Meta {
    pub fn of(manifest: &Manifest) -> Self {
        Self {
            schema: manifest.schema.clone(),
            compiler: manifest.compiler.clone(),
            project: manifest.project.clone(),
            version: manifest.version.clone(),
            edition: manifest.edition.clone(),
            complete: manifest.complete,
            source_hash: manifest.source_hash.clone(),
            warnings: manifest.count("warnings").unwrap_or(0) as usize,
            modules: manifest.modules.clone(),
        }
    }
}

/// A row of `diag.jsonl`.
#[derive(Clone, Debug, PartialEq)]
pub struct Diag {
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<usize>,
    pub raw: Value,
}

/// The graph's endpoints and, per relation, the edges out of and into each one (D-60).
#[derive(Clone, Debug, Default)]
pub struct KgIndex {
    /// Every node ID and every edge endpoint, in byte order; an endpoint's number is its position.
    names: Vec<String>,
    numbers: BTreeMap<String, usize>,
    /// The node of each endpoint that is one.
    nodes: Vec<Option<usize>>,
    /// Per relation, the edges (indices into `KgData::edges`) out of and into each endpoint,
    /// ordered by the other end, then position.
    out: BTreeMap<String, Vec<Vec<usize>>>,
    into: BTreeMap<String, Vec<Vec<usize>>>,
}

impl KgIndex {
    fn build(nodes: &[Node], edges: &[Edge]) -> Self {
        let mut numbers: BTreeMap<String, usize> = BTreeMap::new();
        for node in nodes {
            numbers.insert(node.id.clone(), 0);
        }
        for edge in edges {
            numbers.insert(edge.from.clone(), 0);
            numbers.insert(edge.to.clone(), 0);
        }
        let names: Vec<String> = numbers.keys().cloned().collect();
        for (number, name) in names.iter().enumerate() {
            numbers.insert(name.clone(), number);
        }
        let mut node_of = vec![None; names.len()];
        for (index, node) in nodes.iter().enumerate() {
            node_of[numbers[&node.id]] = Some(index);
        }
        let mut out: BTreeMap<String, Vec<Vec<usize>>> = BTreeMap::new();
        let mut into: BTreeMap<String, Vec<Vec<usize>>> = BTreeMap::new();
        for (index, edge) in edges.iter().enumerate() {
            out.entry(edge.rel.clone())
                .or_insert_with(|| vec![Vec::new(); names.len()])[numbers[&edge.from]]
                .push(index);
            into.entry(edge.rel.clone())
                .or_insert_with(|| vec![Vec::new(); names.len()])[numbers[&edge.to]]
                .push(index);
        }
        let order = |list: &mut Vec<usize>, other: &dyn Fn(&Edge) -> &str| {
            list.sort_by(|a, b| {
                let (a, b) = (&edges[*a], &edges[*b]);
                (other(a), &a.span.path, a.span.start, a.span.end).cmp(&(
                    other(b),
                    &b.span.path,
                    b.span.start,
                    b.span.end,
                ))
            })
        };
        for lists in out.values_mut() {
            for list in lists.iter_mut() {
                order(list, &|edge| edge.to.as_str());
            }
        }
        for lists in into.values_mut() {
            for list in lists.iter_mut() {
                order(list, &|edge| edge.from.as_str());
            }
        }
        Self {
            names,
            numbers,
            nodes: node_of,
            out,
            into,
        }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The node (an index into `KgData::nodes`) of the endpoint `number`, if it is a node.
    pub fn node_of(&self, number: usize) -> Option<usize> {
        self.nodes.get(number).copied().flatten()
    }

    pub fn number(&self, id: &str) -> Option<usize> {
        self.numbers.get(id).copied()
    }

    pub fn name(&self, number: usize) -> &str {
        &self.names[number]
    }

    /// Every endpoint ID in byte order (node IDs and the ends of edges).
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The graph of the given relations over the endpoints (one edge per pair).
    pub fn digraph(&self, edges: &[Edge], relations: &[&str]) -> Digraph {
        Digraph::from_edges(
            self.names.len(),
            edges
                .iter()
                .filter(|edge| relations.contains(&edge.rel.as_str()))
                .map(|edge| (self.numbers[&edge.from], self.numbers[&edge.to])),
        )
    }
}

/// A graph's rows. `edges`, `tests` and `facts` are `None` when the file is absent.
#[derive(Clone, Debug)]
pub struct KgData {
    pub meta: Meta,
    pub nodes: Vec<Node>,
    pub edges: Option<Vec<Edge>>,
    pub tests: Option<Vec<TestEntry>>,
    /// The rows of `facts/*.jsonl`, read one module's file at a time.
    pub facts: Option<Tables<Fact>>,
    /// The rows of `layout/*.jsonl`, read one module's file at a time.
    pub layouts: Option<Tables<Value>>,
    pub diag: Vec<Diag>,
    pub index: KgIndex,
    by_id: BTreeMap<String, usize>,
    /// The position in `meta.modules` of each module, by name.
    modules_by_name: BTreeMap<String, usize>,
    source: Source,
    /// The first file of `facts` or `layouts` that did not match the manifest (`intact`).
    failure: Arc<OnceLock<manifest::Freshness>>,
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn optional(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn span(value: &Value) -> Span {
    let span = value.get("span").cloned().unwrap_or(Value::Null);
    Span {
        path: string(&span, "path"),
        start: number(&span, "start") as usize,
        end: number(&span, "end") as usize,
        line: number(&span, "line") as usize,
        column: number(&span, "column") as usize,
    }
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn sites(value: &Value, key: &str) -> Vec<Site> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| Site {
                    kind: string(item, "kind"),
                    proven: flag(item, "proven"),
                    span: span(item),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The JSON rows of a JSON Lines file, or the line number of a line that is not JSON.
fn lines(bytes: &[u8]) -> Result<Vec<Value>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "is not UTF-8".to_owned())?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(number, line)| {
            serde_json::from_str(line).map_err(|_| format!("line {} is not JSON", number + 1))
        })
        .collect()
}

/// How a reader gets a file of the graph by path: `None` when the graph has no such file.
type ReadFile<'a> = &'a dyn Fn(&str) -> Result<Option<Vec<u8>>, String>;

/// How many times a reader starts again when a writer replaced the graph while it read it.
const READ_ATTEMPTS: usize = 3;

/// The bytes of the file at `path` of the graph at `dir`, which must have the SHA-256 `sha`: an
/// error when it cannot be read, a directory on its way is not a plain directory, or the bytes
/// differ. `Manifest::parse` keeps only plain relative paths, so the file is below `dir`.
fn read_file(dir: &Path, path: &str, sha: &str) -> Result<Vec<u8>, String> {
    let names: Vec<&str> = path.split('/').collect();
    let mut parent = dir.to_path_buf();
    for name in &names[..names.len() - 1] {
        parent.push(name);
        if !std::fs::symlink_metadata(&parent).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(format!("{path} cannot be read"));
        }
    }
    let bytes = crate::cache::read_regular(&dir.join(path), FILE_LIMIT)
        .map_err(|_| format!("{path} cannot be read"))?;
    if super::input::digest(&bytes) != sha {
        return Err(format!("{path} does not match manifest.json"));
    }
    Ok(bytes)
}

/// A file of the graph at `dir` for a reader: `None` when the manifest does not list it (an
/// optional table), else as `read_file` reads it.
fn read_listed(dir: &Path, manifest: &Manifest, path: &str) -> Result<Option<Vec<u8>>, String> {
    match manifest.files.get(path) {
        Some(sha) => read_file(dir, path, sha).map(Some),
        None => Ok(None),
    }
}

/// Runs `body` over one publication of the graph at `dir`: it reads files through the function
/// it gets (see `read_listed`). A writer replaces the whole directory, so a file that does not
/// match the manifest was read across a replacement when the manifest has changed since: `body`
/// runs again, up to `READ_ATTEMPTS` times. Otherwise the graph is broken. The stamp of the
/// marker (`None` without one) is that publication's too: the marker read before the manifest and
/// after the files must be the same text, whose token no other publication has, else the read
/// crossed a replacement (even by the same compiler) and starts again.
fn with_snapshot<T>(
    dir: &Path,
    body: impl Fn(&Manifest, ReadFile) -> Result<T, String>,
) -> Result<(T, Option<String>), manifest::Freshness> {
    let mut attempt = 1;
    loop {
        let marker = super::write::read_marker(dir);
        let manifest = manifest::read(dir)?;
        match body(&manifest, &|path| read_listed(dir, &manifest, path)) {
            Ok(value) if super::write::read_marker(dir) == marker => {
                return Ok((value, marker.as_deref().map(super::write::stamp_of)));
            }
            Ok(_) if attempt == READ_ATTEMPTS => {
                return Err(manifest::Freshness::Broken(
                    "it was replaced while it was read".into(),
                ));
            }
            Ok(_) => attempt += 1,
            Err(reason) => {
                let replaced = manifest::read(dir).ok().as_ref() != Some(&manifest);
                if !replaced || attempt == READ_ATTEMPTS {
                    return Err(manifest::Freshness::Broken(reason));
                }
                attempt += 1;
            }
        }
    }
}

/// One publication of a graph: its manifest, the files that the reader took, and the stamp of
/// its marker.
pub struct Snapshot {
    pub manifest: Manifest,
    pub files: Files,
    pub stamp: Option<String>,
}

/// The manifest and the files that `wanted` selects of one publication of the graph at `dir`,
/// each with the SHA-256 that the manifest lists, and its marker's stamp (`kg export`,
/// `kg verify`).
pub fn read_snapshot(
    dir: &Path,
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Snapshot, manifest::Freshness> {
    let ((manifest, files), stamp) = with_snapshot(dir, |manifest, read| {
        let mut files = Files::default();
        for path in manifest.files.keys().filter(|path| wanted(path)) {
            let bytes = read(path)?.expect("a listed file");
            files.put(path.clone(), bytes);
        }
        Ok((manifest.clone(), files))
    })?;
    Ok(Snapshot {
        manifest,
        files,
        stamp,
    })
}

/// Where the per-module files of a graph come from.
#[derive(Clone, Debug)]
enum Source {
    Directory(PathBuf),
    Memory(Arc<BTreeMap<String, Vec<u8>>>),
}

impl Source {
    /// The bytes of the file at `path`, which the opened manifest lists with `sha`. A file of a
    /// directory that does not match was replaced when the manifest there lists it otherwise now.
    fn read(&self, path: &str, sha: &str) -> Result<Vec<u8>, manifest::Freshness> {
        match self {
            Self::Directory(dir) => read_file(dir, path, sha).map_err(|reason| {
                let listed =
                    read_manifest(dir).and_then(|manifest| manifest.files.get(path).cloned());
                if listed.as_deref() == Some(sha) {
                    manifest::Freshness::Broken(reason)
                } else {
                    manifest::Freshness::Replaced
                }
            }),
            Self::Memory(files) => files
                .get(path)
                .cloned()
                .ok_or_else(|| manifest::Freshness::Broken(format!("{path} cannot be read"))),
        }
    }

    fn directory(&self) -> Option<&Path> {
        match self {
            Self::Directory(dir) => Some(dir),
            Self::Memory(_) => None,
        }
    }
}

/// A table with one file per module (`facts/` or `layout/`). A file is read and parsed the first
/// time a row of its module is asked for, so that a query opens the files it needs and not one
/// per module of the project. It must have the SHA-256 of the manifest that the graph was opened
/// with: a file that does not (the graph was replaced since, or is broken) has no rows, and the
/// first such failure is kept for `KgData::intact`, so that no answer mixes two graphs.
#[derive(Clone, Debug)]
pub struct Tables<T> {
    source: Source,
    /// By path: the SHA-256 that the manifest lists, and the rows once read.
    files: BTreeMap<String, (String, OnceLock<Rows<T>>)>,
    /// The paths of each module's files by the lower-case stem of the module key.
    groups: BTreeMap<String, Vec<String>>,
    row: fn(Value) -> T,
    id: fn(&T) -> Option<&str>,
    failure: Arc<OnceLock<manifest::Freshness>>,
}

/// How a table reads a row from its JSON, and the row's ID.
type RowOf<T> = (fn(Value) -> T, fn(&T) -> Option<&str>);

/// The rows of one file, and the first row of each ID.
#[derive(Clone, Debug)]
struct Rows<T> {
    rows: Vec<T>,
    by_id: BTreeMap<String, usize>,
}

/// The lower-case stem of a module key that the table file at `path` holds: its name without
/// `.jsonl` and without the `~n` that `write::unique_names` adds.
fn group_of(path: &str) -> String {
    // One extension: the module `Util.jsonl` has the file `Util.jsonl.jsonl`.
    let file = path.rsplit('/').next().unwrap_or(path);
    let name = file.strip_suffix(".jsonl").unwrap_or(file);
    let base = match name.rsplit_once('~') {
        Some((base, number)) if number.bytes().all(|byte| byte.is_ascii_digit()) => base,
        _ => name,
    };
    base.to_lowercase()
}

impl<T> Tables<T> {
    fn new(
        source: Source,
        manifest: &Manifest,
        prefix: &str,
        (row, id): RowOf<T>,
        failure: Arc<OnceLock<manifest::Freshness>>,
    ) -> Self {
        let files: BTreeMap<String, (String, OnceLock<Rows<T>>)> = manifest
            .files
            .iter()
            .filter(|(path, _)| path.starts_with(prefix) && path.ends_with(".jsonl"))
            .map(|(path, sha)| (path.clone(), (sha.clone(), OnceLock::new())))
            .collect();
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in files.keys() {
            groups.entry(group_of(path)).or_default().push(path.clone());
        }
        Self {
            source,
            files,
            groups,
            row,
            id,
            failure,
        }
    }

    /// The files of the table with their SHA-256 (empty when unknown), by path.
    pub fn files(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files
            .iter()
            .map(|(path, (sha, _))| (path.as_str(), sha.as_str()))
    }

    /// The rows of the file at `path`.
    pub fn rows(&self, path: &str) -> &[T] {
        self.loaded(path).map_or(&[], |rows| &rows.rows)
    }

    fn loaded(&self, path: &str) -> Option<&Rows<T>> {
        let (sha, rows) = self.files.get(path)?;
        Some(rows.get_or_init(|| {
            let parsed = self.source.read(path, sha).and_then(|bytes| {
                lines(&bytes)
                    .map_err(|reason| manifest::Freshness::Broken(format!("{path} {reason}")))
            });
            let rows: Vec<T> = match parsed {
                Ok(rows) => rows.into_iter().map(self.row).collect(),
                Err(failure) => {
                    let _ = self.failure.set(failure);
                    Vec::new()
                }
            };
            let mut by_id = BTreeMap::new();
            for (index, row) in rows.iter().enumerate() {
                if let Some(id) = (self.id)(row) {
                    by_id.entry(id.to_owned()).or_insert(index);
                }
            }
            Rows { rows, by_id }
        }))
    }

    /// The files of the module with this key. Its file is named by the key's stem, with `~n` after
    /// it when the stem differs from another one only in case (`write::unique_names`); the files
    /// of the stem's case-insensitive group are all read.
    fn module_files(&self, key: &str) -> &[String] {
        self.groups
            .get(&super::write::stem(key).to_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// The rows of the module with this key.
    pub fn of_module(&self, key: &str) -> impl Iterator<Item = &T> {
        self.module_files(key)
            .iter()
            .flat_map(|path| self.rows(path))
    }

    /// The first row with this ID among the module's rows.
    pub fn find(&self, key: &str, id: &str) -> Option<&T> {
        self.module_files(key).iter().find_map(|path| {
            let rows = self.loaded(path)?;
            rows.by_id.get(id).map(|index| &rows.rows[*index])
        })
    }

    /// Every row of the table, file by file.
    pub fn all(&self) -> impl Iterator<Item = &T> {
        self.files.keys().flat_map(|path| self.rows(path))
    }
}

fn fact_of(row: Value) -> Fact {
    Fact {
        id: string(&row, "id"),
        effects: strings(&row, "effects"),
        effect_sites: row
            .get("effect_sites")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| EffectSite {
                        effect: string(item, "effect"),
                        span: span(item),
                        via: strings(item, "via"),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        effects_complete: flag(&row, "effects_complete"),
        copy_sites: sites(&row, "copy_sites"),
        alloc_sites: sites(&row, "alloc_sites"),
        alloc_complete: flag(&row, "alloc_complete"),
        trap_sites: sites(&row, "trap_sites"),
        recursion: optional(&row, "recursion").unwrap_or_else(|| "none".into()),
        specializations: number(&row, "specializations"),
        raw: row,
    }
}

impl KgData {
    /// Reads the graph at `dir`, all of one publication: `Err` with the reason when the manifest
    /// or a file it lists cannot be read or does not match (the caller reports `kg_missing`,
    /// `kg_broken` or `kg_incomplete`). The files of `facts/` and `layout/` are read when first
    /// used, against the same manifest (see `Tables`).
    pub fn open(dir: &Path) -> Result<KgData, manifest::Freshness> {
        with_snapshot(dir, |manifest, read| {
            Self::parse(manifest, read, Source::Directory(dir.to_path_buf()))
        })
        .map(|(data, _)| data)
    }

    /// Reads the graph at `dir` as `open` does, and judges the manifest of that publication: the
    /// graph of `source_hash` written by this compiler (`stamp`), complete. A graph that replaced
    /// a fresh one after the caller looked is not taken for it.
    pub fn open_fresh(
        dir: &Path,
        source_hash: &str,
        stamp: &str,
    ) -> Result<KgData, manifest::Freshness> {
        let ((manifest, data), marker) = with_snapshot(dir, |manifest, read| {
            let data = Self::parse(manifest, read, Source::Directory(dir.to_path_buf()))?;
            Ok((manifest.clone(), data))
        })?;
        match manifest::judge(&manifest, marker.as_deref(), source_hash, stamp) {
            manifest::Freshness::Fresh => Ok(data),
            other => Err(other),
        }
    }

    /// The graph of files in memory, as `open` would read them once written.
    pub fn from_parts(manifest: &Manifest, files: &Files) -> Result<KgData, String> {
        let read = |path: &str| Ok(files.get(path).map(<[u8]>::to_vec));
        let tables = files
            .0
            .iter()
            .filter(|(path, _)| path.starts_with("facts/") || path.starts_with("layout/"))
            .map(|(path, bytes)| (path.clone(), bytes.clone()))
            .collect();
        Self::parse(manifest, &read, Source::Memory(Arc::new(tables)))
    }

    fn parse(manifest: &Manifest, read: ReadFile, source: Source) -> Result<KgData, String> {
        let rows = |path: &str| -> Result<Option<Vec<Value>>, String> {
            match read(path)? {
                Some(bytes) => lines(&bytes)
                    .map(Some)
                    .map_err(|reason| format!("{path} {reason}")),
                None => Ok(None),
            }
        };
        let nodes: Vec<Node> = rows("nodes.jsonl")?
            .unwrap_or_default()
            .into_iter()
            .map(|row| Node {
                id: string(&row, "id"),
                kind: string(&row, "kind"),
                module: string(&row, "module"),
                name: string(&row, "name"),
                signature: string(&row, "signature"),
                params: row
                    .get("params")
                    .and_then(Value::as_array)
                    .map(|params| {
                        params
                            .iter()
                            .map(|param| Param {
                                name: string(param, "name"),
                                ty: string(param, "type"),
                                conv: string(param, "conv"),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                result: optional(&row, "result"),
                public: flag(&row, "public"),
                export: flag(&row, "export"),
                pure: flag(&row, "pure"),
                doc: optional(&row, "doc"),
                deprecated: optional(&row, "deprecated"),
                origin: string(&row, "origin"),
                span: span(&row),
                raw: row,
            })
            .collect();
        let edges: Option<Vec<Edge>> = rows("edges.jsonl")?.map(|rows| {
            rows.into_iter()
                .map(|row| Edge {
                    rel: string(&row, "rel"),
                    from: string(&row, "from"),
                    to: string(&row, "to"),
                    args: row.get("args").map(|_| strings(&row, "args")),
                    span: span(&row),
                    raw: row,
                })
                .collect()
        });
        let tests: Option<Vec<TestEntry>> = rows("tests.jsonl")?.map(|rows| {
            rows.into_iter()
                .map(|row| TestEntry {
                    id: string(&row, "id"),
                    kind: string(&row, "kind"),
                    module: string(&row, "module"),
                    name: string(&row, "name"),
                    index: number(&row, "index") as usize,
                    span: span(&row),
                })
                .collect()
        });
        // A graph of a failed analysis has no tables; a complete one has them, maybe empty.
        let tables = |prefix: &str| {
            manifest.complete
                || manifest
                    .files
                    .keys()
                    .any(|path| path.starts_with(prefix) && path.ends_with(".jsonl"))
        };
        let failure = Arc::new(OnceLock::new());
        let facts = tables("facts/").then(|| {
            Tables::new(
                source.clone(),
                manifest,
                "facts/",
                (fact_of, |fact| Some(&fact.id)),
                failure.clone(),
            )
        });
        let layouts = tables("layout/").then(|| {
            Tables::new(
                source.clone(),
                manifest,
                "layout/",
                (|row| row, |row| row.get("id").and_then(Value::as_str)),
                failure.clone(),
            )
        });
        let diag = rows("diag.jsonl")?
            .unwrap_or_default()
            .into_iter()
            .map(|row| Diag {
                severity: string(&row, "severity"),
                code: optional(&row, "code"),
                message: string(&row, "message"),
                path: optional(&row, "path"),
                line: row
                    .get("span")
                    .and_then(|span| span.get("line"))
                    .and_then(Value::as_u64)
                    .map(|line| line as usize),
                raw: row,
            })
            .collect();
        let index = KgIndex::build(&nodes, edges.as_deref().unwrap_or_default());
        let by_id = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), index))
            .collect();
        let meta = Meta::of(manifest);
        let mut modules_by_name = BTreeMap::new();
        for (index, module) in meta.modules.iter().enumerate() {
            modules_by_name.entry(module.name.clone()).or_insert(index);
        }
        Ok(KgData {
            meta,
            nodes,
            edges,
            tests,
            facts,
            layouts,
            diag,
            index,
            by_id,
            modules_by_name,
            source,
            failure,
        })
    }

    /// `Ok` when every file that the graph read on demand matched the manifest it was opened with;
    /// else `E2009`: the graph was replaced since it was opened (`kg_stale`), or is broken. A
    /// command checks it before it prints an answer.
    pub fn intact(&self) -> Result<(), Diagnostic> {
        match self.failure.get() {
            None => Ok(()),
            Some(failure) => {
                let dir = self.source.directory().unwrap_or(Path::new("<memory>"));
                Err(manifest::diagnostic(dir, failure).expect("a failure is not fresh"))
            }
        }
    }

    /// Reads every file of `facts/` and `layout/` now (and checks them, see `intact`): for a graph
    /// whose directory another graph is about to replace.
    pub fn read_all(&self) -> Result<(), Diagnostic> {
        if let Some(facts) = &self.facts {
            facts.all().for_each(drop);
        }
        if let Some(layouts) = &self.layouts {
            layouts.all().for_each(drop);
        }
        self.intact()
    }

    /// The directory that the graph was read from (`None` for a graph in memory).
    pub fn directory(&self) -> Option<&Path> {
        self.source.directory()
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.by_id.get(id).map(|index| &self.nodes[*index])
    }

    pub fn edges(&self) -> &[Edge] {
        self.edges.as_deref().unwrap_or_default()
    }

    /// The edges of `rel` out of `id`, ordered by target, then position.
    pub fn edges_from(&self, rel: &str, id: &str) -> Vec<&Edge> {
        self.adjacent(&self.index.out, rel, id)
    }

    /// The edges of `rel` into `id`, ordered by source, then position.
    pub fn edges_to(&self, rel: &str, id: &str) -> Vec<&Edge> {
        self.adjacent(&self.index.into, rel, id)
    }

    fn adjacent(
        &self,
        table: &BTreeMap<String, Vec<Vec<usize>>>,
        rel: &str,
        id: &str,
    ) -> Vec<&Edge> {
        let (Some(lists), Some(number)) = (table.get(rel), self.index.number(id)) else {
            return Vec::new();
        };
        lists[number]
            .iter()
            .map(|edge| &self.edges()[*edge])
            .collect()
    }

    /// The children of `parent` of the given kind, through `contains`, in declaration order.
    pub fn children(&self, parent: &str, kind: &str) -> Vec<&Node> {
        let mut children: Vec<&Node> = if self.edges.is_some() {
            self.edges_from("contains", parent)
                .into_iter()
                .filter_map(|edge| self.node(&edge.to))
                .filter(|node| node.kind == kind)
                .collect()
        } else {
            let prefix = format!("{parent}.");
            self.nodes
                .iter()
                .filter(|node| {
                    node.kind == kind
                        && node
                            .id
                            .strip_prefix(&prefix)
                            .is_some_and(|rest| !rest.contains('.'))
                })
                .collect()
        };
        children.sort_by(|a, b| (a.span.start, &a.id).cmp(&(b.span.start, &b.id)));
        children
    }

    /// The node IDs and the endpoints of edges, in byte order.
    pub fn all_ids(&self) -> Vec<&str> {
        self.index.names().iter().map(String::as_str).collect()
    }

    pub fn test_index(&self, id: &str) -> Option<usize> {
        self.tests
            .as_ref()?
            .iter()
            .find(|test| test.id == id)
            .map(|test| test.index)
    }

    /// Replaces the modules of the manifest (a graph made of rows alone has none), with their
    /// index by name.
    pub fn set_modules(&mut self, modules: Vec<ModuleEntry>) {
        self.modules_by_name.clear();
        for (index, module) in modules.iter().enumerate() {
            self.modules_by_name
                .entry(module.name.clone())
                .or_insert(index);
        }
        self.meta.modules = modules;
    }

    /// The key of the module `id` (from the manifest's modules).
    pub fn module_key(&self, id: &str) -> Option<&str> {
        self.modules_by_name
            .get(id)
            .map(|index| self.meta.modules[*index].key.as_str())
    }

    /// The facts row of the node `id`, from its module's file.
    pub fn fact(&self, id: &str) -> Option<&Fact> {
        let node = self.node(id)?;
        let key = self.module_key(&node.module).unwrap_or(&node.module);
        self.facts.as_ref()?.find(key, id)
    }

    /// The structural invariants of the graph (D-60): node IDs are unique, every edge starts at a
    /// node (`implements` may start at a type that is not one), and `contains` is a forest whose
    /// roots are the modules. The first violation, if any.
    pub fn violation(&self) -> Option<String> {
        if self.by_id.len() != self.nodes.len() {
            return Some("nodes.jsonl has two nodes with one ID".into());
        }
        let mut parents: BTreeMap<&str, &str> = BTreeMap::new();
        for edge in self.edges() {
            if edge.rel != "implements" && self.node(&edge.from).is_none() {
                return Some(format!("edge from '{}' that is no node", edge.from));
            }
            if edge.rel == "contains" {
                if self.node(&edge.to).is_none() {
                    return Some(format!("contains '{}' that is no node", edge.to));
                }
                if parents.insert(&edge.to, &edge.from).is_some() {
                    return Some(format!("'{}' has two parents", edge.to));
                }
            }
        }
        self.edges.as_ref()?;
        for node in &self.nodes {
            let has_parent = parents.contains_key(node.id.as_str());
            if (node.kind == "module") == has_parent {
                return Some(format!("'{}' breaks the contains forest", node.id));
            }
        }
        let numbering: BTreeMap<&str, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.as_str(), index))
            .collect();
        let parent_of: Vec<Option<usize>> = self
            .nodes
            .iter()
            .map(|node| {
                parents
                    .get(node.id.as_str())
                    .and_then(|parent| numbering.get(parent).copied())
            })
            .collect();
        super::algo::forest_cycle(&parent_of)
            .map(|node| format!("'{}' is on a contains cycle", self.nodes[node].id))
    }
}

/// Where a view reads the standard library's graph (G27 Phase 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdSource {
    /// No std graph: std IDs are external endpoints.
    None,
    /// The std graph in the compiler's cache, built there once; in memory when there is no cache.
    Cache,
    /// The std graph in the compiler's cache if it is there, never built (`--no-refresh`).
    Cached,
    /// The std graph built in memory and never written.
    Memory,
}

/// A project's graph and, behind it, the standard library's: lookups take the project first.
pub struct KgView {
    project: Option<Arc<KgData>>,
    std: StdSource,
    loaded: OnceLock<Option<Arc<KgData>>>,
}

impl KgView {
    pub fn new(project: Option<Arc<KgData>>, std: StdSource) -> Self {
        Self {
            project,
            std,
            loaded: OnceLock::new(),
        }
    }

    pub fn project(&self) -> Option<&KgData> {
        self.project.as_deref()
    }

    /// `KgData::intact` of the project's graph and of the std graph if it was read.
    pub fn intact(&self) -> Result<(), Diagnostic> {
        if let Some(project) = self.project() {
            project.intact()?;
        }
        match self.loaded.get() {
            Some(Some(std)) => std.intact(),
            _ => Ok(()),
        }
    }

    /// The std graph, built or read the first time it is needed.
    pub fn std(&self) -> Option<&KgData> {
        if self.std == StdSource::None {
            return None;
        }
        self.loaded
            .get_or_init(|| super::std_kg::load(self.std).map(Arc::new))
            .as_deref()
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.project()
            .and_then(|data| data.node(id))
            .or_else(|| self.std().and_then(|data| data.node(id)))
    }

    pub fn edges_from(&self, rel: &str, id: &str) -> Vec<&Edge> {
        let found = self
            .project()
            .map(|data| data.edges_from(rel, id))
            .unwrap_or_default();
        if !found.is_empty() {
            return found;
        }
        self.std()
            .map(|data| data.edges_from(rel, id))
            .unwrap_or_default()
    }

    pub fn edges_to(&self, rel: &str, id: &str) -> Vec<&Edge> {
        self.project()
            .map(|data| data.edges_to(rel, id))
            .unwrap_or_default()
    }
}

/// The manifest of a graph read from `dir` (for callers that need it with the rows).
pub fn read_manifest(dir: &Path) -> Option<Manifest> {
    let bytes = crate::cache::read_regular(&dir.join(MANIFEST), manifest::MANIFEST_LIMIT).ok()?;
    Manifest::parse(std::str::from_utf8(&bytes).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kg::input::digest;

    /// The files of a graph of one node named `id`, with its manifest.
    fn graph(id: &str) -> Files {
        graph_of(id, &"00".repeat(32), true)
    }

    /// The files of a graph of one node named `id` written for `source_hash` by this compiler.
    fn graph_of(id: &str, source_hash: &str, complete: bool) -> Files {
        let mut files = Files::default();
        files.put("nodes.jsonl", format!("{{\"id\":\"{id}\"}}\n").into_bytes());
        files.put(
            "facts/Main.jsonl",
            format!("{{\"id\":\"{id}\"}}\n").into_bytes(),
        );
        let manifest = Manifest {
            schema: "tz-kg/1".into(),
            compiler: manifest::compiler(),
            edition: "2026".into(),
            project: "p".into(),
            version: None,
            complete,
            source_hash: source_hash.into(),
            counts: Vec::new(),
            modules: Vec::new(),
            files: files
                .0
                .iter()
                .map(|(path, bytes)| (path.clone(), digest(bytes)))
                .collect(),
        };
        files.put(MANIFEST, manifest.render().into_bytes());
        files
    }

    /// A reader that a writer interrupts reads the files of one publication: the read that
    /// crossed the replacement starts again on the new graph.
    #[test]
    fn a_read_across_a_replacement_starts_again() {
        let root = crate::driver::TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let dir = root.path.join("kg");
        super::super::write::publish(&dir, &graph("old"), "00").unwrap();
        let attempts = std::cell::Cell::new(0);
        let ((manifest, files), stamp) = with_snapshot(&dir, |manifest, read| {
            attempts.set(attempts.get() + 1);
            let nodes = read("nodes.jsonl")?.unwrap();
            if attempts.get() == 1 {
                // A writer replaces the graph between the reader's two files.
                super::super::write::publish(&dir, &graph("new"), "00").unwrap();
            }
            let facts = read("facts/Main.jsonl")?.unwrap();
            Ok((manifest.clone(), (nodes, facts)))
        })
        .unwrap();
        assert_eq!(attempts.get(), 2);
        assert_eq!(stamp.as_deref(), Some("00"));
        assert_eq!(files.0, b"{\"id\":\"new\"}\n");
        assert_eq!(files.1, b"{\"id\":\"new\"}\n");
        assert_eq!(manifest.files["nodes.jsonl"], digest(&files.0));
        // A file read on demand that does not match an unchanged manifest is broken, and so is a
        // missing one; `nodes.jsonl` is read when the graph is opened.
        let message = |data: &KgData| data.read_all().unwrap_err().message.clone();
        std::fs::write(dir.join("facts/Main.jsonl"), b"{}\n").unwrap();
        let data = KgData::open(&dir).unwrap();
        assert!(
            message(&data).contains("is broken (facts/Main.jsonl does not match manifest.json)")
        );
        std::fs::remove_file(dir.join("facts/Main.jsonl")).unwrap();
        let data = KgData::open(&dir).unwrap();
        assert!(message(&data).contains("is broken (facts/Main.jsonl cannot be read)"));
        std::fs::remove_file(dir.join("nodes.jsonl")).unwrap();
        assert_eq!(
            KgData::open(&dir).unwrap_err(),
            manifest::Freshness::Broken("nodes.jsonl cannot be read".into())
        );
    }

    /// A graph that a writer replaces after a reader opened it gives the reader no rows of the
    /// new graph: the reader learns that it was replaced.
    #[test]
    fn lazy_rows_of_a_replaced_graph_are_refused() {
        let root = crate::driver::TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let dir = root.path.join("kg");
        super::super::write::publish(&dir, &graph("old"), "00").unwrap();
        let data = KgData::open(&dir).unwrap();
        assert_eq!(data.nodes[0].id, "old");
        super::super::write::publish(&dir, &graph("new"), "00").unwrap();
        assert_eq!(data.facts.as_ref().unwrap().all().count(), 0);
        let error = data.intact().unwrap_err();
        assert_eq!(error.code, "E2009");
        assert!(
            error
                .message
                .ends_with("was replaced while it was read; run the command again")
        );
        // A graph that read everything first keeps it.
        let data = KgData::open(&dir).unwrap();
        data.read_all().unwrap();
        super::super::write::publish(&dir, &graph("newer"), "00").unwrap();
        let ids: Vec<&str> = data
            .facts
            .as_ref()
            .unwrap()
            .all()
            .map(|fact| fact.id.as_str())
            .collect();
        assert_eq!(ids, ["new"]);
        data.intact().unwrap();
    }

    /// The stamp of a snapshot is that of its own marker: a graph of the same files that another
    /// compiler publishes between the reader's manifest and its last file is read again.
    #[test]
    fn a_snapshot_keeps_the_stamp_of_its_marker() {
        let root = crate::driver::TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let dir = root.path.join("kg");
        super::super::write::publish(&dir, &graph("a"), "00").unwrap();
        let attempts = std::cell::Cell::new(0);
        let (_, stamp) = with_snapshot(&dir, |_, read| {
            attempts.set(attempts.get() + 1);
            let nodes = read("nodes.jsonl")?;
            if attempts.get() == 1 {
                // The same files, published by a compiler with another stamp.
                super::super::write::publish(&dir, &graph("a"), "01").unwrap();
            }
            Ok(nodes)
        })
        .unwrap();
        assert_eq!(attempts.get(), 2);
        assert_eq!(stamp.as_deref(), Some("01"));
        assert_eq!(
            KgData::open_fresh(&dir, &"00".repeat(32), "00").unwrap_err(),
            manifest::Freshness::Stale("built by a different compiler")
        );
    }

    /// Another publication by the same compiler between the reader's marker and its last file (A,
    /// then B, then A again) is read again too: each publication's marker has its own token.
    #[test]
    fn a_snapshot_is_of_one_publication_of_the_same_compiler() {
        let root = crate::driver::TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let dir = root.path.join("kg");
        super::super::write::publish(&dir, &graph("a"), "00").unwrap();
        let attempts = std::cell::Cell::new(0);
        let (_, stamp) = with_snapshot(&dir, |_, read| {
            attempts.set(attempts.get() + 1);
            let nodes = read("nodes.jsonl")?;
            if attempts.get() == 1 {
                super::super::write::publish(&dir, &graph("a"), "01").unwrap();
                super::super::write::publish(&dir, &graph("a"), "00").unwrap();
            }
            Ok(nodes)
        })
        .unwrap();
        assert_eq!(attempts.get(), 2);
        assert_eq!(stamp.as_deref(), Some("00"));
    }

    /// A reader judges the publication that it read: a graph of other sources, of another compiler
    /// or incomplete is not taken for the fresh one that it replaced.
    #[test]
    fn a_fresh_read_judges_the_publication_it_read() {
        let root = crate::driver::TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let dir = root.path.join("kg");
        let fresh = "00".repeat(32);
        super::super::write::publish(&dir, &graph("a"), "00").unwrap();
        assert!(KgData::open_fresh(&dir, &fresh, "00").is_ok());
        assert_eq!(
            KgData::open_fresh(&dir, &fresh, "01").unwrap_err(),
            manifest::Freshness::Stale("built by a different compiler")
        );
        super::super::write::publish(&dir, &graph_of("b", &"11".repeat(32), true), "00").unwrap();
        assert_eq!(
            KgData::open_fresh(&dir, &fresh, "00").unwrap_err(),
            manifest::Freshness::Stale("sources changed")
        );
        super::super::write::publish(&dir, &graph_of("c", &fresh, false), "00").unwrap();
        assert!(matches!(
            KgData::open_fresh(&dir, &fresh, "00").unwrap_err(),
            manifest::Freshness::Incomplete(_)
        ));
    }

    /// A module's rows are its files' (the stem's case-insensitive group, `~n` included), and a
    /// row is found by its ID: the first one with it.
    #[test]
    fn module_rows_are_found_by_group_and_id() {
        let mut bytes = BTreeMap::new();
        bytes.insert(
            "facts/Acme.Main.jsonl".to_owned(),
            b"{\"id\":\"a\",\"effects\":[\"io\"]}\n{\"id\":\"a\"}\n".to_vec(),
        );
        bytes.insert(
            "facts/acme.main~1.jsonl".to_owned(),
            b"{\"id\":\"b\"}\n".to_vec(),
        );
        bytes.insert(
            "facts/Acme.Mainly.jsonl".to_owned(),
            b"{\"id\":\"c\"}\n".to_vec(),
        );
        bytes.insert(
            "facts/Util.jsonl.jsonl".to_owned(),
            b"{\"id\":\"d\"}\n".to_vec(),
        );
        let manifest = Manifest {
            schema: "tz-kg/1".into(),
            compiler: manifest::compiler(),
            edition: "2026".into(),
            project: "p".into(),
            version: None,
            complete: true,
            source_hash: "00".repeat(32),
            counts: Vec::new(),
            modules: Vec::new(),
            files: bytes
                .iter()
                .map(|(path, bytes)| (path.clone(), digest(bytes)))
                .collect(),
        };
        let tables = Tables::new(
            Source::Memory(Arc::new(bytes)),
            &manifest,
            "facts/",
            (fact_of, |fact: &Fact| Some(fact.id.as_str())),
            Arc::new(OnceLock::new()),
        );
        let ids: Vec<&str> = tables
            .of_module("Acme::Main")
            .map(|fact| fact.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "a", "b"]);
        assert_eq!(tables.find("Acme::Main", "a").unwrap().effects, ["io"]);
        assert_eq!(tables.find("Acme::Main", "b").unwrap().id, "b");
        assert!(tables.find("Acme::Main", "c").is_none());
        assert_eq!(tables.find("Acme::Mainly", "c").unwrap().id, "c");
        // The module `Util.jsonl`: one extension is the file's.
        assert_eq!(tables.find("Util.jsonl", "d").unwrap().id, "d");
        assert!(tables.find("Acme::Other", "a").is_none());
    }

    /// A directory on the way to a listed file that is a link is not followed.
    #[cfg(unix)]
    #[test]
    fn linked_directories_are_not_followed() {
        let root = crate::driver::TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let dir = root.path.join("kg");
        super::super::write::publish(&dir, &graph("a"), "00").unwrap();
        let outside = root.path.join("outside");
        std::fs::rename(dir.join("facts"), &outside).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("facts")).unwrap();
        let data = KgData::open(&dir).unwrap();
        assert!(
            data.read_all()
                .unwrap_err()
                .message
                .contains("is broken (facts/Main.jsonl cannot be read)")
        );
    }
}
