//! `kg find` (G28 Phase 2, D-60): searching the knowledge graph by type. The entries are the
//! function-like nodes of the project's graph and of std's (`def`, `const`, `extern`, a field as
//! its accessor `ref Record -> T`, a case as its constructor), with the types as their signatures
//! write them; a query is a type in the same syntax, where `'a` is a variable and `_` any type.
//! Both sides' variables are unified. The tiers are those of G25's `tsuzuri find`: `exact` (the
//! same parameters and result), `borrow` (the same without the outer `ref`s) and `partial` (the
//! query's parameters among the entry's, in order, with the extra count). A named type of an entry
//! is the type node that its module, its `uses_type` edges or its namespace give, so that `A.Point`
//! and `B.Point` stay apart; two names match when one is the other or ends with it after a `.` or
//! `::`. The length of a fixed array is a literal or a variable bound once per match. A constraint (`=>`)
//! whose variable the match binds to a primitive type is checked with the checker's own rule for
//! the builtin classes (`Float<i64>` fails), unless an instance node says otherwise; the other
//! constraints are shown, not checked (the graph does not hold every instance).

use std::collections::{BTreeMap, BTreeSet};

use super::json::Object;
use super::store::{KgData, KgView, Node};

pub const LIMIT_RANGE: (usize, usize) = (1, 100);
pub const DEFAULT_LIMIT: usize = 20;

/// A type of a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    /// A type variable or `_` (a fresh variable per occurrence), numbered.
    Var(usize),
    /// A named type with its arguments: `i64`, `Maybe<'a>`, `Shapes.Point`, or the ID of the type
    /// node that an entry's name resolves to.
    Named(String, Vec<Ty>),
    Ref(Box<Ty>, bool),
    Array(Box<Ty>),
    View(Box<Ty>),
    List(Box<Ty>),
    /// A fixed array and its length: a literal (`Named("4")`) or a variable (`N`).
    Fixed(Box<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    Function(Vec<Ty>, Box<Ty>),
}

