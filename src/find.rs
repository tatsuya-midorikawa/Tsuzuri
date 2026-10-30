//! Typed holes and name suggestions (G25). A hole `?` or `?name` in an expression is always an
//! error (`E1030`, kind `typed_hole`) that reports the type expected at its position, the locals
//! in scope, and the declarations whose result fits there. The checker recovers from it like from
//! any other error of an expression, so a hole never reaches the typed IR. An unresolved name
//! (`E1002`, `E1004`, `E1007`, `E1020`) gets the names near it as candidates.
//!
//! The index of declarations (`Entry`) is built once per check, on the first hole, from the
//! checker's own tables: the `def`s and `const`s with their schemes, the builtins with the type a
//! throwaway checker gives them, the fields of records and the cases of unions. `tsuzuri find`
//! searches the knowledge graph instead (`kg::find`, D-60).

use super::polymorph::{self, Classes, Inference, Scheme};
use super::*;
use crate::diagnostic::{Candidate, Fix};

/// The module whose point of view the index takes for a throwaway checker of a builtin.
pub const REQUESTER: &str = "Main";
/// The entries that one hole examines at most.
pub const MAX_ENTRIES_EXAMINED: usize = 5_000;
/// The holes of one function (one checker) that get candidates.
pub const MAX_HOLE_SEARCHES: usize = 16;
/// The longest candidate text; a longer one is dropped.
const MAX_CANDIDATE_TEXT: usize = 200;
/// The locals that the `scope` of a hole lists at most (D-51).
const MAX_SCOPE_ENTRIES: usize = 32;

impl Checker<'_> {
    /// The `E1030` of a hole: the expected type if it is known, the visible locals (those that
    /// fit the type first), and the candidates of the first `MAX_HOLE_SEARCHES` holes.
    pub(super) fn typed_hole(
        &mut self,
        name: Option<&Ident>,
        expected: Option<&Type>,
        span: Span,
    ) -> Diagnostic {
        let wanted = expected
            .map(|ty| self.inference.resolve(ty))
            .filter(|ty| !matches!(ty, Type::Infer(_)) && !ty.contains_error());
        let label = name.map_or_else(|| "'?'".to_owned(), |name| format!("'?{}'", name.text));
        let (message, help, shown) = match &wanted {
            Some(ty) => {
                let shown = self.hole_type(ty);
                (
                    format!(
                        "typed hole {label} of type '{shown}'; replace it with an expression of that type"
                    ),
                    format!("fill the hole with a value of type '{shown}'"),
                    shown,
                )
            }
            None => (
                format!("typed hole {label} of unknown type; replace it with an expression"),
                "fill the hole with an expression; its type is not known here".to_owned(),
                "_".to_owned(),
            ),
        };
        let scope = self.hole_scope(wanted.as_ref());
        let entries = scope
            .iter()
            .map(|(name, ty)| ScopeEntry {
                name: name.clone(),
                ty: self.hole_type(ty),
            })
            .collect();
        let mut diagnostic = Diagnostic::of(DiagnosticKind::TypedHole, message, span)
            .with_help(help)
            .with_scope(entries);
        diagnostic.detail.expected = Some(shown);
        if let Some(wanted) = &wanted
            && self.holes_searched < MAX_HOLE_SEARCHES
        {
            self.holes_searched += 1;
            let (candidates, fixes) = self.hole_candidates(wanted, &scope, name, span);
            diagnostic = diagnostic.with_candidates(candidates);
            for fix in fixes {
                diagnostic = diagnostic.fix(fix.title, fix.applicability, fix.edits);
            }
        }
        diagnostic
    }

    /// A type as a hole's diagnostic shows it: the open type of a number literal is its default
    /// (`i64`), and what inference has not decided, or got wrong, is `_`.
    pub(super) fn hole_type(&self, ty: &Type) -> String {
        let ty = self.inference.resolve(ty);
        polymorph::map_type(&ty, &mut |t| match t {
            Type::Infer(_) => self
                .inference
                .numeric_default(t)
                .unwrap_or_else(|| Type::Handle("_".into())),
            Type::Error => Type::Handle("_".into()),
            _ => t.clone(),
        })
        .display(&self.types)
    }

    /// The user's locals that the hole sees, an inner one hiding an outer one of the same name:
    /// those whose type unifies with `wanted` first, each group in declaration order.
    fn hole_scope(&self, wanted: Option<&Type>) -> Vec<(String, Type)> {
        let mut seen = BTreeSet::new();
        let mut locals: Vec<&Local> = Vec::new();
        for frame in self.scopes.iter().rev() {
            for (local_name, local) in frame {
                if local.provenance == Provenance::User
                    && !local_name.starts_with('$')
                    && seen.insert(local_name.as_str())
                {
                    locals.push(local);
                }
            }
        }
        locals.sort_by_key(|local| local.id);
        let (first, rest): (Vec<&Local>, Vec<&Local>) = locals
            .into_iter()
            .partition(|local| wanted.is_some_and(|wanted| self.fits(&local.ty, wanted)));
        first
            .into_iter()
            .chain(rest)
            .take(MAX_SCOPE_ENTRIES)
            .map(|local| (local.name.clone(), self.inference.resolve(&local.ty)))
            .collect()
    }

    /// Whether a value of type `ty` can stand where `wanted` is expected, on a copy of both
    /// types: nothing of the checker's inference changes.
    fn fits(&self, ty: &Type, wanted: &Type) -> bool {
        let mut search = Inference::default();
        let mut map = BTreeMap::new();
        let goal = import_type(&self.inference.resolve(wanted), &mut search, &mut map);
        let ty = import_type(&self.inference.resolve(ty), &mut search, &mut map);
        search
            .unify(&ty, &goal, &self.types, Span::default())
            .is_ok()
    }
}

