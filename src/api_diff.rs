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

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::rc::Rc;

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
    /// Whether a parameter may carry a function value that the function calls, or the signature
    /// has a type variable or a length parameter. An effect attribute added to such a function
    /// also binds its callers, which must then pass callbacks (or instantiate it with types and
    /// lengths) that keep it (A20 Phase 6).
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

/// What a renaming does with the names it reaches (A24): it gives them, or it computes a key of
/// an anonymous union's member, which renames nothing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Blind {
    /// Names are given by first appearance.
    Off,
    /// The key that sorts the members (`Renaming::members`): every type variable is `'_`, and a
    /// region or a length parameter that has no name yet is numbered by where it first appears in
    /// the member (`_0`, `_1`, ...: `unnamed_regions`, `unnamed_lengths`), so a key keeps which
    /// of them is which and depends neither on their names nor on the other members.
    Order,
    /// The key that orders the members whose `Order` keys tie (`Renaming::refined`): as `Order`,
    /// except that a type variable that has a name keeps it, the others are numbered too
    /// (`unnamed_variables`), and a number is written `~0`, which sorts after a given name.
    Refine,
}

/// The kinds of names that a renaming gives.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NameKind {
    Variable,
    Region,
    Length,
}

/// Where each name, as written, occurs in what one search renders (`Renaming::least`): the place
/// of each occurrence (`Renaming::place`).
type Census = BTreeMap<(NameKind, String), Vec<String>>;

/// For each name of a census: the color of its places (the rank of their sorted list among those
/// of every name, so names with the same places have the same color) and how often it occurs.
type Colors = BTreeMap<(NameKind, String), (usize, usize)>;

/// The state of the search over the orders of tied members (`Renaming::least`): the choice made
/// at each tie that a run reaches, replayed by the next run up to its last entry.
#[derive(Clone, Default)]
struct Ties {
    /// The candidate taken at each tie reached so far, and the number of candidates there.
    script: Vec<(usize, usize)>,
    /// The next tie of this run.
    next: usize,
    /// The colors of the names in everything that the search renders, once the census is taken.
    counts: Option<Rc<Colors>>,
}

/// The most runs that the searches over the orders of tied members make for one declaration, all
/// of them together (`Renaming::spend`): the runs of `least` after its first one, those of the
/// searches of a class's methods inside each run of the class's, and those of `local` after its
/// first one.
const MAX_TIE_RUNS: usize = 4096;

#[cfg(test)]
thread_local! {
    /// The calls of `render` that `Renaming::least` makes, which the unit tests count.
    static RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The runs that `Renaming::local` makes, which the unit tests count.
    static LOCAL_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Advances `taken`, the choices of the run just made, to the script of the next run: the next
/// candidate at the last choice that has one. `false` when every combination has run.
fn advance(taken: &mut Vec<(usize, usize)>) -> bool {
    while let Some((choice, candidates)) = taken.pop() {
        if choice + 1 < candidates {
            taken.push((choice + 1, candidates));
            return true;
        }
    }
    false
}

/// Renames type variables, regions and declared length parameters by their first appearance, so
/// that two declarations that differ only in those names print the same; a renaming that is not
/// `canonical` leaves every name as written. A name in a length position that no declaration
/// has declared as a length parameter (`const N: i64`) is a constant, and keeps its name.
#[derive(Clone)]
struct Renaming<'a> {
    canonical: bool,
    /// Whether the renaming computes a key of a member instead of giving names (A24).
    blind: Blind,
    resolved: Option<&'a ResolvedApi>,
    variables: BTreeMap<String, String>,
    regions: BTreeMap<String, String>,
    /// The declared length parameters, by the name without its `#`.
    lengths: BTreeMap<String, String>,
    /// While `blind`: the type variables (`Refine` only), the regions and the length parameters
    /// without a name, in the order in which the key being computed reaches them; the position
    /// is the number.
    unnamed_variables: Vec<String>,
    unnamed_regions: Vec<String>,
    unnamed_lengths: Vec<String>,
    /// While a census runs (`least`): every name reached is recorded here as written, with its
    /// place, and nothing is renamed. Copies of the renaming (a class's methods) record into the
    /// same table.
    census: Option<Rc<RefCell<Census>>>,
    /// While a census runs: the labels of the parts around the type being read (`placed`), from
    /// the declaration down. A label names a part by what it is and its position among parts
    /// whose order matters (a parameter, a tuple's element, a type argument, a field by name, a
    /// constraint by class); a member of an anonymous union by its blind text, since the order of
    /// the members does not matter. So no consistent renaming or order of members changes a
    /// place, and a place tells where a name occurs.
    place: Vec<String>,
    /// The search over the orders of tied members that this renaming runs in (`least`); outside
    /// one, a tie takes its first candidate.
    ties: Option<Ties>,
    /// The runs that the searches of this declaration may still make (`MAX_TIE_RUNS`), shared by
    /// every copy of the renaming, so a class's methods spend from the class's.
    budget: Rc<Cell<usize>>,
    /// The calls of `ty`, which the unit tests count.
    #[cfg(test)]
    visits: usize,
}

impl<'a> Renaming<'a> {
    fn new(canonical: bool, resolved: Option<&'a ResolvedApi>) -> Self {
        Self {
            canonical,
            blind: Blind::Off,
            resolved: resolved.filter(|_| canonical),
            variables: BTreeMap::new(),
            regions: BTreeMap::new(),
            lengths: BTreeMap::new(),
            unnamed_variables: Vec::new(),
            unnamed_regions: Vec::new(),
            unnamed_lengths: Vec::new(),
            census: None,
            place: Vec::new(),
            ties: None,
            budget: Rc::new(Cell::new(MAX_TIE_RUNS)),
            #[cfg(test)]
            visits: 0,
        }
    }

    /// Takes one run from the budget of the declaration's searches; `false` when it is spent.
    fn spend(&self) -> bool {
        let left = self.budget.get();
        if left == 0 {
            return false;
        }
        self.budget.set(left - 1);
        true
    }

    /// Records a name and its place in the census, if one runs; `true` when it did.
    fn counted(&self, kind: NameKind, name: &str) -> bool {
        let Some(census) = &self.census else {
            return false;
        };
        census
            .borrow_mut()
            .entry((kind, name.to_owned()))
            .or_default()
            .push(self.place.join("/"));
        true
    }

    /// `read` with `label` added to the place while a census runs (`place`).
    fn placed<R>(
        &mut self,
        label: impl FnOnce() -> String,
        read: impl FnOnce(&mut Self) -> R,
    ) -> R {
        if self.census.is_none() {
            return read(self);
        }
        self.place.push(label());
        let result = read(self);
        self.place.pop();
        result
    }

    fn named(&self, kind: NameKind, name: &str) -> bool {
        match kind {
            NameKind::Variable => self.variables.contains_key(name),
            NameKind::Region => self.regions.contains_key(name),
            NameKind::Length => self.lengths.contains_key(name),
        }
    }

    /// The name given to `name`, if one is.
    fn given(&self, kind: NameKind, name: &str) -> Option<String> {
        match kind {
            NameKind::Variable => self.variables.get(name),
            NameKind::Region => self.regions.get(name),
            NameKind::Length => self.lengths.get(name),
        }
        .cloned()
    }

    fn forget_unnamed(&mut self) {
        self.unnamed_variables.clear();
        self.unnamed_regions.clear();
        self.unnamed_lengths.clear();
    }

    /// What `render` makes with this renaming, the least of it over the orders of tied members
    /// (A24). Members whose `Order` keys tie differ only in the names of their variables, regions
    /// or length parameters, so nothing in the members themselves orders them; which order is
    /// right depends on where else those names occur, in the rest of the signature or of the
    /// declaration. `tied` picks the next member among the tied ones by what it can tell without
    /// trying (`refined`, `ordered`), and each pick that remains open is a choice of this search:
    /// every combination of choices is rendered, and the least result is the canonical form. Two
    /// declarations that differ only in their names and in the order of members reach the same
    /// set of results, so they print the same; the results are renderings of the declaration, so
    /// declarations that differ in anything else never print the same.
    ///
    /// A run that meets no open choice is the only one, which is every declaration without tied
    /// members: the checker rejects such members (`E1041`), so only the syntactic comparison of a
    /// rejected signature searches. Then a census records where each name that `render` reaches
    /// occurs (`place`), which orders more members without trying and tells which of them can be
    /// swapped (`tied`). The census also tells which tied members, or which groups of them, hold
    /// every occurrence of their names that have no name yet: their choices change nothing
    /// outside them, so `local` settles those choices where they are met, and they are no choices
    /// of this search. Unions that share no names with the rest of the declaration are then
    /// searched one after the other instead of in every combination. The runs replay the choices
    /// of the previous one and take the next candidate at the last choice that has one, while the
    /// declaration's budget lasts (`spend`, `MAX_TIE_RUNS` for all its searches together); past
    /// that, the least result found so far is the form, which then may depend on the written
    /// order.
    fn least<T: Ord>(&self, mut render: impl FnMut(&mut Self) -> T) -> T {
        let (first, script) = self.replay(&mut render, Vec::new(), None);
        if script.is_empty() || !self.spend() {
            return first;
        }
        let census = Rc::new(RefCell::new(Census::new()));
        let mut counting = self.clone();
        counting.census = Some(Rc::clone(&census));
        #[cfg(test)]
        RUNS.with(|runs| runs.set(runs.get() + 1));
        render(&mut counting);
        drop(counting);
        let census = census.take();
        let sorted: Vec<Vec<String>> = census
            .values()
            .map(|places| {
                let mut places = places.clone();
                places.sort_unstable();
                places
            })
            .collect();
        let mut distinct = sorted.clone();
        distinct.sort_unstable();
        distinct.dedup();
        let counts: Rc<Colors> = Rc::new(
            census
                .into_iter()
                .zip(sorted)
                .map(|((name, places), sorted)| {
                    let color = distinct.binary_search(&sorted).unwrap_or_default();
                    (name, (color, places.len()))
                })
                .collect(),
        );
        let mut best: Option<T> = None;
        let mut script = Vec::new();
        while self.spend() {
            let (result, mut taken) = self.replay(&mut render, script, Some(Rc::clone(&counts)));
            if best.as_ref().is_none_or(|best| result < *best) {
                best = Some(result);
            }
            if !advance(&mut taken) {
                break;
            }
            script = taken;
        }
        best.unwrap_or(first)
    }

