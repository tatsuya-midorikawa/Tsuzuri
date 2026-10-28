//! Diagrams of a knowledge graph (G30, D-60): seven kinds made deterministically from the graph's
//! rows. `module`, `calls` and `concurrency` are flowcharts of one `Graph` model written as Mermaid
//! or DOT; `types` is a class diagram; `ownership` and `layout` are Markdown; and `transitions`,
//! the only one that reads sources, is a state diagram of a union's `match` arms. Node IDs are
//! numbers and labels are quoted and escaped, so every Mermaid text passes `validate_mermaid`.

mod calls;
mod concurrency;
mod dot;
mod escape;
mod graph;
mod input;
mod layout;
mod mermaid;
mod module;
mod ownership;
mod resolve;
mod transitions;
mod types;
mod validate;
mod views;

pub use input::{
    Event, FactRow, Input, LayoutCase, LayoutField, LayoutRow, ModuleRow, Param, Relation, Symbol,
};
pub use ownership::mermaid_blocks;
pub use transitions::{ProjectSources, Sources};
pub use validate::validate_mermaid;
pub use views::views;

use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};

/// The most edges (or relations) a diagram draws; the rest are counted in a comment.
pub const MAX_EDGES: usize = 400;
/// The most characters of a diagram's text.
pub const MAX_TEXT: usize = 45_000;
pub const DEFAULT_MAX_NODES: usize = 60;
pub const DEFAULT_DEPTH: usize = 2;
pub const MAX_NODES_RANGE: (usize, usize) = (5, 200);
pub const DEPTH_RANGE: (usize, usize) = (1, 6);

/// The classes of the flowcharts, in the order of their `classDef` lines.
pub(crate) const CLASS_ORDER: [&str; 10] = [
    "pure", "io", "alloc", "trap", "ext", "cycle", "dep", "region", "cell", "note",
];

pub(crate) fn class_def(name: &str) -> &'static str {
    match name {
        "pure" => "fill:#dafbe1,stroke:#1a7f37",
        "io" => "fill:#ddf4ff,stroke:#0969da",
        "alloc" => "fill:#fff8c5,stroke:#9a6700",
        "trap" | "cycle" => "fill:#ffebe9,stroke:#cf222e",
        "ext" => "fill:#f6f8fa,stroke:#8c959f,stroke-dasharray:4 2",
        "dep" => "fill:#f6f8fa,stroke:#8c959f",
        "region" => "fill:#fff1e5,stroke:#bc4c00",
        "cell" => "fill:#fbefff,stroke:#8250df",
        _ => "fill:#fff8c5,stroke:#d4a72c,stroke-dasharray:4 2",
    }
}

pub(crate) fn dot_fill(name: &str) -> &'static str {
    match name {
        "pure" => "#dafbe1",
        "io" => "#ddf4ff",
        "alloc" | "note" => "#fff8c5",
        "trap" | "cycle" => "#ffebe9",
        "ext" | "dep" => "#f6f8fa",
        "region" => "#fff1e5",
        _ => "#fbefff",
    }
}

/// The character limits of the labels: nodes and members, flowchart edges (and `transitions`),
/// class diagram relations. A text over `MAX_TEXT` is made again once with half of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub label: usize,
    pub edge: usize,
    pub relation: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            label: 60,
            edge: 40,
            relation: 30,
        }
    }
}

impl Limits {
    fn halved(self) -> Self {
        Self {
            label: self.label / 2,
            edge: self.edge / 2,
            relation: self.relation / 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Module,
    Types,
    Calls,
    Ownership,
    Concurrency,
    Layout,
    Transitions,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Module,
        Kind::Types,
        Kind::Calls,
        Kind::Ownership,
        Kind::Concurrency,
        Kind::Layout,
        Kind::Transitions,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Module => "module",
            Kind::Types => "types",
            Kind::Calls => "calls",
            Kind::Ownership => "ownership",
            Kind::Concurrency => "concurrency",
            Kind::Layout => "layout",
            Kind::Transitions => "transitions",
        }
    }

    pub fn parse(text: &str) -> Option<Kind> {
        Self::ALL.into_iter().find(|kind| kind.name() == text)
    }

    /// Whether the diagram needs an ID.
    pub fn needs_id(self) -> bool {
        matches!(
            self,
            Kind::Calls | Kind::Ownership | Kind::Layout | Kind::Transitions
        )
    }

