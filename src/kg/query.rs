//! The queries of `tsuzuri kg` (G28 Phase 1, D-60): ID resolution, `brief`, `show`, `callers`,
//! `callees`, `uses`, `tests-for`, `search` and `path`. They read a `KgView` and never touch the
//! disk, so MCP (G29) calls them as they are. Text is built with the cards' `Card` (one item per
//! line, items dropped from the end under `--budget`); JSON is one object per line.

use std::collections::{BTreeMap, BTreeSet};

use super::algo;
use super::brief::{self, Card, ELLIPSIS, clip, first_line, norm, plural};
use super::json::{self, Object};
use super::store::{Edge, KgData, KgView, Node, Span};
use crate::diagnostic::{Diagnostic, DiagnosticKind, Span as SourceSpan};

/// The default text budgets.
pub const SHOW_BUDGET: usize = 3000;
pub const LIST_BUDGET: usize = 2000;
/// The bounds of `--budget` and of the depth of `callers` and `callees`.
pub const BUDGET_RANGE: (usize, usize) = (100, 100_000);
pub const DEPTH_RANGE: (usize, usize) = (1, 8);
/// The bounds and default of the depth of `path` (D-60).
pub const PATH_DEPTH_RANGE: (usize, usize) = (1, 16);
pub const PATH_DEPTH: usize = 8;
/// The relations that `path` may follow.
pub const RELATIONS: [&str; 7] = [
    "calls",
    "contains",
    "uses_type",
    "implements",
    "instance_of",
    "imports",
    "tested_by",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    One(String),
    Many(Vec<String>),
    Unknown(Vec<String>),
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub budget: Option<usize>,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BriefTarget {
    Project,
    Module(String),
    Id(String),
}

/// The text of a query and its JSON Lines rows (without newlines).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub text: String,
    pub rows: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryError {
    Ambiguous {
        text: String,
        candidates: Vec<String>,
        more: usize,
    },
    Unknown {
        text: String,
        suggestions: Vec<String>,
    },
    Empty(&'static str),
}

impl QueryError {
    /// `E2000`: an ambiguous or unknown ID ends the command with 1, an empty argument with 2.
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Ambiguous {
                text,
                candidates,
                more,
            } => {
                let mut message = format!(
                    "ambiguous id '{text}'; candidates: {}",
                    candidates.join(", ")
                );
                if *more > 0 {
                    message.push_str(&format!(", {ELLIPSIS}{more} more"));
                }
                Diagnostic::of(
                    DiagnosticKind::KgAmbiguousId,
                    message,
                    SourceSpan::default(),
                )
            }
            Self::Unknown { text, suggestions } => {
                let message = if suggestions.is_empty() {
                    format!("unknown id '{text}'; use tsuzuri kg search TEXT to find ids")
                } else {
                    format!(
                        "unknown id '{text}'; did you mean: {}",
                        suggestions.join(", ")
                    )
                };
                Diagnostic::of(DiagnosticKind::KgUnknownId, message, SourceSpan::default())
            }
            Self::Empty(message) => {
                Diagnostic::of(DiagnosticKind::UsageError, *message, SourceSpan::default())
            }
        }
    }
}

fn site(span: &Span) -> String {
    format!("{}:{}:{}", span.path, span.line, span.column)
}

fn span_json(span: &Span) -> String {
    Object::new()
        .string("path", &span.path)
        .uint("start", span.start)
        .uint("end", span.end)
        .uint("line", span.line)
        .uint("column", span.column)
        .finish()
}

/// The candidates of resolution: the project's node IDs and edge ends, or its modules (by ID and
/// key) for `modules`.
fn universe(data: &KgData, modules: bool) -> BTreeMap<String, String> {
    if modules {
        let mut ids: BTreeMap<String, String> = data
            .nodes
            .iter()
            .filter(|node| node.kind == "module")
            .map(|node| (node.id.clone(), node.id.clone()))
            .collect();
        for module in &data.meta.modules {
            ids.entry(module.key.clone())
                .or_insert_with(|| module.name.clone());
        }
        ids
    } else {
        data.all_ids()
            .into_iter()
            .map(|id| (id.to_owned(), id.to_owned()))
            .collect()
    }
}

