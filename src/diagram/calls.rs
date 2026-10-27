//! `calls` (G30): the functions that one function reaches by calls, level by level up to a depth,
//! colored by their effects.

use std::collections::{BTreeMap, BTreeSet};

use super::graph::{Direction, Graph, Shape, fold};
use super::input::{Input, Relation};

const IO_EFFECTS: [&str; 10] = [
    "io",
    "os",
    "net",
    "task",
    "atomic",
    "rc",
    "extern",
    "debug",
    "user_drop",
    "gpu",
];

/// The class of a node by its effects: the first of `io`, `alloc`, `trap` and `pure` that holds;
/// `ext` outside the graph; none without a facts row.
pub fn effect_class(input: &Input, id: &str) -> Option<&'static str> {
    let symbol = input.symbol(id)?;
    let facts = input.facts.get(id)?;
    let has = |set: &[&str]| {
        facts
            .effects
            .iter()
            .any(|effect| set.contains(&effect.as_str()))
    };
    if has(&IO_EFFECTS) {
        Some("io")
    } else if has(&["alloc", "copy"]) {
        Some("alloc")
    } else if has(&["trap", "diverge"]) {
        Some("trap")
    } else if facts.effects_complete || symbol.pure {
        Some("pure")
    } else {
        None
    }
}

fn node_shape(input: &Input, id: &str) -> Shape {
    match input.symbol(id) {
        None => Shape::Stadium,
        Some(symbol) if symbol.name == "$entry" => Shape::Subroutine,
        Some(symbol) => match symbol.kind.as_str() {
            "method" => Shape::Round,
            "test" | "bench" => Shape::Hexagon,
            _ => Shape::Box,
        },
    }
}

/// The graph from `root`, and whether it was folded.
pub fn graph(input: &Input, root: &str, depth: usize, max_nodes: usize) -> (Graph, bool) {
    let mut callees: BTreeMap<&str, Vec<&Relation>> = BTreeMap::new();
    for edge in input.edges.iter().filter(|edge| edge.rel == "calls") {
        callees.entry(&edge.from).or_default().push(edge);
    }
    for rows in callees.values_mut() {
        rows.sort_by(|a, b| (a.line, a.column, &a.to).cmp(&(b.line, b.column, &b.to)));
    }
    let none: Vec<&Relation> = Vec::new();
    let out_of = |id: &str| callees.get(id).unwrap_or(&none);
    let mut levels: BTreeMap<String, usize> = BTreeMap::from([(root.to_owned(), 0)]);
    let mut frontier: Vec<String> = vec![root.to_owned()];
    for level in 1..=depth {
        let mut following = Vec::new();
        frontier.sort();
        for id in &frontier {
            for row in out_of(id) {
                if !levels.contains_key(&row.to) {
                    levels.insert(row.to.clone(), level);
                    following.push(row.to.clone());
                }
            }
        }
        frontier = following;
    }
    let mut graph = Graph::new(Direction::TopDown);
    let mut grouped: BTreeMap<(&str, &str), Vec<&Relation>> = BTreeMap::new();
    for id in levels.keys() {
        for row in out_of(id) {
            if levels.contains_key(&row.to) {
                grouped.entry((id, &row.to)).or_default().push(row);
            }
        }
    }
    for (id, level) in &levels {
        let truncated =
            *level == depth && out_of(id).iter().any(|row| !levels.contains_key(&row.to));
        let label = if truncated {
            format!("{id} \u{2026}")
        } else {
            id.clone()
        };
        graph.node(
            id,
            label,
            node_shape(input, id),
            effect_class(input, id).or_else(|| input.symbol(id).is_none().then_some("ext")),
            id == root,
        );
    }
    for ((a, b), rows) in grouped {
        let mut distinct: Vec<String> = Vec::new();
        for row in &rows {
            let text = row.args.join(",");
            if !text.is_empty() && !distinct.contains(&text) {
                distinct.push(text);
            }
        }
        let mut label = distinct.join(" / ");
        if rows.len() >= 2 {
            if !label.is_empty() {
                label.push(' ');
            }
            label.push_str(&format!("\u{d7}{}", rows.len()));
        }
        graph.edge(
            a,
            b,
            (!label.is_empty()).then_some(label),
            false,
            false,
            rows.len(),
        );
    }
    if levels.len() > max_nodes {
        let group_of = |id: &str| -> String {
            match input.symbol(id) {
                Some(symbol) => symbol.module.clone(),
                None => match id.split_once('.') {
                    Some((head, _)) => format!("std:{head}"),
                    None => "std:builtin".to_owned(),
                },
            }
        };
        let keep = BTreeSet::from([root.to_owned()]);
        return (fold(&graph, &group_of, &keep, max_nodes), true);
    }
    (graph, false)
}
