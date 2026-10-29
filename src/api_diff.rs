//! `tsuzuri api-diff OLD NEW` (G19 Phase 2): the differences between the public API of two
//! versions of a package or project, each marked as breaking or not.
//!
//! By default an API is read from the parsed declarations of the root package, the way
//! `tsuzuri doc` renders them: neither version is type checked, dependencies are not loaded,
//! and a type alias is not resolved. The opt-in `--effective` mode analyzes both projects and
//! compares resolved types, parameter modes, pure stages and checked region contracts without
//! including dependency declarations in the report. Analysis errors never fall back to the
//! syntax-only mode. The names of type variables
//! and regions do not matter, nor does the number of parameters that a definition's lambda
//! takes. A declaration is identified by its module's full name and its own name, so a package
//! whose namespace changes loses every declaration and gains them under the new name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::check::semantic::{ResolvedApi, SemanticIndex, api_key};
use crate::check::{Asserts, ModuleOrigin};
use crate::diagnostic::{Diagnostic, Span, json_string};
use crate::docgen::{
    constraints_text, json_text, parameters_text, region_text, signature, type_text,
};
use crate::driver::{Project, SourceFile};
use crate::package::Version;
use crate::syntax::*;

/// The most changes that a report lists, breaking ones first. The counts include every change.
pub const MAX_LISTED_CHANGES: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Removed,
    Added,
    Changed,
    Deprecated,
    Undeprecated,
    Documented,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Removed => "removed",
            Self::Added => "added",
            Self::Changed => "changed",
            Self::Deprecated => "deprecated",
            Self::Undeprecated => "undeprecated",
            Self::Documented => "documented",
        }
    }
}

/// One difference. `kind` names what changed (`def`, `record`, `field`, `case`, ...) and `name`
/// is its qualified name, as `Acme::Shapes.Circle.radius`. `old` and `new` are the declarations
/// or members before and after, in the form that `tsuzuri doc` shows them; `note` says why a
/// change is breaking when that is not obvious.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub breaking: bool,
    pub action: Action,
    pub kind: &'static str,
    pub name: String,
    pub old: Option<String>,
    pub new: Option<String>,
    pub note: &'static str,
}

impl Change {
    pub fn severity(&self) -> &'static str {
        if self.breaking {
            "breaking"
        } else {
            "non-breaking"
        }
    }

    /// The change as one line of text, without its severity.
    pub fn message(&self) -> String {
        let mut text = format!("{} {} {}", self.action.name(), self.kind, self.name);
        match (self.action, &self.old, &self.new) {
            // The note of a deprecation is the only detail.
            (Action::Deprecated, _, Some(note)) | (Action::Undeprecated, Some(note), _) => {
                if !note.is_empty() {
                    text.push_str(&format!(": {note}"));
                }
            }
            (_, Some(old), Some(new)) => text.push_str(&format!(": `{old}` => `{new}`")),
            (_, Some(shown), None) | (_, None, Some(shown)) => {
                text.push_str(&format!(": `{shown}`"));
            }
            (_, None, None) => {}
        }
        if !self.note.is_empty() {
            text.push_str(&format!(" ({})", self.note));
        }
        text
    }

    fn json(&self) -> String {
        let optional = |value: &Option<String>| value.as_deref().map_or("null".into(), json_string);
        let note = if self.note.is_empty() {
            "null".into()
        } else {
            json_string(self.note)
        };
        format!(
            "{{\"type\":\"change\",\"severity\":{},\"action\":{},\"kind\":{},\"name\":{},\"old\":{},\"new\":{},\"note\":{note}}}",
            json_string(self.severity()),
            json_string(self.action.name()),
            json_string(self.kind),
            json_string(&self.name),
            optional(&self.old),
            optional(&self.new),
        )
    }
}

/// What the comparison found: the changes, breaking ones first and each group by name, and the
/// versions of the two manifests when both have one.
#[derive(Debug, Default)]
pub struct Report {
    pub changes: Vec<Change>,
    pub old_version: Option<Version>,
    pub new_version: Option<Version>,
}

impl Report {
    pub fn breaking(&self) -> usize {
        self.changes.iter().filter(|change| change.breaking).count()
    }

    pub fn non_breaking(&self) -> usize {
        self.changes.len() - self.breaking()
    }

    /// Whether registry dependents that accept the old version also take the new one: the new
    /// version is compatible with the old one and not older. `None` unless both are known.
    pub fn compatible_versions(&self) -> Option<bool> {
        Some(self.new_version?.satisfies(self.old_version?))
    }

    fn compatibility_note(&self) -> Option<String> {
        let (old, new) = (self.old_version?, self.new_version?);
        (self.breaking() > 0 && new.satisfies(old)).then(|| {
            format!(
                "version {new} is compatible with {old}, so registry dependents that accept {old} would take it; raise the major version (the minor version before 1.0.0) for a breaking change"
            )
        })
    }

    fn listed(&self) -> &[Change] {
        &self.changes[..self.changes.len().min(MAX_LISTED_CHANGES)]
    }

    fn omitted(&self) -> usize {
        self.changes.len() - self.listed().len()
    }

    /// One line per change, then a summary and `note:` lines for a cut list and for versions
    /// that make dependents take a breaking release.
    pub fn render_text(&self) -> String {
        let mut text = String::new();
        for change in self.listed() {
            text.push_str(&format!("{}: {}\n", change.severity(), change.message()));
        }
        if self.omitted() != 0 {
            text.push_str(&format!(
                "note: {} more changes not shown\n",
                self.omitted()
            ));
        }
        text.push_str(&format!(
            "summary: {} breaking, {} non-breaking\n",
            self.breaking(),
            self.non_breaking()
        ));
        if let Some(note) = self.compatibility_note() {
            text.push_str(&format!("note: {note}\n"));
        }
        text
    }