/// Resolves `text` to one ID (G28 §2): an exact ID, then a unique suffix after `::` or `.`, then
/// the same ignoring case, then a unique prefix. Several matches at the first stage that has any
/// are ambiguous; none at all is unknown, with suggestions.
pub fn resolve(view: &KgView, text: &str, modules: bool) -> Resolution {
    let mut candidates = view
        .project()
        .map(|data| universe(data, modules))
        .unwrap_or_default();
    let found = resolve_in(&candidates, text);
    if !matches!(found, Resolution::Unknown(_)) || modules {
        return found;
    }
    // An ID of the standard library, when the view has its graph.
    if let Some(std) = view.std() {
        let before = candidates.len();
        for node in &std.nodes {
            candidates.insert(node.id.clone(), node.id.clone());
        }
        if candidates.len() > before {
            return resolve_in(&candidates, text);
        }
    }
    found
}

fn resolve_in(candidates: &BTreeMap<String, String>, text: &str) -> Resolution {
    if let Some(id) = candidates.get(text) {
        return Resolution::One(id.clone());
    }
    let low = text.to_lowercase();
    let suffix = |id: &str, part: &str| {
        id.ends_with(&format!("::{part}")) || id.ends_with(&format!(".{part}"))
    };
    type Stage<'a> = Box<dyn Fn(&str) -> bool + 'a>;
    let stages: [Stage<'_>; 5] = [
        Box::new(|id| suffix(id, text)),
        Box::new(|id| id.to_lowercase() == low),
        Box::new(|id| suffix(&id.to_lowercase(), &low)),
        Box::new(|id| id.starts_with(text)),
        Box::new(|id| id.to_lowercase().starts_with(&low)),
    ];
    for stage in stages {
        let found: BTreeSet<&String> = candidates
            .iter()
            .filter(|(key, _)| stage(key))
            .map(|(_, id)| id)
            .collect();
        match found.len() {
            0 => continue,
            1 => return Resolution::One(found.into_iter().next().unwrap().clone()),
            _ => return Resolution::Many(found.into_iter().cloned().collect()),
        }
    }
    let ids: Vec<&str> = candidates.keys().map(String::as_str).collect();
    Resolution::Unknown(super::suggest::suggestions(&ids, text))
}

/// The ID of `text`, or the error of the resolution.
pub fn resolve_one(view: &KgView, text: &str, modules: bool) -> Result<String, QueryError> {
    if text.is_empty() {
        return Err(QueryError::Empty("id must not be empty"));
    }
    match resolve(view, text, modules) {
        Resolution::One(id) => Ok(id),
        Resolution::Many(ids) => Err(QueryError::Ambiguous {
            text: text.to_owned(),
            more: ids.len().saturating_sub(10),
            candidates: ids.into_iter().take(10).collect(),
        }),
        Resolution::Unknown(suggestions) => Err(QueryError::Unknown {
            text: text.to_owned(),
            suggestions,
        }),
    }
}

/// A list: header lines and the summary always stay; items are dropped from the end.
pub(crate) fn listing(
    header: &[String],
    items: Vec<String>,
    footer: &[String],
    limit: usize,
) -> String {
    let mut card = Card::new(limit, "\n");
    card.block(
        None,
        header.iter().map(|line| (line.clone(), 0)).collect(),
        false,
    );
    card.block(
        None,
        items.into_iter().map(|line| (line, 1)).collect(),
        false,
    );
    card.block(
        None,
        footer.iter().map(|line| (line.clone(), 0)).collect(),
        false,
    );
    card.render()
}

fn no_graph(header: String, summary: String) -> Output {
    Output {
        text: format!("{header}\nnote: this KG has no edges.jsonl\n{summary}\n"),
        rows: vec![
            Object::new()
                .string("type", "note")
                .string("message", "this KG has no edges.jsonl")
                .finish(),
        ],
    }
}