/// `ty` for a search's own `Inference`: each `Infer(k)` becomes one new variable (the same for
/// the same `k`, through `map`), and each `Error` a new variable of its own.
pub(super) fn import_type(
    ty: &Type,
    fresh: &mut Inference,
    map: &mut BTreeMap<usize, Type>,
) -> Type {
    polymorph::map_type(ty, &mut |t| match t {
        Type::Infer(k) => map.entry(*k).or_insert_with(|| fresh.fresh()).clone(),
        Type::Error => fresh.fresh(),
        _ => t.clone(),
    })
}

/// What an entry of the index declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Def,
    Const,
    Builtin,
    Field,
    Case,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Def => "def",
            Self::Const => "const",
            Self::Builtin => "builtin",
            Self::Field => "field",
            Self::Case => "case",
        }
    }
}

/// One declaration with its type, in the form that holes search.
#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Kind,
    /// The name as source code writes it from the root namespace.
    pub name: String,
    /// The key of the owning module (the part before the first `.` for a builtin).
    pub module: String,
    pub public: bool,
    pub origin: ModuleOrigin,
    /// The function id of a `def` or `const`.
    pub id: Option<usize>,
    pub variables: Vec<String>,
    pub parameters: Vec<Type>,
    pub result: Type,
    /// `(class, type)` pairs over `variables`.
    pub constraints: Vec<(usize, Type)>,
    /// `Numeric<'a> => ref ['a] -> 'a`
    pub text: String,
}

impl Entry {
    pub fn visible_from(&self, requester: &str) -> bool {
        self.public || self.module == requester
    }
}

fn render(
    types: &TypeContext<'_>,
    classes: &Classes,
    parameters: &[Type],
    result: &Type,
    constraints: &[(usize, Type)],
) -> String {
    let function = if parameters.is_empty() {
        result.display(types)
    } else {
        Type::function(parameters.to_vec(), result.clone()).display(types)
    };
    if constraints.is_empty() {
        return function;
    }
    let prefix: Vec<String> = constraints
        .iter()
        .map(|(class, ty)| {
            format!(
                "{}<{}>",
                type_display(classes.declared_name(*class)),
                ty.display(types)
            )
        })
        .collect();
    format!("{} => {function}", prefix.join(", "))
}