    /// One JSON object per line: each change, then the summary.
    pub fn render_json(&self) -> String {
        let mut text = String::new();
        for change in self.listed() {
            text.push_str(&change.json());
            text.push('\n');
        }
        let version = |version: Option<Version>| {
            version.map_or("null".into(), |version| json_string(&version.to_string()))
        };
        let compatible = match self.compatible_versions() {
            Some(true) => "true",
            Some(false) => "false",
            None => "null",
        };
        text.push_str(&format!(
            "{{\"type\":\"summary\",\"breaking\":{},\"non_breaking\":{},\"omitted\":{},\"old_version\":{},\"new_version\":{},\"compatible_versions\":{compatible}}}\n",
            self.breaking(),
            self.non_breaking(),
            self.omitted(),
            version(self.old_version),
            version(self.new_version),
        ));
        text
    }
}

/// A public declaration and what its users can see of it. Its `header` and members' `text` have
/// canonical type variables and decide what changed; `shown` is the form that `tsuzuri doc`
/// shows, which the report quotes.
struct Item {
    kind: &'static str,
    name: String,
    /// The declaration without its members and its `export` marker.
    header: String,
    shown: String,
    exported: bool,
    /// A `const def`, which a `const` initializer of another module may call.
    constant: bool,
    /// Conservative until API comparison resolves parameter types and aliases.
    borrowing_defaults: Vec<bool>,
    /// The arity protected by the pure contract, excluding returned callable stages.
    pure: Option<usize>,
    /// The effect attributes (A20): promises that adding keeps and removing breaks, except on a
    /// function that may call the functions it is given (see `callbacks`).
    asserts: Asserts,
    /// Whether a parameter may carry a function value that the function calls, or has a type
    /// variable. An effect attribute added to such a function also binds its callers, which must
    /// then pass callbacks (or instantiate it with types) that keep it (A20 Phase 6).
    callbacks: bool,
    /// The contract lines (A21) as written: `@requires ...` and `@ensures \r -> ...`.
    contracts: BTreeSet<String>,
    contract: Option<String>,
    /// Fields, cases or methods, in declaration order.
    members: Vec<Member>,
    derives: BTreeSet<&'static str>,
    doc: Option<String>,
    deprecated: Option<String>,
}

struct Member {
    kind: &'static str,
    name: String,
    text: String,
    shown: String,
    /// A class method that has a default.
    default: bool,
    pure: Option<usize>,
    contract: Option<String>,
}

impl Item {
    fn new(kind: &'static str, name: String, header: String) -> Self {
        Self {
            kind,
            name,
            shown: header.clone(),
            header,
            exported: false,
            constant: false,
            borrowing_defaults: Vec::new(),
            pure: None,
            asserts: Asserts(0),
            callbacks: false,
            contracts: BTreeSet::new(),
            contract: None,
            members: Vec::new(),
            derives: BTreeSet::new(),
            doc: None,
            deprecated: None,
        }
    }

    fn documented(mut self, doc: Option<&Documentation>) -> Self {
        self.deprecated = doc.and_then(Documentation::deprecation).map(str::to_owned);
        self.doc = doc.map(|doc| doc.text.clone());
        self
    }

    /// The declaration as `tsuzuri doc` shows it, with `export` and `const` in front where it has them.
    fn text(&self) -> String {
        let mut text = String::new();
        for kind in self.asserts.kinds() {
            text.push('@');
            text.push_str(kind.name());
            text.push(' ');
        }
        if self.exported {
            text.push_str("export ");
        }
        if self.constant {
            text.push_str("const ");
        }
        if self.pure.is_some() {
            text.push_str("pure ");
        }
        text.push_str(&self.shown);
        text
    }
}

impl Member {
    fn new(kind: &'static str, name: String, text: String) -> Self {
        Self {
            kind,
            name,
            shown: text.clone(),
            text,
            default: false,
            pure: None,
            contract: None,
        }
    }
}

type Items = BTreeMap<(String, &'static str), Item>;

/// The API of one version: its public declarations by qualified name and kind.
pub struct Api {
    items: Items,
    /// The root package's default namespace as written, `None` without a manifest.
    namespace: Option<String>,
    package: Option<String>,
    version: Option<Version>,
}

/// Renames type variables, regions and declared length parameters by their first appearance, so
/// that two declarations that differ only in those names print the same; a renaming that is not
/// `canonical` leaves every name as written. A name in a length position that no declaration
/// has declared as a length parameter (`const N: i64`) is a constant, and keeps its name.
#[derive(Clone)]
struct Renaming<'a> {
    canonical: bool,
    resolved: Option<&'a ResolvedApi>,
    variables: BTreeMap<String, String>,
    regions: BTreeMap<String, String>,
    /// The declared length parameters, by the name without its `#`.
    lengths: BTreeMap<String, String>,
}

impl<'a> Renaming<'a> {
    fn new(canonical: bool, resolved: Option<&'a ResolvedApi>) -> Self {
        Self {
            canonical,
            resolved: resolved.filter(|_| canonical),
            variables: BTreeMap::new(),
            regions: BTreeMap::new(),
            lengths: BTreeMap::new(),
        }
    }

    fn variable(&mut self, name: &str) -> String {
        if !self.canonical {
            return name.to_owned();
        }
        let next = self.variables.len();
        self.variables
            .entry(name.to_owned())
            .or_insert_with(|| format!("t{next}"))
            .clone()
    }

    fn region(&mut self, name: &str) -> String {
        if !self.canonical {
            return name.to_owned();
        }
        let next = self.regions.len();
        self.regions
            .entry(name.to_owned())
            .or_insert_with(|| format!("r{next}"))
            .clone()
    }