/// The data that holds `id`: the project's graph, then std's.
fn owner<'a>(view: &'a KgView, id: &str) -> Option<&'a KgData> {
    match view.project() {
        Some(data) if data.node(id).is_some() => Some(data),
        _ => view.std().filter(|data| data.node(id).is_some()),
    }
}

/// `kg brief`: L0, the L1 of a module, or the L2 of a node (L1 for a module node).
pub fn brief(view: &KgView, target: &BriefTarget, options: &Options) -> Result<Output, QueryError> {
    let project = view.project();
    let (level, id, text) = match target {
        BriefTarget::Project => {
            let data = project.expect("a project view");
            (
                "L0",
                None,
                brief::l0(data, options.budget.unwrap_or(brief::L0_LIMIT)),
            )
        }
        BriefTarget::Module(text) => {
            let id = resolve_one(view, text, true)?;
            let data = owner(view, &id).expect("a resolved module has a node");
            let card = brief::l1(data, &id, options.budget.unwrap_or(brief::L1_LIMIT))
                .expect("a module node");
            ("L1", Some(id), card)
        }
        BriefTarget::Id(text) => {
            let id = resolve_one(view, text, false)?;
            let Some(data) = owner(view, &id) else {
                return Err(QueryError::Unknown {
                    text: text.clone(),
                    suggestions: Vec::new(),
                });
            };
            if data.node(&id).is_some_and(|node| node.kind == "module") {
                let card = brief::l1(data, &id, options.budget.unwrap_or(brief::L1_LIMIT))
                    .expect("a module node");
                ("L1", Some(id), card)
            } else {
                let card = brief::l2(data, &id, options.budget.unwrap_or(brief::L2_LIMIT))
                    .expect("a node");
                ("L2", Some(id), card)
            }
        }
    };
    let row = Object::new()
        .string("type", "brief")
        .string("level", level)
        .optional("id", id.as_deref())
        .string("text", &text)
        .finish();
    Ok(Output {
        text,
        rows: vec![row],
    })
}

fn args_text(edge: &Edge) -> String {
    match &edge.args {
        Some(args) if !args.is_empty() => format!("args {}", args.join(",")),
        _ => String::new(),
    }
}