struct Parser<'a> {
    chars: Vec<char>,
    at: usize,
    variables: &'a mut BTreeMap<String, usize>,
    next: &'a mut usize,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
        // A region annotation `{r s}` changes nothing that the search compares. Without its `}`
        // it stays, and the query does not parse.
        if self.chars.get(self.at) == Some(&'{')
            && let Some(close) = self.chars[self.at..].iter().position(|c| *c == '}')
        {
            self.at += close + 1;
            self.skip();
        }
    }

    fn eat(&mut self, text: &str) -> bool {
        self.skip();
        let wanted: Vec<char> = text.chars().collect();
        if self.chars[self.at..].starts_with(&wanted) {
            let after = self.chars.get(self.at + wanted.len());
            let word = text.chars().all(|c| c.is_alphanumeric());
            if word && after.is_some_and(|c| c.is_alphanumeric() || *c == '_') {
                return false;
            }
            self.at += wanted.len();
            true
        } else {
            false
        }
    }

    fn fresh(&mut self) -> usize {
        *self.next += 1;
        *self.next - 1
    }

    /// The variable that `key` names in this type, the same for every occurrence.
    fn variable(&mut self, key: String) -> usize {
        if let Some(id) = self.variables.get(&key) {
            return *id;
        }
        let id = self.fresh();
        self.variables.insert(key, id);
        id
    }

    /// A name, qualified with `.` or `::`; it stops before `..`, which ends a view (`[i64..]`).
    fn name(&mut self) -> Option<String> {
        self.skip();
        let start = self.at;
        while let Some(&c) = self.chars.get(self.at) {
            let part = match c {
                '.' => self.chars.get(self.at + 1) != Some(&'.'),
                _ => c.is_alphanumeric() || c == '_' || c == ':' || c == '#',
            };
            if !part {
                break;
            }
            self.at += 1;
        }
        (self.at > start).then(|| self.chars[start..self.at].iter().collect())
    }

    /// `A -> B -> R`, or a single type.
    fn function(&mut self) -> Option<Ty> {
        let mut parts = vec![self.atom()?];
        while self.eat("->") {
            parts.push(self.atom()?);
        }
        let result = parts.pop()?;
        Some(if parts.is_empty() {
            result
        } else {
            Ty::Function(parts, Box::new(result))
        })
    }

    fn atom(&mut self) -> Option<Ty> {
        self.skip();
        if self.eat("ref") {
            let mutable = self.eat("mut");
            return Some(Ty::Ref(Box::new(self.atom()?), mutable));
        }
        if self.eat("own") || self.eat("mut") {
            return self.atom();
        }
        if self.eat("pure") {
            return self.atom();
        }
        if self.eat("fn()") {
            self.eat("->");
            let result = self.atom()?;
            return Some(Ty::Function(Vec::new(), Box::new(result)));
        }
        if self.eat("[|") {
            let element = self.function()?;
            return self.eat("|]").then(|| Ty::List(Box::new(element)));
        }
        if self.eat("[") {
            let element = self.function()?;
            if self.eat("..") {
                return self.eat("]").then(|| Ty::View(Box::new(element)));
            }
            if self.eat(";") {
                let length = self.name()?;
                // `N` is a length variable (apart from the type variable `'N`), `4` a length.
                let length = if length.starts_with(char::is_alphabetic) {
                    Ty::Var(self.variable(format!(";{length}")))
                } else {
                    Ty::Named(length, Vec::new())
                };
                return self
                    .eat("]")
                    .then(|| Ty::Fixed(Box::new(element), Box::new(length)));
            }
            return self.eat("]").then(|| Ty::Array(Box::new(element)));
        }
        if self.eat("(") {
            let first = self.function()?;
            let mut elements = vec![first];
            while self.eat("*") {
                elements.push(self.function()?);
            }
            if !self.eat(")") {
                return None;
            }
            return Some(if elements.len() == 1 {
                elements.pop().unwrap()
            } else {
                Ty::Tuple(elements)
            });
        }
        if self.eat("'") {
            let name = self.name()?;
            return Some(Ty::Var(self.variable(name)));
        }
        if self.eat("_") {
            return Some(Ty::Var(self.fresh()));
        }
        let name = self.name()?;
        let mut arguments = Vec::new();
        if self.eat("<") {
            arguments.push(self.function()?);
            while self.eat(",") {
                arguments.push(self.function()?);
            }
            if !self.eat(">") {
                return None;
            }
        }
        Some(Ty::Named(name, arguments))
    }
}

/// Whether `long` is `short`, or ends with it after a `.` or `::` (`Acme::Geo.Point` and
/// `Geo.Point`, not `MyPoint`).
fn ends_with_name(long: &str, short: &str) -> bool {
    long.strip_suffix(short)
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('.') || rest.ends_with("::"))
}

/// Whether two type names can name one type: one ends with the other. `A::Point` and `B::Point` do
/// not, nor does a qualifier that no type has.
fn same_name(a: &str, b: &str) -> bool {
    ends_with_name(a, b) || ends_with_name(b, a)
}

/// The types of a graph (records, unions, aliases and handles) by the last segment of their IDs.
struct Types<'a>(BTreeMap<&'a str, Vec<&'a str>>);

impl<'a> Types<'a> {
    fn of(data: &'a KgData) -> Self {
        let mut types: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for node in &data.nodes {
            if matches!(node.kind.as_str(), "record" | "union" | "alias" | "handle") {
                types
                    .entry(last_segment(&node.id))
                    .or_default()
                    .push(&node.id);
            }
        }
        Self(types)
    }

    fn has(&self, id: &str) -> bool {
        self.0
            .get(last_segment(id))
            .is_some_and(|ids| ids.contains(&id))
    }

    /// The ID of the type that `name` names in a signature of `module` whose node refers to the
    /// types `used` (its `uses_type` edges): the module's own, one it uses, one of its namespace,
    /// or the type with that ID. Otherwise `name` as it is written (a primitive, a std type, or
    /// a name the graph does not decide).
    fn resolve(&self, name: &str, module: &str, used: &BTreeSet<&str>) -> String {
        let own = format!("{module}.{name}");
        if self.has(&own) {
            return own;
        }
        let mut using = used.iter().filter(|id| ends_with_name(id, name));
        if let (Some(id), None) = (using.next(), using.next()) {
            return (*id).to_owned();
        }
        if let Some((namespace, _)) = module.rsplit_once("::") {
            let id = format!("{namespace}::{name}");
            if self.has(&id) {
                return id;
            }
        }
        name.to_owned()
    }
}