    /// A declared type parameter, or a length parameter written `#N`.
    fn parameter(&mut self, parameter: &Ident) -> Ident {
        let text = match parameter.text.strip_prefix('#') {
            Some(name) => format!("#{}", self.length(name)),
            None => self.variable(&parameter.text),
        };
        Ident {
            text,
            ..parameter.clone()
        }
    }

    /// The name of a declared length parameter, given by its position among the declared ones.
    fn length(&mut self, name: &str) -> String {
        if !self.canonical {
            return name.to_owned();
        }
        let next = self.lengths.len();
        self.lengths
            .entry(name.to_owned())
            .or_insert_with(|| format!("n{next}"))
            .clone()
    }

    /// A length, or a type argument that may be one: the name of a declared length parameter is
    /// renamed, and anything else (a number, a constant, a type) is as written.
    fn length_argument(&mut self, argument: &TypeExpr) -> TypeExpr {
        match &argument.kind {
            TypeExprKind::Named(name) if self.lengths.contains_key(name) => TypeExpr {
                kind: TypeExprKind::Named(self.lengths[name].clone()),
                span: argument.span,
            },
            TypeExprKind::Length(_) | TypeExprKind::Named(_) => argument.clone(),
            _ => self.ty(argument),
        }
    }

    fn regions(&mut self, regions: &[Ident]) -> Vec<Ident> {
        regions
            .iter()
            .map(|region| Ident {
                text: self.region(&region.text),
                ..region.clone()
            })
            .collect()
    }

    fn public_name(&self, name: &str) -> String {
        self.resolved
            .and_then(|resolved| resolved.public_names.get(name))
            .cloned()
            .unwrap_or_else(|| name.to_owned())
    }

    fn class_name(&self, name: &Ident) -> Ident {
        let text = self
            .resolved
            .and_then(|resolved| resolved.classes.get(&api_key(name.span)))
            .map_or_else(
                || self.public_name(&name.text),
                |name| self.public_name(name),
            );
        Ident {
            text,
            ..name.clone()
        }
    }

    fn ty(&mut self, ty: &TypeExpr) -> TypeExpr {
        use TypeExprKind::*;
        let ty = if ty.span.source.is_some() {
            self.resolved
                .and_then(|resolved| resolved.annotations.get(&api_key(ty.span)))
                .unwrap_or(ty)
        } else {
            ty
        };
        let kind = match &ty.kind {
            Named(name) => Named(self.public_name(name)),
            Variable(name) if name.starts_with('#') => Named(self.length(&name[1..])),
            Variable(name) => Variable(self.variable(name)),
            Apply(head, arguments) => {
                let head = match head.text.strip_prefix('\'') {
                    Some(name) => Ident {
                        text: format!("'{}", self.variable(name)),
                        ..(**head).clone()
                    },
                    None => Ident {
                        text: self.public_name(&head.text),
                        ..(**head).clone()
                    },
                };
                let arguments = arguments
                    .iter()
                    .map(|argument| self.length_argument(argument))
                    .collect();
                Apply(Box::new(head), arguments)
            }
            Regions(inner, names) => {
                let names = self.regions(names).into_boxed_slice();
                Regions(Box::new(self.ty(inner)), names)
            }
            Quantified(names, inner) => {
                let names = self.regions(names).into_boxed_slice();
                Quantified(names, Box::new(self.ty(inner)))
            }
            Array(inner) => Array(Box::new(self.ty(inner))),
            ArrayView(inner) => ArrayView(Box::new(self.ty(inner))),
            FixedArray(element, length) => FixedArray(
                Box::new(self.ty(element)),
                Box::new(self.length_argument(length)),
            ),
            Length(length) => Length(*length),
            List(inner) => List(Box::new(self.ty(inner))),
            Tuple(elements) => Tuple(elements.iter().map(|element| self.ty(element)).collect()),
            Task(result) => Task(Box::new(self.ty(result))),
            Function(parameters, result) => Function(
                parameters
                    .iter()
                    .map(|parameter| self.ty(parameter))
                    .collect(),
                Box::new(self.ty(result)),
            ),
            Reference(inner, mutable) => Reference(Box::new(self.ty(inner)), *mutable),
            Own(inner) => Own(Box::new(self.ty(inner))),
            Mut(inner) => Mut(Box::new(self.ty(inner))),
            Pure(inner) => Pure(Box::new(self.ty(inner))),
            Dyn(dyn_type) => {
                let mut dyn_type = (**dyn_type).clone();
                for class in &mut dyn_type.classes {
                    class.text = self.public_name(&class.text);
                }
                Dyn(Box::new(dyn_type))
            }
        };
        TypeExpr {
            kind,
            span: ty.span,
        }
    }

    fn constraint(&mut self, constraint: &ConstraintExpr) -> ConstraintExpr {
        let ty = self.ty(&constraint.ty);
        let name = match &constraint.name {
            ConstraintName::Class(class) => ConstraintName::Class(self.class_name(class)),
            ConstraintName::Function(name, annotation) => ConstraintName::Function(
                name.clone(),
                annotation
                    .as_ref()
                    .map(|annotation| Box::new(self.ty(annotation))),
            ),
        };
        ConstraintExpr { name, ty }
    }