/// `kg show`.
pub fn show(view: &KgView, text: &str, options: &Options) -> Result<Output, QueryError> {
    let id = resolve_one(view, text, false)?;
    let data = owner(view, &id).or(view.project());
    let node = data.and_then(|data| data.node(&id));
    let mut lines: Vec<(String, usize)> = Vec::new();
    let fact = data.and_then(|data| data.fact(&id));
    match node {
        None => {
            lines.push((id.clone(), 0));
            lines.push(("symbol: not in this KG".into(), 0));
        }
        Some(node) => {
            let mut flags = vec![if node.public { "public" } else { "private" }];
            if node.export {
                flags.push("export");
            }
            if node.pure {
                flags.push("pure");
            }
            lines.push((node.id.clone(), 0));
            lines.push((
                format!("{} | {} | {}", node.kind, flags.join(" "), site(&node.span)),
                0,
            ));
            lines.push((
                format!("signature: {}", clip(&norm(&node.signature), 160)),
                0,
            ));
            let doc = first_line(node.doc.as_deref());
            if !doc.is_empty() {
                lines.push((format!("doc: {doc}"), 1));
            }
            if matches!(node.kind.as_str(), "def" | "extern" | "method" | "const")
                && !node.params.is_empty()
            {
                lines.push(("params:".into(), 1));
                for param in &node.params {
                    let name = if param.name.is_empty() {
                        "_"
                    } else {
                        &param.name
                    };
                    lines.push((format!("  {name}  {}  {}", param.conv, param.ty), 1));
                }
                lines.push((
                    format!("result: {}", node.result.as_deref().unwrap_or("null")),
                    1,
                ));
            }
            for (kind, child, label) in [
                ("record", "field", "fields"),
                ("union", "case", "cases"),
                ("class", "method", "methods"),
            ] {
                if node.kind != kind {
                    continue;
                }
                let children = data
                    .map(|data| data.children(&id, child))
                    .unwrap_or_default();
                if !children.is_empty() {
                    lines.push((format!("{label}:"), 1));
                    for child in children {
                        lines.push((format!("  {}", norm(&child.signature)), 1));
                    }
                }
            }
            if let Some(fact) = fact {
                let effects = if fact.effects.is_empty() {
                    "none".to_owned()
                } else {
                    fact.effects.join(", ")
                };
                lines.push((format!("effects: {effects}"), 1));
                let unproven =
                    |sites: &[super::store::Site]| sites.iter().filter(|site| !site.proven).count();
                let proven = fact.trap_sites.iter().filter(|site| site.proven).count();
                let mut sites = format!(
                    "sites: trap {}, alloc {}, copy {}",
                    unproven(&fact.trap_sites),
                    unproven(&fact.alloc_sites),
                    unproven(&fact.copy_sites)
                );
                if proven > 0 {
                    sites.push_str(&format!(
                        " ({proven} proven trap {} not counted)",
                        if proven == 1 { "site" } else { "sites" }
                    ));
                }
                lines.push((sites, 1));
                if fact.recursion != "none" {
                    lines.push((format!("recursion: {}", fact.recursion), 1));
                }
            }
        }
    }
    let project = view.project();
    let graph = data.filter(|data| data.edges.is_some());
    let callers_data = project.filter(|data| data.edges.is_some()).or(graph);
    let mut callees: Vec<&Edge> = Vec::new();
    let mut callers: Vec<&Edge> = Vec::new();
    let mut tests: Vec<&Edge> = Vec::new();
    let mut used: Vec<&Edge> = Vec::new();
    if let Some(graph) = graph
        && node.is_some()
    {
        callees = graph.edges_from("calls", &id);
        tests = graph.edges_from("tested_by", &id);
        used = ["uses_type", "implements", "instance_of"]
            .iter()
            .flat_map(|rel| graph.edges_to(rel, &id))
            .collect();
        used.sort_by(|a, b| (&a.from, &a.rel, a.span.start).cmp(&(&b.from, &b.rel, b.span.start)));
    }
    if let Some(data) = callers_data {
        callers = data.edges_to("calls", &id);
    }
    let mut group = |label: &str, edges: &[&Edge], format: &dyn Fn(&Edge) -> String| {
        if edges.is_empty() {
            return;
        }
        lines.push((format!("{label} ({}):", edges.len()), 2));
        for edge in edges {
            lines.push((format(edge), 3));
        }
    };
    let with_args = |end: &str, edge: &Edge| {
        let args = args_text(edge);
        let mut line = format!("  {end}  {}", site(&edge.span));
        if !args.is_empty() {
            line.push_str(&format!("  {args}"));
        }
        line
    };
    group("callees", &callees, &|edge| with_args(&edge.to, edge));
    group("callers", &callers, &|edge| with_args(&edge.from, edge));
    group("tests", &tests, &|edge| {
        format!("  {}  {}", edge.to, site(&edge.span))
    });
    group("used by", &used, &|edge| {
        format!("  {}  {}  {}", edge.from, edge.rel, site(&edge.span))
    });
    let mut card = Card::new(options.budget.unwrap_or(SHOW_BUDGET), "\n");
    card.block(None, lines, false);
    let edges = |list: &[&Edge]| json::array(list.iter().map(|edge| edge.raw.to_string()));
    let row = Object::new()
        .string("type", "symbol")
        .raw(
            "symbol",
            &node.map_or_else(|| "null".to_owned(), |node| node.raw.to_string()),
        )
        .raw(
            "facts",
            &fact.map_or_else(|| "null".to_owned(), |fact| fact.raw.to_string()),
        )
        .raw("callees", &edges(&callees))
        .raw("callers", &edges(&callers))
        .raw("tests", &edges(&tests))
        .raw("used_by", &edges(&used))
        .finish();
    Ok(Output {
        text: card.render(),
        rows: vec![row],
    })
}

/// A node that `callers` or `callees` reached: its depth, the first edge to it and their count, and
/// the node it was reached from.
struct Reached<'a> {
    depth: usize,
    id: String,
    edge: &'a Edge,
    sites: usize,
    via: Option<String>,
}