    /// What `render` makes from this renaming, the least of it over the choices that it meets,
    /// searched here; this renaming then holds the names that the least run gave (A24). `tied`
    /// uses it for a part whose names that have no name yet occur nowhere else in the
    /// declaration (the census of the enclosing `least` tells), so the part's choices change
    /// only the part: the names given after it are numbered the same in every run, and no later
    /// key, color or choice reads the part's names. Its runs replay their choices as `least`'s
    /// do and are compared by `render`'s key, with the colors of the enclosing search; the
    /// enclosing search's own script is not touched. The first run is free, the others spend the
    /// declaration's budget.
    fn local<K: Ord, T>(&mut self, mut render: impl FnMut(&mut Self) -> (K, T)) -> T {
        let outer = self.ties.take();
        let counts = outer.as_ref().and_then(|ties| ties.counts.clone());
        let mut best: Option<(K, T, Self)> = None;
        let mut script = Vec::new();
        loop {
            #[cfg(test)]
            LOCAL_RUNS.with(|runs| runs.set(runs.get() + 1));
            let mut run = self.clone();
            run.ties = Some(Ties {
                script,
                next: 0,
                counts: counts.clone(),
            });
            let (key, value) = render(&mut run);
            let mut taken = run.ties.take().map(|ties| ties.script).unwrap_or_default();
            if best.as_ref().is_none_or(|(least, ..)| key < *least) {
                best = Some((key, value, run));
            }
            if !advance(&mut taken) || !self.spend() {
                break;
            }
            script = taken;
        }
        let (_, value, run) = best.expect("the search runs at least once");
        *self = run;
        self.ties = outer;
        value
    }

    /// One run of `least`: `render` with a copy of this renaming that takes the choices of
    /// `script`, and the choices that the run took.
    fn replay<T>(
        &self,
        render: &mut impl FnMut(&mut Self) -> T,
        script: Vec<(usize, usize)>,
        counts: Option<Rc<Colors>>,
    ) -> (T, Vec<(usize, usize)>) {
        #[cfg(test)]
        RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut renaming = self.clone();
        renaming.ties = Some(Ties {
            script,
            next: 0,
            counts,
        });
        let result = render(&mut renaming);
        let script = renaming
            .ties
            .take()
            .map(|ties| ties.script)
            .unwrap_or_default();
        (result, script)
    }

    /// The candidate to take at a tie with `candidates` candidates: the script's at a tie that
    /// the script reaches, the first at a new one, which joins the script.
    fn choose(&mut self, candidates: usize) -> usize {
        let Some(ties) = &mut self.ties else {
            return 0;
        };
        let choice = match ties.script.get(ties.next) {
            Some(&(choice, recorded)) => {
                debug_assert_eq!(recorded, candidates, "a replayed run reaches the same ties");
                choice.min(candidates - 1)
            }
            None => {
                ties.script.push((0, candidates));
                0
            }
        };
        ties.next += 1;
        choice
    }

    /// The number of a name that has no name yet, in a blind key: its position in `unnamed`,
    /// which it joins when the key first reaches it. It is written `_0` in an `Order` key and
    /// `~0` in a `Refine` key, where it sorts after the names given so far (`t0`, `r0`, `n0`).
    fn unnamed(blind: Blind, unnamed: &mut Vec<String>, name: &str) -> String {
        let position = unnamed
            .iter()
            .position(|known| known == name)
            .unwrap_or_else(|| {
                unnamed.push(name.to_owned());
                unnamed.len() - 1
            });
        let mark = if blind == Blind::Refine { '~' } else { '_' };
        format!("{mark}{position}")
    }

    fn variable(&mut self, name: &str) -> String {
        if !self.canonical || self.counted(NameKind::Variable, name) {
            return name.to_owned();
        }
        match self.blind {
            Blind::Off => {}
            Blind::Order => return "_".to_owned(),
            Blind::Refine => {
                return match self.variables.get(name) {
                    Some(variable) => variable.clone(),
                    None => Self::unnamed(self.blind, &mut self.unnamed_variables, name),
                };
            }
        }
        let next = self.variables.len();
        self.variables
            .entry(name.to_owned())
            .or_insert_with(|| format!("t{next}"))
            .clone()
    }

