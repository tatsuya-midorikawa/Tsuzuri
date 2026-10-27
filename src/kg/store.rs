//! Reading a knowledge graph (G27 Phase 2, D-60): `KgData` holds the rows of a graph's files and
//! `KgIndex`, built once when the graph is read: every endpoint numbered in the byte order of its
//! ID, and for each relation the edges out of and into each endpoint. Queries, cards, `impact`,
//! diagrams and MCP read graphs through it and never touch the disk.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use serde_json::Value;

use super::algo::Digraph;
use super::manifest::{self, Manifest, ModuleEntry};
use super::write::{Files, MANIFEST};

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
    pub facts: Option<BTreeMap<String, Fact>>,
    /// The rows of every `layout/*.jsonl`.
    pub layouts: Option<Vec<Value>>,
    pub diag: Vec<Diag>,
    pub index: KgIndex,
    by_id: BTreeMap<String, usize>,
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

impl KgData {
    /// Reads the graph at `dir`: `Err` with the reason when the manifest or a file it lists cannot
    /// be read (the caller reports `kg_missing`, `kg_broken` or `kg_incomplete`).
    pub fn open(dir: &Path) -> Result<KgData, manifest::Freshness> {
        let manifest = manifest::read(dir)?;
        let read = |path: &str| crate::cache::read_regular(&dir.join(path), FILE_LIMIT).ok();
        Self::parse(&manifest, &read).map_err(manifest::Freshness::Broken)
    }

    /// The graph of files in memory, as `open` would read them once written.
    pub fn from_parts(manifest: &Manifest, files: &Files) -> Result<KgData, String> {
        let read = |path: &str| files.get(path).map(<[u8]>::to_vec);
        Self::parse(manifest, &read)
    }

    fn parse(
        manifest: &Manifest,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<KgData, String> {
        let rows = |path: &str| -> Result<Option<Vec<Value>>, String> {
            match read(path) {
                Some(bytes) => lines(&bytes)
                    .map(Some)
                    .map_err(|reason| format!("{path} {reason}")),
                None => Ok(None),
            }
        };
        let listed = |prefix: &str| -> Vec<String> {
            manifest
                .files
                .keys()
                .filter(|path| path.starts_with(prefix) && path.ends_with(".jsonl"))
                .cloned()
                .collect()
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
        let fact_files = listed("facts/");
        let facts = if fact_files.is_empty() && !manifest.complete {
            None
        } else {
            let mut facts = BTreeMap::new();
            for path in fact_files {
                for row in rows(&path)?.unwrap_or_default() {
                    let fact = Fact {
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
                    };
                    facts.insert(fact.id.clone(), fact);
                }
            }
            Some(facts)
        };
        let layout_files = listed("layout/");
        let layouts = if layout_files.is_empty() && !manifest.complete {
            None
        } else {
            let mut layouts = Vec::new();
            for path in layout_files {
                layouts.extend(rows(&path)?.unwrap_or_default());
            }
            Some(layouts)
        };
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
        Ok(KgData {
            meta: Meta::of(manifest),
            nodes,
            edges,
            tests,
            facts,
            layouts,
            diag,
            index,
            by_id,
        })
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

    /// The key of the module `id` (from the manifest's modules).
    pub fn module_key(&self, id: &str) -> Option<&str> {
        self.meta
            .modules
            .iter()
            .find(|module| module.name == id)
            .map(|module| module.key.as_str())
    }

    pub fn fact(&self, id: &str) -> Option<&Fact> {
        self.facts.as_ref()?.get(id)
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
