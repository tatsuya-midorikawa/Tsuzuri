//! The rows of the knowledge graph (G26, D-60) and their JSON Lines. A row's keys are written in a
//! fixed order; rows are sorted (nodes, tests, facts and layouts by `id`, edges by `rel`, `from`,
//! `to`, then position), equal rows are written once, and a row whose position is not in a source
//! is not written.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use super::json::{self, Object};
use crate::SourceInput;
use crate::check::CheckedModule;
use crate::check::kg_capture::Capture;
use crate::diagnostic::Span;

/// The facts keys that this build writes, in their order (`KgModel::fact_keys`).
pub const FACT_KEYS: [&str; 14] = [
    "id",
    "effects",
    "effect_sites",
    "effects_complete",
    "copy_sites",
    "alloc_sites",
    "alloc_complete",
    "trap_sites",
    "ownership",
    "recursion",
    "loops",
    "specializations",
    "parallel",
    "truncated",
];

/// A position in a source: the path relative to the project root with `/` separators, the UTF-8
/// byte range, and the line and column (in characters) of its start, both from 1.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Location {
    pub(crate) fn json(&self) -> String {
        Object::new()
            .string("path", &self.path)
            .uint("start", self.start)
            .uint("end", self.end)
            .uint("line", self.line)
            .uint("column", self.column)
            .finish()
    }
}

/// The line starts of every source, to turn spans into locations.
pub(crate) struct SpanTable<'a> {
    sources: Vec<(String, &'a str, Vec<usize>)>,
}

impl<'a> SpanTable<'a> {
    pub(crate) fn new(inputs: &[SourceInput<'a>]) -> Self {
        Self {
            sources: inputs
                .iter()
                .map(|input| {
                    let starts = std::iter::once(0)
                        .chain(
                            input
                                .text
                                .bytes()
                                .enumerate()
                                .filter(|(_, byte)| *byte == b'\n')
                                .map(|(at, _)| at + 1),
                        )
                        .collect();
                    (input.path.replace('\\', "/"), input.text, starts)
                })
                .collect(),
        }
    }

    /// The location of `span`, or `None` when it names no source or lies outside its text.
    pub(crate) fn locate(&self, span: Span) -> Option<Location> {
        let (path, text, starts) = self.sources.get(span.source?)?;
        if span.start > span.end || span.end > text.len() {
            return None;
        }
        let mut offset = span.start;
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = starts.partition_point(|&start| start <= offset);
        let column = text[starts[line - 1]..offset].chars().count() + 1;
        Some(Location {
            path: path.clone(),
            start: span.start,
            end: span.end,
            line,
            column,
        })
    }

    /// The text of `span`.
    pub(crate) fn text(&self, span: Span) -> Option<&'a str> {
        let (_, text, _) = self.sources.get(span.source?)?;
        text.get(span.start..span.end)
    }

    /// The path and the whole text of the source that `span` points into.
    pub(crate) fn source(&self, span: Span) -> Option<(&str, &'a str)> {
        let (path, text, _) = self.sources.get(span.source?)?;
        Some((path.as_str(), *text))
    }
}

/// A module of the graph: its ID, its key (the name of its facts and layout files), where it comes
/// from (`project`, `dependency:<package>` or `std`) and the index of its source in the inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleInfo {
    pub id: String,
    pub key: String,
    pub origin: String,
    pub source: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamRow {
    pub name: String,
    pub ty: String,
    pub conv: &'static str,
}

/// A node: one declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRow {
    pub id: String,
    pub kind: &'static str,
    pub module: String,
    pub name: String,
    pub signature: String,
    pub params: Vec<ParamRow>,
    pub result: Option<String>,
    pub public: bool,
    pub export: bool,
    pub pure: bool,
    pub doc: Option<String>,
    pub deprecated: Option<String>,
    pub origin: String,
    pub span: Location,
}