/// The index of every declaration that a hole may use, in the order of the checker's tables.
pub(super) fn build(
    names: &Names,
    types: TypeContext<'_>,
    signatures: &[Scheme],
    classes: &Classes,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (key, info) in &names.functions {
        let scheme = &signatures[info.id];
        if key.contains('$') || key.contains("__") || scheme.is_poisoned() {
            continue;
        }
        let constraints: Vec<(usize, Type)> = scheme
            .constraints
            .iter()
            .map(|constraint| (constraint.class, constraint.ty.clone()))
            .collect();
        let kind = if names.constants.contains(&info.id) {
            Kind::Const
        } else {
            Kind::Def
        };
        entries.push(Entry {
            kind,
            name: declaration_display(key),
            module: info.module.clone(),
            public: info.visibility == Visibility::Public,
            origin: names.origin(&info.module),
            id: Some(info.id),
            variables: scheme.variables.clone(),
            parameters: scheme.signature.parameters.clone(),
            result: scheme.signature.result.clone(),
            text: render(
                &types,
                classes,
                &scheme.signature.parameters,
                &scheme.signature.result,
                &constraints,
            ),
            constraints,
        });
    }
    for builtin in Builtin::ALL {
        let name = builtin.name();
        if name.contains('$') || name.contains("__") {
            continue;
        }
        // A throwaway checker: `names`, `types` and `signatures` are shared references, so nothing
        // of the checker that is running changes. Builtins private to a std module fail here.
        let mut checker = Checker::new(REQUESTER, names, types, signatures, classes);
        let Ok((_, ty)) = checker.builtin(*builtin, Span::default()) else {
            continue;
        };
        // A result that a type family computes from an argument (`Int.abs_diff`) is not a
        // variable that unification could bind to anything; the index leaves the builtin out.
        if !checker.families.is_empty() {
            continue;
        }
        let mut letters: BTreeMap<usize, String> = BTreeMap::new();
        let mut letter = |k: usize| -> String {
            let next = letters.len();
            letters
                .entry(k)
                .or_insert_with(|| ((b'a' + (next % 26) as u8) as char).to_string())
                .clone()
        };
        let named = polymorph::map_type(&ty, &mut |t| match t {
            Type::Infer(k) => Type::Variable(letter(*k)),
            _ => t.clone(),
        });
        let constraints: Vec<(usize, Type)> = checker
            .constraints
            .iter()
            .map(|constraint| {
                let ty = polymorph::map_type(
                    &checker.inference.resolve(&constraint.ty),
                    &mut |t| match t {
                        Type::Infer(k) => Type::Variable(letter(*k)),
                        _ => t.clone(),
                    },
                );
                (constraint.class, ty)
            })
            .collect();
        let (parameters, result) = match named {
            Type::Function(parameters, result) => (parameters.types, *result),
            other => (Vec::new(), other),
        };
        entries.push(Entry {
            kind: Kind::Builtin,
            name: name.to_owned(),
            module: name
                .split_once('.')
                .map_or_else(String::new, |(prefix, _)| prefix.to_owned()),
            public: true,
            origin: ModuleOrigin::Std,
            id: None,
            variables: polymorph::variables(&Type::function(parameters.clone(), result.clone())),
            text: render(&types, classes, &parameters, &result, &constraints),
            parameters,
            result,
            constraints,
        });
    }
    for (key, info) in &names.records {
        if key.contains('$') {
            continue;
        }
        let record = &types.records[info.id];
        let arguments: Box<[Type]> = record
            .parameters
            .iter()
            .map(|parameter| Type::Variable(parameter.clone()))
            .collect();
        let whole = Type::Record(info.id, arguments);
        for (field, ty) in &record.fields {
            entries.push(Entry {
                kind: Kind::Field,
                name: format!("{}.{field}", type_display(key)),
                module: info.module.clone(),
                public: info.visibility == Visibility::Public,
                origin: names.origin(&info.module),
                id: None,
                variables: record.parameters.clone(),
                parameters: vec![whole.clone()],
                result: ty.clone(),
                constraints: Vec::new(),
                text: render(&types, classes, std::slice::from_ref(&whole), ty, &[]),
            });
        }
    }
    for (key, case) in &names.cases {
        let union = &types.unions[case.info.id];
        let arguments: Box<[Type]> = union
            .parameters
            .iter()
            .map(|parameter| Type::Variable(parameter.clone()))
            .collect();
        let whole = Type::Union(case.info.id, arguments);
        let parameters: Vec<Type> = union.cases[case.case].1.clone().into_iter().collect();
        entries.push(Entry {
            kind: Kind::Case,
            name: declaration_display(key),
            module: case.info.module.clone(),
            public: case.info.visibility == Visibility::Public,
            origin: names.origin(&case.info.module),
            id: None,
            variables: union.parameters.clone(),
            text: render(&types, classes, &parameters, &whole, &[]),
            parameters,
            result: whole,
            constraints: Vec::new(),
        });
    }
    entries
}