/// The breadth-first walk of `calls` from `start` (D-60: `algo::bfs` over the KG's index).
fn walk<'a>(data: &'a KgData, start: &str, depth: usize, forward: bool) -> Vec<Reached<'a>> {
    let graph = data.index.digraph(data.edges(), &["calls"]);
    let graph = if forward { graph } else { graph.reversed() };
    let Some(origin) = data.index.number(start) else {
        return Vec::new();
    };
    let mut reached: Vec<Reached<'a>> = algo::bfs(&graph, &[origin], depth, usize::MAX)
        .into_iter()
        .filter(|visit| visit.depth > 0)
        .filter_map(|visit| {
            let parent = data.index.name(visit.parent?).to_owned();
            let id = data.index.name(visit.node).to_owned();
            let edges: Vec<&Edge> = if forward {
                data.edges_from("calls", &parent)
                    .into_iter()
                    .filter(|edge| edge.to == id)
                    .collect()
            } else {
                data.edges_to("calls", &parent)
                    .into_iter()
                    .filter(|edge| edge.from == id)
                    .collect()
            };
            let mut edges = edges;
            edges.sort_by(|a, b| (&a.span.path, a.span.start).cmp(&(&b.span.path, b.span.start)));
            Some(Reached {
                depth: visit.depth,
                via: (visit.depth > 1).then_some(parent),
                id,
                sites: edges.len(),
                edge: edges.first().copied()?,
            })
        })
        .collect();
    reached.sort_by(|a, b| (a.depth, &a.id).cmp(&(b.depth, &b.id)));
    reached
}

/// The graph to walk from `id`. Forward (callees, a path's start): the graph that declares `id`,
/// since the project's graph knows a std function only as the end of its calls. Backward
/// (callers): the project's first, whose calls into std end at std IDs.
fn graph_data<'a>(view: &'a KgView, id: &str, forward: bool) -> Option<&'a KgData> {
    if forward
        && let Some(owner) = view
            .project()
            .filter(|data| data.edges.is_some() && data.node(id).is_some())
            .or_else(|| view.std().filter(|data| data.node(id).is_some()))
    {
        return Some(owner);
    }
    view.project()
        .filter(|data| data.edges.is_some())
        .filter(|data| data.index.number(id).is_some())
        .or_else(|| view.std().filter(|data| data.index.number(id).is_some()))
        .or(view.project().filter(|data| data.edges.is_some()))
}

/// `kg callers` (`forward` false) and `kg callees`.
pub fn calls(
    view: &KgView,
    text: &str,
    options: &Options,
    forward: bool,
) -> Result<Output, QueryError> {
    let id = resolve_one(view, text, false)?;
    let word = if forward { "callees" } else { "callers" };
    let depth = options.depth.max(1);
    let header = format!("{word} of {id} (depth {depth})");
    if view.project().is_some_and(|data| data.edges.is_none()) {
        return Ok(no_graph(
            header,
            format!("summary: 0 {word}, depth {depth}"),
        ));
    }
    let reached = graph_data(view, &id, forward)
        .map(|data| walk(data, &id, depth, forward))
        .unwrap_or_default();
    let mut items = Vec::new();
    let mut rows = Vec::new();
    for item in &reached {
        let mut line = format!("{} {}  {}", item.depth, item.id, site(&item.edge.span));
        let args = args_text(item.edge);
        if item.depth == 1 && !args.is_empty() {
            line.push_str(&format!("  {args}"));
        }
        if item.sites > 1 {
            line.push_str(&format!("  +{} sites", item.sites - 1));
        }
        if let Some(via) = &item.via {
            line.push_str(&format!("  via {via}"));
        }
        items.push(line);
        let object = Object::new()
            .string("type", "edge")
            .uint("depth", item.depth)
            .string("id", &item.id)
            .optional("via", item.via.as_deref())
            .uint("sites", item.sites)
            .raw("span", &span_json(&item.edge.span));
        let object = match (&item.edge.args, item.depth) {
            (Some(args), 1) => object.strings("args", args),
            _ => object.null("args"),
        };
        rows.push(object.finish());
    }
    rows.push(
        Object::new()
            .string("type", "summary")
            .uint("count", reached.len())
            .uint("depth", depth)
            .finish(),
    );
    let singular = &word[..word.len() - 1];
    let text = listing(
        &[header],
        items,
        &[format!(
            "summary: {}, depth {depth}",
            plural(reached.len(), singular, word)
        )],
        options.budget.unwrap_or(LIST_BUDGET),
    );
    Ok(Output { text, rows })
}

