//! `kg impact` (G28 Phase 3, D-60): what changed between two knowledge graphs, classified like
//! `api-diff` (breaking, non-breaking) and beyond it (info: facts and warnings), and which tests
//! the change reaches. The tests are a reverse reachability in the head graph: from the nodes of
//! the changed modules against `calls`, with a call of a class method reaching the same method of
//! every instance of the class. It never touches the disk.

use std::collections::{BTreeMap, BTreeSet};

use super::algo::{self, Digraph};
use super::brief::{norm, plural};
use super::json::Object;
use super::store::{KgData, Node};

const MEMBERS: [&str; 3] = ["field", "case", "method"];
/// How deep the reverse walks go.
pub const MAX_DEPTH: usize = 8;
/// The most changes the text and JSON list.
pub const MAX_LISTED_CHANGES: usize = 1000;
const ACTIONS: [&str; 11] = [
    "removed",
    "added",
    "changed",
    "deprecated",
    "undeprecated",
    "documented",
    "increased",
    "decreased",
    "new",
    "fixed",
    "skipped",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Breaking,
    NonBreaking,
    Info,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::Info => "info",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub severity: Severity,
    pub action: &'static str,
    pub kind: String,
    pub name: String,
    pub old: Option<String>,
    pub new: Option<String>,
    pub note: String,
}

impl Change {
    fn new(severity: Severity, action: &'static str, kind: &str, name: &str) -> Self {
        Self {
            severity,
            action,
            kind: kind.to_owned(),
            name: name.to_owned(),
            old: None,
            new: None,
            note: String::new(),
        }
    }

    fn values(mut self, old: Option<String>, new: Option<String>) -> Self {
        self.old = old;
        self.new = new;
        self
    }

    fn note(mut self, note: &str) -> Self {
        self.note = note.to_owned();
        self
    }

    /// The text of the change, in `api-diff`'s form where both have it.
    pub fn message(&self) -> String {
        if self.kind == "effect" {
            let effect = self.old.as_deref().or(self.new.as_deref()).unwrap_or("");
            let mut text = format!("{} effect {effect} on {}", self.action, self.name);
            if !self.note.is_empty() {
                text.push_str(&format!(" ({})", self.note));
            }
            return text;
        }
        if self.kind == "warning" {
            let at = self.old.as_deref().or(self.new.as_deref()).unwrap_or("");
            return format!("{} warning {} at {at}", self.action, self.name);
        }
        let mut text = format!("{} {}", self.action, self.kind);
        if !self.name.is_empty() {
            text.push_str(&format!(" {}", self.name));
        }
        match (self.action, &self.old, &self.new) {
            ("deprecated", _, Some(new)) => text.push_str(&format!(": {new}")),
            ("undeprecated", Some(old), _) => text.push_str(&format!(": {old}")),
            ("deprecated" | "undeprecated", _, _) => {}
            (_, Some(old), Some(new)) => text.push_str(&format!(": `{old}` => `{new}`")),
            (_, Some(value), None) | (_, None, Some(value)) => {
                text.push_str(&format!(": `{value}`"))
            }
            (_, None, None) => {}
        }
        if !self.note.is_empty() {
            text.push_str(&format!(" ({})", self.note));
        }
        text
    }

    fn key(&self) -> (Severity, &str, &str, usize, bool, &str, bool, &str) {
        (
            self.severity,
            &self.name,
            &self.kind,
            ACTIONS
                .iter()
                .position(|action| *action == self.action)
                .unwrap_or(ACTIONS.len()),
            self.old.is_some(),
            self.old.as_deref().unwrap_or(""),
            self.new.is_some(),
            self.new.as_deref().unwrap_or(""),
        )
    }
}

/// The tests that the change reaches, or every test with the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tests {
    Some(Vec<String>),
    All(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Impact {
    pub base: String,
    pub head: String,
    pub changes: Vec<Change>,
    pub tests: Tests,
    pub total_tests: usize,
    /// For each affected test, a shortest chain of calls from a changed node to it (D-60).
    pub paths: BTreeMap<String, Vec<String>>,
}

/// A signature with its type variables renamed `'t0`, `'t1`, ... in order of appearance.
pub fn canon(signature: &str) -> String {
    let text = norm(signature);
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, character)) = chars.next() {
        if character == '\''
            && let Some(&(_, first)) = chars.peek()
            && (first.is_ascii_alphabetic() || first == '_')
        {
            let mut name = String::from("'");
            while let Some(&(_, next)) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            let count = names.len();
            let number = *names.entry(name).or_insert(count);
            out.push_str(&format!("'t{number}"));
        } else {
            out.push(character);
        }
    }
    out
}

fn parent_id(id: &str) -> &str {
    id.rsplit_once('.').map_or(id, |(parent, _)| parent)
}

fn shown(node: &Node) -> String {
    norm(&node.signature)
}

fn convs(node: &Node) -> Vec<&str> {
    node.params
        .iter()
        .map(|param| param.conv.as_str())
        .collect()
}

