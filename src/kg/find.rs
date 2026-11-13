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
use std::rc::Rc;

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
    /// An anonymous union `(A | B)` (A24): its members flattened and sorted by `member_key`, so
    /// that two spellings of one union in different orders usually pair their members by
    /// position; `Subst::solve` tries the other pairings. A type alias is not expanded,
    /// as nowhere else in the search.
    Union(Vec<Ty>),
    Function(Vec<Ty>, Box<Ty>),
}

/// The order of the members of a `Ty::Union`: the text of the member with every variable written
/// `_` and every name by its last segment, so that the order does not depend on how variables are
/// numbered or names qualified. Members that only these tell apart keep their written order.
fn member_key(ty: &Ty) -> String {
    let all = |types: &[Ty], separator: &str| {
        types
            .iter()
            .map(member_key)
            .collect::<Vec<_>>()
            .join(separator)
    };
    match ty {
        Ty::Var(_) => "_".into(),
        Ty::Named(name, arguments) if arguments.is_empty() => last_segment(name).to_owned(),
        Ty::Named(name, arguments) => format!("{}<{}>", last_segment(name), all(arguments, ", ")),
        Ty::Ref(inner, mutable) => {
            format!(
                "ref {}{}",
                if *mutable { "mut " } else { "" },
                member_key(inner)
            )
        }
        Ty::Array(inner) => format!("[{}]", member_key(inner)),
        Ty::View(inner) => format!("[{}..]", member_key(inner)),
        Ty::List(inner) => format!("[|{}|]", member_key(inner)),
        Ty::Fixed(inner, length) => format!("[{}; {}]", member_key(inner), member_key(length)),
        Ty::Tuple(elements) => format!("({})", all(elements, " * ")),
        Ty::Union(members) => format!("({})", all(members, " | ")),
        Ty::Function(parameters, result) => {
            format!("({} -> {})", all(parameters, " -> "), member_key(result))
        }
    }
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

    /// The members after the first of an anonymous union `(first | B | ...)`, whose first `|` is
    /// read: each member is a whole type, as in a signature (`(i64 -> i64 | string)` holds a
    /// function, `(i64 * i64 | string)` a tuple and `(i64 -> i64 * i64 | string)` a function that
    /// returns a tuple). A nested union is flattened.
    fn union(&mut self, first: Ty) -> Option<Ty> {
        let mut members = vec![first];
        loop {
            members.push(self.function()?);
            if !self.eat("|") {
                break;
            }
        }
        if !self.eat(")") {
            return None;
        }
        let mut flat = Vec::new();
        for member in members {
            match member {
                Ty::Union(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        flat.sort_by_cached_key(member_key);
        Some(Ty::Union(flat))
    }

    /// `A * B * C`, a tuple, or a single type. A product binds more tightly than `->`, as in the
    /// compiler's `type_expr`: `i64 -> i64 * i64` returns a tuple and `i64 * i64 -> i64` takes one.
    fn product(&mut self) -> Option<Ty> {
        let mut elements = vec![self.atom()?];
        while self.eat("*") {
            elements.push(self.atom()?);
        }
        Some(if elements.len() == 1 {
            elements.pop().unwrap()
        } else {
            Ty::Tuple(elements)
        })
    }

    /// `A -> B -> R` whose parts are products, or a single product.
    fn function(&mut self) -> Option<Ty> {
        let mut parts = vec![self.product()?];
        while self.eat("->") {
            parts.push(self.product()?);
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
            // The result is a whole type, as in the compiler: `fn() -> A -> B` returns `A -> B`.
            self.eat("->");
            let result = self.function()?;
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
            if self.eat("|") {
                return self.union(first);
            }
            return self.eat(")").then_some(first);
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
        Ty::Union(members) => Ty::Union(all(members)),
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
            Ty::Named(_, args) | Ty::Tuple(args) | Ty::Union(args) => {
                args.iter().any(|arg| self.occurs(id, arg))
            }
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

    /// `ty` with the variables that the substitution binds looked through at the top only.
    fn shallow<'t>(&'t self, mut ty: &'t Ty) -> &'t Ty {
        while let Ty::Var(id) = ty {
            match self.0.get(id) {
                Some(bound) => ty = bound,
                None => break,
            }
        }
        ty
    }

    /// Calls `visit` with each variable of `ty` under the substitution, once per occurrence.
    fn each_variable(&self, ty: &Ty, visit: &mut dyn FnMut(usize)) {
        match self.shallow(ty) {
            Ty::Var(id) => visit(*id),
            Ty::Named(_, types) | Ty::Tuple(types) | Ty::Union(types) => {
                for ty in types {
                    self.each_variable(ty, visit);
                }
            }
            Ty::Ref(inner, _) | Ty::Array(inner) | Ty::View(inner) | Ty::List(inner) => {
                self.each_variable(inner, visit);
            }
            Ty::Fixed(inner, length) => {
                self.each_variable(inner, visit);
                self.each_variable(length, visit);
            }
            Ty::Function(parameters, result) => {
                for parameter in parameters {
                    self.each_variable(parameter, visit);
                }
                self.each_variable(result, visit);
            }
        }
    }

    /// `ty` under the substitution, with every variable for which `alone` holds written as one
    /// placeholder.
    fn shape(&self, ty: &Ty, alone: &dyn Fn(usize) -> bool) -> Ty {
        let all = |types: &[Ty]| types.iter().map(|ty| self.shape(ty, alone)).collect();
        let one = |ty: &Ty| Box::new(self.shape(ty, alone));
        match self.shallow(ty) {
            Ty::Var(id) if alone(*id) => Ty::Var(usize::MAX),
            Ty::Var(id) => Ty::Var(*id),
            Ty::Named(name, arguments) => Ty::Named(name.clone(), all(arguments)),
            Ty::Ref(inner, mutable) => Ty::Ref(one(inner), *mutable),
            Ty::Array(inner) => Ty::Array(one(inner)),
            Ty::View(inner) => Ty::View(one(inner)),
            Ty::List(inner) => Ty::List(one(inner)),
            Ty::Fixed(inner, length) => Ty::Fixed(one(inner), one(length)),
            Ty::Tuple(elements) => Ty::Tuple(all(elements)),
            Ty::Union(members) => Ty::Union(all(members)),
            Ty::Function(parameters, result) => Ty::Function(all(parameters), one(result)),
        }
    }

    /// Whether `a` and `b` unify: `solve` with one pair and no further check (the tests' form).
    #[cfg(test)]
    fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        self.solve(&[Goal::Same(a.clone(), b.clone())], &[], &|_| true)
    }

    /// Whether `goals` all hold, each under the bindings of those before it, and `holds` then
    /// accepts the substitution, which it reads only through the types of `watched`.
    ///
    /// The members of two anonymous unions are paired first by position (`member_key` sorts both
    /// sides alike); the other pairings serve the members that the key does not tell apart or
    /// order alike (`A.Point` and `B.Point`, a name beside its qualified form, a variable, which
    /// may be any member). A pairing stays open until the whole comparison holds: when a later
    /// part or `holds` fails, the newest choice takes its next partner, so that a variable that a
    /// union shares with a later parameter is bound by the pairing that suits both. Two rules
    /// leave out pairings that cannot change the answer: a pairing of two unions whose variables
    /// occur nowhere after them and not in `watched` is kept once found (`Goal::Cut`), and a
    /// member takes only the first of partners that are the same type up to variables that occur
    /// nowhere else, such as `_` (`Subst::twins`). `Goal::Assign` gives each wanted type a
    /// parameter greedily, as the `partial` tier always has, and gives it again after each
    /// change of a pairing before it.
    ///
    /// A second pass keeps the first pairing of every union, as the matcher did before it
    /// searched across the comparison (`commit`), so that every match that the earlier matcher
    /// found is found: its greedy assignment can take another parameter than the first pass's
    /// (one that only another pairing allows is not taken), and it does not spend attempts on
    /// the other pairings of a union whose pairing is complete. It runs only when the first pass
    /// failed after it went back into such a union (`Goal::Done`); until then both passes take
    /// the same steps, so the second would fail as well.
    ///
    /// Each pass tries at most `MEMBER_ATTEMPTS` member pairs; past them its answer is no. The
    /// search is a loop over a stack of goals and of choices, so that the call stack does not
    /// grow with the size of the types. On success the substitution is the one that `holds`
    /// accepted; otherwise it is as it was.
    fn solve(&mut self, goals: &[Goal], watched: &[Ty], holds: &dyn Fn(&Subst) -> bool) -> bool {
        let mut reopened = false;
        self.search(goals, watched, holds, false, &mut reopened)
            || (reopened && self.search(goals, watched, holds, true, &mut reopened))
    }

    /// One pass of `solve`. With `commit`, every pairing of union members is kept once it holds;
    /// without it, only one that nothing after it reads, and `reopened` is set when the pass
    /// tries another pairing of a union whose pairing was complete.
    fn search(
        &mut self,
        goals: &[Goal],
        watched: &[Ty],
        holds: &dyn Fn(&Subst) -> bool,
        commit: bool,
        reopened: &mut bool,
    ) -> bool {
        let before = self.0.clone();
        let mut goals: Vec<Goal> = goals.iter().rev().cloned().collect();
        let mut choices: Vec<Choice> = Vec::new();
        let mut attempts = MEMBER_ATTEMPTS;
        loop {
            let going = match goals.pop() {
                None if holds(self) => return true,
                None => false,
                Some(Goal::Same(a, b)) => self.step(a, b, &mut goals),
                Some(Goal::Union(xs, ys)) => {
                    let (unseen, alone) = self.survey(&xs, &ys, &goals, watched);
                    goals.push(if commit || unseen {
                        Goal::Cut(choices.len())
                    } else {
                        Goal::Done(choices.len())
                    });
                    goals.push(Goal::Members(xs, ys, Rc::new(alone)));
                    true
                }
                Some(Goal::Members(mut xs, partners, alone)) => {
                    if xs.is_empty() {
                        true
                    } else {
                        // The first partner is tried as the next choice below.
                        let first = xs.remove(0);
                        let twins = self.twins(&partners, &alone);
                        choices.push(Choice {
                            bindings: self.0.clone(),
                            goals: std::mem::take(&mut goals),
                            pick: Pick::Partner {
                                first,
                                others: xs,
                                partners,
                                twins,
                                alone,
                            },
                            next: 0,
                            done: false,
                        });
                        false
                    }
                }
                Some(Goal::Cut(depth)) => {
                    choices.truncate(depth);
                    true
                }
                Some(Goal::Done(depth)) => {
                    for choice in &mut choices[depth..] {
                        choice.done = true;
                    }
                    true
                }
                Some(Goal::Assign(wanted, free)) => {
                    if wanted.is_empty() {
                        true
                    } else {
                        choices.push(Choice {
                            bindings: self.0.clone(),
                            goals: std::mem::take(&mut goals),
                            pick: Pick::Parameter { wanted, free },
                            next: 0,
                            done: false,
                        });
                        false
                    }
                }
                Some(Goal::Keep(depth)) => {
                    let choice = &mut choices[depth];
                    debug_assert!(matches!(choice.pick, Pick::Parameter { .. }));
                    choice.next = usize::MAX;
                    true
                }
            };
            if going {
                continue;
            }
            // Back to the newest choice with an alternative left to try.
            loop {
                let depth = choices.len();
                let Some(choice) = choices.last_mut() else {
                    self.0 = before;
                    return false;
                };
                let resumed = match &choice.pick {
                    Pick::Partner {
                        first,
                        others,
                        partners,
                        twins,
                        alone,
                    } => (choice.next..partners.len())
                        .find(|&index| !twins[index])
                        .map(|index| {
                            let mut rest = partners.clone();
                            let partner = rest.remove(index);
                            let mut goals = choice.goals.clone();
                            goals.push(Goal::Members(others.clone(), rest, alone.clone()));
                            goals.push(Goal::Same(first.clone(), partner));
                            (index, goals)
                        }),
                    Pick::Parameter { wanted, free } => (choice.next < free.len()).then(|| {
                        let mut rest = free.clone();
                        let parameter = rest.remove(choice.next);
                        let mut goals = choice.goals.clone();
                        goals.push(Goal::Assign(wanted[1..].to_vec(), rest));
                        goals.push(Goal::Keep(depth - 1));
                        goals.push(Goal::Same(parameter, wanted[0].clone()));
                        (choice.next, goals)
                    }),
                };
                let Some((index, resumed)) = resumed else {
                    choices.pop();
                    continue;
                };
                if let Pick::Partner { .. } = choice.pick {
                    if attempts == 0 {
                        self.0 = before;
                        return false;
                    }
                    attempts -= 1;
                    *reopened |= choice.done;
                }
                choice.next = index + 1;
                self.0 = choice.bindings.clone();
                goals = resumed;
                break;
            }
        }
    }

    /// What `search` reads of two unions with the members `xs` and `ys` before it pairs them:
    /// whether none of their variables occurs in `goals` (what is left after them) or in
    /// `watched`, so that no pairing of their members can change what follows and the first that
    /// holds is as good as any; and the variables that occur once in the whole comparison. Such a variable stays alone while the members pair: only the
    /// unification of its own member can put it into another type.
    fn survey(
        &self,
        xs: &[Ty],
        ys: &[Ty],
        goals: &[Goal],
        watched: &[Ty],
    ) -> (bool, BTreeSet<usize>) {
        let mut inside: BTreeMap<usize, usize> = BTreeMap::new();
        for member in xs.iter().chain(ys) {
            self.each_variable(member, &mut |id| *inside.entry(id).or_default() += 1);
        }
        let mut outside = BTreeSet::new();
        if !inside.is_empty() {
            let mut visit = |id| {
                if inside.contains_key(&id) {
                    outside.insert(id);
                }
            };
            for ty in watched {
                self.each_variable(ty, &mut visit);
            }
            for goal in goals {
                goal.each_type(&mut |ty| self.each_variable(ty, &mut visit));
            }
        }
        let alone = inside
            .iter()
            .filter(|&(id, count)| *count == 1 && !outside.contains(id))
            .map(|(id, _)| *id)
            .collect();
        (outside.is_empty(), alone)
    }

    /// For each of `partners`, whether an earlier one is the same type when every variable of
    /// `alone` (those that occur once in the whole comparison, such as `_`) is read as one
    /// placeholder. Pairing a member with such a partner searches what the earlier one searched
    /// with those variables renamed, so it is left out.
    fn twins(&self, partners: &[Ty], alone: &BTreeSet<usize>) -> Vec<bool> {
        if partners.len() < 2 {
            return vec![false; partners.len()];
        }
        let shapes: Vec<Ty> = partners
            .iter()
            .map(|partner| self.shape(partner, &|id| alone.contains(&id)))
            .collect();
        (0..shapes.len())
            .map(|index| shapes[..index].contains(&shapes[index]))
            .collect()
    }

    /// One step of `solve`: whether `a` and `b` agree at the top, binding a variable, with the
    /// pairs of their parts pushed on `goals` so that the first part is unified next.
    fn step(&mut self, a: Ty, b: Ty, goals: &mut Vec<Goal>) -> bool {
        let pairs = |goals: &mut Vec<Goal>, xs: Vec<Ty>, ys: Vec<Ty>| {
            goals.extend(xs.into_iter().zip(ys).rev().map(|(x, y)| Goal::Same(x, y)));
        };
        match (self.resolve(&a), self.resolve(&b)) {
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (Ty::Var(x), other) | (other, Ty::Var(x)) => {
                if self.occurs(x, &other) {
                    return false;
                }
                self.0.insert(x, other);
                true
            }
            (Ty::Named(x, xs), Ty::Named(y, ys)) => {
                let agree = same_name(&x, &y) && xs.len() == ys.len();
                if agree {
                    pairs(goals, xs, ys);
                }
                agree
            }
            (Ty::Ref(x, m), Ty::Ref(y, n)) => {
                if m == n {
                    goals.push(Goal::Same(*x, *y));
                }
                m == n
            }
            (Ty::Array(x), Ty::Array(y))
            | (Ty::View(x), Ty::View(y))
            | (Ty::List(x), Ty::List(y)) => {
                goals.push(Goal::Same(*x, *y));
                true
            }
            (Ty::Fixed(x, n), Ty::Fixed(y, m)) => {
                // The lengths first.
                goals.push(Goal::Same(*x, *y));
                goals.push(Goal::Same(*n, *m));
                true
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) => {
                let agree = xs.len() == ys.len();
                if agree {
                    pairs(goals, xs, ys);
                }
                agree
            }
            (Ty::Union(xs), Ty::Union(ys)) => {
                let agree = xs.len() == ys.len();
                if agree {
                    goals.push(Goal::Union(xs, ys));
                }
                agree
            }
            (Ty::Function(xs, x), Ty::Function(ys, y)) => {
                // The parameters first, then the result.
                let agree = xs.len() == ys.len();
                if agree {
                    goals.push(Goal::Same(*x, *y));
                    pairs(goals, xs, ys);
                }
                agree
            }
            _ => false,
        }
    }
}

/// The most member pairs that one pass of `Subst::solve` tries.
const MEMBER_ATTEMPTS: usize = 4096;

/// What `Subst::solve` has left to do.
#[derive(Clone)]
enum Goal {
    /// Two types to unify.
    Same(Ty, Ty),
    /// The members of two anonymous unions with as many members, before their pairing starts.
    Union(Vec<Ty>, Vec<Ty>),
    /// The members of an anonymous union left to pair, their partners left in the other, and
    /// the variables that occur once in the comparison (`Subst::survey`).
    Members(Vec<Ty>, Vec<Ty>, Rc<BTreeSet<usize>>),
    /// The end of a pairing that nothing after it reads: the choices from this depth up, made
    /// since it started, are dropped.
    Cut(usize),
    /// The end of a pairing that something after it reads: the choices from this depth up are
    /// kept and marked `done`.
    Done(usize),
    /// The wanted types of the `partial` tier or of `--takes` left to place, and the parameters
    /// that no earlier one took, in order: each wanted type takes the first parameter that it
    /// unifies with.
    Assign(Vec<Ty>, Vec<Ty>),
    /// The parameter of the choice at this depth unified: the parameters after it are not tried.
    Keep(usize),
}

impl Goal {
    fn each_type(&self, visit: &mut dyn FnMut(&Ty)) {
        match self {
            Goal::Same(a, b) => {
                visit(a);
                visit(b);
            }
            Goal::Union(xs, ys) | Goal::Members(xs, ys, _) | Goal::Assign(xs, ys) => {
                xs.iter().chain(ys).for_each(visit);
            }
            Goal::Cut(_) | Goal::Done(_) | Goal::Keep(_) => {}
        }
    }
}

/// A choice of `Subst::solve` to come back to: the bindings and goals from before it, what it
/// picks from, and the next alternative to try.
struct Choice {
    bindings: BTreeMap<usize, Ty>,
    goals: Vec<Goal>,
    pick: Pick,
    next: usize,
    /// Whether the pairing of the union that the choice belongs to was once complete.
    done: bool,
}

/// What a `Choice` picks.
enum Pick {
    /// The partner of the member `first` among `partners`, with `others` to pair after it;
    /// `twins` marks the partners that `Subst::twins` leaves out, and `alone` is that of
    /// `Goal::Members`.
    Partner {
        first: Ty,
        others: Vec<Ty>,
        partners: Vec<Ty>,
        twins: Vec<bool>,
        alone: Rc<BTreeSet<usize>>,
    },
    /// The parameter for `wanted[0]` among `free`; the first that unifies is kept (`Goal::Keep`).
    Parameter { wanted: Vec<Ty>, free: Vec<Ty> },
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

impl Entry<'_> {
    /// The type arguments of the constraints: what `satisfied` reads of a substitution.
    fn constrained(&self) -> Vec<Ty> {
        self.constraints.iter().map(|(_, ty)| ty.clone()).collect()
    }
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
    // One comparison of the parameters, the result and the constraints, so that a pairing of
    // union members stays open until all of them hold (`Subst::solve`).
    let watched = entry.constrained();
    let holds = |subst: &Subst| satisfied(entry, subst, view);
    if n == m {
        for (tier, strip) in [("exact", false), ("borrow", true)] {
            let side = |ty: &Ty| if strip { unref(ty) } else { ty.clone() };
            let goals: Vec<Goal> = entry
                .parameters
                .iter()
                .zip(parameters)
                .chain([(&entry.result, result)])
                .map(|(p, q)| Goal::Same(side(p), side(q)))
                .collect();
            if Subst::default().solve(&goals, &watched, &holds) {
                return Some((tier, 0));
            }
        }
    }
    if n >= m {
        let goals = vec![
            Goal::Same(unref(&entry.result), unref(result)),
            Goal::Assign(
                parameters.iter().map(unref).collect(),
                entry.parameters.iter().map(unref).collect(),
            ),
        ];
        return Subst::default()
            .solve(&goals, &watched, &holds)
            .then_some(("partial", n - m));
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
    let watched = entry.constrained();
    for (tier, strip) in [("exact", false), ("borrow", true)] {
        let side = |ty: &Ty| if strip { unref(ty) } else { ty.clone() };
        let mut goals = Vec::new();
        if let Some(result) = result {
            goals.push(Goal::Same(side(&entry.result), side(result)));
        }
        goals.push(Goal::Assign(
            takes.iter().map(side).collect(),
            entry.parameters.iter().map(side).collect(),
        ));
        if Subst::default().solve(&goals, &watched, &|subst| satisfied(entry, subst, graph)) {
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

    /// A24: `(A | B)` reads as one type whatever the order of its members, flattened; the members
    /// are whole types, and a union never matches a non-union.
    #[test]
    fn find_reads_anonymous_unions() {
        let named = |name: &str| Ty::Named(name.into(), vec![]);
        assert_eq!(
            ty("(string | i64)"),
            Ty::Union(vec![named("i64"), named("string")])
        );
        assert_eq!(ty("(i64 | (bool | string))"), ty("((string | i64) | bool)"));
        assert_eq!(
            ty("(i64 -> i64 | i64 * i64)"),
            Ty::Union(vec![
                Ty::Tuple(vec![named("i64"), named("i64")]),
                Ty::Function(vec![named("i64")], Box::new(named("i64"))),
            ])
        );
        assert!(matches!(
            ty("Maybe<(i64 | string)> -> (bool | Shapes.Point)"),
            Ty::Function(..)
        ));
        // The order does not depend on qualification or on how variables are numbered.
        let keys = |text: &str| match ty(text) {
            Ty::Union(members) => members.iter().map(member_key).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            keys("(Shapes.Point | Maybe<'z> | char)"),
            ["Maybe<_>", "Point", "char"]
        );
        assert_eq!(
            keys("(char | Point | Maybe<'a>)"),
            ["Maybe<_>", "Point", "char"]
        );
        let mut subst = Subst::default();
        assert!(subst.unify(&ty("(string | i64)"), &ty("(i64 | string)")));
        assert!(subst.unify(&ty("'a"), &ty("(i64 | string)")));
        assert!(!subst.unify(&ty("(i64 | string)"), &ty("(i64 * string)")));
        assert!(!subst.unify(&ty("(i64 | string)"), &ty("(i64 | string | bool)")));
        assert!(!subst.unify(&ty("(i64 | string)"), &ty("i64")));
        for broken in ["(i64 | string", "(i64 |)", "(| i64)", "(i64 | string]"] {
            let mut next = 0;
            assert!(
                parse(broken, &mut next, &mut BTreeMap::new()).is_none(),
                "{broken}"
            );
        }
    }

    /// A24 (review round 1): the members pair in any order, also where the sort of `member_key`
    /// cannot put both sides in one order: names that differ only in their qualifiers, a name
    /// beside its qualified form, and a variable. A pairing that fails binds nothing.
    #[test]
    fn find_pairs_union_members_in_any_order() {
        let unifies = |left: &str, right: &str| {
            let mut next = 0;
            let left = parse(left, &mut next, &mut BTreeMap::new()).unwrap();
            let right = parse(right, &mut next, &mut BTreeMap::new()).unwrap();
            Subst::default().unify(&left, &right)
        };
        for (left, right) in [
            ("(Shapes.Point | Main.Point)", "(Main.Point | Shapes.Point)"),
            ("(Main.Point | Shapes.Point)", "(Shapes.Point | Main.Point)"),
            // The entry's names are qualified; the query's need not be.
            ("(Shapes.Point | Point)", "(Alpha.Point | Shapes.Point)"),
            ("(Point | Shapes.Point)", "(Shapes.Point | Zeta.Point)"),
            (
                "(Shapes.Point | Point | i64)",
                "(i64 | Zeta.Point | Shapes.Point)",
            ),
            // `_` sorts between the names; it is whichever member is left.
            ("(_ | string)", "(string | unit)"),
            ("(_ | string)", "(Point | string)"),
            ("('a | Maybe<'a>)", "(Maybe<i64> | i64)"),
            (
                "(Maybe<'a> | Maybe<i64> | 'a)",
                "(Maybe<i64> | string | Maybe<string>)",
            ),
            (
                "Maybe<(B.Point | A.Point)> -> i64",
                "Maybe<(A.Point | B.Point)> -> i64",
            ),
        ] {
            assert!(unifies(left, right), "{left} / {right}");
            assert!(unifies(right, left), "{right} / {left}");
        }
        for (left, right) in [
            ("(Shapes.Point | Main.Point)", "(Main.Point | Other.Point)"),
            ("(Main.Point | Main.Point)", "(Main.Point | Shapes.Point)"),
            ("('a | Maybe<'a>)", "(Maybe<i64> | string)"),
            ("(_ | string)", "(i64 | unit)"),
            ("(_ | string)", "(i64 | string | unit)"),
        ] {
            assert!(!unifies(left, right), "{left} / {right}");
            assert!(!unifies(right, left), "{right} / {left}");
        }
        // A failed comparison leaves the substitution as it was.
        let mut next = 0;
        let mut variables = BTreeMap::new();
        let query = parse("('a | Maybe<'a>)", &mut next, &mut variables).unwrap();
        let variable = parse("'a", &mut next, &mut variables).unwrap();
        let mut subst = Subst::default();
        assert!(!subst.unify(&query, &ty("(Maybe<i64> | string)")));
        assert_eq!(subst.resolve(&variable), variable);
        assert!(subst.unify(&query, &ty("(bool | Maybe<bool>)")));
        assert_eq!(subst.resolve(&variable), ty("bool"));
        // Many members that nothing tells apart stop at the attempt limit, with an answer.
        let wide = |last: &str| {
            let members: Vec<String> = (0..12).map(|_| "Maybe<_>".to_owned()).collect();
            format!("({} | {last})", members.join(" | "))
        };
        assert!(unifies(&wide("i64"), &wide("i64")));
        assert!(!unifies(&wide("i64"), &wide("string")));
    }

    /// Copilot review 1: `*` binds more tightly than `->` everywhere, as in the compiler's
    /// `type_expr` (products between the arrows): `i64 -> i64 * i64` returns a tuple, also as a
    /// member of an anonymous union, in brackets, in type arguments and after `fn()`.
    #[test]
    fn products_bind_more_tightly_than_arrows() {
        let named = |name: &str| Ty::Named(name.into(), vec![]);
        let pair = || Ty::Tuple(vec![named("i64"), named("i64")]);
        let returns_pair = || Ty::Function(vec![named("i64")], Box::new(pair()));
        assert_eq!(ty("i64 -> i64 * i64"), returns_pair());
        assert_eq!(ty("(i64 -> i64 * i64)"), returns_pair());
        assert_eq!(
            ty("i64 * i64 -> i64"),
            Ty::Function(vec![pair()], Box::new(named("i64")))
        );
        // Parentheses still group.
        assert_eq!(
            ty("(i64 -> i64) * i64"),
            Ty::Tuple(vec![
                Ty::Function(vec![named("i64")], Box::new(named("i64"))),
                named("i64"),
            ])
        );
        // `(i64 -> (i64 * i64))` sorts before `bool` (`(` before `b`).
        assert_eq!(
            ty("(bool | i64 -> i64 * i64) -> unit"),
            Ty::Function(
                vec![Ty::Union(vec![returns_pair(), named("bool")])],
                Box::new(named("unit"))
            )
        );
        assert_eq!(ty("[i64 * i64]"), Ty::Array(Box::new(pair())));
        assert_eq!(
            ty("Maybe<i64 * i64>"),
            Ty::Named("Maybe".into(), vec![pair()])
        );
        assert_eq!(
            ty("fn() -> i64 * i64"),
            Ty::Function(vec![], Box::new(pair()))
        );
        // The result of `fn()` is a whole type (`type_expr` in the compiler).
        assert_eq!(
            ty("fn() -> i64 -> i64"),
            Ty::Function(
                vec![],
                Box::new(Ty::Function(vec![named("i64")], Box::new(named("i64"))))
            )
        );
        assert_eq!(
            ty("(fn() -> i64) -> i64"),
            Ty::Function(
                vec![Ty::Function(vec![], Box::new(named("i64")))],
                Box::new(named("i64"))
            )
        );
        assert_eq!(
            ty("(fn() -> i64 | string)"),
            Ty::Union(vec![
                Ty::Function(vec![], Box::new(named("i64"))),
                named("string"),
            ])
        );
        // The query against the signature as the knowledge graph writes it (`docgen::type_text`
        // puts every tuple and function in parentheses).
        let unifies = |left: &str, right: &str| Subst::default().unify(&ty(left), &ty(right));
        assert!(unifies(
            "(bool | i64 -> i64 * i64) -> unit",
            "(bool | (i64 -> (i64 * i64))) -> unit"
        ));
        assert!(!unifies(
            "(bool | i64 -> i64 * i64) -> unit",
            "(bool | ((i64 -> i64) * i64)) -> unit"
        ));
        assert!(unifies(
            "(bool | (i64 -> i64) * i64) -> unit",
            "(bool | ((i64 -> i64) * i64)) -> unit"
        ));
        assert!(unifies(
            "(string | i64 * i64 -> i64) -> unit",
            "(((i64 * i64) -> i64) | string) -> unit"
        ));
    }

    /// Copilot review 1: a pairing of union members stays open until the whole comparison holds,
    /// so that a variable that a union shares with a later part is bound by the pairing that
    /// suits both, in a function, a tuple, a type argument and across two unions.
    #[test]
    fn member_pairings_stay_open_until_the_comparison_holds() {
        let unifies = |left: &str, right: &str| {
            let mut next = 0;
            let left = parse(left, &mut next, &mut BTreeMap::new()).unwrap();
            let right = parse(right, &mut next, &mut BTreeMap::new()).unwrap();
            Subst::default().unify(&left, &right)
        };
        for (left, right) in [
            // By position `'a` is `i64`; `string` after it wants the other pairing.
            (
                "(Maybe<'a> | Maybe<'b>) -> 'a -> i64",
                "(Maybe<i64> | Maybe<string>) -> string -> i64",
            ),
            (
                "(Maybe<'a> | Maybe<'b>) -> 'b -> i64",
                "(Maybe<i64> | Maybe<string>) -> string -> i64",
            ),
            (
                "((Maybe<'a> | Maybe<'b>) * 'a)",
                "((Maybe<i64> | Maybe<string>) * string)",
            ),
            (
                "Maybe<(Maybe<'a> | Maybe<'b>)> -> 'a -> i64",
                "Maybe<(Maybe<i64> | Maybe<string>)> -> string -> i64",
            ),
            // `'c` is `bool`, so the second union gives `'b = i64`, and the first `'a = string`.
            (
                "((Maybe<'a> | Maybe<'b>) * (Maybe<'b> | Maybe<'c>) * 'c)",
                "((Maybe<i64> | Maybe<string>) * (Maybe<i64> | Maybe<bool>) * bool)",
            ),
        ] {
            assert!(unifies(left, right), "{left} / {right}");
            assert!(unifies(right, left), "{right} / {left}");
        }
        for (left, right) in [
            // `'a` would be `string` (the parameter) and `i64` (the result).
            (
                "(Maybe<'a> | Maybe<'b>) -> 'a -> 'a",
                "(Maybe<i64> | Maybe<string>) -> string -> i64",
            ),
            (
                "((Maybe<'a> | Maybe<'b>) * 'a)",
                "((Maybe<i64> | Maybe<string>) * bool)",
            ),
            // `'c = i64` gives `'b = bool`, which the first union does not hold.
            (
                "((Maybe<'a> | Maybe<'b>) * (Maybe<'b> | Maybe<'c>) * 'c)",
                "((Maybe<i64> | Maybe<string>) * (Maybe<i64> | Maybe<bool>) * i64)",
            ),
        ] {
            assert!(!unifies(left, right), "{left} / {right}");
            assert!(!unifies(right, left), "{right} / {left}");
        }
        // The substitution after a success is the pairing that suits the whole comparison.
        let mut next = 0;
        let mut variables = BTreeMap::new();
        let query = parse(
            "(Maybe<'a> | Maybe<'b>) -> 'a -> i64",
            &mut next,
            &mut variables,
        )
        .unwrap();
        let a = parse("'a", &mut next, &mut variables).unwrap();
        let b = parse("'b", &mut next, &mut variables).unwrap();
        let entry = parse(
            "(Maybe<i64> | Maybe<string>) -> string -> i64",
            &mut next,
            &mut BTreeMap::new(),
        )
        .unwrap();
        let mut subst = Subst::default();
        assert!(subst.unify(&query, &entry));
        assert_eq!(subst.resolve(&a), ty("string"));
        assert_eq!(subst.resolve(&b), ty("i64"));
    }

    /// Copilot review 1: the check after the goals (the entry's constraints in `tier_of`) also
    /// reopens the pairings, and one attempt limit covers the whole comparison.
    #[test]
    fn solve_tries_other_pairings_until_the_check_holds() {
        let mut next = 0;
        let mut variables = BTreeMap::new();
        let query = parse("(Maybe<'a> | Maybe<'b>)", &mut next, &mut variables).unwrap();
        let a = parse("'a", &mut next, &mut variables).unwrap();
        let entry = parse("(Maybe<f64> | Maybe<i64>)", &mut next, &mut BTreeMap::new()).unwrap();
        // By position `'a` is `f64` (`Maybe<f64>` sorts first); the check wants `i64`.
        let seen = std::cell::RefCell::new(Vec::new());
        let wants_i64 = |subst: &Subst| {
            seen.borrow_mut().push(subst.resolve(&a));
            subst.resolve(&a) == ty("i64")
        };
        // The check reads `'a`, so the pairing that binds it stays open.
        let watched = std::slice::from_ref(&a);
        let mut subst = Subst::default();
        assert!(subst.solve(
            &[Goal::Same(query.clone(), entry.clone())],
            watched,
            &wants_i64
        ));
        assert_eq!(*seen.borrow(), [ty("f64"), ty("i64")]);
        assert_eq!(subst.resolve(&a), ty("i64"));
        // A check that no pairing passes leaves the substitution as it was.
        let mut subst = Subst::default();
        assert!(!subst.solve(&[Goal::Same(query, entry)], watched, &|_| false));
        assert_eq!(subst.resolve(&a), a);
        // Two wide unions whose comparison fails only after them still answer within the one limit.
        let unifies = |left: &str, right: &str| {
            let mut next = 0;
            let left = parse(left, &mut next, &mut BTreeMap::new()).unwrap();
            let right = parse(right, &mut next, &mut BTreeMap::new()).unwrap();
            Subst::default().unify(&left, &right)
        };
        let members: Vec<String> = (0..12).map(|_| "Maybe<_>".to_owned()).collect();
        let wide = format!("({} | i64)", members.join(" | "));
        assert!(unifies(
            &format!("({wide} * {wide} * bool)"),
            &format!("({wide} * {wide} * bool)")
        ));
        assert!(!unifies(
            &format!("({wide} * {wide} * bool)"),
            &format!("({wide} * {wide} * string)")
        ));
    }

    /// The `partial` tier's comparison: the entry's `result` with the query's, then the wanted
    /// types of the query among the entry's `parameters` (`Goal::Assign`).
    fn assigns(result: (&str, &str), parameters: &[&str], wanted: &[&str]) -> bool {
        let mut next = 0;
        let (mut entry, mut query) = (BTreeMap::new(), BTreeMap::new());
        let mut goals = vec![Goal::Same(
            parse(result.0, &mut next, &mut entry).unwrap(),
            parse(result.1, &mut next, &mut query).unwrap(),
        )];
        let free = parameters
            .iter()
            .map(|text| parse(text, &mut next, &mut entry).unwrap())
            .collect();
        let wanted = wanted
            .iter()
            .map(|text| parse(text, &mut next, &mut query).unwrap())
            .collect();
        goals.push(Goal::Assign(wanted, free));
        Subst::default().solve(&goals, &[], &|_| true)
    }

    /// Copilot review 1, verification: `Goal::Assign` gives each wanted type the first free
    /// parameter that it unifies with under the bindings so far, as the `partial` tier always has,
    /// and gives it again under each other pairing of the unions before it.
    #[test]
    fn parameters_are_assigned_again_under_each_pairing() {
        let maybes = ("(Maybe<i64> | Maybe<string>)", "(Maybe<'a> | Maybe<'b>)");
        // By position `'a = i64`: `'a` passes over `string`, and `string` is left for `string`.
        assert!(assigns(maybes, &["string", "i64"], &["'a", "string"]));
        assert!(assigns(
            maybes,
            &["string", "i64", "bool"],
            &["'a", "string"]
        ));
        assert!(assigns(maybes, &["i64", "string"], &["'b", "'a"]));
        // `'a = i64` (by position) leaves no parameter for `'a`; the other pairing gives it `string`.
        assert!(assigns(
            ("i64", "i64"),
            &["(Maybe<i64> | Maybe<string>)", "string"],
            &["(Maybe<'a> | Maybe<'b>)", "'a"]
        ));
        // Nothing binds `'a` first, so it takes `string`, and `string` finds no parameter left:
        // the assignment itself is greedy, with or without a union.
        assert!(!assigns(
            ("unit", "unit"),
            &["string", "i64"],
            &["'a", "string"]
        ));
        assert!(!assigns(
            ("(Maybe<i64> | Maybe<bool>)", "(Maybe<'b> | Maybe<'c>)"),
            &["string", "i64"],
            &["'a", "string"]
        ));
        assert!(!assigns(
            ("unit", "unit"),
            &["string"],
            &["string", "string"]
        ));
    }

    /// Copilot review 1, verification: when the search fails, a second pass keeps the first
    /// pairing of every union, as the matcher did before, so that what it found is still found.
    #[test]
    fn a_second_pass_keeps_the_first_pairings() {
        // The nested union's variables occur in the result, so its 7! pairings stay open and use
        // up the attempts before the outer union's other pairing; kept by position, the nested
        // union lets the outer union pair again at once (`'a = Result<…, Maybe<i64>>`), and the
        // result `('p | … | 'v)` is then the entry's members in some order.
        let seven = "(i64 | string | bool | f64 | char | i8 | i32)";
        let entry = format!(
            "(Result<{seven}, Maybe<i64>> | Result<{seven}, Result<{seven}, Maybe<i64>>>) -> {seven}"
        );
        let names = "('p | 'q | 'r | 's | 't | 'u | 'v)";
        let query = format!("(Result<{names}, 'a> | 'a) -> {names}");
        let unifies = |left: &str, right: &str| {
            let mut next = 0;
            let left = parse(left, &mut next, &mut BTreeMap::new()).unwrap();
            let right = parse(right, &mut next, &mut BTreeMap::new()).unwrap();
            Subst::default().unify(&left, &right)
        };
        assert!(unifies(&entry, &query));
        assert!(unifies(&query, &entry));
        // The first pass gives the first wanted type the first parameter that some pairing allows
        // (`'x = string`), which the second wanted type needed; kept by position (`'x = i64`),
        // that parameter does not unify, and the first wanted type takes the second (`'x = bool`).
        assert!(assigns(
            ("unit", "unit"),
            &[
                "((Maybe<i64> | Maybe<string>) * string)",
                "((Maybe<bool> | Maybe<i64>) * bool)"
            ],
            &[
                "((Maybe<'x> | Maybe<'y>) * 'x)",
                "((Maybe<i64> | Maybe<string>) * string)"
            ]
        ));
    }

    /// Copilot review 1, verification: a pairing of union members whose variables nothing after
    /// it reads is kept once found, so that a failure after it goes back to the choices before it.
    /// The members `('p * 'p)` are not alike (each has its own variable), and any of the 7!
    /// pairings of the nested unions holds; only the outer union's other pairing
    /// (`'a = Result<…, Maybe<i64>>`) lets its second member match.
    #[test]
    fn pairings_that_nothing_else_reads_are_kept() {
        let unifies = |left: &str, right: &str| {
            let mut next = 0;
            let left = parse(left, &mut next, &mut BTreeMap::new()).unwrap();
            let right = parse(right, &mut next, &mut BTreeMap::new()).unwrap();
            Subst::default().unify(&left, &right)
        };
        let entry = |inner: &str| {
            format!(
                "(Result<{inner}, Maybe<i64>> | Result<{inner}, Result<{inner}, Maybe<i64>>>) -> unit"
            )
        };
        let pairs = "((i64 * i64) | (string * string) | (bool * bool) | (f64 * f64) | (char * char) | (i8 * i8) | (i32 * i32))";
        let query = "(Result<(('p * 'p) | ('q * 'q) | ('r * 'r) | ('s * 's) | ('t * 't) | ('u * 'u) | ('v * 'v)), 'a> | 'a) -> unit";
        assert!(unifies(&entry(pairs), query));
        assert!(unifies(query, &entry(pairs)));
        // The `_`s of the review's example.
        let seven = "(i64 | string | bool | f64 | char | i8 | i32)";
        let blanks = "(Result<(_|_|_|_|_|_|_), 'a> | 'a) -> unit";
        assert!(unifies(&entry(seven), blanks));
        assert!(unifies(blanks, &entry(seven)));
        // A union whose variable a later part reads keeps its other pairings (`'a = string`); one
        // whose variable the check reads does too (`solve_tries_other_pairings_until_the_check_holds`).
        assert!(unifies(
            "((Maybe<i64> | Maybe<string>) * string)",
            "((Maybe<'a> | Maybe<'b>) * 'a)"
        ));
    }

    /// Copilot review 1, verification: partners that are the same type up to variables that occur
    /// nowhere else (`_`) are tried once for each member. `bool` must take `'a`; by position it
    /// takes the first `_`, and the 7! ways of pairing the rest, which all leave `'a` to another
    /// member, are one way.
    #[test]
    fn alike_partners_are_tried_once() {
        let unifies = |left: &str, right: &str| {
            let mut next = 0;
            let left = parse(left, &mut next, &mut BTreeMap::new()).unwrap();
            let right = parse(right, &mut next, &mut BTreeMap::new()).unwrap();
            Subst::default().unify(&left, &right)
        };
        let entry =
            |last: &str| format!("((i64 | string | bool | f64 | char | i8 | i32 | u8) * {last})");
        let query = "((_ | _ | _ | _ | _ | _ | _ | 'a) * 'a)";
        for (last, holds) in [("bool", true), ("u8", true), ("i64", true), ("unit", false)] {
            assert_eq!(unifies(&entry(last), query), holds, "{last}");
        }
        // Inside a type argument: `Maybe<bool>` sorts first and must take `'a`.
        let query = "((Maybe<_> | Maybe<_> | Maybe<_> | Maybe<_> | Maybe<_> | Maybe<_> | Maybe<_> | 'a) * 'a)";
        let entry = |last: &str| {
            format!(
                "((Maybe<i64> | Maybe<string> | Maybe<bool> | Maybe<f64> | Maybe<char> | Maybe<i8> | Maybe<i32> | Maybe<u8>) * {last})"
            )
        };
        assert!(unifies(&entry("Maybe<bool>"), query));
        assert!(!unifies(&entry("Maybe<unit>"), query));
        // `Maybe<'b>` and `Maybe<_>` are not alike: `'b` occurs again. `'b = string` wants
        // `Maybe<i64>` to take `Maybe<_>`, the second partner.
        assert!(unifies(
            "((Maybe<i64> | Maybe<string> | bool) * string)",
            "((Maybe<'b> | Maybe<_> | 'c) * 'b)"
        ));
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