/// `kg uses TYPE`: the `uses_type`, `implements` and `instance_of` edges into a type.
pub fn uses(view: &KgView, text: &str, options: &Options) -> Result<Output, QueryError> {
    let id = resolve_one(view, text, false)?;
    let header = format!("uses of {id}");
    let Some(data) = view.project() else {
        return Ok(no_graph(header, "summary: 0 uses".into()));
    };
    if data.edges.is_none() {
        return Ok(no_graph(header, "summary: 0 uses".into()));
    }
    let mut edges: Vec<&Edge> = ["uses_type", "implements", "instance_of"]
        .iter()
        .flat_map(|rel| data.edges_to(rel, &id))
        .collect();
    edges.sort_by(|a, b| (&a.rel, &a.from, a.span.start).cmp(&(&b.rel, &b.from, b.span.start)));
    let items = edges
        .iter()
        .map(|edge| format!("{} {}  {}", edge.rel, edge.from, site(&edge.span)))
        .collect();
    let mut rows: Vec<String> = edges
        .iter()
        .map(|edge| {
            Object::new()
                .string("type", "use")
                .string("rel", &edge.rel)
                .string("from", &edge.from)
                .raw("span", &span_json(&edge.span))
                .finish()
        })
        .collect();
    rows.push(
        Object::new()
            .string("type", "summary")
            .uint("count", edges.len())
            .finish(),
    );
    let text = listing(
        &[header],
        items,
        &[format!("summary: {}", plural(edges.len(), "use", "uses"))],
        options.budget.unwrap_or(LIST_BUDGET),
    );
    Ok(Output { text, rows })
}

/// `kg tests-for ID`: the tests that reach a node (`tested_by`).
pub fn tests_for(view: &KgView, text: &str, options: &Options) -> Result<Output, QueryError> {
    let id = resolve_one(view, text, false)?;
    let header = format!("tests for {id}");
    let Some(data) = view.project().filter(|data| data.edges.is_some()) else {
        return Ok(no_graph(header, "summary: 0 tests".into()));
    };
    let edges = data.edges_from("tested_by", &id);
    let items = edges
        .iter()
        .map(|edge| format!("{}  {}", edge.to, site(&edge.span)))
        .collect();
    let mut rows: Vec<String> = edges
        .iter()
        .map(|edge| {
            Object::new()
                .string("type", "test")
                .string("id", &edge.to)
                .raw("span", &span_json(&edge.span))
                .finish()
        })
        .collect();
    rows.push(
        Object::new()
            .string("type", "summary")
            .uint("count", edges.len())
            .finish(),
    );
    let text = listing(
        &[header],
        items,
        &[format!("summary: {}", plural(edges.len(), "test", "tests"))],
        options.budget.unwrap_or(LIST_BUDGET),
    );
    Ok(Output { text, rows })
}