    /// The constraints with their names renamed. They are a set, so a canonical renaming puts
    /// them in a fixed order.
    fn constraints(&mut self, constraints: &[ConstraintExpr]) -> Vec<ConstraintExpr> {
        let mut constraints: Vec<_> = constraints
            .iter()
            .map(|constraint| self.constraint(constraint))
            .collect();
        if self.canonical {
            let key = |constraint: &ConstraintExpr| {
                let head = match &constraint.name {
                    ConstraintName::Class(class) => class.text.clone(),
                    ConstraintName::Function(name, annotation) => format!(
                        "#{}{}",
                        name.text,
                        annotation.as_deref().map(type_text).unwrap_or_default()
                    ),
                };
                format!("{head}<{}>", type_text(&constraint.ty))
            };
            constraints.sort_by_cached_key(key);
            if self.resolved.is_some() {
                constraints.dedup_by(|left, right| key(left) == key(right));
            }
        }
        constraints
    }
}

/// The parameter types and the result as one chain of arrows: a result that is itself a
/// function type is spliced in, because `A -> B -> C` is the type of a definition whether its
/// lambda takes both arguments or returns a function. The rule is the parser's: a function
/// type without parameters (`fn() -> T`) is a thunk, and a definition that takes no parameters
/// keeps its function result, so neither is spliced.
fn flatten(mut parameters: Vec<TypeExpr>, mut result: TypeExpr) -> (Vec<TypeExpr>, TypeExpr) {
    while !parameters.is_empty()
        && matches!(&result.kind, TypeExprKind::Function(inner, _) if !inner.is_empty())
    {
        let TypeExprKind::Function(more, inner) = result.kind else {
            unreachable!("the loop condition matched a function type")
        };
        parameters.extend(more);
        result = *inner;
    }
    (parameters, result)
}

/// A function's signature in one line. `renaming` holds the names that the enclosing
/// declaration has already given, such as a class's variable. A canonical renaming also
/// flattens the arrows; the other form is the signature as `tsuzuri doc` shows it.
fn function_text(
    prefix: &str,
    name: &str,
    regions: &[Ident],
    constraints: &[ConstraintExpr],
    parameters: &[&TypeExpr],
    result: &TypeExpr,
    mut renaming: Renaming<'_>,
) -> String {
    let regions = renaming.regions(regions);
    let parameters = parameters
        .iter()
        .map(|parameter| renaming.ty(parameter))
        .collect();
    let result = renaming.ty(result);
    let (parameters, result) = if renaming.canonical {
        flatten(parameters, result)
    } else {
        (parameters, result)
    };
    let constraints = renaming.constraints(constraints);
    signature(
        prefix,
        name,
        &regions,
        &constraints,
        parameters.iter(),
        &result,
    )
    .replace("\n    ", " ")
}

fn derives(derives: &[(DeriveClass, Span)]) -> BTreeSet<&'static str> {
    derives.iter().map(|(class, _)| class.name()).collect()
}

/// The names of a declaration's type parameters, renamed by `renaming`.
fn parameters(parameters: &[Ident], renaming: &mut Renaming<'_>) -> Vec<Ident> {
    parameters
        .iter()
        .map(|parameter| renaming.parameter(parameter))
        .collect()
}

