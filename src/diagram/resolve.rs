//! The ID of a diagram (G30 "診断"): an exact ID of the kinds the diagram takes, else the stages
//! of `kg` (a unique suffix, the same ignoring case, a unique prefix; G28 §2), else an error that
//! names the kind it needs or suggests IDs.

use std::collections::BTreeMap;

use super::input::Input;
use super::{DiagramError, Kind};
use crate::kg::query::{Resolution, resolve_in};

/// The IDs that `kind` takes, and how its error names them.
fn allowed(input: &Input, kind: Kind) -> (Vec<String>, &'static str) {
    let of = |kinds: &[&str]| -> Vec<String> {
        input
            .symbols
            .iter()
            .filter(|symbol| kinds.contains(&symbol.kind.as_str()))
            .map(|symbol| symbol.id.clone())
            .collect()
    };
    match kind {
        Kind::Module | Kind::Types | Kind::Concurrency => (of(&["module"]), "a module id"),
        Kind::Calls | Kind::Ownership => (of(&["def", "method", "test", "bench"]), "a function id"),
        Kind::Layout => (
            input.layouts.iter().map(|row| row.id.clone()).collect(),
            "a type id",
        ),
        Kind::Transitions => (of(&["union"]), "a union id"),
    }
}

fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// The ID that `text` names for a diagram of `kind`.
pub fn resolve(input: &Input, kind: Kind, text: &str) -> Result<String, DiagramError> {
    let (ids, expected) = allowed(input, kind);
    if ids.iter().any(|id| id == text) {
        return Ok(text.to_owned());
    }
    if let Some(symbol) = input.symbol(text) {
        return Err(DiagramError {
            kind: "diagram_wrong_id_kind",
            message: format!(
                "'{text}' is {} {}; diagram kind '{}' needs {expected}",
                article(&symbol.kind),
                symbol.kind,
                kind.name()
            ),
        });
    }
    let candidates: BTreeMap<String, String> =
        ids.iter().map(|id| (id.clone(), id.clone())).collect();
    let suggestions = match resolve_in(&candidates, text) {
        Resolution::One(id) if !text.is_empty() => return Ok(id),
        Resolution::Many(found) => found,
        _ => {
            let last = text.rsplit('.').next().unwrap_or(text);
            let mut found: Vec<String> = ids
                .iter()
                .filter(|id| {
                    !text.is_empty() && (id.rsplit('.').next() == Some(last) || id.contains(text))
                })
                .cloned()
                .collect();
            found.sort();
            found.dedup();
            found
        }
    };
    let mut message = format!("unknown id '{text}' for diagram kind '{}'", kind.name());
    if !suggestions.is_empty() {
        let shown: Vec<&str> = suggestions.iter().take(5).map(String::as_str).collect();
        message.push_str(&format!("; did you mean: {}", shown.join(", ")));
    }
    Err(DiagramError {
        kind: "diagram_unknown_id",
        message,
    })
}