/// Whether `ty` has no unresolved variable, so that a class constraint over it can be decided.
pub(super) fn ground(ty: &Type) -> bool {
    let mut ground = true;
    polymorph::map_type(ty, &mut |t| {
        if matches!(t, Type::Infer(_) | Type::Variable(_) | Type::Error) {
            ground = false;
        }
        t.clone()
    });
    ground
}

/// The entry's parameters, result and constraints with its variables replaced by new variables of
/// `search`.
pub(super) fn instantiate(
    entry: &Entry,
    search: &mut Inference,
) -> (Vec<Type>, Type, Vec<(usize, Type)>) {
    let substitutions: BTreeMap<String, Type> = entry
        .variables
        .iter()
        .map(|variable| (variable.clone(), search.fresh()))
        .collect();
    (
        entry
            .parameters
            .iter()
            .map(|ty| polymorph::substitute(ty, &substitutions))
            .collect(),
        polymorph::substitute(&entry.result, &substitutions),
        entry
            .constraints
            .iter()
            .map(|(class, ty)| (*class, polymorph::substitute(ty, &substitutions)))
            .collect(),
    )
}

/// Whether every constraint whose type `search` has decided holds; one with a variable left may
/// still hold.
pub(super) fn constraints_hold(
    constraints: &[(usize, Type)],
    search: &Inference,
    types: &TypeContext<'_>,
    classes: &Classes,
) -> bool {
    constraints.iter().all(|(class, ty)| {
        let resolved = search.resolve(ty);
        !ground(&resolved) || classes.holds_class(*class, &resolved, types)
    })
}

/// A candidate before the cut: its sort key and what the diagnostic shows.
struct Ranked {
    rank: u8,
    generic: bool,
    unfilled: usize,
    name: String,
    candidate: Candidate,
}