fn symbol_changes(base: &KgData, head: &KgData) -> Vec<Change> {
    use Severity::*;
    let mut out = Vec::new();
    let old: BTreeMap<&str, &Node> = base
        .nodes
        .iter()
        .filter(|node| node.kind != "module")
        .map(|node| (node.id.as_str(), node))
        .collect();
    let new: BTreeMap<&str, &Node> = head
        .nodes
        .iter()
        .filter(|node| node.kind != "module")
        .map(|node| (node.id.as_str(), node))
        .collect();
    for (id, node) in &old {
        if new.contains_key(id) {
            continue;
        }
        if MEMBERS.contains(&node.kind.as_str()) && !new.contains_key(parent_id(id)) {
            continue;
        }
        let note = match node.kind.as_str() {
            "field" => "uses of the field stop compiling",
            "case" => "uses of the case stop compiling",
            "method" => "calls and instances of the method stop compiling",
            _ => "",
        };
        let severity = if node.public { Breaking } else { NonBreaking };
        out.push(
            Change::new(severity, "removed", &node.kind, id)
                .values(Some(shown(node)), None)
                .note(note),
        );
    }
    for (id, node) in &new {
        if old.contains_key(id) {
            continue;
        }
        if MEMBERS.contains(&node.kind.as_str()) && !old.contains_key(parent_id(id)) {
            continue;
        }
        let in_class = node.kind == "method"
            && new
                .get(parent_id(id))
                .is_some_and(|parent| parent.kind == "class");
        let change =
            Change::new(NonBreaking, "added", &node.kind, id).values(None, Some(shown(node)));
        out.push(match node.kind.as_str() {
            "field" if node.public => Change {
                severity: Breaking,
                ..change.note("constructions of the record without it stop compiling")
            },
            "case" if node.public => Change {
                severity: Breaking,
                ..change.note("exhaustive matches over the union stop compiling")
            },
            _ if in_class && node.public => Change {
                severity: Breaking,
                ..change.note("existing instances do not define the method")
            },
            _ => change,
        });
    }
    for (id, before) in &old {
        let Some(after) = new.get(id) else {
            continue;
        };
        let api = if before.public { Breaking } else { NonBreaking };
        if canon(&before.signature) != canon(&after.signature) {
            out.push(
                Change::new(api, "changed", &after.kind, id)
                    .values(Some(shown(before)), Some(shown(after))),
            );
        } else if convs(before) != convs(after) {
            out.push(
                Change::new(api, "changed", "conv", id)
                    .values(Some(convs(before).join(",")), Some(convs(after).join(",")))
                    .note("ownership passing of an argument changed"),
            );
        }
        let flag = |value: bool| if value { "true" } else { "false" }.to_owned();
        if before.public != after.public {
            let visibility = |public: bool| if public { "public" } else { "private" }.to_owned();
            out.push(
                Change::new(
                    if after.public { NonBreaking } else { Breaking },
                    "changed",
                    "visibility",
                    id,
                )
                .values(
                    Some(visibility(before.public)),
                    Some(visibility(after.public)),
                ),
            );
        }
        for (key, was, now) in [
            ("export", before.export, after.export),
            ("pure", before.pure, after.pure),
        ] {
            if was != now {
                out.push(
                    Change::new(if now { NonBreaking } else { Breaking }, "changed", key, id)
                        .values(Some(flag(was)), Some(flag(now))),
                );
            }
        }
        match (&before.deprecated, &after.deprecated) {
            (None, Some(note)) => out.push(
                Change::new(NonBreaking, "deprecated", &after.kind, id)
                    .values(None, Some(note.clone()).filter(|note| !note.is_empty())),
            ),
            (Some(note), None) => out.push(
                Change::new(NonBreaking, "undeprecated", &after.kind, id)
                    .values(Some(note.clone()).filter(|note| !note.is_empty()), None),
            ),
            _ => {}
        }
        if before.doc.as_deref().unwrap_or("") != after.doc.as_deref().unwrap_or("")
            && before.deprecated.is_none() == after.deprecated.is_none()
        {
            out.push(Change::new(NonBreaking, "documented", &after.kind, id));
        }
    }
    if base.edges.is_some() && head.edges.is_some() {
        let implemented = |data: &KgData| -> BTreeSet<(String, String)> {
            data.edges()
                .iter()
                .filter(|edge| edge.rel == "implements")
                .map(|edge| (edge.from.clone(), edge.to.clone()))
                .collect()
        };
        let (was, now) = (implemented(base), implemented(head));
        for (from, to) in was.difference(&now) {
            if head.node(from).is_some_and(|node| node.public) {
                out.push(Change::new(
                    Breaking,
                    "removed",
                    "implements",
                    &format!("{from}.{to}"),
                ));
            }
        }
        for (from, to) in now.difference(&was) {
            out.push(Change::new(
                NonBreaking,
                "added",
                "implements",
                &format!("{from}.{to}"),
            ));
        }
    }
    for (id, before) in &old {
        let Some(_) = new.get(id) else {
            continue;
        };
        let child = match before.kind.as_str() {
            "record" => "field",
            "union" => "case",
            _ => continue,
        };
        let names = |data: &KgData| -> Vec<String> {
            data.children(id, child)
                .into_iter()
                .map(|node| node.name.clone())
                .collect()
        };
        let (was, now) = (names(base), names(head));
        let common_was: Vec<&String> = was.iter().filter(|name| now.contains(name)).collect();
        let common_now: Vec<&String> = now.iter().filter(|name| was.contains(name)).collect();
        if common_was != common_now {
            let join = |names: &[&String]| {
                names
                    .iter()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push(
                Change::new(
                    if before.public { Breaking } else { NonBreaking },
                    "changed",
                    "order",
                    id,
                )
                .values(Some(join(&common_was)), Some(join(&common_now)))
                .note("the order of fields and cases is part of the layout and of derived comparisons"),
            );
        }
    }
    out
}

/// The functions and tests that reach `id` by at most `MAX_DEPTH` reversed calls.
fn reach(head: &KgData, id: &str) -> (usize, usize) {
    let graph = head.index.digraph(head.edges(), &["calls"]);
    let Some(start) = head.index.number(id) else {
        return (0, 0);
    };
    let visits = algo::reverse_reach(&graph, &[start], MAX_DEPTH);
    let tests = visits
        .iter()
        .filter(|visit| visit.depth > 0 && head.index.name(visit.node).starts_with("test:"))
        .count();
    let all = visits.iter().filter(|visit| visit.depth > 0).count();
    (all - tests, tests)
}

fn fact_changes(base: &KgData, head: &KgData) -> Vec<Change> {
    use Severity::*;
    let (Some(old_facts), Some(new_facts)) = (&base.facts, &head.facts) else {
        return vec![skipped()];
    };
    if base.meta.compiler != head.meta.compiler {
        return vec![skipped()];
    }
    // A file with the same SHA-256 in both graphs has the same rows: only the others are read.
    let same: BTreeSet<(&str, &str)> = old_facts
        .files()
        .filter(|(_, sha)| !sha.is_empty())
        .collect();
    let mut out = Vec::new();
    let changed = new_facts
        .files()
        .filter(|file| !same.contains(file))
        .flat_map(|(path, _)| new_facts.rows(path));
    for after in changed {
        let id = &after.id;
        let (Some(before), Some(node)) = (base.fact(id), head.node(id)) else {
            continue;
        };
        if !matches!(node.kind.as_str(), "def" | "method" | "extern") {
            continue;
        }
        let was: BTreeSet<&String> = before.effects.iter().collect();
        let now: BTreeSet<&String> = after.effects.iter().collect();
        for effect in now.difference(&was) {
            let (functions, tests) = if head.edges.is_some() {
                reach(head, id)
            } else {
                (0, 0)
            };
            let note = if functions + tests > 0 {
                format!(
                    "reached by {} and {}",
                    plural(functions, "function", "functions"),
                    plural(tests, "test", "tests")
                )
            } else {
                String::new()
            };
            out.push(
                Change::new(
                    if node.public { Breaking } else { Info },
                    "added",
                    "effect",
                    id,
                )
                .values(None, Some((*effect).clone()))
                .note(&note),
            );
        }
        for effect in was.difference(&now) {
            out.push(
                Change::new(
                    if node.public { NonBreaking } else { Info },
                    "removed",
                    "effect",
                    id,
                )
                .values(Some((*effect).clone()), None),
            );
        }
        for (key, a, b) in [
            ("trap_sites", &before.trap_sites, &after.trap_sites),
            ("alloc_sites", &before.alloc_sites, &after.alloc_sites),
            ("copy_sites", &before.copy_sites, &after.copy_sites),
        ] {
            let count =
                |sites: &[super::store::Site]| sites.iter().filter(|site| !site.proven).count();
            let (a, b) = (count(a), count(b));
            if a != b {
                out.push(
                    Change::new(Info, if b > a { "increased" } else { "decreased" }, key, id)
                        .values(Some(a.to_string()), Some(b.to_string())),
                );
            }
        }
        if before.recursion != after.recursion {
            out.push(Change::new(Info, "changed", "recursion", id).values(
                Some(before.recursion.clone()),
                Some(after.recursion.clone()),
            ));
        }
    }
    out
}

fn skipped() -> Change {
    Change::new(Severity::Info, "skipped", "facts", "")
        .note("the two KGs were built by different compilers or one has no facts")
}

/// The warnings of `diag.jsonl`: (code, message, path, line).
fn warnings(data: &KgData) -> Vec<(String, String, String, usize)> {
    data.diag
        .iter()
        .filter(|row| row.severity == "warning")
        .map(|row| {
            (
                row.code.clone().unwrap_or_default(),
                row.message.clone(),
                row.path.clone().unwrap_or_default(),
                row.line.unwrap_or(0),
            )
        })
        .collect()
}

fn diag_changes(base: &KgData, head: &KgData) -> Vec<Change> {
    let (before, after) = (warnings(base), warnings(head));
    let key = |row: &(String, String, String, usize)| (row.0.clone(), row.1.clone(), row.2.clone());
    let mut out = Vec::new();
    let mut remaining: Vec<_> = before.iter().map(key).collect();
    for row in &after {
        if let Some(at) = remaining.iter().position(|other| *other == key(row)) {
            remaining.remove(at);
        } else {
            out.push(
                Change::new(Severity::Info, "new", "warning", &row.0)
                    .values(None, Some(format!("{}:{}: {}", row.2, row.3, row.1))),
            );
        }
    }
    let mut remaining: Vec<_> = after.iter().map(key).collect();
    for row in &before {
        if let Some(at) = remaining.iter().position(|other| *other == key(row)) {
            remaining.remove(at);
        } else {
            out.push(
                Change::new(Severity::Info, "fixed", "warning", &row.0)
                    .values(Some(format!("{}:{}: {}", row.2, row.3, row.1)), None),
            );
        }
    }
    out
}

fn changed_modules(base: &KgData, head: &KgData) -> BTreeSet<String> {
    let sources = |data: &KgData| -> BTreeMap<String, String> {
        data.meta
            .modules
            .iter()
            .map(|module| (module.name.clone(), module.source.clone()))
            .collect()
    };
    let (old, new) = (sources(base), sources(head));
    old.keys()
        .chain(new.keys())
        .filter(|name| old.get(*name) != new.get(*name))
        .cloned()
        .collect()
}

/// An instance, in a changed module, of a class that the project does not declare: operator
/// overloads call it without a call edge.
fn std_instance_in(data: &KgData, modules: &BTreeSet<String>) -> bool {
    let classes: BTreeMap<&str, &str> = data
        .edges()
        .iter()
        .filter(|edge| edge.rel == "instance_of")
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    data.nodes.iter().any(|node| {
        node.kind == "instance"
            && modules.contains(&node.module)
            && classes
                .get(node.id.as_str())
                .is_none_or(|class| data.node(class).is_none())
    })
}

/// The call graph of the head with the virtual edges of dispatch: a call of `C.m` also reaches
/// `I.m` for every instance `I` of `C`.
fn dispatch_graph(head: &KgData) -> Digraph {
    let index = &head.index;
    let mut instances: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in head.edges().iter().filter(|edge| edge.rel == "instance_of") {
        instances
            .entry(edge.to.as_str())
            .or_default()
            .push(&edge.from);
    }
    let mut pairs = Vec::new();
    for edge in head.edges().iter().filter(|edge| edge.rel == "calls") {
        let (Some(from), Some(to)) = (index.number(&edge.from), index.number(&edge.to)) else {
            continue;
        };
        pairs.push((from, to));
        for (class, members) in &instances {
            let Some(method) = edge.to.strip_prefix(&format!("{class}.")) else {
                continue;
            };
            if method.contains('.') {
                continue;
            }
            for instance in members {
                if let Some(target) = index.number(&format!("{instance}.{method}")) {
                    pairs.push((from, target));
                }
            }
        }
    }
    Digraph::from_edges(index.len(), pairs)
}

fn affected_tests(base: &KgData, head: &KgData) -> (Tests, BTreeMap<String, Vec<String>>) {
    let none = BTreeMap::new();
    if head.edges.is_none() {
        return (Tests::All("the KG has no graph".into()), none);
    }
    if (&base.meta.compiler, &base.meta.schema) != (&head.meta.compiler, &head.meta.schema) {
        return (Tests::All("the compiler changed".into()), none);
    }
    if base.meta.source_hash == head.meta.source_hash {
        return (Tests::Some(Vec::new()), none);
    }
    let modules = changed_modules(base, head);
    if modules.is_empty() {
        return (
            Tests::All("the manifest or a dependency changed".into()),
            none,
        );
    }
    if std_instance_in(base, &modules) || std_instance_in(head, &modules) {
        return (
            Tests::All("an instance of a std class changed".into()),
            none,
        );
    }
    let graph = dispatch_graph(head).reversed();
    let seeds: Vec<usize> = head
        .nodes
        .iter()
        .filter(|node| modules.contains(&node.module) && node.kind != "module")
        .filter_map(|node| head.index.number(&node.id))
        .collect();
    if algo::reaches_beyond(&graph, &seeds, MAX_DEPTH) {
        return (
            Tests::All(format!("callers deeper than {MAX_DEPTH} calls")),
            none,
        );
    }
    let visits = algo::bfs(&graph, &seeds, MAX_DEPTH, usize::MAX);
    let parent: BTreeMap<usize, Option<usize>> = visits
        .iter()
        .map(|visit| (visit.node, visit.parent))
        .collect();
    let mut tests: BTreeSet<String> = visits
        .iter()
        .map(|visit| head.index.name(visit.node))
        .filter(|id| id.starts_with("test:"))
        .map(str::to_owned)
        .collect();
    let removed: BTreeSet<&str> = base
        .nodes
        .iter()
        .filter(|node| head.node(&node.id).is_none())
        .map(|node| node.id.as_str())
        .collect();
    for edge in base.edges() {
        if edge.rel == "calls"
            && removed.contains(edge.to.as_str())
            && edge.from.starts_with("test:")
        {
            tests.insert(edge.from.clone());
        }
    }
    let tests: Vec<String> = tests
        .into_iter()
        .filter(|id| head.node(id).is_some())
        .collect();
    // The witness of each test: the chain from a changed node to it.
    let mut paths = BTreeMap::new();
    for test in &tests {
        let Some(mut at) = head.index.number(test) else {
            continue;
        };
        let mut chain = vec![test.clone()];
        if parent.contains_key(&at) {
            while let Some(Some(previous)) = parent.get(&at) {
                chain.push(head.index.name(*previous).to_owned());
                at = *previous;
            }
            chain.reverse();
        }
        paths.insert(test.clone(), chain);
    }
    (Tests::Some(tests), paths)
}

/// The impact of going from `base` to `head`.
pub fn diff(base: &KgData, head: &KgData, base_label: &str, head_label: &str) -> Impact {
    let mut changes = symbol_changes(base, head);
    changes.extend(fact_changes(base, head));
    changes.extend(diag_changes(base, head));
    changes.sort_by(|a, b| a.key().cmp(&b.key()));
    let (tests, paths) = affected_tests(base, head);
    Impact {
        base: base_label.to_owned(),
        head: head_label.to_owned(),
        changes,
        tests,
        total_tests: head.nodes.iter().filter(|node| node.kind == "test").count(),
        paths,
    }
}

impl Impact {
    /// (breaking, non-breaking, info).
    pub fn counts(&self) -> (usize, usize, usize) {
        let count = |severity| {
            self.changes
                .iter()
                .filter(|change| change.severity == severity)
                .count()
        };
        (
            count(Severity::Breaking),
            count(Severity::NonBreaking),
            count(Severity::Info),
        )
    }

    pub fn render_text(&self) -> String {
        let mut lines = vec![format!("impact: {} -> {}", self.base, self.head)];
        for change in self.changes.iter().take(MAX_LISTED_CHANGES) {
            lines.push(format!("{}: {}", change.severity.name(), change.message()));
        }
        if self.changes.len() > MAX_LISTED_CHANGES {
            lines.push(format!(
                "note: {} more changes not shown",
                self.changes.len() - MAX_LISTED_CHANGES
            ));
        }
        match &self.tests {
            Tests::All(reason) => lines.push(format!("tests: all {} ({reason})", self.total_tests)),
            Tests::Some(tests) => {
                lines.push(format!(
                    "tests: {} of {} affected",
                    tests.len(),
                    self.total_tests
                ));
                lines.extend(tests.iter().map(|test| format!("  {test}")));
            }
        }
        let (breaking, non_breaking, info) = self.counts();
        lines.push(format!(
            "summary: {breaking} breaking, {non_breaking} non-breaking, {info} info"
        ));
        lines.join("\n") + "\n"
    }

    pub fn render_json(&self) -> String {
        let mut rows = vec![
            Object::new()
                .string("type", "impact")
                .string("base", &self.base)
                .string("head", &self.head)
                .finish(),
        ];
        for change in self.changes.iter().take(MAX_LISTED_CHANGES) {
            rows.push(
                Object::new()
                    .string("type", "change")
                    .string("severity", change.severity.name())
                    .string("action", change.action)
                    .string("kind", &change.kind)
                    .string("name", &change.name)
                    .optional("old", change.old.as_deref())
                    .optional("new", change.new.as_deref())
                    .optional(
                        "note",
                        Some(change.note.as_str()).filter(|note| !note.is_empty()),
                    )
                    .finish(),
            );
        }
        let (mode, affected, reason) = match &self.tests {
            Tests::All(reason) => ("all", Vec::new(), Some(reason.as_str())),
            Tests::Some(tests) => ("some", tests.clone(), None),
        };
        let paths = super::json::array(self.paths.iter().map(|(test, via)| {
            Object::new()
                .string("test", test)
                .strings("via", via)
                .finish()
        }));
        rows.push(
            Object::new()
                .string("type", "tests")
                .string("mode", mode)
                .strings("affected", &affected)
                .uint("total", self.total_tests)
                .optional("reason", reason)
                .raw("paths", &paths)
                .finish(),
        );
        let (breaking, non_breaking, info) = self.counts();
        rows.push(
            Object::new()
                .string("type", "summary")
                .uint("breaking", breaking)
                .uint("non_breaking", non_breaking)
                .uint("info", info)
                .uint(
                    "omitted",
                    self.changes.len().saturating_sub(MAX_LISTED_CHANGES),
                )
                .finish(),
        );
        rows.join("\n") + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canon_renames_type_variables_in_order() {
        assert_eq!(
            canon("def f :: 'a -> 'b -> 'a"),
            "def f :: 't0 -> 't1 -> 't0"
        );
        assert_eq!(canon("def g ::  'x  ->  'x"), "def g :: 't0 -> 't0");
        assert_eq!(canon("def h :: 'a1 -> i64"), canon("def h :: 'zz -> i64"));
    }

    #[test]
    fn messages_follow_api_diff() {
        let change = Change::new(Severity::Breaking, "added", "field", "A.B.c")
            .values(None, Some("c: i64".into()))
            .note("constructions of the record without it stop compiling");
        assert_eq!(
            change.message(),
            "added field A.B.c: `c: i64` (constructions of the record without it stop compiling)"
        );
        let change = Change::new(Severity::Info, "new", "warning", "W1001")
            .values(None, Some("Cart.tz:22: unused".into()));
        assert_eq!(change.message(), "new warning W1001 at Cart.tz:22: unused");
        let change = Change::new(Severity::Breaking, "added", "effect", "A.f")
            .values(None, Some("io".into()))
            .note("reached by 1 function and 0 tests");
        assert_eq!(
            change.message(),
            "added effect io on A.f (reached by 1 function and 0 tests)"
        );
    }

    /// The tables of G28 appendix F-1 to F-3, on graphs written row by row.
    mod tables {
        use super::*;
        use crate::kg::manifest::{Manifest, ModuleEntry};
        use crate::kg::write::Files;
        use serde_json::{Value, json};

        #[derive(Clone)]
        struct Kg {
            nodes: Vec<Value>,
            edges: Option<Vec<Value>>,
            facts: Option<Vec<Value>>,
            compiler: &'static str,
            schema: &'static str,
            hash: &'static str,
            modules: Vec<(&'static str, &'static str)>,
        }

        impl Kg {
            fn new() -> Self {
                Self {
                    nodes: Vec::new(),
                    edges: Some(Vec::new()),
                    facts: Some(Vec::new()),
                    compiler: "tsuzuri 0.1.0",
                    schema: "tz-kg/1",
                    hash: "h",
                    modules: Vec::new(),
                }
            }

            /// A public node of module `P` (or the module before the ID's first `.`).
            fn node(self, id: &str, kind: &str, signature: &str) -> Self {
                let bare = id
                    .strip_prefix("test:")
                    .or_else(|| id.strip_prefix("instance:"))
                    .unwrap_or(id);
                let module = bare.split(['.', '<']).next().unwrap_or("P").to_owned();
                self.in_module(&module, id, kind, signature)
            }

            fn in_module(mut self, module: &str, id: &str, kind: &str, signature: &str) -> Self {
                let name = id.rsplit('.').next().unwrap_or(id);
                self.nodes.push(json!({
                    "id": id, "kind": kind, "module": module, "name": name,
                    "signature": signature, "params": [], "result": null,
                    "public": kind != "test", "export": false, "pure": false,
                    "doc": null, "deprecated": null, "origin": "project",
                    "span": {"path": "P.tz", "start": 0, "end": 0, "line": 1, "column": 1}
                }));
                self
            }

            fn set(mut self, id: &str, key: &str, value: Value) -> Self {
                let node = self.nodes.iter_mut().find(|node| node["id"] == id).unwrap();
                if key == "start" {
                    node["span"]["start"] = value;
                } else {
                    node[key] = value;
                }
                self
            }

            fn edge(mut self, rel: &str, from: &str, to: &str) -> Self {
                self.edges.get_or_insert_with(Vec::new).push(json!({
                    "rel": rel, "from": from, "to": to,
                    "span": {"path": "P.tz", "start": 0, "end": 0, "line": 1, "column": 1}
                }));
                self
            }

            fn fact(mut self, id: &str, fields: Value) -> Self {
                let mut row = json!({
                    "id": id, "effects": [], "trap_sites": [], "alloc_sites": [],
                    "copy_sites": [], "recursion": "none"
                });
                for (key, value) in fields.as_object().unwrap() {
                    row[key] = value.clone();
                }
                self.facts.get_or_insert_with(Vec::new).push(row);
                self
            }

            fn data(&self) -> KgData {
                let lines = |rows: &[Value]| -> Vec<u8> {
                    rows.iter()
                        .map(|row| format!("{row}\n"))
                        .collect::<String>()
                        .into_bytes()
                };
                let mut files = Files::default();
                files.put("nodes.jsonl", lines(&self.nodes));
                if let Some(edges) = &self.edges {
                    // The members hang off their parents, as the graph of a build has them.
                    let mut all = edges.clone();
                    for node in &self.nodes {
                        let id = node["id"].as_str().unwrap();
                        if MEMBERS.contains(&node["kind"].as_str().unwrap()) {
                            all.push(json!({"rel": "contains", "from": parent_id(id), "to": id}));
                        }
                    }
                    files.put("edges.jsonl", lines(&all));
                }
                if let Some(facts) = &self.facts {
                    files.put("facts/P.jsonl", lines(facts));
                }
                let manifest = Manifest {
                    schema: self.schema.into(),
                    compiler: self.compiler.into(),
                    edition: "2026".into(),
                    project: "p".into(),
                    version: None,
                    complete: self.facts.is_some(),
                    source_hash: self.hash.into(),
                    counts: Vec::new(),
                    modules: self
                        .modules
                        .iter()
                        .map(|(name, source)| ModuleEntry {
                            name: (*name).into(),
                            key: (*name).into(),
                            origin: "project".into(),
                            path: format!("{name}.tz"),
                            source: (*source).into(),
                        })
                        .collect(),
                    files: files
                        .0
                        .keys()
                        .map(|path| (path.clone(), String::new()))
                        .collect(),
                };
                KgData::from_parts(&manifest, &files).unwrap()
            }
        }

        fn changes(base: &Kg, head: &Kg) -> Vec<String> {
            diff(&base.data(), &head.data(), "base", "head")
                .changes
                .iter()
                .map(|change| format!("{}: {}", change.severity.name(), change.message()))
                .collect()
        }

        fn tests_of(base: &Kg, head: &Kg) -> Vec<String> {
            let text = diff(&base.data(), &head.data(), "base", "head").render_text();
            text.lines()
                .skip_while(|line| !line.starts_with("tests: "))
                .take_while(|line| !line.starts_with("summary: "))
                .map(str::to_owned)
                .collect()
        }

        fn record(kg: Kg) -> Kg {
            kg.node("P.R", "record", "record R")
                .node("P.R.a", "field", "a: i64")
                .set("P.R.a", "start", json!(10))
        }

        fn f(kg: Kg) -> Kg {
            kg.node("P.f", "def", "def f :: i64 -> i64")
        }

        #[test]
        fn each_row_of_the_symbols_table() {
            let empty = Kg::new();
            let rb = |kg: Kg| {
                kg.node("P.R.b", "field", "b: i64")
                    .set("P.R.b", "start", json!(20))
            };
            let union = |kg: Kg| {
                kg.node("P.U", "union", "union U")
                    .node("P.U.C", "case", "C(i64)")
                    .set("P.U.C", "start", json!(10))
            };
            let class = |kg: Kg| kg.node("P.K", "class", "class K<'a>");
            let method = |kg: Kg| kg.node("P.K.m", "method", "def m :: ref 'a -> i64");
            let private = |kg: Kg, id: &str| kg.set(id, "public", json!(false));
            let ri = "instance:K<P.R>";
            #[rustfmt::skip]
            let rows: Vec<(&str, Kg, Kg, Vec<&str>)> = vec![
                ("1", rb(record(empty.clone())), empty.clone(),
                 vec!["breaking: removed record P.R: `record R`"]),
                ("2a", rb(record(empty.clone())), record(empty.clone()),
                 vec!["breaking: removed field P.R.b: `b: i64` (uses of the field stop compiling)"]),
                ("2b", union(empty.clone()).node("P.U.D", "case", "D(i64)"), union(empty.clone()),
                 vec!["breaking: removed case P.U.D: `D(i64)` (uses of the case stop compiling)"]),
                ("2c", method(class(empty.clone())), class(empty.clone()),
                 vec!["breaking: removed method P.K.m: `def m :: ref 'a -> i64` (calls and instances of the method stop compiling)"]),
                ("2d", f(empty.clone()), empty.clone(),
                 vec!["breaking: removed def P.f: `def f :: i64 -> i64`"]),
                ("3a", private(empty.clone().node("P.g", "def", "def g :: i64 -> i64"), "P.g"), empty.clone(),
                 vec!["non-breaking: removed def P.g: `def g :: i64 -> i64`"]),
                ("3b", empty.clone().node("test:P.t", "test", "test \"t\""), empty.clone(),
                 vec!["non-breaking: removed test test:P.t: `test \"t\"`"]),
                ("4", empty.clone(), rb(record(empty.clone())),
                 vec!["non-breaking: added record P.R: `record R`"]),
                ("5a", record(empty.clone()), record(empty.clone()).node("P.R.c", "field", "c: i64"),
                 vec!["breaking: added field P.R.c: `c: i64` (constructions of the record without it stop compiling)"]),
                ("5b", union(empty.clone()), union(empty.clone()).node("P.U.D", "case", "D(i64)"),
                 vec!["breaking: added case P.U.D: `D(i64)` (exhaustive matches over the union stop compiling)"]),
                ("5c", class(empty.clone()), method(class(empty.clone())),
                 vec!["breaking: added method P.K.m: `def m :: ref 'a -> i64` (existing instances do not define the method)"]),
                ("6a", empty.clone(), f(empty.clone()),
                 vec!["non-breaking: added def P.f: `def f :: i64 -> i64`"]),
                ("6b", private(empty.clone().node("P.Q", "record", "record Q"), "P.Q"),
                 private(private(empty.clone().node("P.Q", "record", "record Q"), "P.Q").node("P.Q.z", "field", "z: i64"), "P.Q.z"),
                 vec!["non-breaking: added field P.Q.z: `z: i64`"]),
                ("6c", empty.clone().node(ri, "instance", "instance K<P.R>"),
                 empty.clone().node(ri, "instance", "instance K<P.R>").node("instance:K<P.R>.m", "method", "def m :: ref P.R -> i64"),
                 vec!["non-breaking: added method instance:K<P.R>.m: `def m :: ref P.R -> i64`"]),
                ("7a", f(empty.clone()), empty.clone().node("P.f", "def", "def f :: i64 -> i64 -> i64"),
                 vec!["breaking: changed def P.f: `def f :: i64 -> i64` => `def f :: i64 -> i64 -> i64`"]),
                ("7b", private(f(empty.clone()), "P.f"), private(empty.clone().node("P.f", "def", "def f :: i64 -> i64 -> i64"), "P.f"),
                 vec!["non-breaking: changed def P.f: `def f :: i64 -> i64` => `def f :: i64 -> i64 -> i64`"]),
                ("7c", empty.clone().node("P.f", "def", "def f :: 'a -> 'b -> 'a"),
                 empty.clone().node("P.f", "def", "def f :: 'x -> 'y -> 'x"), vec![]),
                ("8", f(empty.clone()).set("P.f", "params", json!([{"name": "a0", "type": "i64", "conv": "ref"}])),
                 f(empty.clone()).set("P.f", "params", json!([{"name": "a0", "type": "i64", "conv": "own"}])),
                 vec!["breaking: changed conv P.f: `ref` => `own` (ownership passing of an argument changed)"]),
                ("9a", private(f(empty.clone()), "P.f"), f(empty.clone()),
                 vec!["non-breaking: changed visibility P.f: `private` => `public`"]),
                ("9b", f(empty.clone()), private(f(empty.clone()), "P.f"),
                 vec!["breaking: changed visibility P.f: `public` => `private`"]),
                ("10a", f(empty.clone()).set("P.f", "export", json!(true)), f(empty.clone()),
                 vec!["breaking: changed export P.f: `true` => `false`"]),
                ("10b", f(empty.clone()), f(empty.clone()).set("P.f", "export", json!(true)),
                 vec!["non-breaking: changed export P.f: `false` => `true`"]),
                ("11", f(empty.clone()).set("P.f", "pure", json!(true)), f(empty.clone()),
                 vec!["breaking: changed pure P.f: `true` => `false`"]),
                ("12a", f(empty.clone()), f(empty.clone()).set("P.f", "deprecated", json!("use g")),
                 vec!["non-breaking: deprecated def P.f: use g"]),
                ("12b", f(empty.clone()), f(empty.clone()).set("P.f", "deprecated", json!("")),
                 vec!["non-breaking: deprecated def P.f"]),
                ("12c", f(empty.clone()).set("P.f", "deprecated", json!("use g")), f(empty.clone()),
                 vec!["non-breaking: undeprecated def P.f: use g"]),
                ("13", f(empty.clone()).set("P.f", "doc", json!("Old.")), f(empty.clone()).set("P.f", "doc", json!("New.")),
                 vec!["non-breaking: documented def P.f"]),
                ("14a", rb(record(empty.clone())),
                 rb(record(empty.clone())).set("P.R.a", "start", json!(20)).set("P.R.b", "start", json!(10)),
                 vec!["breaking: changed order P.R: `a, b` => `b, a` (the order of fields and cases is part of the layout and of derived comparisons)"]),
                ("14b",
                 private(private(private(empty.clone().node("P.V", "union", "union V").node("P.V.C", "case", "C").set("P.V.C", "start", json!(10))
                     .node("P.V.D", "case", "D").set("P.V.D", "start", json!(20)), "P.V"), "P.V.C"), "P.V.D"),
                 private(private(private(empty.clone().node("P.V", "union", "union V").node("P.V.C", "case", "C").set("P.V.C", "start", json!(20))
                     .node("P.V.D", "case", "D").set("P.V.D", "start", json!(10)), "P.V"), "P.V.C"), "P.V.D"),
                 vec!["non-breaking: changed order P.V: `C, D` => `D, C` (the order of fields and cases is part of the layout and of derived comparisons)"]),
                ("15", record(empty.clone()).edge("implements", "P.R", "Eq"), record(empty.clone()),
                 vec!["breaking: removed implements P.R.Eq"]),
                ("16", record(empty.clone()), record(empty.clone()).edge("implements", "P.R", "Eq"),
                 vec!["non-breaking: added implements P.R.Eq"]),
            ];
            for (row, base, head, expected) in rows {
                assert_eq!(changes(&base, &head), expected, "row {row}");
            }
        }

        fn functions() -> Kg {
            let kg = Kg::new()
                .node("P.f", "def", "def f :: i64 -> i64")
                .node("P.g", "def", "def g :: i64 -> i64")
                .node("P.h", "def", "def h :: i64 -> i64")
                .node("P.k", "def", "def k :: i64 -> i64")
                .set("P.k", "public", json!(false))
                .node("test:P.t", "test", "test \"t\"");
            ["P.f", "P.g", "P.h", "P.k", "test:P.t"]
                .into_iter()
                .fold(kg, |kg, id| kg.fact(id, json!({})))
        }

        fn calls(kg: Kg, pairs: &[(&str, &str)]) -> Kg {
            pairs
                .iter()
                .fold(kg, |kg, (from, to)| kg.edge("calls", from, to))
        }

        fn chain(kg: Kg) -> Kg {
            calls(kg, &[("P.g", "P.f"), ("P.h", "P.g"), ("test:P.t", "P.h")])
        }

        fn refact(mut kg: Kg, id: &str, fields: Value) -> Kg {
            let facts = kg.facts.as_mut().unwrap();
            facts.retain(|row| row["id"] != id);
            kg.fact(id, fields)
        }

        #[test]
        fn effects_added_to_a_public_function_are_breaking() {
            let io = json!({"effects": ["io"]});
            let unproven = json!({"kind": "bounds_check", "proven": false});
            let proven = json!({"kind": "bounds_check", "proven": true});
            let site = json!({"kind": "array"});
            let skipped = "info: skipped facts (the two KGs were built by different compilers or one has no facts)";
            #[rustfmt::skip]
            let rows: Vec<(&str, Kg, Kg, Vec<&str>)> = vec![
                ("X1", chain(functions()), refact(chain(functions()), "P.f", io.clone()),
                 vec!["breaking: added effect io on P.f (reached by 2 functions and 1 test)"]),
                ("X2", functions(), refact(functions(), "P.f", io.clone()),
                 vec!["breaking: added effect io on P.f"]),
                ("X3", calls(functions(), &[("P.g", "P.k")]), refact(calls(functions(), &[("P.g", "P.k")]), "P.k", io.clone()),
                 vec!["info: added effect io on P.k (reached by 1 function and 0 tests)"]),
                ("X4a", refact(functions(), "P.f", io.clone()), functions(),
                 vec!["non-breaking: removed effect io on P.f"]),
                ("X4b", refact(functions(), "P.k", io.clone()), functions(),
                 vec!["info: removed effect io on P.k"]),
                ("X5", refact(functions(), "P.f", json!({"trap_sites": [proven.clone()]})),
                 refact(functions(), "P.f", json!({"trap_sites": [proven, unproven]})),
                 vec!["info: increased trap_sites P.f: `0` => `1`"]),
                ("X6", refact(functions(), "P.f", json!({"alloc_sites": [site.clone(), site.clone()]})),
                 refact(functions(), "P.f", json!({"alloc_sites": [site.clone()], "copy_sites": [site.clone(), site.clone(), site]})),
                 vec!["info: decreased alloc_sites P.f: `2` => `1`", "info: increased copy_sites P.f: `0` => `3`"]),
                ("X7", functions(), refact(functions(), "P.f", json!({"recursion": "self"})),
                 vec!["info: changed recursion P.f: `none` => `self`"]),
                ("X8", functions(), refact(functions(), "test:P.t", io.clone()), vec![]),
                ("X9a", functions(), Kg { facts: None, ..functions() }, vec![skipped]),
                ("X9b", functions(), Kg { compiler: "tsuzuri 0.2.0", ..functions() }, vec![skipped]),
            ];
            for (row, base, head, expected) in rows {
                assert_eq!(changes(&base, &head), expected, "row {row}");
            }
        }

        #[test]
        fn tests_table_rows() {
            let base = Kg {
                modules: vec![("P", "1"), ("Q", "2")],
                ..Kg::new()
            }
            .node("P.f", "def", "def f :: i64 -> i64")
            .node("test:P.t", "test", "test \"t\"")
            .node("test:Q.u", "test", "test \"u\"")
            .edge("calls", "test:Q.u", "P.f");
            let with = |kg: &Kg, modules: Vec<(&'static str, &'static str)>| Kg {
                hash: "h2",
                modules,
                ..kg.clone()
            };
            #[rustfmt::skip]
            let rows: Vec<(&str, Kg, Vec<&str>)> = vec![
                ("T1", Kg { edges: None, hash: "h2", ..base.clone() }, vec!["tests: all 2 (the KG has no graph)"]),
                ("T2", Kg { compiler: "tsuzuri 0.2.0", hash: "h2", ..base.clone() }, vec!["tests: all 2 (the compiler changed)"]),
                ("T3", Kg { schema: "tz-kg/2", hash: "h2", ..base.clone() }, vec!["tests: all 2 (the compiler changed)"]),
                ("T4", base.clone(), vec!["tests: 0 of 2 affected"]),
                ("T5", with(&base, vec![("P", "1"), ("Q", "2")]), vec!["tests: all 2 (the manifest or a dependency changed)"]),
                ("T6", with(&base, vec![("P", "9"), ("Q", "2")]), vec!["tests: 2 of 2 affected", "  test:P.t", "  test:Q.u"]),
                ("T6b", with(&base, vec![("P", "1"), ("Q", "9")]), vec!["tests: 1 of 2 affected", "  test:Q.u"]),
            ];
            for (row, head, expected) in rows {
                assert_eq!(tests_of(&base, &head), expected, "row {row}");
            }
        }

        #[test]
        fn dispatch_reaches_instance_methods() {
            // T7: a call of the class method reaches the instance's method in the changed module.
            let base = Kg {
                modules: vec![("P", "1"), ("Q", "1"), ("T", "1")],
                ..Kg::new()
            }
            .node("P.K", "class", "class K<'a>")
            .node("P.K.m", "method", "def m :: ref 'a -> i64")
            .in_module("Q", "instance:K<Q.R>", "instance", "instance K<Q.R>")
            .in_module(
                "Q",
                "instance:K<Q.R>.m",
                "method",
                "def m :: ref Q.R -> i64",
            )
            .in_module("T", "test:T.t", "test", "test \"t\"")
            .edge("instance_of", "instance:K<Q.R>", "P.K")
            .edge("calls", "test:T.t", "P.K.m");
            let head = Kg {
                hash: "h2",
                modules: vec![("P", "1"), ("Q", "2"), ("T", "1")],
                ..base.clone()
            };
            assert_eq!(
                tests_of(&base, &head),
                ["tests: 1 of 1 affected", "  test:T.t"]
            );
            // T8: a test that called a removed declaration.
            let base = Kg {
                modules: vec![("P", "1"), ("Q", "1"), ("T", "1")],
                ..Kg::new()
            }
            .node("P.gone", "def", "def gone :: i64 -> i64")
            .node("Q.alt", "def", "def alt :: i64 -> i64")
            .in_module("T", "test:T.t", "test", "test \"t\"")
            .edge("calls", "test:T.t", "P.gone");
            let head = Kg {
                hash: "h2",
                modules: vec![("P", "2"), ("Q", "1"), ("T", "1")],
                ..Kg::new()
            }
            .node("Q.alt", "def", "def alt :: i64 -> i64")
            .in_module("T", "test:T.t", "test", "test \"t\"")
            .edge("calls", "test:T.t", "Q.alt");
            assert_eq!(
                tests_of(&base, &head),
                ["tests: 1 of 1 affected", "  test:T.t"]
            );
            // T9 to T11: the walk stops at depth 8.
            let ladder = |length: usize, called: usize, hash: &'static str| {
                let mut kg = Kg {
                    hash,
                    modules: vec![("P0", if hash == "h" { "1" } else { "2" })],
                    ..Kg::new()
                };
                for i in 0..length {
                    kg = kg.in_module(
                        &format!("P{i}"),
                        &format!("P{i}.a{i}"),
                        "def",
                        "def a :: i64",
                    );
                    if i > 0 {
                        kg = kg.edge(
                            "calls",
                            &format!("P{i}.a{i}"),
                            &format!("P{}.a{}", i - 1, i - 1),
                        );
                    }
                }
                kg.in_module("P9", "test:P9.t", "test", "test \"t\"").edge(
                    "calls",
                    "test:P9.t",
                    &format!("P{called}.a{called}"),
                )
            };
            for (row, length, called, expected) in [
                ("T9", 9, 7, vec!["tests: 1 of 1 affected", "  test:P9.t"]),
                (
                    "T10",
                    9,
                    8,
                    vec!["tests: all 1 (callers deeper than 8 calls)"],
                ),
                (
                    "T11",
                    10,
                    9,
                    vec!["tests: all 1 (callers deeper than 8 calls)"],
                ),
            ] {
                let (base, head) = (ladder(length, called, "h"), ladder(length, called, "h2"));
                assert_eq!(tests_of(&base, &head), expected, "row {row}");
            }
            // T12: an instance of a class that the project does not declare.
            let base = Kg {
                modules: vec![("Q", "1"), ("T", "1")],
                ..Kg::new()
            }
            .in_module("Q", "Q.R", "record", "record R")
            .in_module("Q", "instance:Eq<Q.R>", "instance", "instance Eq<Q.R>")
            .in_module(
                "Q",
                "instance:Eq<Q.R>.eq",
                "method",
                "def eq :: ref Q.R -> ref Q.R -> bool",
            )
            .in_module("T", "test:T.t", "test", "test \"t\"")
            .edge("instance_of", "instance:Eq<Q.R>", "Eq")
            .edge("uses_type", "test:T.t", "Q.R");
            let head = Kg {
                hash: "h2",
                modules: vec![("Q", "2"), ("T", "1")],
                ..base.clone()
            };
            assert_eq!(
                tests_of(&base, &head),
                ["tests: all 1 (an instance of a std class changed)"]
            );
        }
    }
}