/// The items that one module declares, under `module`, its full name, in the canonical form
/// that decides what changed or in the form that `tsuzuri doc` shows.
fn add_items(
    module: &str,
    program: &Program,
    items: &mut Items,
    canonical: bool,
    edition: Edition,
    resolved: Option<&ResolvedApi>,
) {
    let fresh = || Renaming::new(canonical, resolved);
    let effective = resolved.filter(|_| canonical);
    // Every field is named so that a new kind of declaration cannot be left out silently.
    let Program {
        source_kind: _,
        namespace: _,
        usings: _,
        type_aliases,
        constants,
        externs,
        extern_types,
        records,
        unions,
        functions,
        classes,
        instances,
        active_patterns,
        tests: _,
        benches: _,
        entry: _,
        dyn_types: _,
        cpu_attributes: _,
        effect_attributes,
        // E15: `@trusted` is a reviewer's mark, not part of the API.
        trusted: _,
        noncanonical: _,
        aliases,
        declaration_starts: _,
    } = program;
    let mut add = |item: Item| {
        items.entry((item.name.clone(), item.kind)).or_insert(item);
    };
    let qualified = |name: &str| format!("{module}.{name}");
    for declaration in functions {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let pattern = active_patterns
            .iter()
            .find(|pattern| pattern.function == declaration.name.text);
        let name = pattern.map_or_else(
            || declaration.name.text.clone(),
            |pattern| {
                let mut cases: Vec<_> = pattern
                    .cases
                    .iter()
                    .map(|case| case.text.as_str())
                    .collect();
                if pattern.partial {
                    cases.push("_");
                }
                format!("(|{}|)", cases.join("|"))
            },
        );
        // A multi-case active pattern has the implicit result type `'T`, as in the docs.
        let implicit = pattern
            .filter(|pattern| pattern.cases.len() > 1)
            .map(|_| TypeExpr {
                kind: TypeExprKind::Variable("T".into()),
                span: declaration.result.span,
            });
        let signature =
            effective.and_then(|resolved| resolved.signatures.get(&api_key(declaration.name.span)));
        let types: Vec<_> = signature.map_or_else(
            || {
                declaration
                    .parameters
                    .iter()
                    .map(|parameter| &parameter.ty)
                    .collect()
            },
            |signature| signature.parameters.iter().collect(),
        );
        let result = signature.map_or(&declaration.result, |signature| &signature.result);
        let constraints = effective
            .and_then(|resolved| resolved.requirements.get(&api_key(declaration.name.span)))
            .map_or(declaration.constraints.as_slice(), Vec::as_slice);
        let header = function_text(
            "def",
            &name,
            &declaration.regions,
            constraints,
            &types,
            implicit.as_ref().unwrap_or(result),
            fresh(),
        );
        let mut item =
            Item::new("def", qualified(&name), header).documented(declaration.doc.as_ref());
        item.exported = declaration.exported;
        item.constant = declaration.constant;
        item.pure = declaration.pure.then_some(declaration.parameters.len());
        item.callbacks = types.iter().any(|ty| may_carry_functions(ty))
            || has_type_variable(implicit.as_ref().unwrap_or(result));
        for attribute in effect_attributes
            .iter()
            .filter(|attribute| attribute.function.text == declaration.name.text)
        {
            item.asserts.insert(attribute.kind);
        }
        item.contracts = crate::docgen::contract_lines(&declaration.contracts)
            .into_iter()
            .collect();
        item.contract = effective
            .and_then(|resolved| resolved.contracts.get(&api_key(declaration.name.span)))
            .cloned();
        if effective.is_none() {
            let (parameters, _) = flatten(
                declaration
                    .parameters
                    .iter()
                    .map(|p| p.ty.clone())
                    .collect(),
                declaration.result.clone(),
            );
            item.borrowing_defaults = parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    (edition.borrows_parameters() || declaration.pure)
                        && !declaration.constant
                        && !declaration.parameters.get(index).is_some_and(|p| p.mutable)
                        && match &parameter.kind {
                            TypeExprKind::Own(_)
                            | TypeExprKind::Mut(_)
                            | TypeExprKind::Reference(..)
                            | TypeExprKind::Regions(..)
                            | TypeExprKind::Quantified(..) => false,
                            TypeExprKind::Named(name) => crate::numeric::primitive(name)
                                .is_none_or(|ty| {
                                    matches!(
                                        ty,
                                        crate::check::Type::String | crate::check::Type::Utf8String
                                    )
                                }),
                            _ => true,
                        }
                })
                .collect();
        }
        add(item);
    }
    for declaration in externs {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let signature =
            effective.and_then(|resolved| resolved.signatures.get(&api_key(declaration.name.span)));
        let types: Vec<_> = signature.map_or_else(
            || declaration.parameters.iter().collect(),
            |signature| signature.parameters.iter().collect(),
        );
        let result = signature.map_or(&declaration.result, |signature| &signature.result);
        let header = function_text(
            "extern def",
            &declaration.name.text,
            &declaration.regions,
            &declaration.constraints,
            &types,
            result,
            fresh(),
        );
        let mut item = Item::new("extern def", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref());
        item.exported = declaration.exported;
        add(item);
    }
    for declaration in extern_types {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let header = format!("extern type {}", declaration.name.text);
        add(
            Item::new("extern type", qualified(&declaration.name.text), header)
                .documented(declaration.doc.as_ref()),
        );
    }
    for declaration in records {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let mut renaming = fresh();
        let names = parameters(&declaration.parameters, &mut renaming);
        let regions = renaming.regions(&declaration.regions);
        let header = format!(
            "record {}{}{}",
            declaration.name.text,
            parameters_text(&names),
            region_text(&regions)
        );
        let mut item = Item::new("record", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref());
        for field in &declaration.fields {
            let mut member = Member::new(
                "field",
                field.name.text.clone(),
                format!(
                    "{}{}{}: {}",
                    json_text(field.json.as_ref()),
                    if field.mutable { "mut " } else { "" },
                    field.name.text,
                    type_text(&renaming.ty(&field.ty))
                ),
            );
            member.contract = effective
                .and_then(|resolved| resolved.contracts.get(&api_key(field.name.span)))
                .cloned();
            item.members.push(member);
        }
        item.derives = derives(&declaration.derives);
        add(item);
    }
    for declaration in unions {
        if declaration.visibility == Visibility::Private
            || declaration.name.provenance == Provenance::Generated
        {
            continue;
        }
        let mut renaming = fresh();
        let names = parameters(&declaration.parameters, &mut renaming);
        let header = format!("union {}{}", declaration.name.text, parameters_text(&names));
        let mut item = Item::new("union", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref());
        for case in &declaration.cases {
            let payload = case.payload.as_ref().map_or(String::new(), |ty| {
                format!(" of {}", type_text(&renaming.ty(ty)))
            });
            item.members.push(Member::new(
                "case",
                case.name.text.clone(),
                format!(
                    "{}{}{payload}",
                    json_text(case.json.as_ref()),
                    case.name.text
                ),
            ));
        }
        item.derives = derives(&declaration.derives);
        add(item);
    }
    for declaration in type_aliases {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let mut renaming = fresh();
        let names = parameters(&declaration.parameters, &mut renaming);
        let target = renaming.ty(&declaration.target);
        let constraints = effective
            .and_then(|resolved| resolved.requirements.get(&api_key(declaration.name.span)))
            .map(|constraints| renaming.constraints(constraints))
            .unwrap_or_default();
        let header = format!(
            "type {}{} = {}{}",
            declaration.name.text,
            parameters_text(&names),
            type_text(&target),
            constraints_text(&constraints),
        );
        add(Item::new("type", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref()));
    }
    for declaration in constants {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let header = format!(
            "const {}: {}",
            declaration.name.text,
            type_text(&fresh().ty(&declaration.ty))
        );
        add(
            Item::new("const", qualified(&declaration.name.text), header)
                .documented(declaration.doc.as_ref()),
        );
    }
    for declaration in classes {
        let mut renaming = fresh();
        let variable = renaming.variable(&declaration.variable.text);
        let superclasses = renaming.constraints(&declaration.superclasses);
        let kind = if declaration.kind == Kind::Type {
            String::new()
        } else {
            format!(": {}", declaration.kind.display())
        };
        let header = format!(
            "class {}{}<'{variable}{kind}>",
            constraints_text(&superclasses),
            declaration.name.text
        );
        let mut item = Item::new("class", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref());
        for method in &declaration.methods {
            if method.visibility == Visibility::Private {
                continue;
            }
            let signature =
                effective.and_then(|resolved| resolved.signatures.get(&api_key(method.name.span)));
            let types: Vec<_> = signature.map_or_else(
                || method.parameters.iter().collect(),
                |signature| signature.parameters.iter().collect(),
            );
            let result = signature.map_or(&method.result, |signature| &signature.result);
            let mut member = Member::new(
                "method",
                method.name.text.clone(),
                function_text(
                    if method.pure { "pure def" } else { "def" },
                    &method.name.text,
                    &method.regions,
                    &method.constraints,
                    &types,
                    result,
                    renaming.clone(),
                ),
            );
            member.pure = method.pure.then_some(method.parameters.len());
            member.contract = effective
                .and_then(|resolved| resolved.contracts.get(&api_key(method.name.span)))
                .cloned();
            member.default = declaration
                .defaults
                .iter()
                .any(|default| default.name.text == method.name.text);
            item.members.push(member);
        }
        add(item);
    }
    for declaration in instances {
        let mut renaming = fresh();
        let ty = renaming.ty(&declaration.ty);
        let constraints = renaming.constraints(&declaration.constraints);
        let class = renaming.class_name(&declaration.class);
        let name = format!("{}<{}>", class.text, type_text(&ty));
        let header = format!("instance {}{name}", constraints_text(&constraints));
        let mut key = Renaming::new(true, resolved);
        let ty = key.ty(&declaration.ty);
        let class = key.class_name(&declaration.class);
        let name = format!("{}<{}>", class.text, type_text(&ty));
        // Two modules can spell the same instance for their own types, so the module is part of it.
        let mut item = Item::new("instance", qualified(&name), header);
        item.contract = effective
            .and_then(|resolved| resolved.contracts.get(&api_key(declaration.class.span)))
            .cloned();
        add(item);
    }
    for alias in aliases {
        add(Item::new(
            "alias",
            qualified(&alias.text),
            format!("@alias {}", alias.text),
        ));
    }
}

