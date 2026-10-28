//! Trust boundaries and capabilities (E15): the capabilities that each package of a compiled
//! program uses, the `[capabilities]` policy of its manifests, and the `tsuzuri audit` report.
//!
//! A program reaches the world only through the private std builtins (users cannot call them,
//! `E1022`) and through `extern` functions. A std function's capabilities are those of the
//! builtins its body names and of the std functions it names, transitively; a user function
//! uses the capabilities of the std functions and the `ffi` of the `extern` functions that its
//! body names. The analysis reads the typed program after specialization and closure lifting,
//! never the source text, and runs after type checking as its own step: it changes neither the
//! analysis nor the generated code. A policy is checked only when the root manifest declares
//! one; it limits static uses and is not a run-time sandbox.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use crate::check::{CheckedModule, FunctionRef, ModuleOrigin, TypedExprKind};
use crate::diagnostic::{Diagnostic, DiagnosticKind, RelatedRole, Span, json_string};
use crate::package::PackageId;

/// The most functions and expression nodes that the analysis reads (`E1017`).
pub const MAX_FUNCTIONS: usize = 65_536;
pub const MAX_NODES: usize = 16_777_216;
/// The most sites of one capability that `audit --json` lists; the rest are counted in `truncated`.
pub const MAX_SITES_SHOWN: usize = 100;
/// The most sites of one capability that the text report lists.
pub const MAX_SITES_TEXT: usize = 5;
/// The most std calls that a site's chain to the host keeps.
const MAX_VIA: usize = 8;

/// A capability (D-57). The order of the names is the order of the bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    Env,
    Ffi,
    Fs,
    Gpu,
    Net,
    Process,
    Random,
    Stdio,
    Time,
}

impl Capability {
    pub const ALL: [Capability; 9] = [
        Self::Env,
        Self::Ffi,
        Self::Fs,
        Self::Gpu,
        Self::Net,
        Self::Process,
        Self::Random,
        Self::Stdio,
        Self::Time,
    ];

    /// The names in order, as error messages list them.
    pub const NAMES: &'static str = "env, ffi, fs, gpu, net, process, random, stdio, time";

