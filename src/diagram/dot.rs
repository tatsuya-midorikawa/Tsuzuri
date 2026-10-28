//! The DOT text of a `Graph` (G30 "DOT の形"), for Graphviz.

use super::escape::dot_esc;
use super::graph::{Direction, Graph, Shape};
use super::{Limits, MAX_EDGES, dot_fill};

/// The DOT text, and whether edges beyond `MAX_EDGES` were left out.
pub fn dot(graph: &Graph, limits: &Limits) -> (String, bool) {
    let (ids, mut edges) = graph.numbered();
    let omitted = edges.len().saturating_sub(MAX_EDGES);
    edges.truncate(MAX_EDGES);
    let mut lines = vec![
        "digraph tsuzuri {".to_owned(),
        format!(
            "    rankdir={};",
            match graph.direction {
                Direction::TopDown => "TB",
                Direction::LeftRight => "LR",
            }
        ),
        "    node [shape=box, fontname=\"monospace\"];".to_owned(),
        "    edge [fontname=\"monospace\"];".to_owned(),
    ];
    for (key, node) in &graph.nodes {
        let shape = match node.shape {
            Shape::Box | Shape::Round | Shape::Subroutine => "box",
            Shape::Stadium => "ellipse",
            Shape::Hexagon => "hexagon",
        };
        let mut styles = Vec::new();
        let mut attributes = vec![
            format!("label=\"{}\"", dot_esc(&node.label, limits.label)),
            format!("shape={shape}"),
        ];
        if node.shape == Shape::Round {
            styles.push("rounded");
        }
        if node.shape == Shape::Subroutine {
            attributes.push("peripheries=2".to_owned());
        }
        if let Some(class) = node.class {
            styles.push("filled");
            attributes.push(format!("fillcolor=\"{}\"", dot_fill(class)));
            if matches!(class, "ext" | "note") {
                styles.push("dashed");
            }
        }
        if !styles.is_empty() {
            attributes.push(format!("style=\"{}\"", styles.join(",")));
        }
        if node.bold {
            attributes.push("penwidth=3".to_owned());
        }
        lines.push(format!(
            "    n{} [{}];",
            ids[key.as_str()],
            attributes.join(", ")
        ));
    }
    for edge in &edges {
        let mut attributes = Vec::new();
        if let Some(label) = edge.label.as_deref().filter(|label| !label.is_empty()) {
            attributes.push(format!("label=\"{}\"", dot_esc(label, limits.edge)));
        }
        if edge.dashed {
            attributes.push("style=dashed".to_owned());
        }
        if edge.red {
            attributes.push("color=\"#cf222e\"".to_owned());
        }
        let suffix = if attributes.is_empty() {
            String::new()
        } else {
            format!(" [{}]", attributes.join(", "))
        };
        lines.push(format!(
            "    n{} -> n{}{suffix};",
            ids[edge.src.as_str()],
            ids[edge.dst.as_str()]
        ));
    }
    if omitted > 0 {
        lines.push(format!("    // omitted: {omitted} edges"));
    }
    lines.push("}".to_owned());
    (lines.join("\n") + "\n", omitted > 0)
}
