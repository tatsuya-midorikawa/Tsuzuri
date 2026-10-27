//! The one model of the flowcharts (G30 D3): nodes keyed by their original IDs, numbered `n1`,
//! `n2`, ... in byte order, and edges sorted by their numbered ends. `fold` merges the nodes
//! outside `keep` by group when a diagram has too many.

use std::collections::{BTreeMap, BTreeSet};

use super::escape::ELLIPSIS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Box,
    Round,
    Stadium,
    Subroutine,
    Hexagon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    LeftRight,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub label: String,
    pub shape: Shape,
    pub class: Option<&'static str>,
    pub bold: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub src: String,
    pub dst: String,
    pub label: Option<String>,
    pub dashed: bool,
    pub red: bool,
    pub weight: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub direction: Direction,
    pub nodes: BTreeMap<String, Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn new(direction: Direction) -> Self {
        Self {
            direction,
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        }
    }

    pub fn node(
        &mut self,
        key: &str,
        label: String,
        shape: Shape,
        class: Option<&'static str>,
        bold: bool,
    ) {
        self.nodes.insert(
            key.to_owned(),
            Node {
                label,
                shape,
                class,
                bold,
            },
        );
    }

    pub fn edge(
        &mut self,
        src: &str,
        dst: &str,
        label: Option<String>,
        dashed: bool,
        red: bool,
        weight: usize,
    ) {
        self.edges.push(Edge {
            src: src.to_owned(),
            dst: dst.to_owned(),
            label,
            dashed,
            red,
            weight,
        });
    }

    /// The number of each node (from 1, in key order) and the edges sorted by (source number,
    /// target number, label, dashed before solid).
    pub fn numbered(&self) -> (BTreeMap<&str, usize>, Vec<&Edge>) {
        let ids: BTreeMap<&str, usize> = self
            .nodes
            .keys()
            .enumerate()
            .map(|(index, key)| (key.as_str(), index + 1))
            .collect();
        let mut edges: Vec<&Edge> = self.edges.iter().collect();
        edges.sort_by_key(|edge| {
            (
                ids[edge.src.as_str()],
                ids[edge.dst.as_str()],
                edge.label.as_deref().unwrap_or(""),
                !edge.dashed,
            )
        });
        (ids, edges)
    }
}

/// The key of the node that stands for the groups beyond the room.
const REST: &str = "group:\u{2026}";

/// Folds every node outside `keep` into one node per group (`group_of`), and keeps the groups with
/// the most edges when there are more than the room (G30 "畳み込み").
pub fn fold(
    graph: &Graph,
    group_of: &dyn Fn(&str) -> String,
    keep: &BTreeSet<String>,
    max_nodes: usize,
) -> Graph {
    let mut groups: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for key in graph.nodes.keys() {
        if !keep.contains(key) {
            groups.entry(group_of(key)).or_default().push(key);
        }
    }
    let mut degree: BTreeMap<String, usize> = BTreeMap::new();
    for edge in &graph.edges {
        *degree.entry(group_of(&edge.src)).or_default() += 1;
        *degree.entry(group_of(&edge.dst)).or_default() += 1;
    }
    let mut ranked: Vec<&String> = groups.keys().collect();
    ranked.sort_by_key(|group| {
        (
            std::cmp::Reverse(degree.get(*group).copied().unwrap_or(0)),
            *group,
        )
    });
    // `room - 1` groups and one for the rest; a negative room counts from the end, as a slice
    // of the reference does.
    let room = max_nodes as isize - keep.len() as isize;
    let shown: Vec<&String> = if ranked.len() as isize <= room {
        ranked.clone()
    } else {
        let end = room - 1;
        let end = if end < 0 {
            (ranked.len() as isize + end).max(0)
        } else {
            end.min(ranked.len() as isize)
        };
        ranked[..end as usize].to_vec()
    };
    let shown_set: BTreeSet<&String> = shown.iter().copied().collect();
    let rest: Vec<&String> = ranked
        .iter()
        .copied()
        .filter(|group| !shown_set.contains(group))
        .collect();
    let mut out = Graph::new(graph.direction);
    for key in keep {
        if let Some(node) = graph.nodes.get(key) {
            out.nodes.insert(key.clone(), node.clone());
        }
    }
    for group in &shown {
        out.node(
            &format!("group:{group}"),
            format!("{group} ({})", groups[*group].len()),
            Shape::Box,
            None,
            false,
        );
    }
    if !rest.is_empty() {
        let members: usize = rest.iter().map(|group| groups[*group].len()).sum();
        out.node(
            REST,
            format!("{ELLIPSIS}{} more groups ({members})", rest.len()),
            Shape::Box,
            None,
            false,
        );
    }
    let mut mapping: BTreeMap<&str, String> =
        keep.iter().map(|key| (key.as_str(), key.clone())).collect();
    for (group, members) in &groups {
        let target = if shown_set.contains(group) {
            format!("group:{group}")
        } else {
            REST.to_owned()
        };
        for key in members {
            mapping.insert(key, target.clone());
        }
    }
    struct Merged {
        weight: usize,
        labels: Vec<Option<String>>,
        dashed: bool,
    }
    let mut merged: BTreeMap<(String, String), Merged> = BTreeMap::new();
    for edge in &graph.edges {
        let (a, b) = (&mapping[edge.src.as_str()], &mapping[edge.dst.as_str()]);
        if a != b || keep.contains(a) {
            let entry = merged.entry((a.clone(), b.clone())).or_insert(Merged {
                weight: 0,
                labels: Vec::new(),
                dashed: true,
            });
            entry.weight += edge.weight;
            entry.labels.push(edge.label.clone());
            entry.dashed = entry.dashed && edge.dashed;
        }
    }
    for ((a, b), entry) in merged {
        let label = if entry.labels.len() == 1 && entry.weight == 1 {
            entry.labels[0].clone()
        } else if entry.weight >= 2 {
            Some(format!("\u{d7}{}", entry.weight))
        } else {
            None
        };
        out.edge(&a, &b, label, entry.dashed, false, entry.weight);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_numbers_follow_byte_order() {
        let mut graph = Graph::new(Direction::TopDown);
        for key in ["b", "a", "B", "\u{e9}"] {
            graph.node(key, key.to_owned(), Shape::Box, None, false);
        }
        let (ids, _) = graph.numbered();
        assert_eq!(
            ids.into_iter().collect::<Vec<_>>(),
            [("B", 1), ("a", 2), ("b", 3), ("\u{e9}", 4)]
        );
    }

    #[test]
    fn edges_sort_by_endpoint_numbers_then_label() {
        let mut graph = Graph::new(Direction::TopDown);
        for key in ["a", "b", "c"] {
            graph.node(key, key.to_owned(), Shape::Box, None, false);
        }
        graph.edge("b", "a", None, false, false, 1);
        graph.edge("a", "c", Some("z".into()), false, false, 1);
        graph.edge("a", "c", Some("y".into()), false, false, 1);
        graph.edge("a", "b", None, false, false, 1);
        graph.edge("a", "b", None, true, false, 1);
        let (_, edges) = graph.numbered();
        let order: Vec<(&str, &str, Option<&str>, bool)> = edges
            .iter()
            .map(|edge| {
                (
                    edge.src.as_str(),
                    edge.dst.as_str(),
                    edge.label.as_deref(),
                    edge.dashed,
                )
            })
            .collect();
        assert_eq!(
            order,
            [
                ("a", "b", None, true),
                ("a", "b", None, false),
                ("a", "c", Some("y"), false),
                ("a", "c", Some("z"), false),
                ("b", "a", None, false),
            ]
        );
    }
}
