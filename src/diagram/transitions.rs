//! `transitions` (G30, D4): the state diagram of a union whose cases a function maps to cases, from
//! the `match` over the function's first parameter in the source. The source must be the one the
//! graph was built from (its SHA-256 is in `manifest.json`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::escape::{dot_esc, esc, transition};
use super::input::{Input, Symbol};
use super::{DiagramError, MAX_EDGES};
use crate::syntax::{Edition, Expr, ExprKind, Pattern, PatternKind};

/// Where `transitions` reads the sources: `render` never opens files itself (D14).
pub trait Sources {
    fn read(&self, relative_path: &str) -> Option<String>;
}

/// The sources of a project on disk: `root.join(path)`, for relative paths without `..`.
pub struct DiskSources {
    pub root: PathBuf,
}

/// The largest source this reads.
const SOURCE_LIMIT: u64 = 64 * 1024 * 1024;

impl Sources for DiskSources {
    fn read(&self, relative_path: &str) -> Option<String> {
        let path = std::path::Path::new(relative_path);
        if relative_path.is_empty()
            || !path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return None;
        }
        let bytes = crate::cache::read_regular(&self.root.join(path), SOURCE_LIMIT).ok()?;
        String::from_utf8(bytes).ok()
    }
}

/// The sources of a test, by path.
impl Sources for BTreeMap<String, String> {
    fn read(&self, relative_path: &str) -> Option<String> {
        self.get(relative_path).cloned()
    }
}

/// A type's head: without a leading `ref ` or `ref mut `, and up to its first `<`.
fn head(ty: &str) -> &str {
    let ty = ty.trim_start();
    let ty = ty
        .strip_prefix("ref mut ")
        .or_else(|| ty.strip_prefix("ref "))
        .unwrap_or(ty);
    ty.split('<').next().unwrap_or(ty).trim()
}

/// The last word of a name: after its last `.` or `:`.
fn last_word(text: &str) -> &str {
    text.rsplit(['.', ':']).next().unwrap_or(text)
}

/// The `match` over the parameter `name` in a function's body.
fn find_match<'a>(body: &'a Expr, name: &str) -> Option<&'a [crate::syntax::MatchArm]> {
    match &body.kind {
        ExprKind::Match { value, arms, .. } => match &value.kind {
            ExprKind::Name(id) if id.text == name => Some(arms),
            _ => None,
        },
        ExprKind::Block { result, .. } => find_match(result, name),
        ExprKind::If {
            then_branch,
            else_branch,
            ..
        } => find_match(then_branch, name).or_else(|| find_match(else_branch, name)),
        ExprKind::Checked(inner) => find_match(inner, name),
        _ => None,
    }
}

/// The cases a pattern starts from: `None` for a wildcard (every case no earlier arm names).
enum Start {
    Cases(BTreeSet<String>),
    Rest,
}

fn starts(pattern: &Pattern, cases: &[&str]) -> Option<Start> {
    match &pattern.kind {
        PatternKind::Wildcard => Some(Start::Rest),
        PatternKind::Binding(id) | PatternKind::Apply(id, _) => {
            let word = last_word(&id.text);
            if cases.contains(&word) {
                Some(Start::Cases(BTreeSet::from([word.to_owned()])))
            } else if matches!(pattern.kind, PatternKind::Binding(_))
                && word.starts_with(|c: char| c.is_lowercase())
            {
                Some(Start::Rest)
            } else {
                None
            }
        }
        PatternKind::Or(left, right) => match (starts(left, cases), starts(right, cases)) {
            (Some(Start::Rest), _) | (_, Some(Start::Rest)) => Some(Start::Rest),
            (Some(Start::Cases(mut a)), Some(Start::Cases(b))) => {
                a.extend(b);
                Some(Start::Cases(a))
            }
            (Some(one), None) | (None, Some(one)) => Some(one),
            (None, None) => None,
        },
        PatternKind::As(inner, _) | PatternKind::Annotated(inner, _) => starts(inner, cases),
        _ => None,
    }
}