    pub fn name(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Ffi => "ffi",
            Self::Fs => "fs",
            Self::Gpu => "gpu",
            Self::Net => "net",
            Self::Process => "process",
            Self::Random => "random",
            Self::Stdio => "stdio",
            Self::Time => "time",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.name() == name)
    }

    fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A set of capabilities, bit `i` for `Capability::ALL[i]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilitySet(u16);

impl CapabilitySet {
    pub const EMPTY: Self = Self(0);
    pub const ALL: Self = Self((1 << Capability::ALL.len()) - 1);

    pub fn single(capability: Capability) -> Self {
        Self(capability.bit())
    }

    pub fn contains(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    pub fn insert(&mut self, capability: Capability) {
        self.0 |= capability.bit();
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The capabilities of the set, in `Capability::ALL` order.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |capability| self.contains(*capability))
    }

    fn names(self) -> Vec<&'static str> {
        self.iter().map(Capability::name).collect()
    }
}

/// A package of the graph, for reports. `owner` is `None` for the root package, as in
/// `Project::dependency_package`.
#[derive(Clone, Debug)]
pub struct PackageInfo {
    pub owner: Option<PackageId>,
    pub name: String,
    pub version: Option<String>,
    pub root: bool,
}

/// `parent` (`None`: the root) lets `child` use `granted`: the dependency entry's
/// `capabilities`, empty when the entry has none.
#[derive(Clone, Debug)]
pub struct Edge {
    pub parent: Option<PackageId>,
    pub child: PackageId,
    pub granted: CapabilitySet,
}

/// The capabilities that the manifests of a project allow each package.
#[derive(Clone, Debug)]
pub struct Policy {
    /// The root manifest has `[capabilities]`, or one of its dependency entries has `capabilities`.
    pub enforced: bool,
    /// The root package first, then by name.
    pub packages: Vec<PackageInfo>,
    /// By child name, then parent name.
    pub edges: Vec<Edge>,
    /// The root is `None`. Every package has `ALL` when the policy is not enforced.
    pub effective: BTreeMap<Option<PackageId>, CapabilitySet>,
}

/// The first limit that leaves a capability out of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// The root's `allow` does not list it.
    RootAllow,
    /// The entry of `child` in the manifest of `parent` does not grant it.
    Edge { parent: String, child: String },
}

impl Policy {
    /// The policy of the packages that `driver::load_packages` read from the project `root`:
    /// `None` without a root manifest. Each package's effective capabilities are the
    /// intersection, over every edge into it, of its parent's and the edge's (D5).
    pub(crate) fn build(root: &Path, packages: &[crate::driver::LoadedPackage]) -> Option<Policy> {
        let root_package = packages.iter().find(|package| package.id.root == root)?;
        let manifest = &root_package.manifest;
        let enforced = manifest.capabilities.is_some()
            || manifest
                .dependencies
                .values()
                .any(|dependency| dependency.capabilities.is_some());
        let owner = |package: &crate::driver::LoadedPackage| {
            (package.id != root_package.id).then(|| package.id.clone())
        };
        let by_root: BTreeMap<&Path, &crate::driver::LoadedPackage> = packages
            .iter()
            .map(|package| (package.id.root.as_path(), package))
            .collect();
        let name_of = |owner: &Option<PackageId>| {
            owner
                .as_ref()
                .map_or(root_package.id.name.as_str(), |id| id.name.as_str())
                .to_owned()
        };
        let mut edges = Vec::new();
        for package in packages {
            for (name, child_root) in &package.dependency_roots {
                let Some(child) = by_root.get(child_root.as_path()) else {
                    continue;
                };
                let granted = package
                    .manifest
                    .dependencies
                    .get(name)
                    .and_then(|dependency| dependency.capabilities)
                    .unwrap_or(CapabilitySet::EMPTY);
                edges.push(Edge {
                    parent: owner(package),
                    child: child.id.clone(),
                    granted,
                });
            }
        }
        edges.sort_by(|left, right| {
            (&left.child.name, name_of(&left.parent), &left.child.root).cmp(&(
                &right.child.name,
                name_of(&right.parent),
                &right.child.root,
            ))
        });
        let mut effective = BTreeMap::new();
        let allow = if enforced {
            manifest
                .capabilities
                .map_or(CapabilitySet::ALL, |(allow, _)| allow)
        } else {
            CapabilitySet::ALL
        };
        effective.insert(None, allow);
        // Kahn's order: a package is settled once every parent that depends on it is.
        let mut incoming: BTreeMap<&PackageId, usize> = BTreeMap::new();
        for edge in &edges {
            *incoming.entry(&edge.child).or_default() += 1;
        }
        let mut ready = VecDeque::from([None::<PackageId>]);
        while let Some(parent) = ready.pop_front() {
            let parent_set = effective[&parent];
            for edge in edges.iter().filter(|edge| edge.parent == parent) {
                let candidate = if enforced {
                    parent_set.intersection(edge.granted)
                } else {
                    CapabilitySet::ALL
                };
                let child = Some(edge.child.clone());
                let set = effective
                    .get(&child)
                    .map_or(candidate, |set: &CapabilitySet| set.intersection(candidate));
                effective.insert(child.clone(), set);
                let left = incoming.get_mut(&edge.child).expect("counted above");
                *left -= 1;
                if *left == 0 {
                    ready.push_back(child);
                }
            }
        }
        let mut infos: Vec<PackageInfo> = packages
            .iter()
            .map(|package| PackageInfo {
                owner: owner(package),
                name: package.manifest.name.clone(),
                version: Some(package.manifest.version.clone()),
                root: package.id == root_package.id,
            })
            .collect();
        infos.sort_by(|left, right| {
            (!left.root, &left.name, &left.owner).cmp(&(!right.root, &right.name, &right.owner))
        });
        Some(Policy {
            enforced,
            packages: infos,
            edges,
            effective,
        })
    }

    /// The capabilities that `package` may use.
    pub fn allowed(&self, package: &Option<PackageId>) -> CapabilitySet {
        if !self.enforced {
            return CapabilitySet::ALL;
        }
        self.effective
            .get(package)
            .copied()
            .unwrap_or(CapabilitySet::EMPTY)
    }

    /// The first limit that leaves `capability` out of `package`: the root's allow, else the
    /// first edge into the package (by parent name) that does not grant it, or the limit of the
    /// first parent that does not have it.
    pub fn blocker(&self, package: &Option<PackageId>, capability: Capability) -> Option<Blocker> {
        self.blocker_within(package, capability, self.packages.len())
    }

    fn blocker_within(
        &self,
        package: &Option<PackageId>,
        capability: Capability,
        depth: usize,
    ) -> Option<Blocker> {
        if !self.allowed(&None).contains(capability) {
            return Some(Blocker::RootAllow);
        }
        let child = package.as_ref()?;
        for edge in self.edges.iter().filter(|edge| edge.child == *child) {
            if !edge.granted.contains(capability) {
                return Some(Blocker::Edge {
                    parent: self.name(&edge.parent),
                    child: child.name.clone(),
                });
            }
            if !self.allowed(&edge.parent).contains(capability) && depth > 0 {
                return self.blocker_within(&edge.parent, capability, depth - 1);
            }
        }
        None
    }

    /// The manifest name of a package; the root's for `None`.
    pub fn name(&self, package: &Option<PackageId>) -> String {
        match package {
            Some(id) => id.name.clone(),
            None => self
                .packages
                .iter()
                .find(|info| info.root)
                .map_or_else(String::new, |info| info.name.clone()),
        }
    }
}

/// A source file, by the index that `Span::source` uses (the order of `Project::sources`).
#[derive(Clone, Debug)]
pub struct SourceInfo {
    /// The module name: the file stem, such as `File` or `Main`.
    pub name: String,
    pub std: bool,
    /// The dependency package of the source; `None` for the root package and std.
    pub package: Option<PackageId>,
    /// The path from the package root, `/`-separated.
    pub path: String,
}

/// Whether a use belongs to the program or only to its tests and benchmarks (D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Program,
    Test,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Test => "test",
        }
    }
}

/// One std call on the way from a used function to the host.
#[derive(Clone, Debug)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub span: Span,
}

/// A reference in a package's own code to something that uses a capability.
#[derive(Clone, Debug)]
pub struct Site {
    pub capability: Capability,
    pub span: Span,
    /// The std function (`File.read_text`), extern function or builtin that the code names.
    pub callee: String,
    /// The declared function that holds the reference (a generated helper's family root).
    pub function: String,
    pub scope: Scope,
    /// The std calls from `callee` to the function that names the host builtin (at most 8).
    pub via: Vec<Link>,
    /// The source of the capability: the builtin reference or the extern declaration.
    pub source_span: Option<Span>,
    /// The builtin that reaches the host; `None` for `ffi` and for an explicit capability.
    pub primitive: Option<&'static str>,
}

/// An extern function of the program and the references to it.
#[derive(Clone, Debug)]
pub struct ExternUse {
    pub function: usize,
    pub name: String,
    pub symbol: String,
    pub package: Option<PackageId>,
    /// The name of the declaration.
    pub span: Span,
    /// `None` without `@trusted`; else the bits of its properties (`syntax::TRUSTED_PROPERTIES`).
    pub trusted: Option<u8>,
    /// The references from functions that are not extern, by source and start, without repeats.
    pub uses: Vec<Span>,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// By package, source, start and capability; one per source, start, end and capability.
    pub sites: Vec<(Option<PackageId>, Site)>,
    pub externs: Vec<ExternUse>,
    /// References to `Os.__read` or `Os.__write` from a std file other than `File`, `Dir` and
    /// `Env`, whose capability the table does not know (appendix A.2; a test pins it at 0).
    pub multiplexed_outside: usize,
    /// The capabilities of each public std function of the program that has any, by declared
    /// name (`File.read_text`), over all of its instances.
    pub std_functions: BTreeMap<String, CapabilitySet>,
    /// The functions that each package declares with a body, and their effect promises.
    pub declared: BTreeMap<Option<PackageId>, Declared>,
}

/// How many functions a package declares with a body (externs, tests and benchmarks aside), and
/// how many of them are `pure def` or carry each effect attribute (A19, A20).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Declared {
    pub functions: usize,
    pub pure: usize,
    /// In `syntax::EffectKind::ALL` order: `no_alloc`, `no_copy`, `no_trap`, `terminates`.
    pub asserted: [usize; 4],
}

/// The capability that a reference to the builtin `name` from the std file `file` stands for.
/// `Os.__read` and `Os.__write` multiplex several operations, so their file decides.
pub fn primitive(name: &str, file: &str) -> Option<Capability> {
    use Capability::*;
    Some(match name {
        "Debug.__print_string" | "IO.__read_line" | "IO.__write" => Stdio,
        "Os.__read" => match file {
            "File" | "Dir" => Fs,
            "Env" => Env,
            _ => return None,
        },
        "Os.__write" => match file {
            "File" | "Dir" => Fs,
            _ => return None,
        },
        "Os.__open" | "Os.__handle" | "Os.__close" => Fs,
        "Os.__args" => Env,
        "Os.__random" => Random,
        "Os.__clock" | "Os.__sleep" | "Async.__clock" | "Async.__wait" | "Async.__posted"
        | "Bench.now" => Time,
        "Os.__spawn" => Process,
        "Net.__resolve" | "Net.__open" | "Net.__accept" | "Net.__read" | "Net.__write"
        | "Net.__close" | "Net.__watch" | "Net.__unwatch" | "Net.__connect" | "Net.__names"
        | "Net.__send" => Net,
        "Gpu.__open" | "Gpu.__features" | "Gpu.__run" | "Gpu.__select" | "Gpu.__last" => Gpu,
        _ => return None,
    })
}

/// The private builtins that touch only the CPU and memory (appendix A.2 and the ones added since).
pub const PRIVATE_WITHOUT_CAPABILITY: [&str; 20] = [
    "Net.__classify",
    "Unicode.__table_length",
    "Unicode.__table_entry",
    "Array.__set",
    "Array.__sort_with",
    "Vec.__view",
    "Vec.__set_capacity",
    "Vec.__insert",
    "Vec.__splice",
    "Arena.__next_id",
    "Async.__resume",
    "Async.__take",
    "Async.__put",
    "Async.__next_id",
    "Async.__retire",
    "Channel.__close_sender",
    "Channel.__close_receiver",
    "Bench.consume",
    "Alloc.__counter",
    "Gen.__seed",
];

fn multiplexed(name: &str) -> bool {
    matches!(name, "Os.__read" | "Os.__write")
}

/// The std functions whose capability comes from intent, not from their body (appendix A.3):
/// only a program that names another backend reroutes them to the private `Gpu` builtins.
fn explicit(declared: &str) -> Option<Capability> {
    matches!(
        declared,
        "Gpu.request" | "Gpu.init" | "Gpu.map" | "Gpu.init_relaxed" | "Gpu.map_relaxed"
    )
    .then_some(Capability::Gpu)
}

/// The declared name of a function: an instance `f.$mono.3` is `f`.
fn declared(qualified: &str) -> &str {
    qualified.split(".$mono.").next().unwrap_or(qualified)
}

/// A declaration's name across its instances: `Main.f.$mono.2` is `Main.f`.
fn declared_name_key(function: &crate::check::CheckedFunction) -> String {
    declared(&function.qualified_name()).to_owned()
}

/// How the capability of a std function was first found.
#[derive(Clone, Copy)]
enum Step {
    Direct(Span, &'static str),
    Via(usize, Span),
    Explicit,
}

pub fn analyze(module: &CheckedModule, sources: &[SourceInfo]) -> Result<Analysis, Diagnostic> {
    analyze_with_limits(module, sources, MAX_FUNCTIONS, MAX_NODES)
}

/// `analyze` with other limits, for tests of the limits.
pub fn analyze_with_limits(
    module: &CheckedModule,
    sources: &[SourceInfo],
    max_functions: usize,
    max_nodes: usize,
) -> Result<Analysis, Diagnostic> {
    let functions = &module.functions;
    let count = functions.len();
    if count > max_functions {
        return Err(Diagnostic::of(
            DiagnosticKind::CapabilityAnalysisLimit,
            format!(
                "capability analysis supports at most {max_functions} functions; split the program"
            ),
            Span::default(),
        ));
    }
    // The family root of each function: an instance is its own root, a lifted lambda or a
    // builtin wrapper belongs to the function that holds it.
    let roots: Vec<usize> = (0..count)
        .map(|id| {
            let mut current = id;
            let mut seen = BTreeSet::from([id]);
            while let Some(parent) = functions[current].origin.parent {
                if !seen.insert(parent) {
                    break;
                }
                current = parent;
            }
            current
        })
        .collect();
    let source_of = |id: usize| {
        functions[roots[id]]
            .span
            .source
            .and_then(|source| sources.get(source))
    };
    let is_std = |id: usize| functions[id].origin.module == ModuleOrigin::Std;
    let is_extern = |id: usize| matches!(functions[id].body.kind, TypedExprKind::HostCall(..));
    let shown = |id: usize| {
        let function = if functions[id].module.starts_with('$') {
            &functions[roots[id]]
        } else {
            &functions[id]
        };
        declared(&function.qualified_name()).to_owned()
    };
    let mut edges: Vec<Vec<(usize, Span)>> = vec![Vec::new(); count];
    let mut builtins: Vec<Vec<(&'static str, Span)>> = vec![Vec::new(); count];
    let mut nodes = 0usize;
    for (id, function) in functions.iter().enumerate() {
        let mut stack = vec![&function.body];
        while let Some(expression) = stack.pop() {
            nodes += 1;
            if nodes > max_nodes {
                return Err(Diagnostic::of(
                    DiagnosticKind::CapabilityAnalysisLimit,
                    format!(
                        "capability analysis supports at most {max_nodes} expression nodes; split the program"
                    ),
                    Span::default(),
                ));
            }
            match &expression.kind {
                TypedExprKind::Function(FunctionRef::User(target))
                | TypedExprKind::GenericFunction(target, _)
                | TypedExprKind::Closure(target, _) => edges[id].push((*target, expression.span)),
                TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                    builtins[id].push((instance.builtin.name(), expression.span))
                }
                _ => {}
            }
            stack.extend(expression.children());
        }
    }
    // A builtin used as a value becomes a `$builtin` wrapper that every function using the same
    // instance shares, so a reference to one counts as a reference to its builtin from the
    // function that makes it (whose file decides `Os.__read` and `Os.__write`).
    let wrapper = |id: usize| functions[id].module == "$builtin";
    let mut direct: Vec<Vec<(Capability, Span, &'static str)>> = vec![Vec::new(); count];
    let mut multiplexed_outside = 0;
    for id in (0..count).filter(|&id| !wrapper(id)) {
        let file = source_of(id).map_or("", |source| source.name.as_str());
        let named = builtins[id].iter().copied().chain(
            edges[id]
                .iter()
                .filter(|(target, _)| wrapper(*target))
                .flat_map(|&(target, span)| {
                    builtins[target].iter().map(move |&(name, _)| (name, span))
                }),
        );
        for (name, span) in named {
            match primitive(name, file) {
                Some(capability) => direct[id].push((capability, span, name)),
                None if multiplexed(name) => multiplexed_outside += 1,
                None => {}
            }
        }
    }
    for list in &mut edges {
        list.retain(|(target, _)| !wrapper(*target));
    }
    // The capabilities of the std functions: a fixed point over the references between them.
    let mut caps = vec![CapabilitySet::EMPTY; count];
    let mut first: Vec<BTreeMap<Capability, Step>> = vec![BTreeMap::new(); count];
    for id in (0..count).filter(|&id| is_std(id)) {
        for &(capability, span, name) in &direct[id] {
            caps[id].insert(capability);
            first[id]
                .entry(capability)
                .or_insert(Step::Direct(span, name));
        }
        if let Some(capability) = explicit(declared(&functions[id].qualified_name())) {
            caps[id].insert(capability);
            first[id].entry(capability).or_insert(Step::Explicit);
        }
    }
    loop {
        let mut changed = false;
        for id in (0..count).filter(|&id| is_std(id)) {
            for &(target, span) in &edges[id] {
                if !is_std(target) {
                    continue;
                }
                let added = caps[target].difference(caps[id]);
                for capability in added.iter() {
                    first[id].insert(capability, Step::Via(target, span));
                }
                if !added.is_empty() {
                    caps[id] = caps[id].union(added);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let chain = |start: usize, capability: Capability| {
        let mut via = Vec::new();
        let mut current = start;
        for _ in 0..count {
            match first[current].get(&capability) {
                Some(Step::Via(next, span)) => {
                    // A lifted lambda or a builtin wrapper shows as the function that holds it.
                    let (from, to) = (shown(current), shown(*next));
                    if from != to && via.len() < MAX_VIA {
                        via.push(Link {
                            from,
                            to,
                            span: *span,
                        });
                    }
                    current = *next;
                }
                Some(Step::Direct(span, name)) => return (via, Some(*span), Some(*name)),
                Some(Step::Explicit) | None => break,
            }
        }
        (via, None, None)
    };
    // The uses of each function outside std: its references to std, extern and builtins.
    let mut sites: BTreeMap<(Option<PackageId>, usize, usize, usize, Capability), Site> =
        BTreeMap::new();
    let mut externs: BTreeMap<usize, BTreeSet<(usize, usize, usize)>> = BTreeMap::new();
    for id in (0..count).filter(|&id| !is_std(id) && !is_extern(id)) {
        let Some(source) = source_of(id) else {
            continue;
        };
        let root = &functions[roots[id]];
        let scope = if root.origin.test.is_some() || root.origin.bench.is_some() {
            Scope::Test
        } else {
            Scope::Program
        };
        let function = declared(&root.qualified_name()).to_owned();
        let mut add = |site: Site| {
            let key = (
                source.package.clone(),
                site.span.source.unwrap_or(usize::MAX),
                site.span.start,
                site.span.end,
                site.capability,
            );
            sites.entry(key).or_insert(site);
        };
        for &(target, span) in &edges[id] {
            if is_extern(target) {
                externs.entry(target).or_default().insert((
                    span.source.unwrap_or(usize::MAX),
                    span.start,
                    span.end,
                ));
                add(Site {
                    capability: Capability::Ffi,
                    span,
                    callee: functions[target].name.clone(),
                    function: function.clone(),
                    scope,
                    via: Vec::new(),
                    source_span: Some(functions[target].span),
                    primitive: None,
                });
            } else if is_std(target) {
                for capability in caps[target].iter() {
                    let (via, source_span, primitive) = chain(target, capability);
                    add(Site {
                        capability,
                        span,
                        callee: shown(target),
                        function: function.clone(),
                        scope,
                        via,
                        source_span,
                        primitive,
                    });
                }
            }
        }
        for &(capability, span, name) in &direct[id] {
            add(Site {
                capability,
                span,
                callee: name.to_owned(),
                function: function.clone(),
                scope,
                via: Vec::new(),
                source_span: Some(span),
                primitive: Some(name),
            });
        }
    }
    let mut sites: Vec<(Option<PackageId>, Site)> = sites
        .into_iter()
        .map(|((package, ..), site)| (package, site))
        .collect();
    sites.sort_by(|(left_package, left), (right_package, right)| {
        (
            left_package,
            left.span.source,
            left.span.start,
            left.capability,
        )
            .cmp(&(
                right_package,
                right.span.source,
                right.span.start,
                right.capability,
            ))
    });
    let mut extern_uses = Vec::new();
    for (id, function) in functions.iter().enumerate() {
        let TypedExprKind::HostCall(import, _) = &function.body.kind else {
            continue;
        };
        let uses = externs
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .map(|(source, start, end)| Span {
                start,
                end,
                source: (source != usize::MAX).then_some(source),
            })
            .collect();
        extern_uses.push(ExternUse {
            function: id,
            name: function.name.clone(),
            symbol: import.native_symbol.clone(),
            package: source_of(id).and_then(|source| source.package.clone()),
            span: function.span,
            trusted: function.origin.trusted,
            uses,
        });
    }
    extern_uses.sort_by(|left, right| {
        (&left.package, left.span.source, left.span.start).cmp(&(
            &right.package,
            right.span.source,
            right.span.start,
        ))
    });
    let mut std_functions: BTreeMap<String, CapabilitySet> = BTreeMap::new();
    for (id, function) in functions.iter().enumerate() {
        if is_std(id)
            && !caps[id].is_empty()
            && !function.module.starts_with('$')
            && function.visibility == crate::syntax::Visibility::Public
        {
            let entry = std_functions
                .entry(declared(&function.qualified_name()).to_owned())
                .or_default();
            *entry = entry.union(caps[id]);
        }
    }
    let mut seen = BTreeSet::new();
    let mut declared: BTreeMap<Option<PackageId>, Declared> = BTreeMap::new();
    for (id, function) in functions.iter().enumerate() {
        let origin = &function.origin;
        if is_std(id)
            || is_extern(id)
            || function.module.starts_with('$')
            || origin.provenance != crate::syntax::Provenance::User
            || origin.test.is_some()
            || origin.bench.is_some()
        {
            continue;
        }
        let Some(source) = source_of(id) else {
            continue;
        };
        if !seen.insert(declared_name_key(function)) {
            continue;
        }
        let counts = declared.entry(source.package.clone()).or_default();
        counts.functions += 1;
        counts.pure += usize::from(origin.pure);
        for (index, kind) in crate::syntax::EffectKind::ALL.into_iter().enumerate() {
            counts.asserted[index] += usize::from(origin.asserts.contains(kind));
        }
    }
    Ok(Analysis {
        sites,
        externs: extern_uses,
        multiplexed_outside,
        std_functions,
        declared,
    })
}

/// Where a diagnostic sorts: its source, its start and its capability.
type Position = (Option<usize>, usize, Capability);

/// `E1036` for each use of the program (not of its tests) that the policy does not allow, by
/// source, start and capability.
pub fn denied(analysis: &Analysis, policy: &Policy) -> Vec<Diagnostic> {
    let mut found: Vec<(Position, Diagnostic)> = Vec::new();
    for (package, site) in &analysis.sites {
        if site.scope != Scope::Program || policy.allowed(package).contains(site.capability) {
            continue;
        }
        let capability = site.capability.name();
        let callee = &site.callee;
        let (message, help) = match (package, policy.blocker(package, site.capability)) {
            (None, _) => (
                format!(
                    "the root package may not use capability '{capability}' ('{callee}'); add \"{capability}\" to allow in [capabilities]"
                ),
                format!("add \"{capability}\" to allow in [capabilities]"),
            ),
            (Some(id), Some(Blocker::Edge { parent, child })) => (
                format!(
                    "package '{}' may not use capability '{capability}' ('{callee}'); grant it with capabilities = [\"{capability}\"] in the entry of '{child}' in '{parent}'",
                    id.name
                ),
                format!(
                    "grant \"{capability}\" to package '{}' in Tsuzuri.toml",
                    id.name
                ),
            ),
            (Some(id), _) => (
                format!(
                    "package '{}' may not use capability '{capability}' ('{callee}'); the root [capabilities] allow does not list \"{capability}\"",
                    id.name
                ),
                format!(
                    "grant \"{capability}\" to package '{}' in Tsuzuri.toml",
                    id.name
                ),
            ),
        };
        let mut diagnostic =
            Diagnostic::of(DiagnosticKind::CapabilityDenied, message, site.span).with_help(help);
        for link in &site.via {
            diagnostic = diagnostic.related(
                RelatedRole::ViaCall,
                format!("'{}' calls '{}'", link.from, link.to),
                link.span,
            );
        }
        match (site.source_span, site.primitive) {
            (Some(span), _) if site.capability == Capability::Ffi => {
                diagnostic = diagnostic.related(
                    RelatedRole::DeclaredHere,
                    "extern function declared here",
                    span,
                );
            }
            (Some(span), Some(primitive)) => {
                let last = site
                    .via
                    .last()
                    .map_or(callee.as_str(), |link| link.to.as_str());
                diagnostic = diagnostic.related(
                    RelatedRole::Note,
                    format!("'{last}' reaches the host through '{primitive}'"),
                    span,
                );
            }
            _ => {}
        }
        found.push((
            (site.span.source, site.span.start, site.capability),
            diagnostic,
        ));
    }
    found.sort_by_key(|(position, _)| *position);
    found
        .into_iter()
        .map(|(_, diagnostic)| diagnostic)
        .collect()
}

/// `W1010` at each extern function without `@trusted` that a function which is not extern
/// names; `root_only` keeps those that the root package declares.
pub fn untrusted_externs(analysis: &Analysis, root_only: bool) -> Vec<Diagnostic> {
    analysis
        .externs
        .iter()
        .filter(|extern_use| {
            extern_use.trusted.is_none()
                && !extern_use.uses.is_empty()
                && (!root_only || extern_use.package.is_none())
        })
        .map(|extern_use| {
            Diagnostic::of(
                DiagnosticKind::UntrustedExtern,
                format!(
                    "extern function '{}' is used but not marked '@trusted'; review its contract and add '@trusted' above its declaration",
                    extern_use.name
                ),
                extern_use.span,
            )
            .related(RelatedRole::Note, "first used here", extern_use.uses[0])
            .with_help("add '@trusted' above the extern declaration once its contract is checked")
        })
        .collect()
}

/// A site as `tsuzuri audit` prints it: a path from its package root and a 1-based position.
#[derive(Clone, Debug)]
pub struct ReportSite {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub callee: String,
    pub function: String,
    pub scope: Scope,
}

#[derive(Clone, Debug)]
pub struct UsedCapability {
    pub capability: Capability,
    pub count: usize,
    /// At most `MAX_SITES_SHOWN`, in source order; `truncated` counts the rest.
    pub sites: Vec<ReportSite>,
    pub truncated: usize,
}

#[derive(Clone, Debug)]
pub struct PackageReport {
    pub info: PackageInfo,
    pub allowed: CapabilitySet,
    /// The parents' grants, by parent name; only for a dependency under an enforced policy.
    pub granted: Vec<(String, CapabilitySet)>,
    pub used: Vec<UsedCapability>,
    /// What the program (not its tests) uses beyond `allowed`.
    pub denied: CapabilitySet,
    /// What `allowed` grants and the program does not use; empty without an enforced policy.
    pub unused: CapabilitySet,
    /// The package's declared functions and their effect promises (E15 Phase 4).
    pub effects: Declared,
}

#[derive(Clone, Debug)]
pub struct ExternReport {
    pub package: String,
    pub name: String,
    pub symbol: String,
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub trusted: Option<u8>,
    pub used: usize,
}

/// The `tsuzuri audit` report (E15): per package, the capabilities used, allowed, denied and
/// unused, and the trust of each extern function. It names no absolute path, time or variable.
#[derive(Clone, Debug)]
pub struct Report {
    pub policy_enforced: bool,
    pub root: String,
    pub version: Option<String>,
    pub packages: Vec<PackageReport>,
    pub externs: Vec<ExternReport>,
    /// The uses that `E1036` reports.
    pub denied: usize,
    /// The extern functions that `W1010` reports.
    pub untrusted_externs: usize,
}

impl Report {
    /// The report of `analysis`. `root` names the root package when there is no policy (no root
    /// manifest); `texts` are the sources' texts, in the order of `sources`.
    pub fn new(
        policy: Option<&Policy>,
        root: &str,
        analysis: &Analysis,
        sources: &[SourceInfo],
        texts: &[&str],
    ) -> Report {
        let enforced = policy.is_some_and(|policy| policy.enforced);
        let infos = match policy {
            Some(policy) => policy.packages.clone(),
            None => vec![PackageInfo {
                owner: None,
                name: root.to_owned(),
                version: None,
                root: true,
            }],
        };
        let place = |span: Span| {
            let source = span.source.and_then(|source| sources.get(source));
            let text = span.source.and_then(|source| texts.get(source)).copied();
            let (line, column) = text.map_or((0, 0), |text| {
                crate::diagnostic::location(text, span.start.min(text.len()))
            });
            (
                source.map_or_else(String::new, |source| source.path.clone()),
                line,
                column,
            )
        };
        let mut packages = Vec::new();
        let mut denied_sites = 0;
        for info in infos {
            let allowed = policy.map_or(CapabilitySet::ALL, |policy| policy.allowed(&info.owner));
            let granted = match (policy, &info.owner) {
                (Some(policy), Some(owner)) if enforced => policy
                    .edges
                    .iter()
                    .filter(|edge| edge.child == *owner)
                    .map(|edge| (policy.name(&edge.parent), edge.granted))
                    .collect(),
                _ => Vec::new(),
            };
            let mut by_capability: BTreeMap<Capability, Vec<&Site>> = BTreeMap::new();
            let mut program = CapabilitySet::EMPTY;
            for (package, site) in &analysis.sites {
                if *package != info.owner {
                    continue;
                }
                by_capability.entry(site.capability).or_default().push(site);
                if site.scope == Scope::Program {
                    program.insert(site.capability);
                    if enforced && !allowed.contains(site.capability) {
                        denied_sites += 1;
                    }
                }
            }
            let used = by_capability
                .into_iter()
                .map(|(capability, sites)| UsedCapability {
                    capability,
                    count: sites.len(),
                    truncated: sites.len().saturating_sub(MAX_SITES_SHOWN),
                    sites: sites
                        .into_iter()
                        .take(MAX_SITES_SHOWN)
                        .map(|site| {
                            let (path, line, column) = place(site.span);
                            ReportSite {
                                path,
                                line,
                                column,
                                callee: site.callee.clone(),
                                function: site.function.clone(),
                                scope: site.scope,
                            }
                        })
                        .collect(),
                })
                .collect();
            let denied = if enforced {
                program.difference(allowed)
            } else {
                CapabilitySet::EMPTY
            };
            let unused = if enforced {
                allowed.difference(program)
            } else {
                CapabilitySet::EMPTY
            };
            let effects = analysis
                .declared
                .get(&info.owner)
                .copied()
                .unwrap_or_default();
            packages.push(PackageReport {
                info,
                allowed,
                granted,
                used,
                denied,
                unused,
                effects,
            });
        }
        let name_of = |owner: &Option<PackageId>| match (owner, policy) {
            (Some(id), _) => id.name.clone(),
            (None, Some(policy)) => policy.name(&None),
            (None, None) => root.to_owned(),
        };
        let externs = analysis
            .externs
            .iter()
            .map(|extern_use| {
                let (path, line, column) = place(extern_use.span);
                ExternReport {
                    package: name_of(&extern_use.package),
                    name: extern_use.name.clone(),
                    symbol: extern_use.symbol.clone(),
                    path,
                    line,
                    column,
                    trusted: extern_use.trusted,
                    used: extern_use.uses.len(),
                }
            })
            .collect();
        let untrusted_externs = analysis
            .externs
            .iter()
            .filter(|extern_use| extern_use.trusted.is_none() && !extern_use.uses.is_empty())
            .count();
        let (root, version) = packages
            .first()
            .map(|package| (package.info.name.clone(), package.info.version.clone()))
            .unwrap_or_else(|| (root.to_owned(), None));
        Report {
            policy_enforced: enforced,
            root,
            version,
            packages,
            externs,
            denied: denied_sites,
            untrusted_externs,
        }
    }

    /// The exit code of `tsuzuri audit`: 1 for a denied use, or for an untrusted extern
    /// function under `--deny-warnings`.
    pub fn exit_code(&self, deny_warnings: bool) -> i32 {
        i32::from(self.denied > 0 || (deny_warnings && self.untrusted_externs > 0))
    }

    fn used(&self) -> CapabilitySet {
        let mut used = CapabilitySet::EMPTY;
        for package in &self.packages {
            for capability in &package.used {
                used.insert(capability.capability);
            }
        }
        used
    }

    pub fn render_text(&self) -> String {
        let listed = |set: CapabilitySet| {
            if set == CapabilitySet::ALL {
                "all".to_owned()
            } else {
                words(set)
            }
        };
        let mut out = String::new();
        out.push_str(&format!(
            "audit: {}\n",
            versioned(&self.root, &self.version)
        ));
        out.push_str(&format!(
            "policy: {}\npackages: {}\n\n",
            policy_name(self.policy_enforced),
            self.packages.len()
        ));
        for package in &self.packages {
            let info = &package.info;
            out.push_str(&format!(
                "package {} ({})\n",
                versioned(&info.name, &info.version),
                if info.root { "root" } else { "dependency" }
            ));
            out.push_str(&format!("  allowed: {}\n", listed(package.allowed)));
            if !package.granted.is_empty() {
                let grants: Vec<String> = package
                    .granted
                    .iter()
                    .map(|(parent, set)| format!("{parent}: {}", words(*set)))
                    .collect();
                out.push_str(&format!("  granted: {}\n", grants.join("; ")));
            }
            let mut used = CapabilitySet::EMPTY;
            for capability in &package.used {
                used.insert(capability.capability);
            }
            out.push_str(&format!("  used: {}\n", words(used)));
            for capability in &package.used {
                out.push_str(&format!(
                    "    {}: {} {}\n",
                    capability.capability.name(),
                    capability.count,
                    if capability.count == 1 {
                        "site"
                    } else {
                        "sites"
                    }
                ));
                for site in capability.sites.iter().take(MAX_SITES_TEXT) {
                    out.push_str(&format!(
                        "      {}:{}:{}  {}\n",
                        site.path, site.line, site.column, site.callee
                    ));
                }
                if capability.count > MAX_SITES_TEXT {
                    out.push_str(&format!(
                        "      ... and {} more\n",
                        capability.count - MAX_SITES_TEXT
                    ));
                }
            }
            out.push_str(&format!("  denied: {}\n", words(package.denied)));
            if self.policy_enforced {
                out.push_str(&format!("  unused: {}\n", words(package.unused)));
            }
            let externs: Vec<&ExternReport> = self
                .externs
                .iter()
                .filter(|extern_report| extern_report.package == info.name)
                .collect();
            if externs.is_empty() {
                out.push_str("  externs: none\n");
            } else {
                let trusted = externs
                    .iter()
                    .filter(|extern_report| extern_report.trusted.is_some())
                    .count();
                out.push_str(&format!(
                    "  externs: {} ({trusted} trusted)\n",
                    externs.len()
                ));
                for extern_report in externs {
                    let trust = match extern_report.trusted {
                        None => "untrusted".to_owned(),
                        Some(bits) => std::iter::once("trusted")
                            .chain(properties(bits))
                            .collect::<Vec<_>>()
                            .join(" "),
                    };
                    out.push_str(&format!(
                        "    {} -> \"{}\"  {}:{}:{}  {trust}\n",
                        extern_report.name,
                        extern_report.symbol,
                        extern_report.path,
                        extern_report.line,
                        extern_report.column
                    ));
                }
            }
            out.push('\n');
        }
        let used = self.used();
        out.push_str(&format!(
            "summary: used={} denied={} untrusted_externs={}\n",
            if used.is_empty() {
                "none".to_owned()
            } else {
                used.names().join(",")
            },
            self.denied,
            self.untrusted_externs
        ));
        out
    }

    /// The report as JSON Lines (`tz-audit/1`): one `audit` row, a `package` row per package,
    /// an `extern` row per extern function and a `summary` row. Keys only ever get added.
    pub fn render_json(&self, deny_warnings: bool) -> String {
        let names = |set: CapabilitySet| strings(set.names());
        let mut out = format!(
            "{{\"type\":\"audit\",\"schema\":\"tz-audit/1\",\"root\":{},\"policy\":{},\"packages\":{}}}\n",
            json_string(&self.root),
            json_string(policy_name(self.policy_enforced)),
            self.packages.len()
        );
        for package in &self.packages {
            let info = &package.info;
            let granted: Vec<String> = package
                .granted
                .iter()
                .map(|(parent, set)| {
                    format!(
                        "{{\"by\":{},\"capabilities\":{}}}",
                        json_string(parent),
                        names(*set)
                    )
                })
                .collect();
            let used: Vec<String> = package
                .used
                .iter()
                .map(|capability| {
                    let sites: Vec<String> = capability
                        .sites
                        .iter()
                        .map(|site| {
                            format!(
                                "{{\"path\":{},\"line\":{},\"column\":{},\"callee\":{},\"function\":{},\"scope\":{}}}",
                                json_string(&site.path),
                                site.line,
                                site.column,
                                json_string(&site.callee),
                                json_string(&site.function),
                                json_string(site.scope.name())
                            )
                        })
                        .collect();
                    let truncated = if capability.truncated > 0 {
                        format!(",\"truncated\":{}", capability.truncated)
                    } else {
                        String::new()
                    };
                    format!(
                        "{{\"capability\":{},\"count\":{},\"sites\":[{}]{truncated}}}",
                        json_string(capability.capability.name()),
                        capability.count,
                        sites.join(",")
                    )
                })
                .collect();
            let effects = &package.effects;
            let asserted: Vec<String> = std::iter::once(("pure", effects.pure))
                .chain(
                    crate::syntax::EffectKind::ALL
                        .into_iter()
                        .zip(effects.asserted)
                        .map(|(kind, count)| (kind.name(), count)),
                )
                .map(|(name, count)| format!("{}:{count}", json_string(name)))
                .collect();
            out.push_str(&format!(
                "{{\"type\":\"package\",\"name\":{},\"version\":{},\"root\":{},\"allowed\":{},\"granted\":[{}],\"used\":[{}],\"denied\":{},\"unused\":{},\"effects\":{{\"declared\":{},\"asserted\":{{{}}}}}}}\n",
                json_string(&info.name),
                info.version
                    .as_deref()
                    .map_or_else(|| "null".to_owned(), json_string),
                info.root,
                names(package.allowed),
                granted.join(","),
                used.join(","),
                names(package.denied),
                names(package.unused),
                effects.functions,
                asserted.join(",")
            ));
        }
        for extern_report in &self.externs {
            out.push_str(&format!(
                "{{\"type\":\"extern\",\"package\":{},\"name\":{},\"symbol\":{},\"path\":{},\"line\":{},\"column\":{},\"trusted\":{},\"properties\":{},\"used\":{}}}\n",
                json_string(&extern_report.package),
                json_string(&extern_report.name),
                json_string(&extern_report.symbol),
                json_string(&extern_report.path),
                extern_report.line,
                extern_report.column,
                extern_report.trusted.is_some(),
                strings(properties(extern_report.trusted.unwrap_or(0)).collect()),
                extern_report.used
            ));
        }
        out.push_str(&format!(
            "{{\"type\":\"summary\",\"packages\":{},\"used\":{},\"denied\":{},\"untrusted_externs\":{},\"exit\":{}}}\n",
            self.packages.len(),
            names(self.used()),
            self.denied,
            self.untrusted_externs,
            self.exit_code(deny_warnings)
        ));
        out
    }
}

fn policy_name(enforced: bool) -> &'static str {
    if enforced { "enforced" } else { "none" }
}

fn versioned(name: &str, version: &Option<String>) -> String {
    match version {
        Some(version) => format!("{name} {version}"),
        None => name.to_owned(),
    }
}

/// The names of a set separated by spaces, or `none`.
fn words(set: CapabilitySet) -> String {
    if set.is_empty() {
        "none".to_owned()
    } else {
        set.names().join(" ")
    }
}

fn strings(values: Vec<&str>) -> String {
    let quoted: Vec<String> = values.into_iter().map(json_string).collect();
    format!("[{}]", quoted.join(","))
}

/// The `@trusted` properties of `bits`, in `TRUSTED_PROPERTIES` order.
fn properties(bits: u8) -> impl Iterator<Item = &'static str> {
    crate::syntax::TRUSTED_PROPERTIES
        .into_iter()
        .enumerate()
        .filter(move |(index, _)| bits & (1 << index) != 0)
        .map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::LoadedPackage;
    use std::path::PathBuf;

    #[test]
    fn capability_names_are_sorted_and_round_trip() {
        let names: Vec<&str> = Capability::ALL
            .iter()
            .map(|capability| capability.name())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert_eq!(names.join(", "), Capability::NAMES);
        for capability in Capability::ALL {
            assert_eq!(Capability::from_name(capability.name()), Some(capability));
        }
        assert_eq!(Capability::from_name("Fs"), None);
        assert_eq!(Capability::from_name("files"), None);
    }

    #[test]
    fn capability_set_operations() {
        use Capability::*;
        let mut fs_net = CapabilitySet::single(Fs);
        fs_net.insert(Net);
        let net_time = CapabilitySet::single(Net).union(CapabilitySet::single(Time));
        assert_eq!(
            fs_net.union(net_time).iter().collect::<Vec<_>>(),
            [Fs, Net, Time]
        );
        assert_eq!(
            fs_net.intersection(net_time).iter().collect::<Vec<_>>(),
            [Net]
        );
        assert_eq!(fs_net.difference(net_time).iter().collect::<Vec<_>>(), [Fs]);
        assert!(CapabilitySet::EMPTY.is_empty());
        assert_eq!(CapabilitySet::ALL.iter().count(), 9);
        assert_eq!(
            CapabilitySet::ALL.iter().collect::<Vec<_>>(),
            Capability::ALL
        );
        assert!(CapabilitySet::ALL.contains(Stdio) && !CapabilitySet::EMPTY.contains(Stdio));
    }

    #[test]
    fn every_private_builtin_is_classified() {
        let private: Vec<&str> = crate::check::Builtin::ALL
            .iter()
            .map(|builtin| builtin.name())
            .filter(|name| name.contains("__") || name.starts_with("Bench."))
            .collect();
        let mut world = 0;
        for name in &private {
            let touches = ["File", "Dir", "Env", "Main"]
                .iter()
                .any(|file| primitive(name, file).is_some());
            let quiet = PRIVATE_WITHOUT_CAPABILITY.contains(name);
            assert!(touches != quiet, "{name} must be in exactly one table");
            world += usize::from(touches);
        }
        // Appendix A.2: 33 builtins reach the host; the rest touch only the CPU and memory.
        assert_eq!(world, 33);
        assert_eq!(private.len(), 33 + PRIVATE_WITHOUT_CAPABILITY.len());
        // The multiplexed builtins follow their file.
        assert_eq!(primitive("Os.__read", "Env"), Some(Capability::Env));
        assert_eq!(primitive("Os.__read", "Dir"), Some(Capability::Fs));
        assert_eq!(primitive("Os.__write", "Env"), None);
        assert_eq!(primitive("Os.__read", "Net"), None);
    }

    /// A package of a graph for `Policy::build`, from manifest text.
    fn package(name: &str, manifest: &str, dependencies: &[&str]) -> LoadedPackage {
        let text = format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\n{manifest}");
        LoadedPackage {
            id: PackageId {
                name: name.to_owned(),
                root: PathBuf::from(format!("/work/{name}")),
            },
            manifest: crate::package::parse_manifest(&text, 0).unwrap(),
            text,
            sha256: None,
            version: None,
            dependency_roots: dependencies
                .iter()
                .map(|child| ((*child).to_owned(), PathBuf::from(format!("/work/{child}"))))
                .collect(),
        }
    }

    fn id(name: &str) -> Option<PackageId> {
        Some(PackageId {
            name: name.to_owned(),
            root: PathBuf::from(format!("/work/{name}")),
        })
    }

    fn set(names: &[Capability]) -> CapabilitySet {
        names.iter().fold(CapabilitySet::EMPTY, |set, capability| {
            set.union(CapabilitySet::single(*capability))
        })
    }

    #[test]
    fn effective_grants_intersect_along_every_path() {
        use Capability::*;
        let packages = [
            package(
                "root",
                "[dependencies]\na = { path = \"../a\", capabilities = [\"fs\", \"net\"] }\nb = { path = \"../b\", capabilities = [\"fs\"] }\n[capabilities]\nallow = [\"fs\", \"net\"]\n",
                &["a", "b"],
            ),
            package(
                "a",
                "[dependencies]\nc = { path = \"../c\", capabilities = [\"fs\", \"net\"] }\n",
                &["c"],
            ),
            package(
                "b",
                "[dependencies]\nc = { path = \"../c\", capabilities = [\"net\"] }\n",
                &["c"],
            ),
            package("c", "", &[]),
        ];
        let policy = Policy::build(Path::new("/work/root"), &packages).unwrap();
        assert!(policy.enforced);
        assert_eq!(policy.allowed(&None), set(&[Fs, Net]));
        assert_eq!(policy.allowed(&id("a")), set(&[Fs, Net]));
        assert_eq!(policy.allowed(&id("b")), set(&[Fs]));
        // `fs` is not granted on b -> c, and `net` is not allowed to b.
        assert_eq!(policy.allowed(&id("c")), CapabilitySet::EMPTY);
        assert_eq!(
            policy.blocker(&id("c"), Fs),
            Some(Blocker::Edge {
                parent: "b".into(),
                child: "c".into()
            })
        );
        assert_eq!(
            policy.blocker(&id("c"), Net),
            Some(Blocker::Edge {
                parent: "root".into(),
                child: "b".into()
            })
        );
        assert_eq!(policy.blocker(&id("c"), Time), Some(Blocker::RootAllow));
        let names: Vec<&str> = policy
            .packages
            .iter()
            .map(|info| info.name.as_str())
            .collect();
        assert_eq!(names, ["root", "a", "b", "c"]);
        let edges: Vec<(String, String)> = policy
            .edges
            .iter()
            .map(|edge| (edge.child.name.clone(), policy.name(&edge.parent)))
            .collect();
        assert_eq!(
            edges,
            [("a", "root"), ("b", "root"), ("c", "a"), ("c", "b")]
                .map(|(child, parent)| (child.to_owned(), parent.to_owned()))
        );
    }

    #[test]
    fn policy_is_off_without_any_capabilities_key() {
        let packages = [
            package("root", "[dependencies]\na = { path = \"../a\" }\n", &["a"]),
            package("a", "[capabilities]\nallow = []\n", &[]),
        ];
        let policy = Policy::build(Path::new("/work/root"), &packages).unwrap();
        assert!(!policy.enforced);
        assert!(
            policy
                .effective
                .values()
                .all(|set| *set == CapabilitySet::ALL)
        );
        assert_eq!(policy.allowed(&id("a")), CapabilitySet::ALL);
        // A root entry with `capabilities` alone turns the policy on; the root then allows all.
        let packages = [
            package(
                "root",
                "[dependencies]\na = { path = \"../a\", capabilities = [\"fs\"] }\n",
                &["a"],
            ),
            package("a", "", &[]),
        ];
        let policy = Policy::build(Path::new("/work/root"), &packages).unwrap();
        assert!(policy.enforced);
        assert_eq!(policy.allowed(&None), CapabilitySet::ALL);
        assert_eq!(policy.allowed(&id("a")), set(&[Capability::Fs]));
        assert!(Policy::build(Path::new("/work/elsewhere"), &packages).is_none());
    }
}