/// The full name of a module as source code spells it: its namespace and its file name joined
/// by `::`. A project without a manifest has no namespace of its own, so `root_namespace` is
/// empty for it.
fn module_name(
    source: &SourceFile,
    program: &Program,
    root_namespace: &str,
    id: usize,
) -> Result<String, Diagnostic> {
    let declared = program.namespace.as_ref().map(|namespace| &namespace.path);
    let (key, namespace) = crate::module_identity(
        &source.relative_path.to_string_lossy(),
        declared,
        root_namespace,
    )
    .map_err(|mut error| {
        error.span.source = Some(id);
        error
    })?;
    let stem = key.rsplit('.').next().unwrap_or(&key);
    let full = if namespace.is_empty() {
        stem.to_owned()
    } else {
        format!("{namespace}.{stem}")
    };
    Ok(full.replace('.', "::"))
}

/// The public API of the root package of `project`, or of all its sources when it has no
/// manifest. A source that does not parse is an error at that source.
pub fn api(project: &Project) -> Result<Api, Diagnostic> {
    api_with_resolution(project, None)
}

/// Fully analyzes the project and compares resolved types, parameter modes and region contracts.
/// Load it with `Project::load_for_docs` so its locked dependencies are available.
pub fn effective_api(project: &Project) -> Result<Api, Diagnostic> {
    let mut index = SemanticIndex {
        resolved_api: Some(Ok(ResolvedApi::default())),
        ..SemanticIndex::default()
    };
    crate::analyze_inputs_semantic(&project.inputs(), &mut index).map_err(crate::first_error)?;
    let resolved = index
        .resolved_api
        .expect("resolved API collection requested")?;
    api_with_resolution(project, Some(&resolved))
}

fn api_with_resolution(
    project: &Project,
    resolved: Option<&ResolvedApi>,
) -> Result<Api, Diagnostic> {
    let root = project.root_package.as_ref();
    let manifest = root
        .and_then(|root| {
            project.manifests.iter().find(|manifest| {
                manifest.package.as_ref() == Some(root)
                    && manifest.relative_path == Path::new("Tsuzuri.toml")
            })
        })
        .and_then(|manifest| crate::package::parse_manifest(&manifest.text, 0).ok());
    let mut items = Items::new();
    let mut shown = Items::new();
    let standard_root = resolved.is_some()
        && project
            .sources
            .iter()
            .all(|source| source.origin == ModuleOrigin::Std);
    for (id, source) in project.sources.iter().enumerate() {
        if (!standard_root && source.origin != ModuleOrigin::User)
            || source.package.as_ref() != root
        {
            continue;
        }
        let program = crate::parser::parse_with_source(&source.text, id, source.edition)?;
        let namespace = if root.is_some() {
            source.namespace.as_str()
        } else {
            ""
        };
        let module = module_name(source, &program, namespace, id)?;
        add_items(
            &module,
            &program,
            &mut items,
            true,
            source.edition,
            resolved,
        );
        add_items(
            &module,
            &program,
            &mut shown,
            false,
            source.edition,
            resolved,
        );
    }
    // Both forms hold the same items and members in the same order.
    for (key, item) in &mut items {
        let Some(written) = shown.get(key) else {
            continue;
        };
        item.shown = written.header.clone();
        for (member, written) in item.members.iter_mut().zip(&written.members) {
            member.shown = written.text.clone();
        }
    }
    Ok(Api {
        items,
        namespace: manifest
            .as_ref()
            .map(|manifest| manifest.namespace.replace('.', "::")),
        package: root.map(|root| root.name.clone()),
        version: manifest
            .as_ref()
            .and_then(|manifest| Version::parse(&manifest.version)),
    })
}

fn removed(item: &Item) -> Change {
    Change {
        breaking: true,
        action: Action::Removed,
        kind: item.kind,
        name: item.name.clone(),
        old: Some(item.text()),
        new: None,
        note: "",
    }
}