fn last_segment(name: &str) -> &str {
    name.rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(name)
}

/// `ty` with each named type replaced by what `resolve` gives (not the length of a fixed array).
fn qualify(ty: Ty, resolve: &dyn Fn(&str) -> String) -> Ty {
    let all =
        |types: Vec<Ty>| -> Vec<Ty> { types.into_iter().map(|ty| qualify(ty, resolve)).collect() };
    match ty {
        Ty::Var(id) => Ty::Var(id),
        Ty::Named(name, arguments) => Ty::Named(resolve(&name), all(arguments)),
        Ty::Ref(inner, mutable) => Ty::Ref(Box::new(qualify(*inner, resolve)), mutable),
        Ty::Array(inner) => Ty::Array(Box::new(qualify(*inner, resolve))),
        Ty::View(inner) => Ty::View(Box::new(qualify(*inner, resolve))),
        Ty::List(inner) => Ty::List(Box::new(qualify(*inner, resolve))),
        Ty::Fixed(inner, length) => Ty::Fixed(Box::new(qualify(*inner, resolve)), length),
        Ty::Tuple(elements) => Ty::Tuple(all(elements)),
        Ty::Function(parameters, result) => {
            Ty::Function(all(parameters), Box::new(qualify(*result, resolve)))
        }
    }
}

/// The type of `text`, with its variables numbered from `next`.
fn parse(text: &str, next: &mut usize, variables: &mut BTreeMap<String, usize>) -> Option<Ty> {
    // A signature's constraints come before `=>`; the search does not check them.
    let text = match text.rfind("=>") {
        Some(at) if !text[..at].contains("->") => &text[at + 2..],
        _ => text,
    };
    let mut parser = Parser {
        chars: text.chars().collect(),
        at: 0,
        variables,
        next,
    };
    let ty = parser.function()?;
    parser.skip();
    (parser.at >= parser.chars.len()).then_some(ty)
}

/// Unification with one substitution for both sides.
#[derive(Default, Clone)]
struct Subst(BTreeMap<usize, Ty>);

impl Subst {
    fn resolve(&self, ty: &Ty) -> Ty {
        let mut ty = ty.clone();
        while let Ty::Var(id) = ty {
            match self.0.get(&id) {
                Some(bound) => ty = bound.clone(),
                None => break,
            }
        }
        ty
    }

    fn occurs(&self, id: usize, ty: &Ty) -> bool {
        match self.resolve(ty) {
            Ty::Var(other) => other == id,
            Ty::Named(_, args) | Ty::Tuple(args) => args.iter().any(|arg| self.occurs(id, arg)),
            Ty::Ref(inner, _) | Ty::Array(inner) | Ty::View(inner) | Ty::List(inner) => {
                self.occurs(id, &inner)
            }
            Ty::Fixed(inner, length) => self.occurs(id, &inner) || self.occurs(id, &length),
            Ty::Function(parameters, result) => {
                parameters
                    .iter()
                    .any(|parameter| self.occurs(id, parameter))
                    || self.occurs(id, &result)
            }
        }
    }

    fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.resolve(a), self.resolve(b));
        match (&a, &b) {
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (Ty::Var(x), other) | (other, Ty::Var(x)) => {
                if self.occurs(*x, other) {
                    return false;
                }
                self.0.insert(*x, other.clone());
                true
            }
            (Ty::Named(x, xs), Ty::Named(y, ys)) => {
                same_name(x, y)
                    && xs.len() == ys.len()
                    && xs.iter().zip(ys).all(|(x, y)| self.unify(x, y))
            }
            (Ty::Ref(x, m), Ty::Ref(y, n)) => m == n && self.unify(x, y),
            (Ty::Array(x), Ty::Array(y))
            | (Ty::View(x), Ty::View(y))
            | (Ty::List(x), Ty::List(y)) => self.unify(x, y),
            (Ty::Fixed(x, n), Ty::Fixed(y, m)) => self.unify(n, m) && self.unify(x, y),
            (Ty::Tuple(xs), Ty::Tuple(ys)) => {
                xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| self.unify(x, y))
            }
            (Ty::Function(xs, x), Ty::Function(ys, y)) => {
                xs.len() == ys.len()
                    && xs.iter().zip(ys).all(|(x, y)| self.unify(x, y))
                    && self.unify(x, y)
            }
            _ => false,
        }
    }
}