    /// Whether the diagram is Markdown, not Mermaid or DOT.
    pub fn markdown(self) -> bool {
        matches!(self, Kind::Ownership | Kind::Layout)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Mermaid,
    Dot,
    Markdown,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Mermaid => "mermaid",
            Format::Dot => "dot",
            Format::Markdown => "markdown",
        }
    }
}

/// What to draw. `format` is `Mermaid` or `Dot`; `ownership` and `layout` are Markdown either way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramRequest {
    pub kind: Kind,
    pub id: Option<String>,
    pub depth: usize,
    pub max_nodes: usize,
    pub format: Format,
}

impl DiagramRequest {
    /// A request with the defaults: depth 2, 60 nodes, Mermaid.
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            id: None,
            depth: DEFAULT_DEPTH,
            max_nodes: DEFAULT_MAX_NODES,
            format: Format::Mermaid,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramOutput {
    pub format: Format,
    pub text: String,
    /// The elements drawn: nodes without the note, classes, variables, rows or cases.
    pub nodes: usize,
    /// Whether nodes were folded or edges or variables left out.
    pub folded: bool,
}

/// An error of a diagram: one of the `diagram_*` kinds of `E2000`, or `kg_stale` and `kg_broken`
/// of `E2009` (the caller decides the code).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramError {
    pub kind: &'static str,
    pub message: String,
}

impl DiagramError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// The diagnostic of the command line: `E2000`, or `E2009` for a graph that does not match.
    pub fn diagnostic(&self) -> Diagnostic {
        let kind = match self.kind {
            "kg_stale" => DiagnosticKind::KgStale,
            "kg_broken" => DiagnosticKind::KgBroken,
            "diagram_unknown_kind" => DiagnosticKind::DiagramUnknownKind,
            "diagram_needs_id" => DiagnosticKind::DiagramNeedsId,
            "diagram_format_unsupported" => DiagnosticKind::DiagramFormatUnsupported,
            "diagram_bad_option" => DiagnosticKind::DiagramBadOption,
            "diagram_unknown_id" => DiagnosticKind::DiagramUnknownId,
            "diagram_wrong_id_kind" => DiagnosticKind::DiagramWrongIdKind,
            "diagram_unavailable" => DiagnosticKind::DiagramUnavailable,
            "diagram_too_large" => DiagnosticKind::DiagramTooLarge,
            _ => DiagnosticKind::DiagramUsage,
        };
        Diagnostic::of(kind, self.message.clone(), Span::default())
    }
}

/// The options of `request` that no diagram takes.
fn check_request(request: &DiagramRequest) -> Result<(), DiagramError> {
    if !(MAX_NODES_RANGE.0..=MAX_NODES_RANGE.1).contains(&request.max_nodes) {
        return Err(DiagramError::new(
            "diagram_bad_option",
            "--max-nodes must be an integer from 5 to 200",
        ));
    }
    if !(DEPTH_RANGE.0..=DEPTH_RANGE.1).contains(&request.depth) {
        return Err(DiagramError::new(
            "diagram_bad_option",
            "--depth must be an integer from 1 to 6",
        ));
    }
    match request.format {
        Format::Dot if request.kind.markdown() => Err(DiagramError::new(
            "diagram_format_unsupported",
            format!(
                "format 'dot' is not available for kind '{}'",
                request.kind.name()
            ),
        )),
        Format::Markdown if !request.kind.markdown() => Err(DiagramError::new(
            "diagram_bad_option",
            "--format must be mermaid or dot",
        )),
        _ => Ok(()),
    }
}

