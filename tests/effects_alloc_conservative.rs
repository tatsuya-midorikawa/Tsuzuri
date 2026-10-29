//! A20 Phase 3: the `alloc` rule of the effect summary is at least as conservative as the IR that
//! the compiler emits. Every function of every corpus program whose own emitted body allocates
//! (an allocator call, directly or through helper functions) has `alloc` among its own direct
//! sources. The rule may say `alloc` where the IR does not (safe false positives, only counted).

mod common;

use std::collections::BTreeMap;

use common::ir_scan;
use tsuzuri::check::effects::{self, Effect, EffectSet, Options, Scope};
use tsuzuri::driver::Project;
use tsuzuri::llvm;

/// `Module.name` of every `def` and `export def` of `std/` (regions and `rec` dropped).
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
            let Some(rest) = line
                .strip_prefix("export def ")
                .or_else(|| line.strip_prefix("def "))
            else {
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

#[test]
fn rule_is_at_least_as_conservative_as_the_ir() {
    let scratch = scratch("effects-alloc");
    let projects = ir_scan::corpus();
    assert!(
        projects.len() >= 100,
        "found only {} projects",
        projects.len()
    );
    let mut matched = 0;
    let mut total = 0;
    let mut rule_alloc = 0;
    let mut ir_alloc = 0;
    let mut false_positives: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut false_positive_functions = 0;
    let mut violations = Vec::new();
    let mut covered = std::collections::BTreeSet::new();
    for directory in &projects {
        let (ir, _) = ir_scan::emit(directory, &scratch);
        let project = Project::load_for_tests(directory).unwrap();
        let module = project.analyze().unwrap();
        // The program's build, and its tests, whose code a build leaves out.
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
        let summary = effects::summarize(
            &module,
            Options {
                sites: true,
                ..Options::new(Scope::All, EffectSet::ALL)
            },
        )
        .unwrap_or_else(|error| panic!("{}: {}", directory.display(), error.message));
        for (id, function) in module.functions.iter().enumerate() {
            total += 1;
            let symbol = format!("@tz.fn.{}", function.qualified_name());
            let found: Vec<_> = emitted
                .iter()
                .filter(|functions| functions.contains_key(&symbol))
                .collect();
            if found.is_empty() {
                continue;
            }
            matched += 1;
            if function.origin.module == tsuzuri::check::ModuleOrigin::Std {
                covered.insert(format!(
                    "{}.{}",
                    function.module,
                    function.name.split(".$mono.").next().unwrap()
                ));
            }
            let rule = summary.local(id).contains(Effect::Alloc);
            let emitted_alloc = found
                .iter()
                .any(|functions| ir_scan::own_alloc(functions, &symbol));
            rule_alloc += usize::from(rule);
            ir_alloc += usize::from(emitted_alloc);
            if emitted_alloc && !rule {
                let path = found
                    .iter()
                    .map(|functions| ir_scan::alloc_path(functions, &symbol))
                    .find(|path| !path.is_empty())
                    .unwrap_or_default();
                violations.push(format!(
                    "{}: {} allocates through {:?}",
                    directory.display(),
                    symbol,
                    path
                ));
            } else if rule && !emitted_alloc {
                false_positive_functions += 1;
                let mut reasons: Vec<&'static str> = summary
                    .sites(id)
                    .iter()
                    .filter(|site| site.effect == Effect::Alloc && !site.exempt)
                    .map(|site| site.why)
                    .collect();
                reasons.sort_unstable();
                reasons.dedup();
                for reason in reasons {
                    *false_positives.entry(reason).or_default() += 1;
                }
            }
        }
    }
    let uncovered: Vec<String> = std_public_functions()
        .into_iter()
        .filter(|name| !covered.contains(name))
        .collect();
    let _ = std::fs::remove_dir_all(&scratch);
    println!(
        "programs {} functions {total} matched {matched}; rule alloc {rule_alloc}, IR alloc {ir_alloc}, false positives {false_positive_functions} {false_positives:?}",
        projects.len()
    );
    // Appendix F.4: every public std function has an instance in the corpus, the sweep fixture
    // `tests/fixtures/effects_sweep` included, so the comparison above saw its IR.
    assert!(
        uncovered.is_empty(),
        "public std functions without an instance in the corpus; use them in tests/fixtures/effects_sweep: {uncovered:?}"
    );
    assert!(
        violations.is_empty(),
        "{} functions allocate in the IR without an 'alloc' source:\n{}",
        violations.len(),
        violations.join("\n")
    );
}