fn unref(ty: &Ty) -> Ty {
    match ty {
        Ty::Ref(inner, _) => (**inner).clone(),
        other => other.clone(),
    }
}

/// One searchable form of a node: its parameters and result.
struct Entry<'a> {
    node: &'a Node,
    kind: &'static str,
    origin: &'static str,
    text: String,
    parameters: Vec<Ty>,
    result: Ty,
    /// The constraints of the signature: the class and its type argument.
    constraints: Vec<(String, Ty)>,
}

/// The constraints before `=>` in a signature's type: `Numeric<'a> => …` or
/// `(Copy<'a>, Float<'b>) => …`, each a class and its one type argument.
fn constraints_of(
    text: &str,
    next: &mut usize,
    variables: &mut BTreeMap<String, usize>,
) -> Vec<(String, Ty)> {
    let Some(at) = text.find("=>").filter(|at| !text[..*at].contains("->")) else {
        return Vec::new();
    };
    let list = text[..at].trim();
    let list = list
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(list);
    // Split at the commas outside angle brackets and parentheses.
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (index, character) in list.char_indices() {
        match character {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&list[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[start..]);
    parts
        .into_iter()
        .filter_map(|part| {
            let part = part.trim();
            let (class, argument) = part.split_once('<')?;
            let argument = argument.strip_suffix('>')?;
            Some((class.trim().to_owned(), parse(argument, next, variables)?))
        })
        .collect()
}

/// Whether the constraints of `entry` can hold under `subst`: one whose type is now a primitive
/// fails when the builtin class's rule says no and no instance node gives it.
fn satisfied(entry: &Entry<'_>, subst: &Subst, view: &KgView) -> bool {
    entry.constraints.iter().all(|(class, ty)| {
        let Ty::Named(name, arguments) = subst.resolve(ty) else {
            return true;
        };
        let Some(primitive) = arguments
            .is_empty()
            .then(|| crate::numeric::primitive(&name))
            .flatten()
        else {
            return true;
        };
        crate::check::builtin_instance_by_type(class, &primitive) != Some(false)
            || view.node(&format!("instance:{class}<{name}>")).is_some()
    })
}

fn after_colons(signature: &str) -> String {
    let signature = super::brief::norm(signature);
    match signature.find(" :: ") {
        Some(at) => signature[at + 4..].to_owned(),
        None => signature,
    }
}

/// The written type of a parameter with its convention: a parameter that the checker reads by
/// default (edition 2027) is a borrow.
fn parameter_text(ty: &str, conv: &str) -> String {
    let trimmed = ty.trim_start();
    match conv {
        "ref" if !trimmed.starts_with("ref ") => format!("ref {ty}"),
        "ref_mut" if !trimmed.starts_with("ref mut ") => format!("ref mut {ty}"),
        _ => ty.to_owned(),
    }
}

/// The type that the record or union of a field or case declares, as text (`Box<'a>`) and
/// parsed with its parameters in `variables`, so that the member's type shares them: the accessor
/// of `value: 'a` is `ref Box<'a> -> 'a`, the constructor `Some of 'a` is `'a -> Maybe<'a>`.
/// The owner is named by its ID.
fn owner_type(
    data: &KgData,
    member: &Node,
    next: &mut usize,
    variables: &mut BTreeMap<String, usize>,
) -> (String, Ty) {
    let owner = member.id.rsplit_once('.').map_or("", |(owner, _)| owner);
    let name = last_segment(owner).to_owned();
    // `record Box<'a> {r}`: the name and parameters, without the keyword and the regions.
    let declared = data
        .node(owner)
        .and_then(|node| node.signature.split_once(' '))
        .map(|(_, rest)| rest.split('{').next().unwrap_or(rest).trim().to_owned())
        .filter(|text| !text.is_empty());
    match declared.and_then(|text| Some((parse(&text, next, variables)?, text))) {
        Some((Ty::Named(_, arguments), text)) => (text, Ty::Named(owner.to_owned(), arguments)),
        Some((ty, text)) => (text, ty),
        None => (name, Ty::Named(owner.to_owned(), Vec::new())),
    }
}

fn entries<'a>(data: &'a KgData, origin: &'static str, next: &mut usize) -> Vec<Entry<'a>> {
    let types = Types::of(data);
    let mut found = Vec::new();
    for node in &data.nodes {
        if node.name.starts_with('$') || (origin == "std" && !node.public) {
            continue;
        }
        // The types that the node's signature refers to, as the checker resolved them; a member's
        // are its owner's.
        let owner = node.id.rsplit_once('.').map_or("", |(owner, _)| owner);
        let used: BTreeSet<&str> = [node.id.as_str(), owner]
            .into_iter()
            .flat_map(|id| data.edges_from("uses_type", id))
            .map(|edge| edge.to.as_str())
            .collect();
        let resolve = |name: &str| types.resolve(name, &node.module, &used);
        let typed = |text: &str, next: &mut usize, variables: &mut BTreeMap<String, usize>| {
            parse(text, next, variables).map(|ty| qualify(ty, &resolve))
        };
        let mut variables = BTreeMap::new();
        let mut constraints = Vec::new();
        let (kind, parameters, result, text) = match node.kind.as_str() {
            "def" | "const" | "extern" => {
                let parameters: Option<Vec<Ty>> = node
                    .params
                    .iter()
                    .map(|param| {
                        typed(
                            &parameter_text(&param.ty, &param.conv),
                            next,
                            &mut variables,
                        )
                    })
                    .collect();
                let result = node
                    .result
                    .as_deref()
                    .and_then(|result| typed(result, next, &mut variables));
                let kind = match node.kind.as_str() {
                    "const" => "const",
                    "extern" => "extern",
                    _ => "def",
                };
                let text = after_colons(&node.signature);
                constraints = constraints_of(&text, next, &mut variables);
                (kind, parameters, result, text)
            }
            "field" => {
                let (record, owner) = owner_type(data, node, next, &mut variables);
                let result = node
                    .result
                    .as_deref()
                    .and_then(|result| typed(result, next, &mut variables));
                (
                    "field",
                    Some(vec![Ty::Ref(Box::new(owner), false)]),
                    result,
                    format!("ref {record} -> {}", node.result.as_deref().unwrap_or("?")),
                )
            }
            "case" => {
                let (union, owner) = owner_type(data, node, next, &mut variables);
                let parameters = match node.result.as_deref() {
                    Some(payload) => typed(payload, next, &mut variables).map(|ty| vec![ty]),
                    None => Some(Vec::new()),
                };
                let text = match &node.result {
                    Some(payload) => format!("{payload} -> {union}"),
                    None => union.clone(),
                };
                ("case", parameters, Some(owner), text)
            }
            _ => continue,
        };
        let (Some(parameters), Some(result)) = (parameters, result) else {
            continue;
        };
        found.push(Entry {
            node,
            kind,
            origin,
            text,
            parameters,
            result,
            constraints,
        });
    }
    found
}

/// What the search looks for.
#[derive(Clone, Debug, Default)]
pub struct Query {
    /// A whole type `P1 -> ... -> R`.
    pub text: Option<String>,
    pub result: Option<String>,
    pub takes: Vec<String>,
    pub module: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub tier: &'static str,
    pub extra: usize,
    pub name: String,
    pub kind: &'static str,
    pub module: String,
    pub origin: &'static str,
    pub signature: String,
    /// The number of parameters: G25's `how` is `call` with some, `value` without.
    pub parameters: usize,
}

/// The parse error of a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryTypeError(pub String);

fn tier_rank(tier: &str) -> usize {
    ["exact", "borrow", "partial"]
        .iter()
        .position(|t| *t == tier)
        .unwrap_or(3)
}

fn kind_rank(kind: &str) -> usize {
    ["def", "const", "extern", "field", "case"]
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(5)
}

fn tier_of(
    entry: &Entry<'_>,
    parameters: &[Ty],
    result: &Ty,
    view: &KgView,
) -> Option<(&'static str, usize)> {
    let (n, m) = (entry.parameters.len(), parameters.len());
    if n == m {
        let mut subst = Subst::default();
        if entry
            .parameters
            .iter()
            .zip(parameters)
            .all(|(p, q)| subst.unify(p, q))
            && subst.unify(&entry.result, result)
            && satisfied(entry, &subst, view)
        {
            return Some(("exact", 0));
        }
        let mut subst = Subst::default();
        if entry
            .parameters
            .iter()
            .zip(parameters)
            .all(|(p, q)| subst.unify(&unref(p), &unref(q)))
            && subst.unify(&unref(&entry.result), &unref(result))
            && satisfied(entry, &subst, view)
        {
            return Some(("borrow", 0));
        }
    }
    if n >= m {
        let mut subst = Subst::default();
        if !subst.unify(&unref(&entry.result), &unref(result)) {
            return None;
        }
        let mut used = vec![false; n];
        for q in parameters {
            let mut matched = false;
            for (index, p) in entry.parameters.iter().enumerate() {
                if used[index] {
                    continue;
                }
                let mut attempt = subst.clone();
                if attempt.unify(&unref(p), &unref(q)) {
                    subst = attempt;
                    used[index] = true;
                    matched = true;
                    break;
                }
            }
            if !matched {
                return None;
            }
        }
        return satisfied(entry, &subst, view).then_some(("partial", n - m));
    }
    None
}

/// `--result` and `--takes`: `exact` with the outer `ref`s, `borrow` without them.
fn tier_of_parts(
    entry: &Entry<'_>,
    result: Option<&Ty>,
    takes: &[Ty],
    graph: &KgView,
) -> Option<(&'static str, usize)> {
    for (tier, strip) in [("exact", false), ("borrow", true)] {
        let view = |ty: &Ty| if strip { unref(ty) } else { ty.clone() };
        let mut subst = Subst::default();
        if let Some(result) = result
            && !subst.unify(&view(&entry.result), &view(result))
        {
            continue;
        }
        let mut used = vec![false; entry.parameters.len()];
        let all = takes.iter().all(|q| {
            for (index, p) in entry.parameters.iter().enumerate() {
                if used[index] {
                    continue;
                }
                let mut attempt = subst.clone();
                if attempt.unify(&view(p), &view(q)) {
                    subst = attempt;
                    used[index] = true;
                    return true;
                }
            }
            false
        });
        if all && satisfied(entry, &subst, graph) {
            return Some((tier, 0));
        }
    }
    None
}

/// Whether `--module NAME` names the module `display`: its full name, with `::` or `.`, or its
/// last part.
fn module_named(display: &str, module: &str) -> bool {
    display == module
        || display.rsplit("::").next() == Some(module)
        || display.replace("::", ".") == module
}

/// Whether the project's or std's graph has a module node that `--module NAME` names (a module
/// that declares only types counts; `search` decides what it holds).
pub fn has_module(view: &KgView, module: &str) -> bool {
    [view.project(), view.std()]
        .into_iter()
        .flatten()
        .any(|data| {
            data.nodes
                .iter()
                .any(|node| node.kind == "module" && module_named(&node.module, module))
        })
}

/// The matches of `query` among the project's and std's function-like nodes, and the number of
/// entries searched.
pub fn search(view: &KgView, query: &Query) -> Result<(Vec<Match>, usize), QueryTypeError> {
    let mut next = 0;
    let mut variables = BTreeMap::new();
    let parse_query = |text: &str, next: &mut usize, variables: &mut BTreeMap<String, usize>| {
        parse(text, next, variables)
            .ok_or_else(|| QueryTypeError(format!("cannot parse the type '{text}'")))
    };
    let whole = match &query.text {
        Some(text) => Some(parse_query(text, &mut next, &mut variables)?),
        None => None,
    };
    let result = match &query.result {
        Some(text) => Some(parse_query(text, &mut next, &mut variables)?),
        None => None,
    };
    let takes = query
        .takes
        .iter()
        .map(|text| parse_query(text, &mut next, &mut variables))
        .collect::<Result<Vec<_>, _>>()?;
    let mut all = Vec::new();
    if let Some(data) = view.project() {
        all.extend(entries(data, "user", &mut next));
    }
    if let Some(data) = view.std() {
        all.extend(entries(data, "std", &mut next));
    }
    let module_matches = |entry: &Entry<'_>| match &query.module {
        None => true,
        Some(module) => module_named(&entry.node.module, module),
    };
    let mut searched = 0;
    let mut matches: BTreeMap<(usize, usize, String, usize), Match> = BTreeMap::new();
    for entry in all.iter().filter(|entry| module_matches(entry)) {
        searched += 1;
        let tier = match &whole {
            Some(Ty::Function(parameters, result)) => tier_of(entry, parameters, result, view),
            Some(value) => tier_of(entry, &[], value, view),
            None => tier_of_parts(entry, result.as_ref(), &takes, view),
        };
        let Some((tier, extra)) = tier else {
            continue;
        };
        let key = (
            tier_rank(tier),
            extra,
            entry.node.id.clone(),
            kind_rank(entry.kind),
        );
        matches.entry(key).or_insert_with(|| Match {
            tier,
            extra,
            name: entry.node.id.clone(),
            kind: entry.kind,
            module: entry.node.module.clone(),
            origin: entry.origin,
            signature: entry.text.clone(),
            parameters: entry.parameters.len(),
        });
    }
    // One match per (name, kind): the best tier.
    let mut seen = std::collections::BTreeSet::new();
    let mut found = Vec::new();
    for (_, item) in matches {
        if seen.insert((item.name.clone(), item.kind)) {
            found.push(item);
        }
    }
    Ok((found, searched))
}

/// The text and JSON Lines of a search.
/// The text (at most `budget` characters, `LIST_BUDGET` by default; items are dropped from the
/// end) and the JSON rows (never cut) of the matches.
pub fn render(
    query_text: &str,
    query: &Query,
    found: &[Match],
    searched: usize,
    budget: Option<usize>,
) -> (String, Vec<String>) {
    let shown: Vec<&Match> = found.iter().take(query.limit).collect();
    let header = if found.is_empty() {
        vec!["no matches".to_owned()]
    } else {
        Vec::new()
    };
    let items = shown
        .iter()
        .map(|item| {
            let mut line = format!("{:<7} {} :: {}", item.tier, item.name, item.signature);
            if item.tier == "partial" && item.extra > 0 {
                line.push_str(&format!("  (+{})", item.extra));
            }
            line
        })
        .collect();
    let mut footer = Vec::new();
    if found.len() > shown.len() {
        footer.push(format!(
            "... {} more; raise --limit (at most {})",
            found.len() - shown.len(),
            LIMIT_RANGE.1
        ));
    }
    let text = super::query::listing(
        &header,
        items,
        &footer,
        budget.unwrap_or(super::query::LIST_BUDGET),
    );
    let mut rows: Vec<String> = shown
        .iter()
        .map(|item| {
            let object = Object::new()
                .string("event", "result")
                .string("tier", item.tier)
                .string("kind", item.kind)
                .string("name", &item.name)
                .string("module", &item.module)
                .string("origin", item.origin)
                .string("signature", &item.signature);
            let object = if item.tier == "partial" {
                object.uint("extra", item.extra)
            } else {
                object
            };
            object.finish()
        })
        .collect();
    rows.push(
        Object::new()
            .string("event", "summary")
            .string("query", query_text)
            .uint("matches", found.len())
            .uint("shown", shown.len())
            .uint("limit", query.limit)
            .uint("searched", searched)
            .finish(),
    );
    (text, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(text: &str) -> Ty {
        let mut next = 0;
        parse(text, &mut next, &mut BTreeMap::new()).unwrap()
    }

    #[test]
    fn parses_the_type_syntax_of_signatures() {
        assert_eq!(
            ty("ref [i64] -> i64"),
            Ty::Function(
                vec![Ty::Ref(
                    Box::new(Ty::Array(Box::new(Ty::Named("i64".into(), vec![])))),
                    false
                )],
                Box::new(Ty::Named("i64".into(), vec![]))
            )
        );
        assert!(matches!(
            ty("Numeric<'a> => ref ['a] -> 'a"),
            Ty::Function(..)
        ));
        assert_eq!(
            ty("Geometry::Point"),
            Ty::Named("Geometry::Point".into(), vec![])
        );
        assert!(matches!(ty("(i64 * string)"), Ty::Tuple(_)));
        assert!(matches!(ty("[|i64|]"), Ty::List(_)));
        assert!(matches!(ty("[i64; 4]"), Ty::Fixed(..)));
        assert!(matches!(ty("fn() -> unit"), Ty::Function(..)));
        assert!(matches!(ty("ref mut {r} [i64..]"), Ty::Ref(_, true)));
        assert!(matches!(ty("[i64..]"), Ty::View(_)));
        assert!(matches!(ty("[Geometry.Point..]"), Ty::View(_)));
        // `'a..` is `'a`: both occurrences are one variable.
        match ty("['a..] -> 'a") {
            Ty::Function(parameters, result) => match (&parameters[0], *result) {
                (Ty::View(element), Ty::Var(b)) => assert_eq!(**element, Ty::Var(b)),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        for broken in [
            "ref [i64 -> ",
            "ref [Line",
            "(i64 * string",
            "Maybe<i64",
            "[|i64",
            "[i64; ]",
            "ref {r",
            "{",
            "ref {r [i64]",
        ] {
            let mut next = 0;
            assert!(
                parse(broken, &mut next, &mut BTreeMap::new()).is_none(),
                "{broken}"
            );
        }
    }

    #[test]
    fn unifies_variables_of_both_sides() {
        let mut subst = Subst::default();
        assert!(subst.unify(&ty("'a"), &ty("i64")));
        let mut next = 0;
        let mut variables = BTreeMap::new();
        let a = parse("'a -> 'a", &mut next, &mut variables).unwrap();
        let b = parse("i64 -> string", &mut next, &mut BTreeMap::new()).unwrap();
        assert!(!Subst::default().unify(&a, &b));
        let c = parse("i64 -> i64", &mut next, &mut BTreeMap::new()).unwrap();
        assert!(Subst::default().unify(&a, &c));
    }

    /// Names match when one ends with the other: a qualifier keeps two types apart.
    #[test]
    fn qualified_names_keep_types_apart() {
        assert!(same_name("Acme::Geo.Point", "Geo.Point"));
        assert!(same_name("Point", "Acme::Geo.Point"));
        assert!(same_name("Acme::Geo.Point", "Acme::Geo.Point"));
        assert!(!same_name("A::Point", "B::Point"));
        assert!(!same_name("Acme::Geo.Point", "Nope.Point"));
        assert!(!same_name("MyPoint", "Point"));
        assert!(!same_name("12", "2"));
        assert!(!Subst::default().unify(&ty("A::Point"), &ty("B::Point")));
        assert!(Subst::default().unify(&ty("Maybe<Geo.Point>"), &ty("Maybe<Acme::Geo.Point>")));
    }

    /// A length variable is one length in every place of a signature.
    #[test]
    fn length_variables_are_bound_once() {
        let mut next = 0;
        let same = parse(
            "[i64; N] -> [i64; N] -> i64",
            &mut next,
            &mut BTreeMap::new(),
        )
        .unwrap();
        let apart = parse(
            "[i64; 2] -> [i64; 3] -> i64",
            &mut next,
            &mut BTreeMap::new(),
        )
        .unwrap();
        let equal = parse(
            "[i64; 2] -> [i64; 2] -> i64",
            &mut next,
            &mut BTreeMap::new(),
        )
        .unwrap();
        assert!(!Subst::default().unify(&same, &apart));
        assert!(Subst::default().unify(&same, &equal));
        // `N` of one side and `N` of the other are two variables; `'N` is a type variable.
        let other = parse(
            "[i64; N] -> [i64; M] -> i64",
            &mut next,
            &mut BTreeMap::new(),
        )
        .unwrap();
        assert!(Subst::default().unify(&other, &apart));
        assert!(Subst::default().unify(&ty("[i64; N] -> 'N"), &ty("[i64; 4] -> string")));
        assert!(!Subst::default().unify(&ty("[i64; 4]"), &ty("[i64; 5]")));
    }
}