/// The cases an arm's body ends in.
fn ends(body: &Expr, cases: &[&str], out: &mut BTreeSet<String>) {
    match &body.kind {
        ExprKind::Name(id) => {
            if cases.contains(&last_word(&id.text)) {
                out.insert(last_word(&id.text).to_owned());
            }
        }
        ExprKind::Call(callee, _) => {
            if let ExprKind::Name(id) = &callee.kind
                && cases.contains(&last_word(&id.text))
            {
                out.insert(last_word(&id.text).to_owned());
            }
        }
        ExprKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            ends(then_branch, cases, out);
            ends(else_branch, cases, out);
        }
        ExprKind::Block { result, .. } => ends(result, cases, out),
        ExprKind::Match { arms, .. } => {
            for arm in arms {
                ends(&arm.body, cases, out);
            }
        }
        ExprKind::Checked(inner) => ends(inner, cases, out),
        _ => {}
    }
}

/// The cases of `union` in declaration order.
pub fn cases<'a>(input: &'a Input, union: &str) -> Vec<&'a Symbol> {
    let mut cases: Vec<&Symbol> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == "case" && input.parent(&symbol.id) == union)
        .collect();
    cases.sort_by_key(|case| (case.line, case.column));
    cases
}

/// Whether `ty`, written in the signature of `function`, names `union`. The written name, with
/// `::` read as `.`, must be the union's ID or end it (`Shapes.Shape` of `Acme::Kg::Shapes.Shape`).
/// A name that several unions end with means the one of the function's module, else the only one
/// of the function's namespace; otherwise it is ambiguous and names none.
fn names_union(input: &Input, function: &Symbol, ty: &str, union: &Symbol) -> bool {
    let written = head(ty).replace("::", ".");
    let ends = |id: &str| {
        let id = id.replace("::", ".");
        id == written || id.ends_with(&format!(".{written}"))
    };
    if !ends(&union.id) {
        return false;
    }
    let named: Vec<&Symbol> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == "union" && ends(&symbol.id))
        .collect();
    if named.len() == 1 {
        return true;
    }
    if named.iter().any(|symbol| symbol.module == function.module) {
        return union.module == function.module;
    }
    let namespace = |module: &str| {
        module
            .split_once("::")
            .map_or("", |(namespace, _)| namespace)
            .to_owned()
    };
    let home = namespace(&function.module);
    namespace(&union.module) == home
        && named
            .iter()
            .filter(|symbol| namespace(&symbol.module) == home)
            .count()
            == 1
}

/// The transitions (source case, target case, function) of the functions `U -> … -> U` that
/// match on their first parameter.
pub fn extract(
    input: &Input,
    sources: &dyn Sources,
    union: &Symbol,
) -> Result<Vec<(String, String, String)>, DiagramError> {
    let cases = cases(input, &union.id);
    let names: Vec<&str> = cases.iter().map(|case| case.name.as_str()).collect();
    let candidates: Vec<&Symbol> = input
        .symbols
        .iter()
        .filter(|symbol| {
            symbol.kind == "def"
                && symbol
                    .params
                    .first()
                    .is_some_and(|param| names_union(input, symbol, &param.ty, union))
                && symbol
                    .result
                    .as_deref()
                    .is_some_and(|result| names_union(input, symbol, result, union))
        })
        .collect();
    let edition = Edition::parse(&input.edition).unwrap_or(Edition::E2026);
    let mut programs: BTreeMap<String, crate::syntax::Program> = BTreeMap::new();
    let mut edges = Vec::new();
    for candidate in candidates {
        let Some(module) = input
            .modules
            .iter()
            .find(|module| module.name == candidate.module)
        else {
            continue;
        };
        if !programs.contains_key(&module.path) {
            let text = sources.read(&module.path).ok_or_else(|| DiagramError {
                kind: "kg_broken",
                message: format!("cannot read source '{}'", module.path),
            })?;
            if crate::kg::input::digest(text.as_bytes()) != module.source {
                return Err(DiagramError {
                    kind: "kg_stale",
                    message: format!(
                        "source '{}' changed after the KG was built; run tsuzuri kg build",
                        module.path
                    ),
                });
            }
            let program =
                crate::parser::parse_with_source_all(&text, 0, edition).map_err(|_| {
                    DiagramError {
                        kind: "kg_broken",
                        message: format!("cannot parse source '{}'", module.path),
                    }
                })?;
            programs.insert(module.path.clone(), program);
        }
        let program = &programs[&module.path];
        let Some(function) = program
            .functions
            .iter()
            .find(|function| function.name.text == candidate.name)
        else {
            continue;
        };
        let Some(parameter) = function.parameters.first() else {
            continue;
        };
        let Some(arms) = find_match(&function.body, &parameter.name.text) else {
            continue;
        };
        let mut named: BTreeSet<String> = BTreeSet::new();
        for arm in arms {
            let from = match starts(&arm.pattern, &names) {
                Some(Start::Cases(cases)) => {
                    named.extend(cases.iter().cloned());
                    cases
                }
                Some(Start::Rest) => names
                    .iter()
                    .filter(|name| !named.contains(**name))
                    .map(|name| (*name).to_owned())
                    .collect(),
                None => continue,
            };
            let mut to = BTreeSet::new();
            ends(&arm.body, &names, &mut to);
            for source in &from {
                for target in &to {
                    edges.push((source.clone(), target.clone(), candidate.name.clone()));
                }
            }
        }
    }
    if edges.is_empty() {
        return Err(DiagramError {
            kind: "diagram_unavailable",
            message: format!(
                "no function of type {} -> \u{2026} -> {} with a 'match' over its first parameter was found",
                union.id, union.id
            ),
        });
    }
    Ok(edges)
}