fn added(item: &Item) -> Change {
    Change {
        breaking: false,
        action: Action::Added,
        kind: item.kind,
        name: item.name.clone(),
        old: None,
        new: Some(item.text()),
        note: "",
    }
}

/// A change of a member of `item`.
fn member_change(
    item: &Item,
    member: &Member,
    action: Action,
    breaking: bool,
    note: &'static str,
) -> Change {
    Change {
        breaking,
        action,
        kind: member.kind,
        name: format!("{}.{}", item.name, member.name),
        old: (action == Action::Removed).then(|| member.shown.clone()),
        new: (action == Action::Added).then(|| member.shown.clone()),
        note,
    }
}

/// A method's declaration with a mark for a default, for a change of the mark alone.
fn with_default(member: &Member) -> String {
    if member.default {
        format!("{} (with a default)", member.shown)
    } else {
        member.shown.clone()
    }
}

/// The changes between the members of two versions of an item.
fn compare_members(old: &Item, new: &Item, changes: &mut Vec<Change>) {
    let by_name = |item: &'_ Item| -> BTreeMap<String, usize> {
        item.members
            .iter()
            .enumerate()
            .map(|(index, member)| (member.name.clone(), index))
            .collect()
    };
    let (before, after) = (by_name(old), by_name(new));
    for member in &old.members {
        if !after.contains_key(&member.name) {
            let note = match member.kind {
                "field" => "uses of the field stop compiling",
                "case" => "uses of the case stop compiling",
                _ => "calls and instances of the method stop compiling",
            };
            changes.push(member_change(old, member, Action::Removed, true, note));
        }
    }
    for member in &new.members {
        if before.contains_key(&member.name) {
            continue;
        }
        let (breaking, note) = match member.kind {
            "field" => (
                true,
                "constructions of the record without it stop compiling",
            ),
            "case" => (true, "exhaustive matches over the union stop compiling"),
            _ if member.default => (false, ""),
            _ => (true, "existing instances do not define the method"),
        };
        changes.push(member_change(new, member, Action::Added, breaking, note));
    }
    for (name, &index) in &before {
        let Some(&other) = after.get(name) else {
            continue;
        };
        let (was, now) = (&old.members[index], &new.members[other]);
        let added_default = !was.default && now.default;
        let signature_changed = was.text != now.text
            || was.pure != now.pure
            || (was.contract != now.contract && !added_default);
        let (shown_was, shown_now) = if signature_changed {
            (was.shown.clone(), now.shown.clone())
        } else if was.default != now.default {
            (with_default(was), with_default(now))
        } else {
            continue;
        };
        // A default that goes away leaves the instances that relied on it unfinished; one
        // that comes is a convenience.
        let lost_default = was.default && !now.default;
        changes.push(Change {
            breaking: signature_changed || lost_default,
            action: Action::Changed,
            kind: was.kind,
            name: format!("{}.{name}", new.name),
            old: Some(shown_was),
            new: Some(shown_now),
            note: if lost_default && !signature_changed {
                "instances that relied on the default must define the method"
            } else {
                ""
            },
        });
    }
    // The order of fields and cases is part of the layout and of derived comparisons.
    if new.members.iter().all(|member| member.kind != "method") {
        let common = |item: &Item, other: &BTreeMap<String, usize>| -> Vec<String> {
            item.members
                .iter()
                .filter(|member| other.contains_key(&member.name))
                .map(|member| member.name.clone())
                .collect()
        };
        let (was, now) = (common(old, &after), common(new, &before));
        if was != now {
            changes.push(Change {
                breaking: true,
                action: Action::Changed,
                kind: "order",
                name: new.name.clone(),
                old: Some(was.join(", ")),
                new: Some(now.join(", ")),
                note: "the order of fields and cases is part of the layout and of derived comparisons",
            });
        }
    }
}

/// Whether a parameter of this type may carry a function value that the function can call, or
/// makes the attribute depend on the caller: a function, task or `dyn` type, a named type that
/// the syntax alone cannot show to be closed (records and unions may hold functions), or a type
/// variable (each instance is checked with the caller's type, which may be a function or copy an
/// array; review of #63). Scalars, strings and the standard collections of closed types are
/// closed.
fn may_carry_functions(ty: &TypeExpr) -> bool {
    const CLOSED: [&str; 31] = [
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "i8u",
        "i16u",
        "i32u",
        "i64u",
        "i128u",
        "byte",
        "ubyte",
        "sbyte",
        "f16",
        "f32",
        "f64",
        "f128",
        "d32",
        "d64",
        "d128",
        "bool",
        "char",
        "utf8char",
        "string",
        "utf8string",
        "unit",
        "bigint",
        "BigInt",
        "Json",
        "Instant",
        "Duration",
    ];
    const COLLECTIONS: [&str; 9] = [
        "Array", "List", "Vec", "Maybe", "Result", "Map", "Set", "HashMap", "HashSet",
    ];
    match &ty.kind {
        TypeExprKind::Variable(_) => true,
        TypeExprKind::Length(_) => false,
        TypeExprKind::Named(name) => !CLOSED.contains(&name.as_str()),
        TypeExprKind::Apply(head, arguments) => {
            let name = head.text.rsplit('.').next().unwrap_or(&head.text);
            !COLLECTIONS.contains(&name) || arguments.iter().any(may_carry_functions)
        }
        TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Own(inner)
        | TypeExprKind::Mut(inner)
        | TypeExprKind::Pure(inner)
        | TypeExprKind::Regions(inner, _) => may_carry_functions(inner),
        TypeExprKind::FixedArray(inner, _) => may_carry_functions(inner),
        TypeExprKind::Tuple(elements) => elements.iter().any(may_carry_functions),
        _ => true,
    }
}