/// The text of a diagram with given label limits: (format, text, nodes, folded).
fn draw(
    input: &Input,
    sources: &dyn Sources,
    request: &DiagramRequest,
    id: Option<&str>,
    limits: &Limits,
) -> Result<DiagramOutput, DiagramError> {
    let dot = request.format == Format::Dot;
    let flow = |graph: &graph::Graph, folded: bool, nodes: usize| {
        let (text, cut) = if dot {
            dot::dot(graph, limits)
        } else {
            mermaid::flowchart(graph, limits)
        };
        DiagramOutput {
            format: request.format,
            text,
            nodes,
            folded: folded || cut,
        }
    };
    Ok(match request.kind {
        Kind::Module => {
            let (graph, folded) = module::graph(input, id, request.max_nodes);
            flow(&graph, folded, graph.nodes.len())
        }
        Kind::Calls => {
            let root = id.unwrap_or_default();
            let (graph, folded) = calls::graph(input, root, request.depth, request.max_nodes);
            flow(&graph, folded, graph.nodes.len())
        }
        Kind::Concurrency => {
            let (graph, folded) = concurrency::graph(input, id, request.max_nodes)?;
            let nodes = graph.nodes.len() - 1;
            flow(&graph, folded, nodes)
        }
        Kind::Types => {
            if dot {
                let (text, nodes, cut) = types::dot(input, id, request.max_nodes, limits);
                DiagramOutput {
                    format: Format::Dot,
                    text,
                    nodes,
                    folded: cut,
                }
            } else {
                let (text, nodes, folded) = types::mermaid(input, id, request.max_nodes, limits);
                DiagramOutput {
                    format: Format::Mermaid,
                    text,
                    nodes,
                    folded,
                }
            }
        }
        Kind::Ownership => {
            let function = id.unwrap_or_default();
            let (text, drawn, cut) = ownership::markdown(input, function, limits.label)?;
            let variables = input
                .facts
                .get(function)
                .and_then(|facts| facts.ownership.as_ref())
                .map(|events| {
                    let mut names: Vec<&str> = events
                        .iter()
                        .filter(|event| !event.var.starts_with('$'))
                        .map(|event| event.var.as_str())
                        .collect();
                    names.sort_unstable();
                    names.dedup();
                    names.len()
                })
                .unwrap_or(0);
            DiagramOutput {
                format: Format::Markdown,
                text,
                nodes: drawn,
                folded: drawn < variables || cut,
            }
        }
        Kind::Layout => {
            let id = id.unwrap_or_default();
            let row = input
                .layouts
                .iter()
                .find(|row| row.id == id)
                .ok_or_else(|| {
                    DiagramError::new(
                        "diagram_unknown_id",
                        format!("unknown id '{id}' for diagram kind 'layout'"),
                    )
                })?;
            DiagramOutput {
                format: Format::Markdown,
                text: layout::markdown(row),
                nodes: row.fields.len() + row.cases.len(),
                folded: false,
            }
        }
        Kind::Transitions => {
            let id = id.unwrap_or_default();
            let union = input.symbol(id).ok_or_else(|| {
                DiagramError::new(
                    "diagram_unknown_id",
                    format!("unknown id '{id}' for diagram kind 'transitions'"),
                )
            })?;
            let edges = transitions::extract(input, sources, union)?;
            let (text, nodes, cut) = if dot {
                transitions::dot(input, id, &edges, limits)
            } else {
                transitions::mermaid(input, id, &edges, limits)
            };
            DiagramOutput {
                format: request.format,
                text,
                nodes,
                folded: cut,
            }
        }
    })
}

/// Draws the diagram of `request` (G30). `sources` is read only by `transitions`.
pub fn render(
    input: &Input,
    sources: &dyn Sources,
    request: &DiagramRequest,
) -> Result<DiagramOutput, DiagramError> {
    check_request(request)?;
    if request.kind == Kind::Layout && input.layouts.is_empty() {
        return Err(DiagramError::new(
            "diagram_unavailable",
            "the KG has no layout rows; build the KG with a compiler that records them",
        ));
    }
    let id = match &request.id {
        Some(text) => Some(resolve::resolve(input, request.kind, text)?),
        None if request.kind.needs_id() => {
            return Err(DiagramError::new(
                "diagram_needs_id",
                format!("diagram kind '{}' needs an id", request.kind.name()),
            ));
        }
        None => None,
    };
    let mut limits = Limits::default();
    for attempt in 0..2 {
        let output = draw(input, sources, request, id.as_deref(), &limits)?;
        let length = output.text.chars().count();
        if length <= MAX_TEXT {
            return Ok(output);
        }
        if attempt == 1 {
            return Err(DiagramError::new(
                "diagram_too_large",
                format!("the diagram is too large ({length} characters); lower --max-nodes"),
            ));
        }
        limits = limits.halved();
    }
    unreachable!("the loop returns")
}
