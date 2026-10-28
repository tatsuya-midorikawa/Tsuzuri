//! The Mermaid flowchart of a `Graph` (G30 "Mermaid のフローチャートの行"): header, nodes, edges,
//! `classDef`, `class`, `style`, `linkStyle`, in that order.

use super::escape::esc;
use super::graph::{Direction, Graph, Shape};
use super::{CLASS_ORDER, Limits, MAX_EDGES, class_def};

fn brackets(shape: Shape) -> (&'static str, &'static str) {
    match shape {
        Shape::Box => ("[\"", "\"]"),
        Shape::Round => ("(\"", "\")"),
        Shape::Stadium => ("([\"", "\"])"),
        Shape::Subroutine => ("[[\"", "\"]]"),
        Shape::Hexagon => ("{{\"", "\"}}"),
    }
}

/// The flowchart text, and whether edges beyond `MAX_EDGES` were left out.
pub fn flowchart(graph: &Graph, limits: &Limits) -> (String, bool) {
    let (ids, mut edges) = graph.numbered();
    let omitted = edges.len().saturating_sub(MAX_EDGES);
    edges.truncate(MAX_EDGES);
    let mut lines = vec![match graph.direction {
        Direction::TopDown => "flowchart TD".to_owned(),
        Direction::LeftRight => "flowchart LR".to_owned(),
    }];
    for (key, node) in &graph.nodes {
        let (open, close) = brackets(node.shape);
        lines.push(format!(
            "    n{}{open}{}{close}",
            ids[key.as_str()],
            esc(&node.label, limits.label)
        ));
    }
    for edge in &edges {
        let arrow = if edge.dashed { "-.->" } else { "-->" };
        let middle = match &edge.label {
            Some(label) if !label.is_empty() => {
                format!("{arrow}|\"{}\"|", esc(label, limits.edge))
            }
            _ => arrow.to_owned(),
        };
        lines.push(format!(
            "    n{} {middle} n{}",
            ids[edge.src.as_str()],
            ids[edge.dst.as_str()]
        ));
    }
    let used: Vec<&str> = CLASS_ORDER
        .iter()
        .copied()
        .filter(|name| graph.nodes.values().any(|node| node.class == Some(*name)))
        .collect();
    for name in &used {
        lines.push(format!("    classDef {name} {}", class_def(name)));
    }
    for name in &used {
        let members: Vec<String> = graph
            .nodes
            .iter()
            .filter(|(_, node)| node.class == Some(*name))
            .map(|(key, _)| format!("n{}", ids[key.as_str()]))
            .collect();
        lines.push(format!("    class {} {name}", members.join(",")));
    }
    for (key, node) in &graph.nodes {
        if node.bold {
            lines.push(format!("    style n{} stroke-width:3px", ids[key.as_str()]));
        }
    }
    for (index, edge) in edges.iter().enumerate() {
        if edge.red {
            lines.push(format!(
                "    linkStyle {index} stroke:#cf222e,stroke-width:2px"
            ));
        }
    }
    if omitted > 0 {
        lines.push(format!("    %% omitted: {omitted} edges"));
    }
    (lines.join("\n") + "\n", omitted > 0)
}
