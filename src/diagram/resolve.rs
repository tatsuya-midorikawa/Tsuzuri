//! The ID of a diagram (G30 "診断"): an exact ID of the kinds the diagram takes, else the stages
//! of `kg` (a unique suffix, the same ignoring case, a unique prefix; G28 §2), else an error that
//! names the kind it needs or suggests IDs.

use std::collections::{BTreeMap, BTreeSet};

use super::input::{Input, Symbol};
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

fn is_type(symbol: &Symbol) -> bool {
    matches!(symbol.kind.as_str(), "record" | "union")
}

/// The layout that the record or union `declaration` names: one of its concrete types
/// (`M.Box<i64>` of the generic `M.Box`, and not `M.Boxed`'s), else an error that lists them.
fn instantiation(text: &str, declaration: &str, ids: &[String]) -> Result<String, DiagramError> {
    let prefix = format!("{declaration}<");
    let mut own: Vec<String> = ids
        .iter()
        .filter(|id| id.starts_with(&prefix))
        .cloned()
        .collect();
    if own.len() == 1 {
        return Ok(own.remove(0));
    }
    own.sort();
    Err(unknown(text, Kind::Layout, &own))
}

/// The ID that `text` names for a diagram of `kind`.
pub fn resolve(input: &Input, kind: Kind, text: &str) -> Result<String, DiagramError> {
    let (ids, expected) = allowed(input, kind);
    if ids.iter().any(|id| id == text) {
        return Ok(text.to_owned());
    }
    let layout = kind == Kind::Layout;
    if let Some(symbol) = input.symbol(text) {
        if layout && is_type(symbol) {
            return instantiation(text, text, &ids);
        }
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
    let mut candidates: BTreeMap<String, String> =
        ids.iter().map(|id| (id.clone(), id.clone())).collect();
    // A layout is also named by its record or union, whose ID is shortened like any other.
    let declarations: BTreeSet<&str> = if layout {
        input
            .symbols
            .iter()
            .filter(|symbol| is_type(symbol))
            .map(|symbol| symbol.id.as_str())
            .collect()
    } else {
        BTreeSet::new()
    };
    for declaration in &declarations {
        candidates
            .entry((*declaration).to_owned())
            .or_insert_with(|| (*declaration).to_owned());
    }
    let named = |id: String| {
        if ids.contains(&id) {
            Ok(id)
        } else {
            instantiation(text, &id, &ids)
        }
    };
    let suggestions = match resolve_in(&candidates, text) {
        Resolution::One(id) if !text.is_empty() => return named(id),
        Resolution::Many(found) => {
            // A record or union and its own concrete types are one type.
            let one = found.iter().find(|id| {
                declarations.contains(id.as_str())
                    && found
                        .iter()
                        .all(|other| other == *id || other.starts_with(&format!("{id}<")))
            });
            if let Some(id) = one
                && !text.is_empty()
            {
                return named(id.clone());
            }
            found
        }
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
    Err(unknown(text, kind, &suggestions))
}

/// `diagram_unknown_id`, with up to 5 of `suggestions`.
fn unknown(text: &str, kind: Kind, suggestions: &[String]) -> DiagramError {
    let mut message = format!("unknown id '{text}' for diagram kind '{}'", kind.name());
    if !suggestions.is_empty() {
        let shown: Vec<&str> = suggestions.iter().take(5).map(String::as_str).collect();
        message.push_str(&format!("; did you mean: {}", shown.join(", ")));
    }
    DiagramError {
        kind: "diagram_unknown_id",
        message,
    }
}
