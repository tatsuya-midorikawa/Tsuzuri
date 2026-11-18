//! A20 Phase 3: the `alloc` rule of the effect summary is at least as conservative as the IR that
//! the compiler emits. Every function of every corpus program whose own emitted body allocates
//! (an allocator call, directly or through helper functions) has `alloc` among its own direct
//! sources. The rule may say `alloc` where the IR does not (safe false positives, only counted).

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use common::ir_scan::{self, IrFunction};
use tsuzuri::check::CheckedModule;
use tsuzuri::check::effects::{self, Effect, EffectSet, Options, Scope};
use tsuzuri::driver::Project;
use tsuzuri::llvm;

/// `Module.name` of every public `def` of `std/`, whatever its modifiers (`export`, `const`,
/// `pure`; review 3 of #63), with regions and `rec` dropped.
fn std_public_functions() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        let path = entry.path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("tz" | "tc")) {
            continue;
        }
        let module = path.file_stem().unwrap().to_string_lossy().into_owned();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let mut rest = line;
            while let Some(after) = ["export ", "const ", "pure "]
                .iter()
                .find_map(|modifier| rest.strip_prefix(modifier))
            {
                rest = after;
            }
            let Some(rest) = rest.strip_prefix("def ") else {
                continue;
            };
            let rest = rest.strip_prefix("rec ").unwrap_or(rest);
            let name = rest.split([' ', '{', ':']).next().unwrap();
            names.push(format!("{module}.{name}"));
        }
    }
    names.sort();
    names.dedup();
    names
}

fn scratch(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("tsuzuri-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// The program in `directory`, analyzed, and the IR of its build and of its tests and benches, whose code a build
/// leaves out.
fn emitted(directory: &Path, scratch: &Path) -> (CheckedModule, Vec<BTreeMap<String, IrFunction>>) {
    let (ir, _) = ir_scan::emit(directory, scratch);
    let project = Project::load_for_tests(directory).unwrap();
    let module = project.analyze().unwrap();
    let mut emitted = vec![ir_scan::parse(&ir)];
    for (runner, entry) in [
        (!module.tests.is_empty(), llvm::Entry::TestRunner),
        (!module.benches.is_empty(), llvm::Entry::BenchRunner),
    ] {
        if runner {
            let ir = llvm::emit(&module, entry)
                .unwrap_or_else(|error| panic!("{}: {}", directory.display(), error.message));
            emitted.push(ir_scan::parse(&ir));
        }
    }
    (module, emitted)
}

/// The symbol of a function of `module` in the IR.
fn symbol(module: &CheckedModule, id: usize) -> String {
    format!("@tz.fn.{}", module.functions[id].qualified_name())
}

/// What the comparison of the rule with the IR found in some programs of the corpus.
#[derive(Default)]
struct Comparison {
    programs: usize,
    total: usize,
    matched: usize,
    rule_alloc: usize,
    ir_alloc: usize,
    false_positives: BTreeMap<&'static str, usize>,
    false_positive_functions: usize,
    violations: Vec<String>,
}

impl Comparison {
    /// Compares the `alloc` sources of the functions of the program in `directory` with the IR that they emit.
    fn add(&mut self, directory: &Path, scratch: &Path) {
        let (module, emitted) = emitted(directory, scratch);
        let summary = effects::summarize(
            &module,
            Options {
                sites: true,
                ..Options::new(Scope::All, EffectSet::ALL)
            },
        )
        .unwrap_or_else(|error| panic!("{}: {}", directory.display(), error.message));
        self.programs += 1;
        for id in 0..module.functions.len() {
            self.total += 1;
            let symbol = symbol(&module, id);
            let found: Vec<_> = emitted
                .iter()
                .filter(|functions| functions.contains_key(&symbol))
                .collect();
            if found.is_empty() {
                continue;
            }
            self.matched += 1;
            let rule = summary.local(id).contains(Effect::Alloc);
            let emitted_alloc = found
                .iter()
                .any(|functions| ir_scan::own_alloc(functions, &symbol));
            self.rule_alloc += usize::from(rule);
            self.ir_alloc += usize::from(emitted_alloc);
            if emitted_alloc && !rule {
                let path = found
                    .iter()
                    .map(|functions| ir_scan::alloc_path(functions, &symbol))
                    .find(|path| !path.is_empty())
                    .unwrap_or_default();
                self.violations.push(format!(
                    "{}: {} allocates through {:?}",
                    directory.display(),
                    symbol,
                    path
                ));
            } else if rule && !emitted_alloc {
                self.false_positive_functions += 1;
                let mut reasons: Vec<&'static str> = summary
                    .sites(id)
                    .iter()
                    .filter(|site| site.effect == Effect::Alloc && !site.exempt)
                    .map(|site| site.why)
                    .collect();
                reasons.sort_unstable();
                reasons.dedup();
                for reason in reasons {
                    *self.false_positives.entry(reason).or_default() += 1;
                }
            }
        }
    }
}

common::slices!(rule_is_at_least_as_conservative_as_the_ir, 6:
    1 => rule_is_at_least_as_conservative_as_the_ir_1_of_6,
    2 => rule_is_at_least_as_conservative_as_the_ir_2_of_6,
    3 => rule_is_at_least_as_conservative_as_the_ir_3_of_6,
    4 => rule_is_at_least_as_conservative_as_the_ir_4_of_6,
    5 => rule_is_at_least_as_conservative_as_the_ir_5_of_6,
    6 => rule_is_at_least_as_conservative_as_the_ir_6_of_6,
);

/// Slice `slice` of `slices` of the corpus: the programs whose index in `ir_scan::corpus` is `slice` modulo `slices`.
/// That the corpus has an instance of every public std function is
/// `rule_is_at_least_as_conservative_as_the_ir_and_sees_every_public_std_function`.
fn rule_is_at_least_as_conservative_as_the_ir(slice: usize, slices: usize) {
    // `cargo test` runs the slices as threads of one process: each has its own directory.
    let scratch = scratch(&format!("effects-alloc-{}-of-{slices}", slice + 1));
    let projects = ir_scan::corpus();
    assert!(
        projects.len() >= 100,
        "found only {} projects",
        projects.len()
    );
    let mut found = Comparison::default();
    for directory in common::slice(&projects, slice, slices) {
        found.add(directory, &scratch);
    }
    let _ = std::fs::remove_dir_all(&scratch);
    let Comparison {
        programs,
        total,
        matched,
        rule_alloc,
        ir_alloc,
        false_positives,
        false_positive_functions,
        violations,
    } = found;
    println!(
        "slice {} of {slices}: programs {programs} functions {total} matched {matched}; rule alloc {rule_alloc}, IR alloc {ir_alloc}, false positives {false_positive_functions} {false_positives:?}",
        slice + 1
    );
    assert!(
        violations.is_empty(),
        "{} functions allocate in the IR without an 'alloc' source:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// The public std functions that the sources of the program in `directory` name as `Module.name`: the text of its
/// sources only (`target/` left out), read as runs of identifier characters and dots whose neighbouring parts are
/// matched against `public`.
fn mentioned(directory: &Path, public: &BTreeSet<String>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name() != Some("target".as_ref()) {
                    pending.push(path);
                }
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("tz" | "tt" | "tc")
            ) {
                let text = std::fs::read_to_string(&path).unwrap();
                for run in text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')) {
                    let parts: Vec<&str> = run.split('.').collect();
                    for pair in parts.windows(2) {
                        let name = format!("{}.{}", pair[0], pair[1]);
                        if public.contains(&name) {
                            found.insert(name);
                        }
                    }
                }
            }
        }
    }
    found
}