    fn region(&mut self, name: &str) -> String {
        if !self.canonical || self.counted(NameKind::Region, name) {
            return name.to_owned();
        }
        if self.blind != Blind::Off {
            return match self.regions.get(name) {
                Some(region) => region.clone(),
                None => Self::unnamed(self.blind, &mut self.unnamed_regions, name),
            };
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
        if !self.canonical || self.counted(NameKind::Length, name) {
            return name.to_owned();
        }
        if self.blind != Blind::Off {
            return match self.lengths.get(name) {
                Some(length) => length.clone(),
                None => Self::unnamed(self.blind, &mut self.unnamed_lengths, name),
            };
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
        #[cfg(test)]
        {
            self.visits += 1;
        }
        let ty = if ty.span.source.is_some() {
            self.resolved
                .and_then(|resolved| resolved.annotations.get(&api_key(ty.span)))
                .unwrap_or(ty)
        } else {
            ty
        };
        // Each part is `placed` under a label of its position, which only a census records.
        let kind = match &ty.kind {
            Named(name) => Named(self.public_name(name)),
            Variable(name) if name.starts_with('#') => Named(self.length(&name[1..])),
            Variable(name) => Variable(self.variable(name)),
            Apply(head, arguments) => {
                let (head, label) = match head.text.strip_prefix('\'') {
                    Some(name) => (
                        Ident {
                            text: format!(
                                "'{}",
                                self.placed(|| "'".to_owned(), |this| this.variable(name))
                            ),
                            ..(**head).clone()
                        },
                        "'".to_owned(),
                    ),
                    None => {
                        let text = self.public_name(&head.text);
                        (
                            Ident {
                                text: text.clone(),
                                ..(**head).clone()
                            },
                            text,
                        )
                    }
                };
                let count = arguments.len();
                let arguments = arguments
                    .iter()
                    .enumerate()
                    .map(|(index, argument)| {
                        self.placed(
                            || format!("{label}<{index}/{count}"),
                            |this| this.length_argument(argument),
                        )
                    })
                    .collect();
                Apply(Box::new(head), arguments)
            }
            Regions(inner, names) => {
                let names = self
                    .placed(|| "{}".to_owned(), |this| this.regions(names))
                    .into_boxed_slice();
                Regions(
                    Box::new(self.placed(|| "{}^".to_owned(), |this| this.ty(inner))),
                    names,
                )
            }
            Quantified(names, inner) => {
                let names = self
                    .placed(|| "{q}".to_owned(), |this| this.regions(names))
                    .into_boxed_slice();
                Quantified(
                    names,
                    Box::new(self.placed(|| "{q}^".to_owned(), |this| this.ty(inner))),
                )
            }
            Array(inner) => Array(Box::new(
                self.placed(|| "[]".to_owned(), |this| this.ty(inner)),
            )),
            ArrayView(inner) => ArrayView(Box::new(
                self.placed(|| "[..]".to_owned(), |this| this.ty(inner)),
            )),
            FixedArray(element, length) => FixedArray(
                Box::new(self.placed(|| "[;]".to_owned(), |this| this.ty(element))),
                Box::new(self.placed(|| "[;]#".to_owned(), |this| this.length_argument(length))),
            ),
            Length(length) => Length(*length),
            List(inner) => List(Box::new(
                self.placed(|| "[||]".to_owned(), |this| this.ty(inner)),
            )),
            Tuple(elements) => {
                let count = elements.len();
                Tuple(
                    elements
                        .iter()
                        .enumerate()
                        .map(|(index, element)| {
                            self.placed(|| format!("*{index}/{count}"), |this| this.ty(element))
                        })
                        .collect(),
                )
            }
            AnonymousUnion(members) if self.canonical => AnonymousUnion(self.members(members)),
            AnonymousUnion(members) => {
                AnonymousUnion(members.iter().map(|member| self.ty(member)).collect())
            }
            Function(parameters, result) => {
                let count = parameters.len();
                Function(
                    parameters
                        .iter()
                        .enumerate()
                        .map(|(index, parameter)| {
                            self.placed(|| format!("->{index}/{count}"), |this| this.ty(parameter))
                        })
                        .collect(),
                    Box::new(self.placed(|| format!("->r/{count}"), |this| this.ty(result))),
                )
            }
            Reference(inner, mutable) => {
                let label = if *mutable { "ref mut" } else { "ref" };
                Reference(
                    Box::new(self.placed(|| label.to_owned(), |this| this.ty(inner))),
                    *mutable,
                )
            }
            Own(inner) => Own(Box::new(
                self.placed(|| "own".to_owned(), |this| this.ty(inner)),
            )),
            Mut(inner) => Mut(Box::new(
                self.placed(|| "mut".to_owned(), |this| this.ty(inner)),
            )),
            Pure(inner) => Pure(Box::new(
                self.placed(|| "pure".to_owned(), |this| this.ty(inner)),
            )),
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

    /// The members of an anonymous union as the comparison sees them (A24): the members of a
    /// union written inside another are its own (`(i64 | (string | bool))`), and they come in one
    /// order whatever order they were written in, so that reordering them is no change. The order
    /// is that of each member's blind text (`blind`: its type variables blanked out, and its
    /// regions and length parameters that have no name yet numbered from `_0` within the member),
    /// so it depends neither on the names of the variables nor on where they first appear; the
    /// variables are renamed afresh in that order. Each member's numbering starts afresh, and each
    /// member of a union inside a key starts from the numbering before that union, so no key
    /// depends on the other members or on the order of the members inside it. A type alias is
    /// not expanded (the syntactic comparison never expands one), so `(Meters | string)` and
    /// `(f64 | string)` stay apart there.
    ///
    /// A blind renaming renames nothing, so a member's blind text is computed once, with the
    /// unions inside it in their own order, and each member is renamed once, after the sort: for
    /// a union nested `d` times through other types (`([([i64] | bool)] | bool)`), the calls of
    /// `ty` grow with `d²`, where renaming each member for its key and again after the sort made
    /// them double at every level.
    ///
    /// Members whose blind texts are equal differ only in the names of their variables, regions
    /// or length parameters; `tied` renames them, in the order that the rest of the declaration
    /// decides (`least`).
    fn members(&mut self, members: &[TypeExpr]) -> Box<[TypeExpr]> {
        let mut flat = Vec::new();
        let mut pending: Vec<&TypeExpr> = members.iter().rev().collect();
        while let Some(member) = pending.pop() {
            match &member.kind {
                TypeExprKind::AnonymousUnion(inner) => pending.extend(inner.iter().rev()),
                _ => flat.push(member),
            }
        }
        if self.census.is_some() {
            // A member's place is its blind text, which no order of the members changes.
            return flat
                .into_iter()
                .map(|member| {
                    let census = self.census.take();
                    self.blind = Blind::Order;
                    self.forget_unnamed();
                    let key = type_text(&self.ty(member));
                    self.forget_unnamed();
                    self.blind = Blind::Off;
                    self.census = census;
                    self.placed(|| format!("|{key}"), |this| this.ty(member))
                })
                .collect();
        }
        if self.blind != Blind::Off {
            let before = (
                self.unnamed_variables.len(),
                self.unnamed_regions.len(),
                self.unnamed_lengths.len(),
            );
            let mut blind: Vec<(String, TypeExpr)> = flat
                .into_iter()
                .map(|member| {
                    let member = self.ty(member);
                    self.unnamed_variables.truncate(before.0);
                    self.unnamed_regions.truncate(before.1);
                    self.unnamed_lengths.truncate(before.2);
                    (type_text(&member), member)
                })
                .collect();
            blind.sort_by(|left, right| left.0.cmp(&right.0));
            return blind.into_iter().map(|(_, member)| member).collect();
        }
        self.blind = Blind::Order;
        let mut keyed: Vec<(String, &TypeExpr)> = flat
            .into_iter()
            .map(|member| {
                self.forget_unnamed();
                (type_text(&self.ty(member)), member)
            })
            .collect();
        self.forget_unnamed();
        self.blind = Blind::Off;
        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        let mut renamed = Vec::with_capacity(keyed.len());
        for group in keyed.chunk_by(|left, right| left.0 == right.0) {
            match group {
                [(_, member)] => renamed.push(self.ty(member)),
                _ => self.tied(
                    group.iter().map(|(_, member)| *member).collect(),
                    &mut renamed,
                ),
            }
        }
        renamed.into_boxed_slice()
    }

    /// Renames the members of one union whose blind texts tie (A24), appending them to `renamed`
    /// in their canonical order (`ordered`). When the group holds every occurrence of its names
    /// that have no name yet (`confined`), the order changes nothing outside the group, so the
    /// group's own choices are searched here (`local`) and the least rendering of the group is
    /// taken: those choices are no choices of the enclosing search, so unions that share no names
    /// with anything else (`(('a * 'b * 'c) | ('b * 'c * 'a)) -> (('d * 'e * 'f) | ...)`) are
    /// searched one after the other, not in every combination.
    #[inline(never)]
    fn tied(&mut self, group: Vec<&TypeExpr>, renamed: &mut Vec<TypeExpr>) {
        let group: Vec<(&TypeExpr, Written)> = group
            .into_iter()
            .map(|member| (member, self.written(member)))
            .collect();
        if self.confined(group.iter().map(|(_, written)| written)) {
            let members = self.local(|this| {
                let mut members = Vec::with_capacity(group.len());
                this.ordered(&group, &mut members);
                let texts: Vec<String> = members.iter().map(type_text).collect();
                (texts, members)
            });
            renamed.extend(members);
        } else {
            self.ordered(&group, renamed);
        }
    }

    /// Whether `members` hold every occurrence, in what the enclosing search renders, of their
    /// names that have no name yet (A24): `false` before the census, and for members whose names
    /// all have names (their `refined` keys then tell apart every two written differently, so
    /// there is no choice to settle).
    fn confined<'w>(&self, members: impl Iterator<Item = &'w Written>) -> bool {
        let Some(counts) = self.ties.as_ref().and_then(|ties| ties.counts.as_deref()) else {
            return false;
        };
        let mut within: BTreeMap<&(NameKind, String), usize> = BTreeMap::new();
        for written in members {
            for (name, count) in &written.names {
                if !self.named(name.0, &name.1) {
                    *within.entry(name).or_default() += count;
                }
            }
        }
        !within.is_empty()
            && within
                .into_iter()
                .all(|(name, count)| counts.get(name).is_some_and(|&(_, total)| total <= count))
    }

    /// Renames the tied members of `group` in their canonical order (`tied`). Each next member
    /// is one of those whose `refined` key is the least now: the key with the names given so far
    /// (a variable named by an earlier member or part of the declaration, which sorts before a
    /// name not given yet, so the next member is one that shares names with those already
    /// renamed), and the colors of the places where the member's other names occur in the whole
    /// declaration (in `(Maybe<'a> | Maybe<'b>) -> 'a -> i64`: `'a` is also the second
    /// parameter, so `Maybe<'b>` comes first). Among those, members written the same are one
    /// candidate. When every candidate holds all the occurrences of its names that have no name
    /// yet (`Refined::private`), swapping two candidates with their names maps the declaration
    /// onto itself as soon as they are alike up to those names, and the choice is made without
    /// trying: without a union inside, their keys tell that they are alike, so the first is
    /// taken; with one, each is rendered alone from the names given so far (`local`), and the
    /// least rendering is taken (alike members render the same). Any other choice between
    /// candidates is left to the search (`choose`). A private member with a union inside is
    /// rendered by `local` too, so the choices inside it are not the search's either. Keys,
    /// colors and these renderings depend only on the declaration and the names given so far,
    /// never on the written order, so they restrict the search to the same orders for every
    /// spelling of one declaration.
    fn ordered(&mut self, group: &[(&TypeExpr, Written)], renamed: &mut Vec<TypeExpr>) {
        let mut left: Vec<usize> = (0..group.len()).collect();
        while !left.is_empty() {
            let keys: Vec<Refined> = left
                .iter()
                .map(|&index| self.refined(group[index].0, &group[index].1))
                .collect();
            let least = keys
                .iter()
                .map(|key| (&key.key, &key.colors))
                .min()
                .expect("a tied group has members");
            let mut candidates: Vec<usize> = Vec::new();
            for (index, key) in keys.iter().enumerate() {
                if (&key.key, &key.colors) == least
                    && !candidates
                        .iter()
                        .any(|&other| group[left[other]].1.text == group[left[index]].1.text)
                {
                    candidates.push(index);
                }
            }
            let private = candidates.iter().all(|&index| keys[index].private);
            let union = group[left[candidates[0]]].1.union;
            let (pick, member) = if private && union {
                let mut alone: Option<(String, usize, TypeExpr, Self)> = None;
                for &index in &candidates {
                    let mut probe = self.clone();
                    let member = group[left[index]].0;
                    let (text, rendered) = probe.local(|this| {
                        let rendered = this.ty(member);
                        (type_text(&rendered), (type_text(&rendered), rendered))
                    });
                    if alone.as_ref().is_none_or(|(least, ..)| text < *least) {
                        alone = Some((text, index, rendered, probe));
                    }
                }
                let (_, pick, member, probe) = alone.expect("a tied group has candidates");
                *self = probe;
                (pick, member)
            } else {
                let pick = if candidates.len() == 1 || private {
                    candidates[0]
                } else {
                    candidates[self.choose(candidates.len())]
                };
                let member = group[left[pick]].0;
                let member = if keys[pick].private && group[left[pick]].1.union {
                    self.local(|this| {
                        let rendered = this.ty(member);
                        (type_text(&rendered), rendered)
                    })
                } else {
                    self.ty(member)
                };
                (pick, member)
            };
            left.remove(pick);
            renamed.push(member);
        }
    }

    /// A tied member as written (the names as written, a type alias resolved as `ty` resolves
    /// it), and how often each of its names occurs in it (`tied`).
    fn written(&mut self, member: &TypeExpr) -> Written {
        let ties = self.ties.take();
        let inside = Rc::new(RefCell::new(Census::new()));
        self.census = Some(Rc::clone(&inside));
        let place = std::mem::take(&mut self.place);
        let written = self.ty(member);
        self.place = place;
        self.census = None;
        self.ties = ties;
        let names = inside
            .take()
            .into_iter()
            .map(|(name, places)| (name, places.len()))
            .collect();
        Written {
            text: type_text(&written),
            names,
            union: has_union(&written),
        }
    }

    /// What orders a tied member without trying (`ordered`): its `Refine` key, the sorted colors
    /// of its names that have no name yet (empty before the census), and whether it holds every
    /// occurrence of those names.
    fn refined(&mut self, member: &TypeExpr, written: &Written) -> Refined {
        self.blind = Blind::Refine;
        self.forget_unnamed();
        let key = type_text(&self.ty(member));
        self.forget_unnamed();
        self.blind = Blind::Off;
        let counts = self.ties.as_ref().and_then(|ties| ties.counts.as_deref());
        let mut colors = Vec::new();
        let mut private = true;
        if let Some(counts) = counts {
            for ((kind, name), within) in &written.names {
                if !self.named(*kind, name) {
                    // Every name of the member is in the census, which renders the member too.
                    let (color, total) = counts
                        .get(&(*kind, name.clone()))
                        .copied()
                        .unwrap_or((0, usize::MAX));
                    private &= total <= *within;
                    colors.push(color);
                }
            }
        }
        colors.sort_unstable();
        // A member whose names all have names has no choice inside, and its key tells it apart
        // from every member written differently.
        let private = counts.is_some() && private && !colors.is_empty();
        Refined {
            key,
            colors,
            private,
        }
    }

    fn constraint(&mut self, constraint: &ConstraintExpr) -> ConstraintExpr {
        // The constraints are a set, so a census places each by its class or function.
        let label = match &constraint.name {
            _ if self.census.is_none() => String::new(),
            ConstraintName::Class(class) => self.class_name(class).text,
            ConstraintName::Function(name, _) => format!("#{}", name.text),
        };
        let ty = self.placed(|| format!("({label})"), |this| this.ty(&constraint.ty));
        let name = match &constraint.name {
            ConstraintName::Class(class) => ConstraintName::Class(self.class_name(class)),
            ConstraintName::Function(name, annotation) => ConstraintName::Function(
                name.clone(),
                annotation.as_ref().map(|annotation| {
                    Box::new(self.placed(|| format!("({label}):"), |this| this.ty(annotation)))
                }),
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

/// What a class's method text depends on in the class's renaming (`add_items`): the method's
/// position, the names given to the names it reaches, and how many variables, regions and length
/// parameters have names.
type MethodKey = (usize, Vec<Option<String>>, [usize; 3]);

/// A member among members whose blind texts tie, as written (`Renaming::written`).
struct Written {
    text: String,
    /// How often each name occurs in the member.
    names: Vec<((NameKind, String), usize)>,
    /// Whether an anonymous union is written in the member.
    union: bool,
}

/// What orders one member among members whose blind texts tie (`Renaming::refined`).
struct Refined {
    key: String,
    colors: Vec<usize>,
    /// Whether the member has names that have no name yet and holds every occurrence of them in
    /// what the search renders, so that its names and the choices inside it change nothing
    /// outside it (`Renaming::ordered`).
    private: bool,
}

/// Whether an anonymous union is written anywhere in `ty`.
fn has_union(ty: &TypeExpr) -> bool {
    use TypeExprKind::*;
    match &ty.kind {
        AnonymousUnion(_) => true,
        Named(_) | Variable(_) | Length(_) | Dyn(_) => false,
        Apply(_, types) => types.iter().any(has_union),
        Tuple(types) => types.iter().any(has_union),
        Regions(inner, _)
        | Quantified(_, inner)
        | Array(inner)
        | ArrayView(inner)
        | List(inner)
        | Reference(inner, _)
        | Own(inner)
        | Mut(inner)
        | Pure(inner) => has_union(inner),
        FixedArray(element, length) => has_union(element) || has_union(length),
        Function(parameters, result) => parameters.iter().any(has_union) || has_union(result),
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
    renaming: Renaming<'_>,
) -> String {
    renaming.least(|renaming| {
        let regions = renaming.regions(regions);
        if renaming.census.is_some() && renaming.canonical {
            // A census places the parameters of the flattened chain by their positions, so that
            // splicing a result function in (`A -> (B -> C)`) changes no place.
            let (parameters, result) = flatten(
                parameters
                    .iter()
                    .map(|&parameter| parameter.clone())
                    .collect(),
                result.clone(),
            );
            let count = parameters.len();
            for (index, parameter) in parameters.iter().enumerate() {
                renaming.placed(|| format!("{index}/{count}"), |this| this.ty(parameter));
            }
            renaming.placed(|| format!("r/{count}"), |this| this.ty(&result));
            renaming.constraints(constraints);
            return String::new();
        }
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
    })
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
    let lengths = Lengths::of(program);
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
        // A24: the synthesized declarations behind anonymous unions are no API of the module.
        uses_anonymous_unions: _,
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
            || types
                .iter()
                .copied()
                .chain([implicit.as_ref().unwrap_or(result)])
                .any(|ty| has_type_variable(ty, &lengths));
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
        let (header, types) = fresh().least(|renaming| {
            let names = parameters(&declaration.parameters, renaming);
            let regions = renaming.regions(&declaration.regions);
            let header = format!(
                "record {}{}{}",
                declaration.name.text,
                parameters_text(&names),
                region_text(&regions)
            );
            let types: Vec<String> = declaration
                .fields
                .iter()
                .map(|field| {
                    let ty = renaming.placed(
                        || format!(".{}", field.name.text),
                        |this| this.ty(&field.ty),
                    );
                    type_text(&ty)
                })
                .collect();
            (header, types)
        });
        let mut item = Item::new("record", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref());
        for (field, ty) in declaration.fields.iter().zip(types) {
            let mut member = Member::new(
                "field",
                field.name.text.clone(),
                format!(
                    "{}{}{}: {ty}",
                    json_text(field.json.as_ref()),
                    if field.mutable { "mut " } else { "" },
                    field.name.text,
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
        let (header, payloads) = fresh().least(|renaming| {
            let names = parameters(&declaration.parameters, renaming);
            let header = format!("union {}{}", declaration.name.text, parameters_text(&names));
            let payloads: Vec<String> = declaration
                .cases
                .iter()
                .map(|case| {
                    case.payload.as_ref().map_or(String::new(), |ty| {
                        let ty = renaming
                            .placed(|| format!("case {}", case.name.text), |this| this.ty(ty));
                        format!(" of {}", type_text(&ty))
                    })
                })
                .collect();
            (header, payloads)
        });
        let mut item = Item::new("union", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref());
        for (case, payload) in declaration.cases.iter().zip(payloads) {
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
        let header = fresh().least(|renaming| {
            let names = parameters(&declaration.parameters, renaming);
            let target = renaming.ty(&declaration.target);
            let constraints = effective
                .and_then(|resolved| resolved.requirements.get(&api_key(declaration.name.span)))
                .map(|constraints| renaming.constraints(constraints))
                .unwrap_or_default();
            format!(
                "type {}{} = {}{}",
                declaration.name.text,
                parameters_text(&names),
                type_text(&target),
                constraints_text(&constraints),
            )
        });
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
            fresh().least(|renaming| type_text(&renaming.ty(&declaration.ty)))
        );
        add(
            Item::new("const", qualified(&declaration.name.text), header)
                .documented(declaration.doc.as_ref()),
        );
    }
    for declaration in classes {
        let methods: Vec<_> = declaration
            .methods
            .iter()
            .filter(|method| method.visibility != Visibility::Private)
            .collect();
        // Each method starts from the names of the class, and the search for the class's own
        // tied members sees the methods (the census counts their names too).
        let text_of = |method: &SignatureDecl, renaming: Renaming<'_>| {
            let signature =
                effective.and_then(|resolved| resolved.signatures.get(&api_key(method.name.span)));
            let types: Vec<_> = signature.map_or_else(
                || method.parameters.iter().collect(),
                |signature| signature.parameters.iter().collect(),
            );
            let result = signature.map_or(&method.result, |signature| &signature.result);
            function_text(
                if method.pure { "pure def" } else { "def" },
                &method.name.text,
                &method.regions,
                &method.constraints,
                &types,
                result,
                renaming,
            )
        };
        // A method's text depends on the class's renaming only through the names that the
        // method reaches (a census of the method alone lists them as written) and how many
        // names the class has given (A24). A run of the class's search that gives those names
        // as an earlier run did reuses the method's text, so the search of a method runs once
        // for each way the class names it, not once in every run of the class's search. Only a
        // class that writes an anonymous union can search (`--effective` reads no tied members:
        // the checker rejects them), so no other class takes the census.
        let constraint_union = |constraint: &ConstraintExpr| {
            has_union(&constraint.ty)
                || matches!(&constraint.name, ConstraintName::Function(_, Some(annotation)) if has_union(annotation))
        };
        let searching = canonical
            && effective.is_none()
            && (declaration.superclasses.iter().any(constraint_union)
                || methods.iter().any(|method| {
                    method.parameters.iter().any(has_union)
                        || has_union(&method.result)
                        || method.constraints.iter().any(constraint_union)
                }));
        let reached: Vec<Vec<(NameKind, String)>> = methods
            .iter()
            .filter(|_| searching)
            .map(|method| {
                let census = Rc::new(RefCell::new(Census::new()));
                let mut renaming = fresh();
                renaming.census = Some(Rc::clone(&census));
                text_of(method, renaming);
                census.take().into_keys().collect()
            })
            .collect();
        let mut known: BTreeMap<MethodKey, String> = BTreeMap::new();
        let (header, texts) = fresh().least(|renaming| {
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
            let texts: Vec<String> = methods
                .iter()
                .enumerate()
                .map(|(index, method)| {
                    let mut method_renaming = renaming.clone();
                    if method_renaming.census.is_some() {
                        method_renaming
                            .place
                            .push(format!("method {}", method.name.text));
                        return text_of(method, method_renaming);
                    }
                    if !searching {
                        return text_of(method, method_renaming);
                    }
                    let given = reached[index]
                        .iter()
                        .map(|(kind, name)| renaming.given(*kind, name))
                        .collect();
                    let counts = [
                        renaming.variables.len(),
                        renaming.regions.len(),
                        renaming.lengths.len(),
                    ];
                    known
                        .entry((index, given, counts))
                        .or_insert_with(|| text_of(method, method_renaming))
                        .clone()
                })
                .collect();
            (header, texts)
        });
        let mut item = Item::new("class", qualified(&declaration.name.text), header)
            .documented(declaration.doc.as_ref());
        for (method, text) in methods.into_iter().zip(texts) {
            let mut member = Member::new("method", method.name.text.clone(), text);
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
        let header = fresh().least(|renaming| {
            let ty = renaming.ty(&declaration.ty);
            let constraints = renaming.constraints(&declaration.constraints);
            let class = renaming.class_name(&declaration.class);
            let name = format!("{}<{}>", class.text, type_text(&ty));
            format!("instance {}{name}", constraints_text(&constraints))
        });
        let name = Renaming::new(true, resolved).least(|key| {
            let ty = key.ty(&declaration.ty);
            let class = key.class_name(&declaration.class);
            format!("{}<{}>", class.text, type_text(&ty))
        });
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
/// makes the attribute depend on the caller: a function, tzroutine body or `dyn` type, a named type that
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
        TypeExprKind::AnonymousUnion(elements) => elements.iter().any(may_carry_functions),
        _ => true,
    }
}

/// What tells a length parameter from a length constant in the signatures of one module.
struct Lengths<'p> {
    /// The constants of the module, which a length may name without qualification.
    constants: BTreeSet<&'p str>,
    /// For each record, union and alias of the module, which of its parameters are lengths.
    positions: BTreeMap<&'p str, Vec<bool>>,
}

impl<'p> Lengths<'p> {
    fn of(program: &'p Program) -> Self {
        let declared = |parameters: &[Ident]| {
            parameters
                .iter()
                .map(|parameter| parameter.text.starts_with('#'))
                .collect()
        };
        let mut positions = BTreeMap::new();
        for record in &program.records {
            positions.insert(record.name.text.as_str(), declared(&record.parameters));
        }
        for union in &program.unions {
            positions.insert(union.name.text.as_str(), declared(&union.parameters));
        }
        for alias in &program.type_aliases {
            positions.insert(alias.name.text.as_str(), declared(&alias.parameters));
        }
        Self {
            constants: program
                .constants
                .iter()
                .map(|constant| constant.name.text.as_str())
                .collect(),
            positions,
        }
    }

    /// Whether a length is a parameter that each caller chooses: `#N` in a resolved signature,
    /// or, as written, a name that is not a constant of the module (another module's constant
    /// is qualified).
    fn parameter(&self, length: &TypeExpr) -> bool {
        match &length.kind {
            TypeExprKind::Variable(_) => true,
            TypeExprKind::Named(name) => {
                !name.contains(['.', ':']) && !self.constants.contains(name.as_str())
            }
            _ => false,
        }
    }
}

/// Whether a type mentions a type variable or a length parameter: an attribute of a generic
/// function is checked for each instance, with the types and lengths that its callers choose
/// (`N = 0` makes `xs[0]` trap; review 3 of #63).
fn has_type_variable(ty: &TypeExpr, lengths: &Lengths<'_>) -> bool {
    match &ty.kind {
        TypeExprKind::Variable(_) => true,
        TypeExprKind::Named(_) | TypeExprKind::Length(_) | TypeExprKind::Dyn(_) => false,
        TypeExprKind::Apply(head, arguments) => {
            let positions = lengths.positions.get(head.text.as_str());
            arguments.iter().enumerate().any(|(index, argument)| {
                if positions.is_some_and(|positions| positions.get(index) == Some(&true)) {
                    lengths.parameter(argument)
                } else {
                    has_type_variable(argument, lengths)
                }
            })
        }
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Quantified(_, inner)
        | TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Own(inner)
        | TypeExprKind::Mut(inner)
        | TypeExprKind::Pure(inner) => has_type_variable(inner, lengths),
        TypeExprKind::FixedArray(inner, length) => {
            has_type_variable(inner, lengths) || lengths.parameter(length)
        }
        TypeExprKind::Tuple(elements) => elements
            .iter()
            .any(|element| has_type_variable(element, lengths)),
        TypeExprKind::AnonymousUnion(members) => members
            .iter()
            .any(|member| has_type_variable(member, lengths)),
        TypeExprKind::Function(parameters, result) => {
            parameters
                .iter()
                .any(|parameter| has_type_variable(parameter, lengths))
                || has_type_variable(result, lengths)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// The type of the first parameter of the first function of `source`.
    fn parameter(source: &str) -> TypeExpr {
        let program = crate::parser::parse(source).unwrap();
        program.functions[0].parameters[0].ty.clone()
    }

    /// Copilot review 2 of #102 (A24): the members of an anonymous union are sorted by their
    /// blind texts, each computed once, and each member is renamed once after the sort. A union
    /// nested `depth` times through an array, `([([i64] | bool)] | bool)`, was renamed twice at
    /// every level (`M(k) = 2 M(k - 1) + 5`, so `6 * 2^depth - 5` calls of `ty`). Counted by
    /// hand: a blind renaming of the union nested `k` times calls `ty` `B(k) = B(k - 1) + 3`
    /// times (the union, the array and `bool` of each level, then `i64`: `B(k) = 3k + 1`), and
    /// the renaming `N(k) = N(k - 1) + B(k - 1) + 5` times: the union, the blind keys of the array
    /// (`1 + B(k - 1)`) and of `bool`, and the renamings of the array (`1 + N(k - 1)`) and of
    /// `bool`. So `N(k) = 1 + 6k + 3k(k - 1) / 2`: 16 calls at depth 2, where there were 19.
    #[test]
    fn nested_union_members_are_renamed_once() {
        for depth in [0usize, 1, 2, 3, 10, 20, 60] {
            let mut ty = "i64".to_owned();
            for _ in 0..depth {
                ty = format!("([{ty}] | bool)");
            }
            let source = format!("def f :: {ty} -> i64 = \\v -> 0\n");
            let mut renaming = Renaming::new(true, None);
            renaming.ty(&parameter(&source));
            let expected = 1 + 6 * depth + 3 * depth * depth.saturating_sub(1) / 2;
            assert_eq!(renaming.visits, expected, "depth {depth}");
        }
    }

    /// The blind keys order the members as the renamed texts with the variables blanked out did:
    /// the order of the members inside a member, the names of the variables and where they first
    /// appear change nothing, and the variables are named in the sorted order. By hand: `'_`
    /// sorts before `[` and `string`, `Maybe<'_>` before `[('_ | bool)]`, and `'_` before `bool`;
    /// `'b` is named first, then `'a` inside `Maybe`.
    #[test]
    fn nested_members_take_one_order() {
        let canonical = |source: &str| {
            let mut renaming = Renaming::new(true, None);
            type_text(&renaming.ty(&parameter(source)))
        };
        let first =
            canonical("def f :: ([(Maybe<'a> | [('b | bool)])] | 'b | string) -> i64 = \\v -> 0\n");
        assert_eq!(first, "('t0 | [(Maybe<'t1> | [('t0 | bool)])] | string)");
        for source in [
            "def f :: (string | 'x | [([(bool | 'x)] | Maybe<'y>)]) -> i64 = \\v -> 0\n",
            "def f :: ('q | (string | [([('q | bool)] | (Maybe<'p>))])) -> i64 = \\v -> 0\n",
        ] {
            assert_eq!(canonical(source), first, "{source}");
        }
    }

    /// A region that first appears inside a member has no name in the blind key, only its number
    /// within the member, so the order of the members does not depend on which of them was
    /// written first (the checker rejects these regions, `E1013`, but the syntactic comparison
    /// reads any signature). By hand: the keys are `(ref {_0} i64 -> i64)` and
    /// `(ref {_0} string -> i64)`, `i64` sorts before `string`, so the region `b` is named first
    /// (`r0`).
    #[test]
    fn members_order_ignores_where_regions_first_appear() {
        let canonical = |source: &str| {
            let mut renaming = Renaming::new(true, None);
            type_text(&renaming.ty(&parameter(source)))
        };
        for source in [
            "def f :: ((ref {a} string -> i64) | (ref {b} i64 -> i64)) -> unit = \\v -> ()\n",
            "def f :: ((ref {b} i64 -> i64) | (ref {a} string -> i64)) -> unit = \\v -> ()\n",
        ] {
            assert_eq!(
                canonical(source),
                "((ref {r0} i64 -> i64) | (ref {r1} string -> i64))",
                "{source}"
            );
        }
    }

    /// Verification of Copilot review 2 of #102: the blind key keeps which unnamed region is
    /// which. Two quantified members that use the same region names in other places had the same
    /// key when every unnamed region was `_` (`({_ _} ref {_} i64 -> ref {_} i64 -> i64)`), so
    /// they kept their written order and, sharing `r0` and `r1`, printed in that order: a reorder
    /// was a change. By hand: the binders number `a` and `b` in each member, so the keys are
    /// `({_0 _1} ref {_0} i64 -> ref {_1} i64 -> i64)` and `({_0 _1} ref {_1} i64 -> ref {_0} i64
    /// -> i64)`; `0` sorts before `1`, so the first member comes first in either written order,
    /// and `a` is `r0` and `b` is `r1` in both members.
    #[test]
    fn members_keep_which_region_is_which() {
        let canonical = |source: &str| {
            let mut renaming = Renaming::new(true, None);
            type_text(&renaming.ty(&parameter(source)))
        };
        let first = "({a b} ref {a} i64 -> ref {b} i64 -> i64)";
        let second = "({a b} ref {b} i64 -> ref {a} i64 -> i64)";
        for (left, right) in [(first, second), (second, first)] {
            let source = format!("def f :: ({left} | {right}) -> unit = \\v -> ()\n");
            assert_eq!(
                canonical(&source),
                "(({r0 r1} ref {r0} i64 -> ref {r1} i64 -> i64) | ({r0 r1} ref {r1} i64 -> ref {r0} i64 -> i64))",
                "{source}"
            );
        }
    }

    /// The comparison's text of the first function of `source`, written with every length name
    /// in `lengths` as a length parameter (`#N`, as `--effective` reads a signature's lengths).
    fn canonical_def_with(source: &str, lengths: &[&str]) -> String {
        fn lengths_of(ty: &mut TypeExpr, lengths: &[&str]) {
            match &mut ty.kind {
                TypeExprKind::FixedArray(element, length) => {
                    lengths_of(element, lengths);
                    if let TypeExprKind::Named(name) = &length.kind
                        && lengths.contains(&name.as_str())
                    {
                        length.kind = TypeExprKind::Variable(format!("#{name}"));
                    }
                }
                TypeExprKind::AnonymousUnion(types) => {
                    types.iter_mut().for_each(|ty| lengths_of(ty, lengths))
                }
                TypeExprKind::Tuple(types) => {
                    types.iter_mut().for_each(|ty| lengths_of(ty, lengths))
                }
                TypeExprKind::Function(parameters, result) => {
                    parameters.iter_mut().for_each(|ty| lengths_of(ty, lengths));
                    lengths_of(result, lengths);
                }
                _ => {}
            }
        }
        let mut program = crate::parser::parse(source).unwrap();
        let function = &mut program.functions[0];
        for parameter in &mut function.parameters {
            lengths_of(&mut parameter.ty, lengths);
        }
        lengths_of(&mut function.result, lengths);
        let types: Vec<_> = function
            .parameters
            .iter()
            .map(|parameter| &parameter.ty)
            .collect();
        function_text(
            "def",
            &function.name.text,
            &function.regions,
            &function.constraints,
            &types,
            &function.result,
            Renaming::new(true, None),
        )
    }

    fn canonical_def(source: &str) -> String {
        canonical_def_with(source, &[])
    }

    /// Copilot review 3 of #102: members whose blind keys tie (they differ only in the names of
    /// their type variables, regions or length parameters) take one order, whatever order they
    /// were written in and whatever their names, also where only the rest of the signature tells
    /// them apart. Each group lists equivalent signatures: the members permuted, the names
    /// changed consistently, the result curried. Expected texts by hand, from the rules of
    /// `Renaming::tied` (a place is written `p:` for the parameter `p` of the flattened chain,
    /// then the parts down to the name; `m` is the place inside the member, the same for tied
    /// members):
    ///
    /// 1. Copilot's example: the keys are `Maybe<'_0>` for both members; `'b` occurs only at `m`,
    ///    `'a` at `m` and `1:`, so `Maybe<'b>` (places `[[m]]`) comes before `Maybe<'a>`
    ///    (`[[m, 1:]]`): `'b` is `'t0`, `'a` is `'t1`, and the parameter is `'t1`.
    /// 2. `'a` is also at `1:` and `'b` at `2:`, so `Maybe<'a>` comes first, curried or not.
    /// 3. Members that share a variable (a chain): `'a` is at `m*0`, `'b` at `m*0` and `m*1`,
    ///    `'c` at `m*1`; `('a * 'b)` has `[[m*0], [m*0, m*1]]`, `('b * 'c)` has
    ///    `[[m*0, m*1], [m*1]]`, so `('a * 'b)` comes first, `('t0 * 't1)` then `('t1 * 't2)`.
    /// 4. The region chain that review 2 left as a limit: as in 3, the member whose first region
    ///    occurs only there (`c, a`) comes first: `{r0} ... {r1}` then `{r1} ... {r2}`.
    /// 5. The region ring: `(a, b, c)` and `(b, c, a)` have the same places, so both orders are
    ///    tried. First `(a, b, c)` gives `(r1, r2, r0)` for the other member, first `(b, c, a)`
    ///    gives `(r2, r0, r1)`; the least text is the first.
    /// 6. The chain of 3 with length parameters (`[i64; N]` as `--effective` reads it):
    ///    `n0, n1` then `n1, n2`, as in 3.
    /// 7. A union inside tied members: the outer member whose variables include the earlier
    ///    parameter (`'a` at `1:`) comes first, and inside each, the member whose variable is a
    ///    parameter comes second (as in 1): `'b 'a 'd 'c` are `t0 t1 t2 t3`, the parameters
    ///    `'t1 -> 't3`.
    /// 8. Seven members told apart only by where their variables occur in a later tuple: by
    ///    those places, `'a0` to `'a6` are `'t0` to `'t6` in either written order (searching
    ///    the 7! orders would pass `MAX_TIE_RUNS`).
    #[test]
    fn tied_members_take_one_order() {
        let chain = |order: &mut dyn Iterator<Item = usize>| {
            let members: Vec<String> = order.map(|n| format!("Maybe<'a{n}>")).collect();
            let tuple: Vec<String> = (0..7).map(|n| format!("'a{n}")).collect();
            format!(
                "def f :: ({}) -> ({}) -> i64 = \\v x -> 0\n",
                members.join(" | "),
                tuple.join(" * ")
            )
        };
        let (forward, backward) = (chain(&mut (0..7)), chain(&mut (0..7).rev()));
        let groups: [(&[&str], &[&str], &str); 8] = [
            (
                &[
                    "def f :: (Maybe<'a> | Maybe<'b>) -> 'a -> i64 = \\v x -> 0\n",
                    "def f :: (Maybe<'b> | Maybe<'a>) -> 'a -> i64 = \\v x -> 0\n",
                    "def f :: (Maybe<'x> | Maybe<'y>) -> 'y -> i64 = \\v x -> 0\n",
                ],
                &[],
                "def f :: (Maybe<'t0> | Maybe<'t1>) -> 't1 -> i64",
            ),
            (
                &[
                    "def f :: (Maybe<'a> | Maybe<'b>) -> 'a -> 'b -> i64 = \\v x y -> 0\n",
                    "def f :: (Maybe<'b> | Maybe<'a>) -> 'a -> 'b -> i64 = \\v x y -> 0\n",
                    "def f :: (Maybe<'a> | Maybe<'b>) -> 'b -> 'a -> i64 = \\v x y -> 0\n",
                    "def f :: (Maybe<'b> | Maybe<'a>) -> 'b -> 'a -> i64 = \\v x y -> 0\n",
                    "def f :: (Maybe<'a> | Maybe<'b>) -> ('a -> 'b -> i64) = \\v -> todo ()\n",
                    "def f :: (Maybe<'b> | Maybe<'a>) -> ('b -> ('a -> i64)) = \\v -> todo ()\n",
                ],
                &[],
                "def f :: (Maybe<'t0> | Maybe<'t1>) -> 't0 -> 't1 -> i64",
            ),
            (
                &[
                    "def f :: (Maybe<('a * 'b)> | Maybe<('b * 'c)>) -> i64 = \\v -> 0\n",
                    "def f :: (Maybe<('b * 'c)> | Maybe<('a * 'b)>) -> i64 = \\v -> 0\n",
                    "def f :: (Maybe<('c * 'b)> | Maybe<('b * 'a)>) -> i64 = \\v -> 0\n",
                ],
                &[],
                "def f :: (Maybe<('t0 * 't1)> | Maybe<('t1 * 't2)>) -> i64",
            ),
            (
                &[
                    "def f :: ((ref {c} string -> ref {a} i64 -> bool) | (ref {a} string -> ref {b} i64 -> bool)) -> unit = \\v -> ()\n",
                    "def f :: ((ref {a} string -> ref {b} i64 -> bool) | (ref {c} string -> ref {a} i64 -> bool)) -> unit = \\v -> ()\n",
                    "def f :: ((ref {x} string -> ref {y} i64 -> bool) | (ref {z} string -> ref {x} i64 -> bool)) -> unit = \\v -> ()\n",
                ],
                &[],
                "def f :: ((ref {r0} string -> ref {r1} i64 -> bool) | (ref {r1} string -> ref {r2} i64 -> bool)) -> unit",
            ),
            (
                &[
                    "def f :: ((ref {a} i64 * ref {b} i64 * ref {c} i64) | (ref {b} i64 * ref {c} i64 * ref {a} i64)) -> unit = \\v -> ()\n",
                    "def f :: ((ref {b} i64 * ref {c} i64 * ref {a} i64) | (ref {a} i64 * ref {b} i64 * ref {c} i64)) -> unit = \\v -> ()\n",
                    "def f :: ((ref {b} i64 * ref {c} i64 * ref {a} i64) | (ref {c} i64 * ref {a} i64 * ref {b} i64)) -> unit = \\v -> ()\n",
                ],
                &[],
                "def f :: ((ref {r0} i64 * ref {r1} i64 * ref {r2} i64) | (ref {r1} i64 * ref {r2} i64 * ref {r0} i64)) -> unit",
            ),
            (
                &[
                    "def f :: (([i64; N] * [i64; M]) | ([i64; M] * [i64; K])) -> i64 = \\v -> 0\n",
                    "def f :: (([i64; M] * [i64; K]) | ([i64; N] * [i64; M])) -> i64 = \\v -> 0\n",
                    "def f :: (([i64; K] * [i64; M]) | ([i64; M] * [i64; N])) -> i64 = \\v -> 0\n",
                ],
                &["N", "M", "K"],
                "def f :: (([i64; n0] * [i64; n1]) | ([i64; n1] * [i64; n2])) -> i64",
            ),
            (
                &[
                    "def f :: ([(Maybe<'a> | Maybe<'b>)] | [(Maybe<'c> | Maybe<'d>)]) -> 'a -> 'c -> i64 = \\v x y -> 0\n",
                    "def f :: ([(Maybe<'d> | Maybe<'c>)] | [(Maybe<'b> | Maybe<'a>)]) -> 'a -> 'c -> i64 = \\v x y -> 0\n",
                    "def f :: ([(Maybe<'c> | Maybe<'d>)] | [(Maybe<'b> | Maybe<'a>)]) -> 'c -> 'a -> i64 = \\v x y -> 0\n",
                ],
                &[],
                "def f :: ([(Maybe<'t0> | Maybe<'t1>)] | [(Maybe<'t2> | Maybe<'t3>)]) -> 't1 -> 't3 -> i64",
            ),
            (
                &[&forward, &backward],
                &[],
                "def f :: (Maybe<'t0> | Maybe<'t1> | Maybe<'t2> | Maybe<'t3> | Maybe<'t4> | Maybe<'t5> | Maybe<'t6>) -> ('t0 * 't1 * 't2 * 't3 * 't4 * 't5 * 't6) -> i64",
            ),
        ];
        for (sources, lengths, expected) in groups {
            for source in sources {
                assert_eq!(canonical_def_with(source, lengths), expected, "{source}");
            }
        }
    }

    /// Tied members whose variables occur nowhere else can be swapped without changing anything,
    /// so they need no search: 32 of them (32! orders) take one run, and identical members
    /// (`Maybe<'z>` twice) are no choice either. By hand: the 34 members tie with the key
    /// `Maybe<'_0>`; every `'vN` occurs only in its member (places `[[m]]`), `'z` in both of its
    /// members and as the second parameter (`[[m, m, 1:]]`), so the 32 `Maybe<'vN>` come first,
    /// in any order, as `'t0` to `'t31`, and the two `Maybe<'z>` follow as `Maybe<'t32>`.
    /// `render` runs 3 times: the first run, which meets the tie, the census of the names, and
    /// one run of the search.
    #[test]
    fn interchangeable_tied_members_need_no_search() {
        let mut members: Vec<String> = (0..32).rev().map(|n| format!("Maybe<'v{n}>")).collect();
        members.insert(7, "Maybe<'z>".to_owned());
        members.push("Maybe<'z>".to_owned());
        let source = format!(
            "def f :: ({}) -> 'z -> i64 = \\v x -> 0\n",
            members.join(" | ")
        );
        RUNS.with(|runs| runs.set(0));
        let text = canonical_def(&source);
        let mut expected: Vec<String> = (0..32).map(|n| format!("Maybe<'t{n}>")).collect();
        expected.push("Maybe<'t32>".to_owned());
        expected.push("Maybe<'t32>".to_owned());
        assert_eq!(
            text,
            format!("def f :: ({}) -> 't32 -> i64", expected.join(" | "))
        );
        assert_eq!(RUNS.with(Cell::get), 3);
    }

    /// A 3-ring of `'a`, `'b`, `'c` as a union of two tuples, the second member first if
    /// `swapped`: `region` writes each variable as a borrow of an `i64` with that region.
    fn ring(a: &str, b: &str, c: &str, swapped: bool, region: bool) -> String {
        let name = |name: &str| {
            if region {
                format!("ref {{{name}}} i64")
            } else {
                format!("'{name}")
            }
        };
        let first = format!("({} * {} * {})", name(a), name(b), name(c));
        let second = format!("({} * {} * {})", name(b), name(c), name(a));
        if swapped {
            format!("({second} | {first})")
        } else {
            format!("({first} | {second})")
        }
    }

    /// Verification of Copilot review 3 of #102: unions that share no names with the rest of the
    /// declaration are settled one after the other (`Renaming::local`), not as one product of
    /// their choices. Fourteen 3-rings have `2^14 = 16,384` combinations, more than
    /// `MAX_TIE_RUNS`; searched as a product, the first ring kept its written order and swapping
    /// its members was a change. By hand, for the ring of `t(n)`, `t(n+1)`, `t(n+2)`: the two
    /// members tie on their keys and colors (each name occurs once in each member) and share
    /// their names, so the ring's own search renders both orders. The member taken first is
    /// `(t(n) * t(n+1) * t(n+2))` either way; the other is `(t(n+1) * t(n+2) * t(n))` after
    /// `(a, b, c)`, and `(t(n+2) * t(n) * t(n+1))` after `(b, c, a)`. With `n = 3k`, no ring has
    /// both a one-digit and a two-digit number in those places, so `t(n+1)` sorts first and both
    /// writings of every ring print the first form. `render` runs 3 times (the first run, the
    /// census, one run of the search, which meets no open choice), and each ring's own search
    /// twice.
    #[test]
    fn independent_ties_are_searched_one_by_one() {
        for region in [false, true] {
            let (variable, prefix) = if region { ("", "r") } else { ("'", "t") };
            let mut expected = Vec::new();
            for k in 0..14 {
                let [a, b, c] =
                    [3 * k, 3 * k + 1, 3 * k + 2].map(|n| format!("{variable}{prefix}{n}"));
                let name = |name: &str| {
                    if region {
                        format!("ref {{{name}}} i64")
                    } else {
                        name.to_owned()
                    }
                };
                expected.push(format!(
                    "(({} * {} * {}) | ({} * {} * {}))",
                    name(&a),
                    name(&b),
                    name(&c),
                    name(&b),
                    name(&c),
                    name(&a)
                ));
            }
            let expected = format!("def f :: {} -> unit", expected.join(" -> "));
            for (swap, letters) in [
                (0usize, ["a", "b", "c"]),
                (1, ["a", "b", "c"]),
                (14, ["x", "y", "z"]),
            ] {
                let parameters: Vec<String> = (0..14)
                    .map(|k| {
                        let [a, b, c] = letters.map(|letter| format!("{letter}{k}"));
                        ring(&a, &b, &c, k < swap, region)
                    })
                    .collect();
                let source = format!(
                    "def f :: {} -> unit = \\v -> todo ()\n",
                    parameters.join(" -> ")
                );
                RUNS.with(|runs| runs.set(0));
                LOCAL_RUNS.with(|runs| runs.set(0));
                assert_eq!(canonical_def(&source), expected, "{source}");
                assert_eq!(RUNS.with(Cell::get), 3, "{source}");
                assert_eq!(LOCAL_RUNS.with(Cell::get), 28, "{source}");
            }
        }
    }

    /// Verification of Copilot review 3 of #102: tied members that hold all the occurrences of
    /// their names are taken without trying also when they have a union inside: each is
    /// rendered alone, and alike members render the same. 32 members `[('vN | i64)]` searched
    /// every order up to `MAX_TIE_RUNS` (the union inside made them never interchangeable). By
    /// hand: the keys tie (`[('_ | i64)]`, `'_` before `i64`) and every `'vN` occurs only in its
    /// member, so the union is one group whose names occur nowhere else: one run of its own
    /// search, in which each pick renders each remaining member alone (32 + 31 + ... + 1 = 528
    /// runs, one each: the members inside do not tie), all as `[('t(n) | i64)]`, and takes the
    /// first. `render` runs 3 times, as above; each of the 4 parameters is such a union, so the
    /// local runs are `4 * 529`.
    #[test]
    fn private_members_with_unions_need_no_search() {
        let union = |parameter: usize, reversed: bool| {
            let mut members: Vec<String> = (0..32)
                .map(|n| format!("[('v{parameter}_{n} | i64)]"))
                .collect();
            if reversed {
                members.reverse();
            }
            format!("({})", members.join(" | "))
        };
        let expected: Vec<String> = (0..4)
            .map(|parameter| {
                let members: Vec<String> = (0..32)
                    .map(|n| format!("[('t{} | i64)]", 32 * parameter + n))
                    .collect();
                format!("({})", members.join(" | "))
            })
            .collect();
        let expected = format!("def f :: {} -> unit", expected.join(" -> "));
        for reversed in [false, true] {
            let parameters: Vec<String> = (0..4)
                .map(|parameter| union(parameter, reversed && parameter % 2 == 0))
                .collect();
            let source = format!(
                "def f :: {} -> unit = \\v -> todo ()\n",
                parameters.join(" -> ")
            );
            RUNS.with(|runs| runs.set(0));
            LOCAL_RUNS.with(|runs| runs.set(0));
            assert_eq!(canonical_def(&source), expected, "{source}");
            assert_eq!(RUNS.with(Cell::get), 3);
            assert_eq!(LOCAL_RUNS.with(Cell::get), 4 * 529);
        }
    }

    /// The comparison's header and member texts of the class of `source`.
    fn canonical_class(source: &str) -> (String, Vec<String>) {
        let program = crate::parser::parse(source).unwrap();
        let mut items = Items::new();
        add_items("Main", &program, &mut items, true, Edition::E2026, None);
        let item = items.values().next().expect("the class is public");
        let members = item
            .members
            .iter()
            .map(|member| member.text.clone())
            .collect();
        (item.header.clone(), members)
    }

    /// Verification of Copilot review 3 of #102: the search of a class runs the searches of its
    /// methods in each of its runs, and they no longer multiply. Four superclass constraints that
    /// are 3-rings of variables and a method with twelve 3-rings of regions took a minute in a
    /// dev build. By hand: `'t` is `'t0`; the rings of the constraints, rendered in the written
    /// order, are `'t1..'t3`, `'t4..'t6`, `'t7..'t9` and `'t10..'t12`, each in its first form as
    /// in `independent_ties_are_searched_one_by_one`, and the constraints are sorted by text
    /// (`'t1 ` before `'t10`, which is before `'t4`). The method's rings are `r0..r35`. The class
    /// renders 3 times (first run, census, one run of the search). The method's text is
    /// computed 3 times: by the census that lists the names it reaches (one run), in the class's
    /// first run (3 runs: first run, census, one run of the search) and in the class's census
    /// (one run: no tie is open there); the class's search run gives `'t` the same name and as
    /// many names before the method as the first run, so it reuses the text: 8 runs in all.
    /// Local runs: 2 for each ring of the method in the method's search run (24), and 2 for each
    /// ring of the constraints in the class's search run (8).
    #[test]
    fn class_searches_do_not_multiply() {
        let source = |swapped: bool| {
            let constraints: Vec<String> = (0..4)
                .map(|k| {
                    let [a, b, c] = ["a", "b", "c"].map(|letter| format!("{letter}s{k}"));
                    format!("Show<{}>", ring(&a, &b, &c, swapped && k == 0, false))
                })
                .collect();
            let rings: Vec<String> = (0..12)
                .map(|k| {
                    let [a, b, c] = ["a", "b", "c"].map(|letter| format!("{letter}m{k}"));
                    ring(&a, &b, &c, swapped && k == 0, true)
                })
                .collect();
            format!(
                "class ({}) => C<'t> {{\n    def m :: 't -> {} -> unit\n}}\n",
                constraints.join(", "),
                rings.join(" -> ")
            )
        };
        let variables = |n: usize| {
            let [a, b, c] = [n, n + 1, n + 2].map(|n| format!("'t{n}"));
            format!("Show<(({a} * {b} * {c}) | ({b} * {c} * {a}))>")
        };
        let header = format!(
            "class ({}, {}, {}, {}) => C<'t0>",
            variables(1),
            variables(10),
            variables(4),
            variables(7)
        );
        let rings: Vec<String> = (0..12)
            .map(|k| {
                let [a, b, c] = [3 * k, 3 * k + 1, 3 * k + 2].map(|n| format!("ref {{r{n}}} i64"));
                format!("(({a} * {b} * {c}) | ({b} * {c} * {a}))")
            })
            .collect();
        let method = format!("def m :: 't0 -> {} -> unit", rings.join(" -> "));
        for swapped in [false, true] {
            RUNS.with(|runs| runs.set(0));
            LOCAL_RUNS.with(|runs| runs.set(0));
            assert_eq!(
                canonical_class(&source(swapped)),
                (header.clone(), vec![method.clone()])
            );
            assert_eq!(RUNS.with(Cell::get), 8);
            assert_eq!(LOCAL_RUNS.with(Cell::get), 32);
        }
    }

    /// Verification of Copilot review 3 of #102: when the class's own search has open choices,
    /// each run reuses the text of a method whose names it gives as an earlier run did. By hand:
    /// the three members of `Show<(Maybe<'x> | Maybe<'y> | Maybe<'z>)>` tie on their keys and
    /// colors (each name also occurs once in `Eq<('x | 'y | 'z)>`), so the class's search runs
    /// all `3! = 6` orders, which print the same: `Eq<...>` sorts before `Show<...>`, and the
    /// members of the `Eq` union, named by then, order by their names. Every run gives `'t` the
    /// name `'t0` and four names before the method, whose twelve region rings print as in
    /// `class_searches_do_not_multiply`. Runs: the census of the method's names (1), the class's
    /// first run with the method's search (1 + 3), the class's census with the method's census
    /// run (1 + 1), and the six runs of the class's search, which reuse the method's text: 13.
    /// Local runs: the method's 12 rings, twice each, in its one search run (24).
    #[test]
    fn class_methods_are_searched_once() {
        let rings: Vec<String> = (0..12)
            .map(|k| {
                let [a, b, c] = ["a", "b", "c"].map(|letter| format!("{letter}{k}"));
                ring(&a, &b, &c, false, true)
            })
            .collect();
        let source = format!(
            "class (Show<(Maybe<'x> | Maybe<'y> | Maybe<'z>)>, Eq<('x | 'y | 'z)>) => C<'t> {{\n    def m :: 't -> {} -> unit\n}}\n",
            rings.join(" -> ")
        );
        let rings: Vec<String> = (0..12)
            .map(|k| {
                let [a, b, c] = [3 * k, 3 * k + 1, 3 * k + 2].map(|n| format!("ref {{r{n}}} i64"));
                format!("(({a} * {b} * {c}) | ({b} * {c} * {a}))")
            })
            .collect();
        RUNS.with(|runs| runs.set(0));
        LOCAL_RUNS.with(|runs| runs.set(0));
        assert_eq!(
            canonical_class(&source),
            (
                "class (Eq<('t1 | 't2 | 't3)>, Show<(Maybe<'t1> | Maybe<'t2> | Maybe<'t3>)>) => C<'t0>"
                    .to_owned(),
                vec![format!("def m :: 't0 -> {} -> unit", rings.join(" -> "))]
            )
        );
        assert_eq!(RUNS.with(Cell::get), 13);
        assert_eq!(LOCAL_RUNS.with(Cell::get), 24);
    }

    /// Verification of Copilot review 3 of #102: the searches of one declaration share one
    /// budget of `MAX_TIE_RUNS` runs, so a search inside each run of another (a class's method)
    /// cannot multiply the work. By hand: seven `Maybe<'aN>` whose names also make up a later
    /// union tie on their keys and colors and are not private, so the `7! = 5,040` orders are
    /// choices of the search (all print the same: the later union then orders its members by
    /// their names). `render` runs once, then the census and the search spend the budget: 1 +
    /// 4,096 runs. In the class, the census of the names that the method reaches renders it
    /// once, then the method's search spends the whole budget in the class's first run, so the
    /// class's own search (its two superclass constraints tie on three members) stops there: 1
    /// + 1 + 4,097 runs.
    #[test]
    fn searches_share_one_budget() {
        let names: Vec<String> = (0..7).map(|n| format!("'a{n}")).collect();
        let maybes: Vec<String> = names.iter().map(|name| format!("Maybe<{name}>")).collect();
        let signature = format!(
            "({}) -> ({}) -> unit",
            maybes.join(" | "),
            names.join(" | ")
        );
        let source = format!("def f :: {signature} = \\v x -> todo ()\n");
        let expected: Vec<String> = (0..7).map(|n| format!("'t{n}")).collect();
        let maybes: Vec<String> = expected
            .iter()
            .map(|name| format!("Maybe<{name}>"))
            .collect();
        RUNS.with(|runs| runs.set(0));
        assert_eq!(
            canonical_def(&source),
            format!(
                "def f :: ({}) -> ({}) -> unit",
                maybes.join(" | "),
                expected.join(" | ")
            )
        );
        assert_eq!(RUNS.with(Cell::get), 1 + MAX_TIE_RUNS);
        let class = format!(
            "class (Show<(Maybe<'x> | Maybe<'y> | Maybe<'z>)>, Eq<('x | 'y | 'z)>) => C<'t> {{\n    def m :: 't -> {signature}\n}}\n"
        );
        RUNS.with(|runs| runs.set(0));
        canonical_class(&class);
        assert_eq!(RUNS.with(Cell::get), 3 + MAX_TIE_RUNS);
    }

    /// Each member of a union inside a blind key is numbered from the numbering before that union,
    /// so the key does not depend on the order of those members either. By hand: in
    /// `[(S | I)]` with `S = ({a} ref {a} string -> i64)` and `I = ({b} ref {b} i64 -> i64)`, both
    /// members number their region `_0`, `i64` sorts before `string`, so the key is
    /// `[(({_0} ref {_0} i64 -> i64) | ({_0} ref {_0} string -> i64))]` in either written order.
    /// It sorts before the key of `Z = [(({c} ref {c} i64 -> string) | bool)]`,
    /// `[(({_0} ref {_0} i64 -> string) | bool)]` (`i64 -> i` before `i64 -> s`). Numbered in the
    /// written order instead, `[(S | I)]` would key as `[(({_0} ref {_0} string -> i64) | ({_1}
    /// ...))]` and sort after `Z`. Renamed in the sorted order: `b` is `r0`, `a` is `r1`, `c` is
    /// `r2`.
    #[test]
    fn nested_members_number_their_regions_afresh() {
        let canonical = |source: &str| {
            let mut renaming = Renaming::new(true, None);
            type_text(&renaming.ty(&parameter(source)))
        };
        let string = "({a} ref {a} string -> i64)";
        let integer = "({b} ref {b} i64 -> i64)";
        let other = "[(({c} ref {c} i64 -> string) | bool)]";
        for (left, right) in [(string, integer), (integer, string)] {
            let nested = format!("[({left} | {right})]");
            for union in [
                format!("({nested} | {other})"),
                format!("({other} | {nested})"),
            ] {
                let source = format!("def f :: {union} -> unit = \\v -> ()\n");
                assert_eq!(
                    canonical(&source),
                    "([(({r0} ref {r0} i64 -> i64) | ({r1} ref {r1} string -> i64))] | [(({r2} ref {r2} i64 -> string) | bool)])",
                    "{source}"
                );
            }
        }
    }
}