impl Checker<'_> {
    /// The candidates of a hole of type `wanted` (known), best first, and a `maybe` fix when one
    /// local alone fits. `scope` is the hole's `scope`.
    fn hole_candidates(
        &self,
        wanted: &Type,
        scope: &[(String, Type)],
        name: Option<&Ident>,
        hole: Span,
    ) -> (Vec<Candidate>, Vec<Fix>) {
        let span = Span::default();
        let mut ranked: Vec<Ranked> = Vec::new();
        let fits = |ty: &Type| {
            let mut search = Inference::default();
            let mut map = BTreeMap::new();
            let goal = import_type(wanted, &mut search, &mut map);
            let ty = import_type(ty, &mut search, &mut map);
            search.unify(&ty, &goal, &self.types, span).is_ok()
        };
        // Rank 0: a local of the type. Rank 1: a field of a local record.
        for (local_name, local_type) in scope {
            if fits(local_type) {
                ranked.push(Ranked {
                    rank: 0,
                    generic: false,
                    unfilled: 0,
                    name: local_name.clone(),
                    candidate: Candidate {
                        name: local_name.clone(),
                        signature: format!("{local_name}: {}", self.hole_type(local_type)),
                        how: "value".to_owned(),
                    },
                });
            }
            let record = match local_type {
                Type::Reference(inner, _) => inner.as_ref(),
                other => other,
            };
            if let Type::Record(id, arguments) = record {
                let record = &self.types.records[*id];
                let fields = self.types.record_fields(*id, arguments);
                for ((field, _), ty) in record.fields.iter().zip(fields) {
                    if fits(&ty) {
                        let text = format!("{local_name}.{field}");
                        ranked.push(Ranked {
                            rank: 1,
                            generic: false,
                            unfilled: 0,
                            name: text.clone(),
                            candidate: Candidate {
                                signature: format!("{text}: {}", self.hole_type(&ty)),
                                name: text,
                                how: "value".to_owned(),
                            },
                        });
                    }
                }
            }
        }
        let entries = self
            .names
            .search_index
            .get_or_init(|| build(self.names, self.types, self.signatures, self.classes));
        let mut examined = 0;
        for entry in entries {
            if !entry.visible_from(self.module) || (entry.id.is_some() && entry.id == self.function)
            {
                continue;
            }
            examined += 1;
            if examined > MAX_ENTRIES_EXAMINED {
                break;
            }
            let mut search = Inference::default();
            let (parameters, result, constraints) = instantiate(entry, &mut search);
            let mut map = BTreeMap::new();
            let goal = import_type(wanted, &mut search, &mut map);
            if search.unify(&result, &goal, &self.types, span).is_err()
                || !constraints_hold(&constraints, &search, &self.types, self.classes)
            {
                continue;
            }
            let mut filled: Vec<Option<String>> = Vec::new();
            for parameter in &parameters {
                let parameter = search.resolve(parameter);
                let mut hit = None;
                for (local_name, local_type) in scope {
                    // A local whose type is still open fits anything; it fills no argument.
                    if matches!(local_type, Type::Infer(_)) {
                        continue;
                    }
                    let mut trial = search.clone();
                    let mut local_map = map.clone();
                    let local_type = import_type(local_type, &mut trial, &mut local_map);
                    let mutable = self.local(local_name).is_some_and(|local| local.mutable);
                    let text = match (&parameter, &local_type) {
                        (
                            Type::Reference(inner, wants_mut),
                            Type::Reference(local_inner, local_mut),
                        ) => (wants_mut == local_mut
                            && trial.unify(local_inner, inner, &self.types, span).is_ok())
                        .then(|| local_name.clone()),
                        (Type::Reference(inner, wants_mut), _) => ((!*wants_mut || mutable)
                            && trial.unify(&local_type, inner, &self.types, span).is_ok())
                        .then(|| {
                            let borrow = if *wants_mut { "ref mut" } else { "ref" };
                            format!("({borrow} {local_name})")
                        }),
                        _ => trial
                            .unify(&local_type, &parameter, &self.types, span)
                            .is_ok()
                            .then(|| local_name.clone()),
                    };
                    // A local after which a constraint fails (`Int.count_ones xs` of `[i64]`)
                    // does not fit.
                    if text.is_some()
                        && constraints_hold(&constraints, &trial, &self.types, self.classes)
                    {
                        search = trial;
                        map = local_map;
                        hit = text;
                        break;
                    }
                }
                filled.push(hit);
            }
            let unfilled = filled.iter().filter(|fill| fill.is_none()).count();
            let rank = if parameters.is_empty() {
                4
            } else if unfilled == 0 {
                2
            } else if unfilled < filled.len() {
                3
            } else {
                5
            };
            let mut text = entry.name.clone();
            for fill in &filled {
                text.push(' ');
                text.push_str(fill.as_deref().unwrap_or("?"));
            }
            if text.len() > MAX_CANDIDATE_TEXT {
                continue;
            }
            ranked.push(Ranked {
                rank,
                generic: matches!(entry.result, Type::Variable(_)),
                unfilled,
                name: entry.name.clone(),
                candidate: Candidate {
                    name: text,
                    signature: format!("{} :: {}", entry.name, entry.text),
                    how: if parameters.is_empty() {
                        "value"
                    } else {
                        "call"
                    }
                    .to_owned(),
                },
            });
        }
        ranked.sort_by(|a, b| {
            (a.rank, a.generic, a.unfilled, &a.name, &a.candidate.name).cmp(&(
                b.rank,
                b.generic,
                b.unfilled,
                &b.name,
                &b.candidate.name,
            ))
        });
        let mut seen = BTreeSet::new();
        ranked.retain(|item| seen.insert(item.candidate.name.clone()));
        let locals: Vec<&Ranked> = ranked.iter().filter(|item| item.rank == 0).collect();
        let fixes = match locals.as_slice() {
            [only] => {
                let label = name.map_or_else(|| "?".to_owned(), |name| format!("?{}", name.text));
                vec![Fix {
                    title: format!("Replace '{label}' with '{}'", only.candidate.name),
                    applicability: Applicability::Maybe,
                    edits: vec![Edit {
                        span: hole,
                        text: only.candidate.name.clone(),
                    }],
                }]
            }
            _ => Vec::new(),
        };
        let candidates = ranked
            .into_iter()
            .take(crate::diagnostic::MAX_CANDIDATES)
            .map(|item| item.candidate)
            .collect();
        (candidates, fixes)
    }
}