/// `projects` in the order of a greedy set cover of the names that their sources mention (`mentioned`): first the
/// program that mentions the most public std functions, then the one that mentions the most of those not mentioned
/// yet, and so on (the first in corpus order on a tie); the programs that mention nothing new follow in corpus order.
fn by_mentions(projects: Vec<PathBuf>, public: &BTreeSet<String>) -> Vec<PathBuf> {
    let mut left: Vec<(PathBuf, BTreeSet<String>)> = projects
        .into_iter()
        .map(|directory| {
            let names = mentioned(&directory, public);
            (directory, names)
        })
        .collect();
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    loop {
        let best = left
            .iter()
            .enumerate()
            .map(|(index, (_, names))| (names.difference(&seen).count(), index))
            .max_by_key(|&(new, index)| (new, std::cmp::Reverse(index)));
        let Some((new, index)) = best else {
            break;
        };
        if new == 0 {
            break;
        }
        let (directory, names) = left.remove(index);
        seen.extend(names);
        order.push(directory);
    }
    order.extend(left.into_iter().map(|(directory, _)| directory));
    order
}

/// Appendix F.4: every public std function has an instance in the corpus, the sweep fixture
/// `tests/fixtures/effects_sweep` included, so the slices of `rule_is_at_least_as_conservative_as_the_ir` saw its IR.
/// A function has an instance when a program defines its symbol in the IR of its build or of its test or bench
/// runner, as in the slices. The programs are emitted one at a time, and the search ends as soon as every public std
/// function has an instance; when the corpus runs out first, the test fails with the functions that have none, so it
/// fails exactly when a search of the whole corpus would. The order only decides how soon the search ends: the
/// programs are read in the order of `by_mentions`, a guess from the text of their sources that never compiles them.
/// A wrong guess costs time, never correctness.
#[test]
fn rule_is_at_least_as_conservative_as_the_ir_and_sees_every_public_std_function() {
    let scratch = scratch("effects-alloc-coverage");
    let public: BTreeSet<String> = std_public_functions().into_iter().collect();
    let mut uncovered = public.clone();
    let mut programs = 0;
    for directory in by_mentions(ir_scan::corpus(), &public) {
        if uncovered.is_empty() {
            break;
        }
        let (module, emitted) = emitted(&directory, &scratch);
        programs += 1;
        for (id, function) in module.functions.iter().enumerate() {
            let symbol = symbol(&module, id);
            if function.origin.module == tsuzuri::check::ModuleOrigin::Std
                && emitted
                    .iter()
                    .any(|functions| functions.contains_key(&symbol))
            {
                uncovered.remove(&format!(
                    "{}.{}",
                    function.module,
                    function.name.split(".$mono.").next().unwrap()
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    println!(
        "{programs} programs emitted to find an instance of each of the {} public std functions",
        public.len()
    );
    let uncovered: Vec<String> = uncovered.into_iter().collect();
    assert!(
        uncovered.is_empty(),
        "public std functions without an instance in the corpus; use them in tests/fixtures/effects_sweep: {uncovered:?}"
    );
}
