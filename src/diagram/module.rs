//! `module` (G30): the dependencies between modules, from the calls, type uses, implementations
//! and instances that cross them, with the `using` imports dashed and the cycles in red.

use std::collections::{BTreeMap, BTreeSet};

use super::graph::{Direction, Graph, Shape};
use super::input::Input;
use crate::kg::algo::{self, Digraph};

const MORE: &str = "\u{2026}more";

/// The graph, and whether it was folded.
pub fn graph(input: &Input, focus: Option<&str>, max_nodes: usize) -> (Graph, bool) {
    let modules = input.module_ids();
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for edge in &input.edges {
        if matches!(
            edge.rel.as_str(),
            "calls" | "uses_type" | "implements" | "instance_of"
        ) && let (Some(a), Some(b)) = (input.module_of(&edge.from), input.module_of(&edge.to))
            && a != b
        {
            *counts.entry((a.to_owned(), b.to_owned())).or_default() += 1;
        }
    }
    let mut using: BTreeSet<(String, String)> = BTreeSet::new();
    for edge in input.edges.iter().filter(|edge| edge.rel == "imports") {
        let prefix = format!("{}::", edge.to);
        for module in &modules {
            if *module != edge.from
                && module
                    .strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.contains("::"))
                && !counts.contains_key(&(edge.from.clone(), (*module).to_owned()))
            {
                using.insert((edge.from.clone(), (*module).to_owned()));
            }
        }
    }
    // The cycles: the strongly connected components of the solid edges.
    let number: BTreeMap<&str, usize> = modules
        .iter()
        .enumerate()
        .map(|(index, module)| (*module, index))
        .collect();
    let digraph = Digraph::from_edges(
        modules.len(),
        counts
            .keys()
            .filter_map(|(a, b)| Some((*number.get(a.as_str())?, *number.get(b.as_str())?))),
    );
    let component = algo::strongly_connected(&digraph);
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for id in &component {
        *sizes.entry(*id).or_default() += 1;
    }
    let cycle_of = |module: &str| -> Option<usize> {
        let id = component[*number.get(module)?];
        (sizes[&id] >= 2).then_some(id)
    };
    let pairs: BTreeSet<(String, String)> = counts.keys().cloned().chain(using).collect();
    let nodes: BTreeSet<String> = match focus {
        Some(focus) => {
            let mut nodes: BTreeSet<String> = BTreeSet::from([focus.to_owned()]);
            for (a, b) in &pairs {
                if a == focus {
                    nodes.insert(b.clone());
                }
                if b == focus {
                    nodes.insert(a.clone());
                }
            }
            nodes
        }
        None => modules.iter().map(|module| (*module).to_owned()).collect(),
    };
    let mut degree: BTreeMap<&str, usize> = BTreeMap::new();
    for (a, b) in &pairs {
        *degree.entry(a).or_default() += 1;
        *degree.entry(b).or_default() += 1;
    }
    let folded = nodes.len() > max_nodes;
    let keep: BTreeSet<String> = if folded {
        let mut ranked: Vec<&String> = nodes.iter().collect();
        ranked.sort_by_key(|module| {
            (
                std::cmp::Reverse(degree.get(module.as_str()).copied().unwrap_or(0)),
                *module,
            )
        });
        ranked
            .into_iter()
            .take(max_nodes.saturating_sub(1))
            .cloned()
            .collect()
    } else {
        nodes.clone()
    };
    let mut graph = Graph::new(Direction::TopDown);
    for module in &keep {
        let class = if cycle_of(module).is_some() {
            Some("cycle")
        } else if input
            .symbol(module)
            .is_some_and(|symbol| symbol.origin != "project")
        {
            Some("dep")
        } else {
            None
        };
        graph.node(module, module.clone(), Shape::Box, class, false);
    }
    if folded {
        graph.node(
            MORE,
            format!("\u{2026}{} more modules", nodes.len() - keep.len()),
            Shape::Box,
            Some("dep"),
            false,
        );
    }
    struct Merged {
        count: usize,
        solid: bool,
        red: bool,
    }
    let mut merged: BTreeMap<(String, String), Merged> = BTreeMap::new();
    for (a, b) in &pairs {
        if !(nodes.contains(a) && nodes.contains(b)) {
            continue;
        }
        let ka = if keep.contains(a) { a.as_str() } else { MORE };
        let kb = if keep.contains(b) { b.as_str() } else { MORE };
        if ka == kb {
            continue;
        }
        let pair = (a.clone(), b.clone());
        let solid_pair = counts.contains_key(&pair);
        let entry = merged
            .entry((ka.to_owned(), kb.to_owned()))
            .or_insert(Merged {
                count: 0,
                solid: false,
                red: false,
            });
        entry.count += counts.get(&pair).copied().unwrap_or(0);
        entry.solid = solid_pair || entry.solid;
        entry.red =
            ka == a && kb == b && solid_pair && cycle_of(a).is_some() && cycle_of(a) == cycle_of(b);
    }
    for ((a, b), entry) in merged {
        let label = if entry.count >= 2 {
            Some(format!("\u{d7}{}", entry.count))
        } else if !entry.solid {
            Some("using".to_owned())
        } else {
            None
        };
        graph.edge(&a, &b, label, !entry.solid, entry.red, entry.count.max(1));
    }
    (graph, folded)
}
