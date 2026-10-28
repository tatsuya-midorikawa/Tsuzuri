//! `concurrency` (G30, D8): an approximation of what runs in parallel and what it shares, from the
//! signatures (`Channel.Sender<T>`, `Channel.Receiver<T>`, `Mutex<T>`, `Atomic<T>`) and the calls
//! (`Task.scope`, `Task.parallel`, `Parallel.*`, the creators of the cells). A note says so.

use std::collections::{BTreeMap, BTreeSet};

use super::DiagramError;
use super::graph::{Direction, Graph, Shape, fold};
use super::input::Input;

/// The key of the note node.
pub const NOTE: &str = "note";

const CELL_EVIDENCE: [(&str, &str, &str); 4] = [
    ("Channel.Sender<", "send", "Channel"),
    ("Channel.Receiver<", "recv", "Channel"),
    ("Mutex<", "lock", "Mutex"),
    ("Atomic<", "atomic", "Atomic"),
];
const CREATORS: [(&str, &str); 3] = [
    ("Channel.bounded", "Channel"),
    ("Mutex.create", "Mutex"),
    ("Atomic.create", "Atomic"),
];

/// The type between the first `opening` and its matching `>`.
fn inner_type(text: &str, opening: &str) -> String {
    let start = text.find(opening).map_or(0, |at| at + opening.len());
    let mut depth = 1;
    for (index, character) in text[start..].char_indices() {
        match character {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return text[start..start + index].to_owned();
                }
            }
            _ => {}
        }
    }
    text[start..].to_owned()
}

/// Whether `opening` occurs where no identifier character comes before it.
fn has_opening(text: &str, opening: &str) -> bool {
    text.match_indices(opening).any(|(at, _)| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_ascii_alphanumeric() || before == '_')
    })
}

/// The graph (its last node the note), and whether it was folded. The regions and cells are never
/// folded: when they, a node for the other functions and the note do not fit in `max_nodes`, the
/// diagram is too large.
pub fn graph(
    input: &Input,
    focus: Option<&str>,
    max_nodes: usize,
) -> Result<(Graph, bool), DiagramError> {
    let mut evidence: BTreeMap<String, BTreeSet<(&str, &str, String)>> = BTreeMap::new();
    let mut region: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut known: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for symbol in &input.symbols {
        if !matches!(symbol.kind.as_str(), "def" | "method") {
            continue;
        }
        let places = symbol
            .params
            .iter()
            .map(|param| (param.ty.as_str(), true))
            .chain(symbol.result.as_deref().map(|result| (result, false)));
        for (text, parameter) in places {
            for (opening, relation, kind) in CELL_EVIDENCE {
                if has_opening(text, opening) {
                    let inner = inner_type(text, opening);
                    known.entry(kind).or_default().insert(inner.clone());
                    evidence.entry(symbol.id.clone()).or_default().insert((
                        if parameter { relation } else { "create" },
                        kind,
                        inner,
                    ));
                }
            }
        }
    }
    for edge in &input.edges {
        if edge.rel != "calls" || input.symbol(&edge.from).is_none() {
            continue;
        }
        if matches!(edge.to.as_str(), "Task.parallel" | "Task.scope")
            || edge.to.starts_with("Parallel.")
        {
            region
                .entry(edge.from.clone())
                .or_default()
                .insert(edge.to.clone());
        }
        if let Some((_, kind)) = CREATORS.iter().find(|(name, _)| *name == edge.to) {
            let inner = match known.get(kind) {
                Some(types) if types.len() == 1 => types.iter().next().unwrap().clone(),
                _ => "?".to_owned(),
            };
            evidence
                .entry(edge.from.clone())
                .or_default()
                .insert(("create", kind, inner));
        }
    }
    let involved: BTreeSet<&String> = evidence
        .keys()
        .chain(region.keys())
        .filter(|function| focus.is_none_or(|focus| input.module_of(function) == Some(focus)))
        .collect();
    let mut graph = Graph::new(Direction::LeftRight);
    for function in &involved {
        match region.get(*function) {
            Some(calls) => {
                let calls: Vec<&str> = calls.iter().map(String::as_str).collect();
                graph.node(
                    function,
                    format!("{function} \u{b7} {}", calls.join(", ")),
                    Shape::Subroutine,
                    Some("region"),
                    false,
                );
            }
            None => graph.node(function, (*function).clone(), Shape::Box, None, false),
        }
    }
    for function in &involved {
        for (relation, kind, inner) in evidence.get(*function).into_iter().flatten() {
            let cell = format!("cell:{kind}<{inner}>");
            graph.node(
                &cell,
                format!("{kind}<{inner}>"),
                Shape::Round,
                Some("cell"),
                false,
            );
            match *relation {
                "recv" => graph.edge(&cell, function, Some("recv".into()), false, false, 1),
                "create" => graph.edge(function, &cell, Some("create".into()), true, false, 1),
                other => graph.edge(function, &cell, Some(other.into()), false, false, 1),
            }
        }
    }
    for edge in &input.edges {
        if edge.rel == "calls"
            && region.contains_key(&edge.from)
            && involved.contains(&edge.from)
            && involved.contains(&edge.to)
            && edge.to != edge.from
        {
            graph.edge(&edge.from, &edge.to, Some("calls".into()), true, false, 1);
        }
    }
    let unique: BTreeSet<_> = graph.edges.drain(..).collect();
    graph.edges = unique.into_iter().collect();
    let folded = graph.nodes.len() + 1 > max_nodes;
    if folded {
        let keep: BTreeSet<String> = graph
            .nodes
            .keys()
            .filter(|key| key.starts_with("cell:") || region.contains_key(*key))
            .cloned()
            .collect();
        let group_of = |key: &str| -> String {
            if key.starts_with("cell:") {
                key.to_owned()
            } else {
                input.module_of(key).unwrap_or_default().to_owned()
            }
        };
        let others = graph.nodes.len() - keep.len();
        let needed = keep.len() + usize::from(others > 0) + 1;
        if needed > max_nodes {
            return Err(DiagramError {
                kind: "diagram_too_large",
                message: format!(
                    "the concurrency diagram has {} regions and cells, more than --max-nodes {max_nodes} can show; raise --max-nodes or name a module",
                    keep.len()
                ),
            });
        }
        graph = fold(&graph, &group_of, &keep, max_nodes - 1);
    }
    graph.node(
        NOTE,
        "approximate: from signatures and call sites".to_owned(),
        Shape::Box,
        Some("note"),
        false,
    );
    Ok((graph, folded))
}