/// Damerau-Levenshtein distance (optimal string alignment): insert, delete, replace, and swap of
/// two neighbors each cost 1.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

/// The most edits that still count as similar: 1 for a name of up to 4 characters, else 2.
fn limit(target: &str) -> usize {
    if target.chars().count() <= 4 { 1 } else { 2 }
}

/// A name that an unresolved name may have meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pooled {
    /// The text to write in place of the target.
    pub text: String,
    /// The name that is compared with the target (the last segment of a qualified `text`).
    pub name: String,
    /// 0 a local, 1 a function, constant or builtin, 2 a case or active pattern, 3 a module,
    /// 4 a type or field.
    pub group: u8,
    /// What the candidate's `signature` says: the local's type, or the kind of name.
    pub signature: String,
}

impl Pooled {
    fn new(text: impl Into<String>, group: u8, signature: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            name: text.clone(),
            text,
            group,
            signature: signature.into(),
        }
    }

    /// `text` qualifies `name`: the two are at distance 0.
    fn qualified(text: String, name: &str, group: u8, signature: &str) -> Self {
        Self {
            text,
            name: name.to_owned(),
            group,
            signature: signature.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Suggestion {
    pub text: String,
    /// 0 when `name` equals the target and `text` only qualifies it.
    pub distance: usize,
    pub group: u8,
    pub signature: String,
}

/// The names of `pool` near `target`, best first: by distance, group and text, at most 10.
pub(super) fn suggest(target: &str, pool: &[Pooled]) -> Vec<Suggestion> {
    let mut found: Vec<Suggestion> = Vec::new();
    for item in pool {
        if item.text == target {
            continue;
        }
        let distance = if item.name == target {
            0
        } else if target.chars().count() < 2 {
            continue;
        } else {
            let distance = distance(target, &item.name);
            if distance > limit(target) {
                continue;
            }
            distance
        };
        found.push(Suggestion {
            text: item.text.clone(),
            distance,
            group: item.group,
            signature: item.signature.clone(),
        });
    }
    found.sort_by(|a, b| (a.distance, a.group, &a.text).cmp(&(b.distance, b.group, &b.text)));
    found.dedup_by(|a, b| a.text == b.text);
    found.truncate(crate::diagnostic::MAX_CANDIDATES);
    found
}

/// The one text a `maybe` fix may write: the only suggestion, or one that is strictly closer
/// than the next.
pub(super) fn best(found: &[Suggestion]) -> Option<&Suggestion> {
    match found {
        [only] => Some(only),
        [first, second, ..] if (first.distance, first.group) < (second.distance, second.group) => {
            Some(first)
        }
        _ => None,
    }
}

/// `diagnostic` with the names of `pool` near `target`: the candidates, a `help` that replaces
/// the one it had, and a `maybe` fix that writes the best name over `replace` when one is best.
/// Without a near name the diagnostic does not change.
pub(super) fn with_suggestions(
    diagnostic: Diagnostic,
    target: &str,
    replace: Span,
    pool: &[Pooled],
) -> Diagnostic {
    let found = suggest(target, pool);
    if found.is_empty() {
        return diagnostic;
    }
    let candidates = found
        .iter()
        .map(|suggestion| Candidate {
            name: suggestion.text.clone(),
            signature: suggestion.signature.clone(),
            how: if suggestion.group == 0 {
                "local"
            } else if suggestion.distance == 0 {
                "qualify"
            } else {
                "similar"
            }
            .to_owned(),
        })
        .collect();
    let best = best(&found);
    let help = match best {
        Some(best) => format!("did you mean '{}'?", best.text),
        None => {
            let names: Vec<String> = found
                .iter()
                .take(3)
                .map(|suggestion| format!("'{}'", suggestion.text))
                .collect();
            format!("did you mean one of: {}?", names.join(", "))
        }
    };
    let diagnostic = diagnostic.with_candidates(candidates).with_help(help);
    match best {
        Some(best) if !target.contains(['.', ':']) => diagnostic.fix(
            format!("Replace '{target}' with '{}'", best.text),
            Applicability::Maybe,
            vec![Edit {
                span: replace,
                text: best.text.clone(),
            }],
        ),
        _ => diagnostic,
    }
}

/// The word of a function's candidate signature.
fn function_word(names: &Names, id: usize) -> &'static str {
    if names.constants.contains(&id) {
        "constant"
    } else {
        "function"
    }
}

fn last_segment(key: &str) -> &str {
    key.rsplit('.').next().unwrap_or(key)
}

/// A name that the index and suggestions leave out: generated or internal.
fn hidden(name: &str) -> bool {
    name.contains('$') || name.contains("__")
}

impl Checker<'_> {
    /// The pool of `unknown value 'target'`: the locals, this module's functions, the unqualified
    /// builtins and cases, the modules for a capitalized name, and every visible function, case or
    /// builtin whose last part is `target`, qualified.
    pub(super) fn value_pool(&self, target: &str) -> Vec<Pooled> {
        let mut pool = Vec::new();
        let mut seen = BTreeSet::new();
        for frame in self.scopes.iter().rev() {
            for (name, local) in frame {
                if local.provenance == Provenance::User
                    && !name.starts_with('$')
                    && name != "_"
                    && seen.insert(name.as_str())
                {
                    pool.push(Pooled::new(
                        name.clone(),
                        0,
                        format!("{name}: {}", self.hole_type(&local.ty)),
                    ));
                }
            }
        }
        for (key, info) in &self.names.functions {
            if hidden(key) {
                continue;
            }
            let word = function_word(self.names, info.id);
            if info.module == self.module {
                pool.push(Pooled::new(last_segment(key), 1, word));
            } else if last_segment(key) == target && info.visible_from(self.module) {
                pool.push(Pooled::qualified(declaration_display(key), target, 1, word));
            }
        }
        for builtin in Builtin::ALL {
            let name = builtin.name();
            if hidden(name) {
                continue;
            }
            match name.split_once('.') {
                None => pool.push(Pooled::new(name, 1, "builtin")),
                Some((_, last)) if last == target => {
                    pool.push(Pooled::qualified(name.to_owned(), target, 1, "builtin"));
                }
                Some(_) => {}
            }
        }
        for alias in self.names.case_aliases.keys() {
            pool.push(Pooled::new(alias.clone(), 2, "union case"));
        }
        for (key, case) in &self.names.cases {
            if last_segment(key) == target
                && case.info.module != self.module
                && case.info.visible_from(self.module)
            {
                pool.push(Pooled::qualified(
                    declaration_display(key),
                    target,
                    2,
                    "union case",
                ));
            }
        }
        if target.starts_with(|c: char| c.is_ascii_uppercase()) {
            for key in self.names.modules.keys() {
                if self.names.searchable(self.module, key) {
                    pool.push(Pooled::new(module_stem(key), 3, "module"));
                }
            }
            for builtin in Builtin::ALL {
                if let Some((prefix, _)) = builtin.name().split_once('.') {
                    pool.push(Pooled::new(prefix, 3, "module"));
                }
            }
        }
        pool
    }

    /// The pool of `module 'M' has no function or union case 'x'`: the members of `M`.
    pub(super) fn member_pool(&self, spelled: &str) -> Vec<Pooled> {
        let mut pool = Vec::new();
        if let Some(key) = self.names.module_path(self.module, spelled) {
            for (name, info) in &self.names.functions {
                if info.module == key && !hidden(name) && info.visible_from(self.module) {
                    pool.push(Pooled::new(
                        last_segment(name),
                        1,
                        function_word(self.names, info.id),
                    ));
                }
            }
            for (name, case) in &self.names.cases {
                if case.info.module == key && case.info.visible_from(self.module) {
                    pool.push(Pooled::new(last_segment(name), 2, "union case"));
                }
            }
        }
        let prefix = format!("{spelled}.");
        for builtin in Builtin::ALL {
            if let Some(member) = builtin.name().strip_prefix(&prefix)
                && !hidden(member)
            {
                pool.push(Pooled::new(member, 1, "builtin"));
            }
        }
        pool
    }

    /// The pool of `unknown function 'n'`: the visible functions, cases and qualified builtins by
    /// their whole names.
    pub(super) fn function_pool(&self) -> Vec<Pooled> {
        let mut pool = Vec::new();
        for (key, info) in &self.names.functions {
            if !hidden(key) && info.visible_from(self.module) {
                pool.push(Pooled::new(
                    declaration_display(key),
                    1,
                    function_word(self.names, info.id),
                ));
            }
        }
        for (key, case) in &self.names.cases {
            if case.info.visible_from(self.module) {
                pool.push(Pooled::new(declaration_display(key), 2, "union case"));
            }
        }
        for builtin in Builtin::ALL {
            let name = builtin.name();
            if name.contains('.') && !hidden(name) {
                pool.push(Pooled::new(name, 1, "builtin"));
            }
        }
        pool
    }

    /// The pool of an unknown field of record `id`: its fields.
    pub(super) fn field_pool(&self, id: usize) -> Vec<Pooled> {
        self.types.records[id]
            .fields
            .iter()
            .map(|(name, _)| Pooled::new(name.clone(), 4, "field"))
            .collect()
    }

    /// The pool of `unknown union case or active pattern 'c'`: the cases and active patterns.
    pub(super) fn pattern_pool(&self) -> Vec<Pooled> {
        let cases = self
            .names
            .case_aliases
            .keys()
            .map(|alias| Pooled::new(alias.clone(), 2, "union case"));
        let actives = self
            .names
            .active_aliases
            .keys()
            .map(|alias| Pooled::new(alias.clone(), 2, "active pattern"));
        cases.chain(actives).collect()
    }
}