/// `kg search TEXT`: nodes by name, ID, signature and the first line of their description.
pub fn search(
    view: &KgView,
    text: &str,
    std: bool,
    options: &Options,
) -> Result<Output, QueryError> {
    if text.is_empty() {
        return Err(QueryError::Empty("search text must not be empty"));
    }
    let low = text.to_lowercase();
    let mut nodes: Vec<&Node> = view
        .project()
        .map(|data| data.nodes.iter().collect())
        .unwrap_or_default();
    if std && let Some(data) = view.std() {
        nodes.extend(data.nodes.iter());
    }
    let mut found: Vec<(usize, &str, &Node)> = Vec::new();
    for node in nodes {
        let name = node.name.to_lowercase();
        let line = first_line(node.doc.as_deref()).to_lowercase();
        let rank = if name == low {
            1
        } else if name.starts_with(&low) {
            2
        } else if name.contains(&low) {
            3
        } else if node.id.to_lowercase().contains(&low) {
            4
        } else if norm(&node.signature).to_lowercase().contains(&low) {
            5
        } else if line.contains(&low) {
            6
        } else {
            continue;
        };
        found.push((rank, node.id.as_str(), node));
    }
    found.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    found.dedup_by(|a, b| a.1 == b.1);
    let items = found
        .iter()
        .enumerate()
        .map(|(index, (_, id, node))| {
            format!(
                "{} {id}  {}  {}",
                index + 1,
                node.kind,
                clip(&norm(&node.signature), 100)
            )
        })
        .collect();
    let mut rows: Vec<String> = found
        .iter()
        .map(|(rank, id, node)| {
            Object::new()
                .string("type", "match")
                .uint("rank", *rank)
                .string("id", id)
                .string("kind", &node.kind)
                .string("signature", &node.signature)
                .finish()
        })
        .collect();
    rows.push(
        Object::new()
            .string("type", "summary")
            .uint("count", found.len())
            .finish(),
    );
    let scope = if std { "project, std" } else { "project" };
    let text = listing(
        &[format!("search '{text}' ({scope})")],
        items,
        &[format!(
            "summary: {}",
            plural(found.len(), "match", "matches")
        )],
        options.budget.unwrap_or(LIST_BUDGET),
    );
    Ok(Output { text, rows })
}

/// `kg path FROM TO` (D-60): a shortest path along the given relations, one edge per step.
pub fn path(
    view: &KgView,
    from: &str,
    to: &str,
    relations: &[&str],
    depth: usize,
    options: &Options,
) -> Result<Output, QueryError> {
    let start = resolve_one(view, from, false)?;
    let end = resolve_one(view, to, false)?;
    let relations_text = relations.join(" or ");
    let header = format!("path from {start} to {end} (by {relations_text}, depth {depth})");
    let data = if view.project().is_some_and(|data| data.edges.is_none()) {
        None
    } else {
        graph_data(view, &start, true)
    };
    let Some(data) = data else {
        return Ok(no_graph(
            header,
            format!("summary: no path within depth {depth}"),
        ));
    };
    let graph = data.index.digraph(data.edges(), relations);
    let found = match (data.index.number(&start), data.index.number(&end)) {
        (Some(a), Some(b)) => algo::shortest_path(&graph, a, b, depth),
        _ => None,
    };
    let mut items = Vec::new();
    let mut rows = Vec::new();
    let steps = found
        .as_ref()
        .map_or(0, |path| path.len().saturating_sub(1));
    if let Some(path) = &found {
        for (index, pair) in path.windows(2).enumerate() {
            let (a, b) = (data.index.name(pair[0]), data.index.name(pair[1]));
            // The first edge from `a` to `b` of the relations, in relation order, then position.
            let edge = relations
                .iter()
                .flat_map(|rel| data.edges_from(rel, a))
                .filter(|edge| edge.to == b)
                .min_by(|x, y| {
                    (&x.rel, &x.span.path, x.span.start).cmp(&(&y.rel, &y.span.path, y.span.start))
                })
                .expect("a step of the path is an edge");
            items.push(format!("{a} --{}--> {b}  {}", edge.rel, site(&edge.span)));
            rows.push(
                Object::new()
                    .string("type", "step")
                    .uint("index", index + 1)
                    .string("from", a)
                    .string("rel", &edge.rel)
                    .string("to", b)
                    .raw("span", &span_json(&edge.span))
                    .finish(),
            );
        }
    }
    rows.push(
        Object::new()
            .string("type", "summary")
            .boolean("found", found.is_some())
            .uint("steps", steps)
            .uint("depth", depth)
            .finish(),
    );
    let summary = if found.is_some() {
        format!("summary: {}", plural(steps, "step", "steps"))
    } else {
        format!("summary: no path within depth {depth}")
    };
    let text = listing(
        &[header],
        items,
        &[summary],
        options.budget.unwrap_or(LIST_BUDGET),
    );
    Ok(Output { text, rows })
}
