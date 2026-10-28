//! `transitions` (G30, D4): the state diagram of a union whose cases a function maps to cases, from
//! the `match` over the function's first parameter in the source. The source must be the one the
//! graph was built from (its SHA-256 is in `manifest.json`).

use std::collections::{BTreeMap, BTreeSet};

use super::escape::{dot_esc, esc, transition};
use super::input::{Input, Symbol};
use super::{DiagramError, MAX_EDGES};
use crate::syntax::{Edition, Expr, ExprKind, Pattern, PatternKind};

/// Where `transitions` reads the sources: `render` never opens files itself (D14).
pub trait Sources {
    fn read(&self, relative_path: &str) -> Option<String>;
}

/// The sources that a project loaded, by the path that the graph's modules name: a dependency's is
/// its logical path (its namespace and file), not a file under the root. Nothing else is read.
pub struct ProjectSources<'a>(BTreeMap<String, &'a str>);

impl<'a> ProjectSources<'a> {
    pub fn of(project: &'a crate::driver::Project) -> Self {
        Self(
            project
                .sources
                .iter()
                .map(|source| {
                    (
                        crate::kg::build::relative(&source.relative_path),
                        source.text.as_str(),
                    )
                })
                .collect(),
        )
    }
}

impl Sources for ProjectSources<'_> {
    fn read(&self, relative_path: &str) -> Option<String> {
        self.0.get(relative_path).map(|text| (*text).to_owned())
    }
}

/// The sources of a test, by path.
impl Sources for BTreeMap<String, String> {
    fn read(&self, relative_path: &str) -> Option<String> {
        self.get(relative_path).cloned()
    }
}

/// A type's name: without a leading `ref ` or `ref mut ` and the reference's region
/// (`ref {r} U`), and up to its arguments (`U<T>`) or its own region (`U {r}`).
fn head(ty: &str) -> &str {
    let ty = ty.trim_start();
    let ty = ty
        .strip_prefix("ref mut ")
        .or_else(|| ty.strip_prefix("ref "))
        .unwrap_or(ty)
        .trim_start();
    let ty = match ty.strip_prefix('{') {
        Some(rest) => rest
            .split_once('}')
            .map_or(rest, |(_, rest)| rest)
            .trim_start(),
        None => ty,
    };
    let end = ty.find(['<', ' ', '{']).unwrap_or(ty.len());
    &ty[..end]
}

/// The last word of a name: after its last `.` or `:`.
fn last_word(text: &str) -> &str {
    text.rsplit(['.', ':']).next().unwrap_or(text)
}

/// The `match`es over the parameter `name` that a function's body returns: the body's own, or
/// those of both branches of an `if`. A block whose local takes the name hides the parameter.
fn find_matches<'a>(body: &'a Expr, name: &str, found: &mut Vec<&'a [crate::syntax::MatchArm]>) {
    match &body.kind {
        ExprKind::Match { value, arms, .. } => {
            if matches!(&value.kind, ExprKind::Name(id) if id.text == name) {
                found.push(arms);
            }
        }
        ExprKind::Block { bindings, result } => {
            if bindings.iter().all(|binding| binding.name.text != name) {
                find_matches(result, name, found);
            }
        }
        ExprKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            find_matches(then_branch, name, found);
            find_matches(else_branch, name, found);
        }
        ExprKind::Checked(inner) => find_matches(inner, name, found),
        _ => {}
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

/// The case that `expr` names: `Red`, or through its union (`Light.Red`, `Util::Light.Red`).
fn case_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Name(id) => Some(last_word(&id.text)),
        ExprKind::Field(base, id) if matches!(base.kind, ExprKind::Name(_)) => Some(&id.text),
        _ => None,
    }
}

/// The cases an arm's body ends in.
fn ends(body: &Expr, cases: &[&str], out: &mut BTreeSet<String>) {
    match &body.kind {
        ExprKind::Name(_) | ExprKind::Field(..) => {
            if let Some(name) = case_name(body)
                && cases.contains(&name)
            {
                out.insert(name.to_owned());
            }
        }
        ExprKind::Call(callee, _) => {
            if let Some(name) = case_name(callee)
                && cases.contains(&name)
            {
                out.insert(name.to_owned());
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

/// The full names of a union, with `::` read as `.`: its ID, and its module's when it is named after
/// its module (`Util::Light` of `Util::Light.Light`, which the source writes as the module).
fn full_names(union: &Symbol) -> Vec<String> {
    let mut names = vec![union.id.replace("::", ".")];
    if union.module.rsplit("::").next() == Some(union.name.as_str()) {
        names.push(union.module.replace("::", "."));
    }
    names
}

/// Whether `ty`, written in the signature of `function`, names `union`. The written name, with
/// `::` read as `.`, must be one of the union's full names or end it (`Shapes.Shape` of
/// `Acme::Kg::Shapes.Shape`). A name that several unions end with means the one of the function's
/// module, else the only one of the function's namespace; otherwise it is ambiguous and names none.
fn names_union(input: &Input, function: &Symbol, ty: &str, union: &Symbol) -> bool {
    let written = head(ty).replace("::", ".");
    let ends = |symbol: &Symbol| {
        full_names(symbol)
            .iter()
            .any(|name| *name == written || name.ends_with(&format!(".{written}")))
    };
    if !ends(union) {
        return false;
    }
    let named: Vec<&Symbol> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == "union" && ends(symbol))
        .collect();
    if named.len() == 1 {
        return true;
    }
    if named.iter().any(|symbol| symbol.module == function.module) {
        return union.module == function.module;
    }
    // The whole enclosing namespace: `Acme::App` of `Acme::App::Main`, not `Acme`.
    let namespace = |module: &str| {
        module
            .rsplit_once("::")
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
        let mut matches = Vec::new();
        find_matches(&function.body, &parameter.name.text, &mut matches);
        // Each `match` names its own cases before its wildcard.
        for arms in matches {
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