/// The edges merged by (source, target), in case order, with their functions joined: at most
/// `MAX_EDGES` of them, and how many more there are.
fn merged(
    order: &[&str],
    edges: &[(String, String, String)],
) -> (Vec<(usize, usize, String)>, usize) {
    let mut functions: BTreeMap<(usize, usize), BTreeSet<&str>> = BTreeMap::new();
    for (source, target, function) in edges {
        let position = |name: &str| order.iter().position(|case| *case == name);
        if let (Some(a), Some(b)) = (position(source), position(target)) {
            functions.entry((a, b)).or_default().insert(function);
        }
    }
    let mut merged: Vec<(usize, usize, String)> = functions
        .into_iter()
        .map(|((a, b), names)| {
            (
                a + 1,
                b + 1,
                names.into_iter().collect::<Vec<_>>().join(", "),
            )
        })
        .collect();
    let omitted = merged.len().saturating_sub(MAX_EDGES);
    merged.truncate(MAX_EDGES);
    (merged, omitted)
}

fn state_label(case: &Symbol) -> String {
    match &case.result {
        Some(payload) => format!("{} of {payload}", case.name),
        None => case.name.clone(),
    }
}

/// The state diagram, its number of cases, and whether transitions beyond `MAX_EDGES` were left
/// out.
pub fn mermaid(
    input: &Input,
    union: &str,
    edges: &[(String, String, String)],
    limits: &super::Limits,
) -> (String, usize, bool) {
    let cases = cases(input, union);
    let order: Vec<&str> = cases.iter().map(|case| case.name.as_str()).collect();
    let mut lines = vec!["stateDiagram-v2".to_owned(), "    direction LR".to_owned()];
    for (index, case) in cases.iter().enumerate() {
        lines.push(format!(
            "    state \"{}\" as s{}",
            esc(&state_label(case), limits.label),
            index + 1
        ));
    }
    let (merged, omitted) = merged(&order, edges);
    for (a, b, names) in merged {
        lines.push(format!(
            "    s{a} --> s{b} : {}",
            transition(&names, limits.edge)
        ));
    }
    if omitted > 0 {
        lines.push(format!("    %% omitted: {omitted} edges"));
    }
    (lines.join("\n") + "\n", cases.len(), omitted > 0)
}

pub fn dot(
    input: &Input,
    union: &str,
    edges: &[(String, String, String)],
    limits: &super::Limits,
) -> (String, usize, bool) {
    let cases = cases(input, union);
    let order: Vec<&str> = cases.iter().map(|case| case.name.as_str()).collect();
    let mut lines = vec![
        "digraph tsuzuri {".to_owned(),
        "    rankdir=LR;".to_owned(),
        "    node [shape=ellipse, fontname=\"monospace\"];".to_owned(),
        "    edge [fontname=\"monospace\"];".to_owned(),
    ];
    for (index, case) in cases.iter().enumerate() {
        lines.push(format!(
            "    s{} [label=\"{}\"];",
            index + 1,
            dot_esc(&state_label(case), limits.label)
        ));
    }
    let (merged, omitted) = merged(&order, edges);
    for (a, b, names) in merged {
        lines.push(format!(
            "    s{a} -> s{b} [label=\"{}\"];",
            dot_esc(&names, limits.edge)
        ));
    }
    if omitted > 0 {
        lines.push(format!("    // omitted: {omitted} edges"));
    }
    lines.push("}".to_owned());
    (lines.join("\n") + "\n", cases.len(), omitted > 0)
}
