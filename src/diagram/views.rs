//! The diagrams that need no ID, written into the knowledge graph as `views/*.mmd` (G30, D10).

use std::collections::BTreeMap;

use super::{DiagramRequest, Input, Kind, render};

/// The views of `input` by path, in path order. A view that cannot be drawn is left out.
pub fn views(input: &Input) -> Vec<(String, String)> {
    let none: BTreeMap<String, String> = BTreeMap::new();
    let mut files = Vec::new();
    for (path, kind, least) in [
        ("views/concurrency.mmd", Kind::Concurrency, 1),
        ("views/modules.mmd", Kind::Module, 2),
        ("views/types.mmd", Kind::Types, 1),
    ] {
        if let Ok(output) = render(input, &none, &DiagramRequest::new(kind))
            && output.nodes >= least
            && (kind != Kind::Module || input.module_ids().len() >= 2)
        {
            files.push((path.to_owned(), output.text));
        }
    }
    files
}