impl NodeRow {
    pub fn json(&self) -> String {
        let params = json::array(self.params.iter().map(|param| {
            Object::new()
                .string("name", &param.name)
                .string("type", &param.ty)
                .string("conv", param.conv)
                .finish()
        }));
        Object::new()
            .string("id", &self.id)
            .string("kind", self.kind)
            .string("module", &self.module)
            .string("name", &self.name)
            .string("signature", &self.signature)
            .raw("params", &params)
            .optional("result", self.result.as_deref())
            .boolean("public", self.public)
            .boolean("export", self.export)
            .boolean("pure", self.pure)
            .optional("doc", self.doc.as_deref())
            .optional("deprecated", self.deprecated.as_deref())
            .string("origin", &self.origin)
            .raw("span", &self.span.json())
            .finish()
    }
}

/// An edge: a typed relation from a node to a node or an external endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeRow {
    pub rel: &'static str,
    pub from: String,
    pub to: String,
    /// The convention of each argument of a `calls` edge (empty for a function used as a value).
    pub args: Option<Vec<&'static str>>,
    pub span: Location,
}

impl EdgeRow {
    pub fn json(&self) -> String {
        let object = Object::new()
            .string("rel", self.rel)
            .string("from", &self.from)
            .string("to", &self.to);
        let object = match &self.args {
            Some(args) => object.strings("args", args),
            None => object,
        };
        object.raw("span", &self.span.json()).finish()
    }

    fn key(&self) -> (&str, &str, &str, &str, usize, usize) {
        (
            self.rel,
            &self.from,
            &self.to,
            &self.span.path,
            self.span.start,
            self.span.end,
        )
    }
}

/// A test or bench with the index that `tsuzuri test --index` and `bench --index` take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRow {
    pub id: String,
    pub kind: &'static str,
    pub module: String,
    pub name: String,
    pub index: usize,
    pub span: Location,
}

impl TestRow {
    pub fn json(&self) -> String {
        Object::new()
            .string("id", &self.id)
            .string("kind", self.kind)
            .string("module", &self.module)
            .string("name", &self.name)
            .uint("index", self.index)
            .raw("span", &self.span.json())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EffectSite {
    pub effect: String,
    pub span: Location,
    pub via: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CopySiteRow {
    pub span: Location,
    pub ty: String,
    pub kind: &'static str,
    pub cost: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AllocSite {
    pub span: Location,
    pub kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrapSite {
    pub span: Location,
    pub kind: &'static str,
    pub proven: bool,
}

/// An ownership event (G26 phase 5): `move`, `borrow`, `borrow_mut`, `reborrow`, `drop` or `copy`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnershipRow {
    pub span: Location,
    pub event: &'static str,
    pub var: String,
    pub to: Option<String>,
}

impl OwnershipRow {
    fn rank(&self) -> usize {
        ["move", "borrow", "borrow_mut", "reborrow", "drop", "copy"]
            .iter()
            .position(|event| *event == self.event)
            .unwrap_or(6)
    }
}

impl Ord for OwnershipRow {
    /// Source order, and at one position the order of the event list.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            &self.span.path,
            self.span.start,
            self.span.end,
            self.rank(),
            &self.var,
            &self.to,
        )
            .cmp(&(
                &other.span.path,
                other.span.start,
                other.span.end,
                other.rank(),
                &other.var,
                &other.to,
            ))
    }
}

impl PartialOrd for OwnershipRow {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LoopRow {
    pub span: Location,
    pub kind: &'static str,
}

/// An entry of `check --report parallel` (F15) in the function that holds the call: a suggestion
/// (`span` is the name of the call) or a `Parallel` call (`span` is the whole call).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParallelRow {
    pub span: Location,
    pub kind: &'static str,
    pub call: &'static str,
    pub parallel_safe: bool,
    pub reason: Option<String>,
}

impl Ord for ParallelRow {
    /// The order of the report: source order, a suggestion before a site at one position.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            &self.span.path,
            self.span.start,
            self.span.end,
            self.kind != "suggestion",
            self.call,
            self.parallel_safe,
            &self.reason,
        )
            .cmp(&(
                &other.span.path,
                other.span.start,
                other.span.end,
                other.kind != "suggestion",
                other.call,
                other.parallel_safe,
                &other.reason,
            ))
    }
}