/// Whether a type mentions a type variable: an attribute of a generic function is checked for
/// each instance, with the types that its callers choose.
fn has_type_variable(ty: &TypeExpr) -> bool {
    match &ty.kind {
        TypeExprKind::Variable(_) => true,
        TypeExprKind::Named(_) | TypeExprKind::Length(_) | TypeExprKind::Dyn(_) => false,
        TypeExprKind::Apply(_, arguments) => arguments.iter().any(has_type_variable),
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Quantified(_, inner)
        | TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Own(inner)
        | TypeExprKind::Mut(inner)
        | TypeExprKind::Pure(inner) => has_type_variable(inner),
        TypeExprKind::FixedArray(inner, length) => {
            has_type_variable(inner) || has_type_variable(length)
        }
        TypeExprKind::Tuple(elements) => elements.iter().any(has_type_variable),
        TypeExprKind::Function(parameters, result) => {
            parameters.iter().any(has_type_variable) || has_type_variable(result)
        }
    }
}

/// The changes between two versions of an item with the same name and kind.
fn compare_items(old: &Item, new: &Item, changes: &mut Vec<Change>) {
    if old.header != new.header
        || old.exported != new.exported
        || old.constant != new.constant
        || old.borrowing_defaults != new.borrowing_defaults
        || old.pure != new.pure
        || old.asserts != new.asserts
        || old.contract != new.contract
    {
        // Exporting or adding a const/pure contract adds a symbol or a use
        // and changes nothing else; revoking either breaks the code that relied on it. An effect
        // attribute added to a function that calls the functions it is given requires its callers
        // to pass callbacks that keep it, so it breaks them.
        let added_obligation = new.callbacks && new.asserts.0 & !old.asserts.0 != 0;
        let widened = old.header == new.header
            && old.exported <= new.exported
            && old.constant <= new.constant
            && old.borrowing_defaults == new.borrowing_defaults
            && (old.pure.is_none() || old.pure == new.pure)
            && old.asserts.0 & !new.asserts.0 == 0
            && !added_obligation
            && old.contract == new.contract;
        changes.push(Change {
            breaking: !widened,
            action: Action::Changed,
            kind: new.kind,
            name: new.name.clone(),
            old: Some(old.text()),
            new: Some(new.text()),
            note: if old.borrowing_defaults != new.borrowing_defaults {
                "default parameter conventions change; effective signatures may differ"
            } else if added_obligation {
                "callers must pass functions (or instantiate it with types) that keep the new effect attribute"
            } else {
                ""
            },
        });
    }
    compare_members(old, new, changes);
    let derive = |class: &str, action, breaking| Change {
        breaking,
        action,
        kind: "derive",
        name: format!("{}.{class}", new.name),
        old: None,
        new: None,
        note: "",
    };
    for class in old.derives.difference(&new.derives) {
        changes.push(derive(class, Action::Removed, true));
    }
    for class in new.derives.difference(&old.derives) {
        changes.push(derive(class, Action::Added, false));
    }
    // A21: removing an `@ensures` takes a guarantee away, adding a `@requires` asks more of
    // the callers; the other two only help them.
    let contract = |line: &String, action, breaking| Change {
        breaking,
        action,
        kind: "contract",
        name: format!("{}.{line}", new.name),
        old: None,
        new: None,
        note: "",
    };
    for line in old.contracts.difference(&new.contracts) {
        changes.push(contract(
            line,
            Action::Removed,
            line.starts_with("@ensures"),
        ));
    }
    for line in new.contracts.difference(&old.contracts) {
        changes.push(contract(line, Action::Added, line.starts_with("@requires")));
    }
    match (&old.deprecated, &new.deprecated) {
        (None, Some(note)) => changes.push(Change {
            breaking: false,
            action: Action::Deprecated,
            kind: new.kind,
            name: new.name.clone(),
            old: None,
            new: Some(note.clone()),
            note: "",
        }),
        (Some(note), None) => changes.push(Change {
            breaking: false,
            action: Action::Undeprecated,
            kind: new.kind,
            name: new.name.clone(),
            old: Some(note.clone()),
            new: None,
            note: "",
        }),
        // Another note, like any other edit of the text, is a change of the documentation.
        _ if old.doc != new.doc => changes.push(Change {
            breaking: false,
            action: Action::Documented,
            kind: new.kind,
            name: new.name.clone(),
            old: None,
            new: None,
            note: "",
        }),
        _ => {}
    }
}

/// The differences between the APIs of two versions.
pub fn compare(old: &Api, new: &Api) -> Report {
    let mut changes = Vec::new();
    if let (Some(was), Some(now)) = (&old.namespace, &new.namespace)
        && was != now
    {
        changes.push(Change {
            breaking: true,
            action: Action::Changed,
            kind: "namespace",
            name: new
                .package
                .clone()
                .or(old.package.clone())
                .unwrap_or_default(),
            old: Some(was.clone()),
            new: Some(now.clone()),
            note: "every declaration of the package is under the new name",
        });
    }
    for (key, item) in &old.items {
        match new.items.get(key) {
            None => changes.push(removed(item)),
            Some(next) => compare_items(item, next, &mut changes),
        }
    }
    for (key, item) in &new.items {
        if !old.items.contains_key(key) {
            changes.push(added(item));
        }
    }
    changes.sort_by(|left, right| {
        right
            .breaking
            .cmp(&left.breaking)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.kind.cmp(right.kind))
            .then_with(|| left.action.cmp(&right.action))
            .then_with(|| (&left.old, &left.new).cmp(&(&right.old, &right.new)))
    });
    Report {
        changes,
        old_version: old.version,
        new_version: new.version,
    }
}