impl Names {
    /// The pool of `unknown record, union, or type alias 'n'`: the type names and the primitive
    /// types.
    pub(super) fn type_pool(&self) -> Vec<Pooled> {
        let declared = self
            .record_aliases
            .keys()
            .chain(self.union_aliases.keys())
            .chain(self.type_alias_names.keys())
            .chain(self.handle_aliases.keys())
            .map(String::as_str);
        let primitive = crate::numeric::NUMERIC_NAMES.into_iter().chain([
            "bool",
            "unit",
            "string",
            "utf8string",
            "char",
            "utf8char",
            "bigint",
        ]);
        declared
            .chain(primitive)
            .map(|name| Pooled::new(name, 4, "type"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pooled(text: &str, group: u8) -> Pooled {
        let name = text.rsplit('.').next().unwrap().to_owned();
        Pooled {
            text: text.to_owned(),
            name,
            group,
            signature: String::new(),
        }
    }

    #[test]
    fn distances_are_damerau_levenshtein() {
        assert_eq!(distance("cnt", "count"), 2);
        assert_eq!(distance("lenght", "length"), 1);
        assert_eq!(distance("Strng", "string"), 2);
        assert_eq!(distance("Circl", "Circle"), 1);
        assert_eq!(distance("abc", "abc"), 0);
        assert_eq!(distance("", "ab"), 2);
        assert_eq!(limit("cnt"), 1);
        assert_eq!(limit("count"), 2);
    }

    #[test]
    fn qualified_names_come_first_and_win_when_alone() {
        let pool = vec![
            pooled("Array.length", 1),
            pooled("String.length", 1),
            pooled("count", 0),
        ];
        let found = suggest("length", &pool);
        assert_eq!(
            found.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            ["Array.length", "String.length"]
        );
        assert!(best(&found).is_none());
        let found = suggest("length", &pool[..1]);
        assert_eq!(best(&found).unwrap().text, "Array.length");
    }

    #[test]
    fn near_names_are_limited_by_length_and_ranked() {
        let pool = vec![
            pooled("count", 0),
            pooled("cnt2", 1),
            pooled("counts", 1),
            pooled("cut", 1),
        ];
        let found = suggest("cnt", &pool);
        assert_eq!(
            found
                .iter()
                .map(|s| (s.text.as_str(), s.distance))
                .collect::<Vec<_>>(),
            [("cnt2", 1), ("cut", 1)]
        );
        assert!(suggest("cnt", &pool[..1]).is_empty());
        let found = suggest("countt", &pool);
        assert_eq!(found[0].text, "count");
        assert_eq!(best(&found).unwrap().text, "count");
        let found = suggest("x", &[pooled("y", 0), pooled("M.x", 1)]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "M.x");
    }
}