impl PartialOrd for ParallelRow {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The facts of one function-like node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactsRow {
    pub id: String,
    pub effects: Vec<String>,
    pub effect_sites: Vec<EffectSite>,
    pub effects_complete: bool,
    pub copy_sites: Vec<CopySiteRow>,
    pub alloc_sites: Vec<AllocSite>,
    pub alloc_complete: bool,
    pub trap_sites: Vec<TrapSite>,
    pub ownership: Vec<OwnershipRow>,
    pub recursion: &'static str,
    pub loops: Vec<LoopRow>,
    pub specializations: usize,
    /// F15: the entries of `check --report parallel` in the node's functions.
    pub parallel: Vec<ParallelRow>,
    pub truncated: BTreeMap<String, usize>,
}

impl FactsRow {
    pub fn json(&self) -> String {
        let effect_sites = json::array(self.effect_sites.iter().map(|site| {
            Object::new()
                .string("effect", &site.effect)
                .raw("span", &site.span.json())
                .strings("via", &site.via)
                .finish()
        }));
        let copy_sites = json::array(self.copy_sites.iter().map(|site| {
            Object::new()
                .raw("span", &site.span.json())
                .string("type", &site.ty)
                .string("kind", site.kind)
                .string("cost", site.cost)
                .finish()
        }));
        let alloc_sites = json::array(self.alloc_sites.iter().map(|site| {
            Object::new()
                .raw("span", &site.span.json())
                .string("kind", site.kind)
                .finish()
        }));
        let trap_sites = json::array(self.trap_sites.iter().map(|site| {
            Object::new()
                .raw("span", &site.span.json())
                .string("kind", site.kind)
                .boolean("proven", site.proven)
                .finish()
        }));
        let ownership = json::array(self.ownership.iter().map(|event| {
            let object = Object::new()
                .string("var", &event.var)
                .string("event", event.event);
            let object = match &event.to {
                Some(to) => object.string("to", to),
                None => object,
            };
            object.raw("span", &event.span.json()).finish()
        }));
        let loops = json::array(self.loops.iter().map(|row| {
            Object::new()
                .string("kind", row.kind)
                .raw("span", &row.span.json())
                .finish()
        }));
        let parallel = json::array(self.parallel.iter().map(|row| {
            let object = Object::new()
                .string("call", row.call)
                .string("kind", row.kind)
                .boolean("parallel_safe", row.parallel_safe);
            let object = match &row.reason {
                Some(reason) => object.string("reason", reason),
                None => object,
            };
            object.raw("span", &row.span.json()).finish()
        }));
        let truncated = self
            .truncated
            .iter()
            .fold(Object::new(), |object, (key, count)| {
                object.uint(key, *count)
            })
            .finish();
        Object::new()
            .string("id", &self.id)
            .strings("effects", &self.effects)
            .raw("effect_sites", &effect_sites)
            .boolean("effects_complete", self.effects_complete)
            .raw("copy_sites", &copy_sites)
            .raw("alloc_sites", &alloc_sites)
            .boolean("alloc_complete", self.alloc_complete)
            .raw("trap_sites", &trap_sites)
            .raw("ownership", &ownership)
            .string("recursion", self.recursion)
            .raw("loops", &loops)
            .uint("specializations", self.specializations)
            .raw("parallel", &parallel)
            .raw("truncated", &truncated)
            .finish()
    }
}

/// The layout of one record or union type (G26 phase 6): native, 64-bit bounds, not measurements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutRow {
    pub id: String,
    pub kind: &'static str,
    pub ty: String,
    pub size: (usize, usize),
    pub align: (usize, usize),
    pub shape: LayoutShape,
    pub drop_flag: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutShape {
    Record {
        /// Name, type and slot of each field in declaration order.
        fields: Vec<(String, String, usize)>,
        reordered: bool,
    },
    Union {
        layout: &'static str,
        tag_bytes: Option<usize>,
        niche: Option<&'static str>,
        cases: Vec<(String, Option<String>)>,
    },
}

impl LayoutRow {
    pub fn json(&self) -> String {
        let pair = |(low, high): (usize, usize)| format!("[{low},{high}]");
        let object = Object::new()
            .string("id", &self.id)
            .string("kind", self.kind)
            .string("type", &self.ty)
            .raw("size", &pair(self.size))
            .raw("align", &pair(self.align));
        let object = match &self.shape {
            LayoutShape::Record { fields, reordered } => object
                .raw(
                    "fields",
                    &json::array(fields.iter().map(|(name, ty, slot)| {
                        Object::new()
                            .string("name", name)
                            .string("type", ty)
                            .uint("slot", *slot)
                            .finish()
                    })),
                )
                .boolean("reordered", *reordered),
            LayoutShape::Union {
                layout,
                tag_bytes,
                niche,
                cases,
            } => {
                let object = object.string("layout", layout);
                let object = match tag_bytes {
                    Some(bytes) => object.uint("tag_bytes", *bytes),
                    None => object.null("tag_bytes"),
                };
                object.optional("niche", *niche).raw(
                    "cases",
                    &json::array(cases.iter().map(|(name, payload)| {
                        Object::new()
                            .string("name", name)
                            .optional("payload", payload.as_deref())
                            .finish()
                    })),
                )
            }
        };
        object.boolean("drop_flag", self.drop_flag).finish()
    }
}

/// The knowledge graph of one analysis: its nodes and edges, and the property tables of tests,
/// facts and layouts.
#[derive(Clone, Debug, Default)]
pub struct KgModel {
    modules: Vec<ModuleInfo>,
    nodes: Vec<NodeRow>,
    edges: Vec<EdgeRow>,
    tests: Vec<TestRow>,
    facts: BTreeMap<String, Vec<FactsRow>>,
    layouts: BTreeMap<String, Vec<LayoutRow>>,
}

fn write_lines(
    out: &mut impl Write,
    lines: impl IntoIterator<Item = String>,
) -> std::io::Result<()> {
    for line in lines {
        out.write_all(line.as_bytes())?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

impl KgModel {
    /// The graph of the analysis whose final module is `module` and whose declaration view the
    /// checker captured in `capture`, with locations from `inputs`.
    pub(crate) fn build(
        inputs: &[SourceInput<'_>],
        module: &CheckedModule,
        capture: Capture,
    ) -> Self {
        let table = SpanTable::new(inputs);
        let mut modules: Vec<ModuleInfo> = capture
            .modules
            .iter()
            .map(|module| ModuleInfo {
                id: module.id.clone(),
                key: module.key.clone(),
                origin: module.origin.clone(),
                source: module.source,
            })
            .collect();
        modules.sort_by(|a, b| a.id.cmp(&b.id));
        let mut nodes: Vec<NodeRow> = capture
            .nodes
            .iter()
            .filter_map(|node| {
                Some(NodeRow {
                    id: node.id.clone(),
                    kind: node.kind,
                    module: node.module.clone(),
                    name: node.name.clone(),
                    signature: node.signature.clone(),
                    params: node
                        .params
                        .iter()
                        .map(|param| ParamRow {
                            name: param.name.clone(),
                            ty: param.ty.clone(),
                            conv: param.conv,
                        })
                        .collect(),
                    result: node.result.clone(),
                    public: node.public,
                    export: node.export,
                    pure: node.pure,
                    doc: node.doc.clone(),
                    deprecated: node.deprecated.clone(),
                    origin: node.origin.clone(),
                    span: table.locate(node.span)?,
                })
            })
            .collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        nodes.dedup();
        let mut edges: Vec<EdgeRow> = capture
            .edges
            .iter()
            .filter_map(|edge| {
                Some(EdgeRow {
                    rel: edge.rel,
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    args: edge.args.clone(),
                    span: table.locate(edge.span)?,
                })
            })
            .collect();
        edges.sort_by(|a, b| a.key().cmp(&b.key()).then_with(|| a.json().cmp(&b.json())));
        edges.dedup();
        let mut tests: Vec<TestRow> = capture
            .tests
            .iter()
            .filter_map(|test| {
                Some(TestRow {
                    id: test.id.clone(),
                    kind: test.kind,
                    module: test.module.clone(),
                    name: test.name.clone(),
                    index: test.index,
                    span: table.locate(test.span)?,
                })
            })
            .collect();
        tests.sort_by(|a, b| a.id.cmp(&b.id));
        let keys: BTreeMap<&str, &str> = modules
            .iter()
            .map(|module| (module.id.as_str(), module.key.as_str()))
            .collect();
        let mut facts: BTreeMap<String, Vec<FactsRow>> = BTreeMap::new();
        let module_of: BTreeMap<&str, &str> = capture
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node.module.as_str()))
            .collect();
        for (id, row) in super::facts::build(module, &capture, &table) {
            if let Some(key) = module_of
                .get(id.as_str())
                .and_then(|module| keys.get(module))
            {
                facts.entry((*key).to_owned()).or_default().push(row);
            }
        }
        let layouts = super::layout::build(module, &capture);
        Self {
            modules,
            nodes,
            edges,
            tests,
            facts,
            layouts,
        }
    }

    /// The modules of the graph by ID.
    pub fn modules(&self) -> &[ModuleInfo] {
        &self.modules
    }

    pub fn nodes(&self) -> &[NodeRow] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EdgeRow] {
        &self.edges
    }

    pub fn tests(&self) -> &[TestRow] {
        &self.tests
    }

    /// The facts rows of the module with this key, by ID.
    pub fn facts(&self, module: &str) -> &[FactsRow] {
        self.facts.get(module).map_or(&[], Vec::as_slice)
    }

    pub fn layouts(&self, module: &str) -> &[LayoutRow] {
        self.layouts.get(module).map_or(&[], Vec::as_slice)
    }

    pub fn write_nodes(&self, out: &mut impl Write) -> std::io::Result<()> {
        write_lines(out, self.nodes.iter().map(NodeRow::json))
    }

    pub fn write_edges(&self, out: &mut impl Write) -> std::io::Result<()> {
        write_lines(out, self.edges.iter().map(EdgeRow::json))
    }

    pub fn write_tests(&self, out: &mut impl Write) -> std::io::Result<()> {
        write_lines(out, self.tests.iter().map(TestRow::json))
    }

    /// The keys of the modules that have facts rows, ascending.
    pub fn fact_modules(&self) -> Vec<&str> {
        self.facts
            .iter()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(key, _)| key.as_str())
            .collect()
    }

    pub fn write_facts(&self, module: &str, out: &mut impl Write) -> std::io::Result<()> {
        write_lines(out, self.facts(module).iter().map(FactsRow::json))
    }

    /// The keys of the modules that have layout rows, ascending.
    pub fn layout_modules(&self) -> Vec<&str> {
        self.layouts
            .iter()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(key, _)| key.as_str())
            .collect()
    }

    pub fn write_layout(&self, module: &str, out: &mut impl Write) -> std::io::Result<()> {
        write_lines(out, self.layouts(module).iter().map(LayoutRow::json))
    }

    /// The keys of the facts rows that this build writes, in their order.
    pub fn fact_keys(&self) -> &'static [&'static str] {
        &FACT_KEYS
    }

    /// The IDs of every node, for the structural checks of the graph.
    pub fn node_ids(&self) -> BTreeSet<&str> {
        self.nodes.iter().map(|node| node.id.as_str()).collect()
    }
}
