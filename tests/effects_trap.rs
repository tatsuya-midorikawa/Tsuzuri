//! A20 Phase 4: the `trap` rule of the effect summary and `@no_trap`. The corpus tests check
//! that every logical trap that the IR keeps has a `trap` site of the rule, as a source or as an
//! exemption with its reason (appendix F.3).

mod common;

use std::collections::BTreeSet;

use common::ir_scan;
use tsuzuri::analyze;
use tsuzuri::check::effects::{self, Effect, EffectSet, Options, Scope};
use tsuzuri::driver::Project;
use tsuzuri::llvm;

/// The `TrapKind` descriptions of the program's logic (D-53): the others are resources running
/// out or unreachable guards.
const LOGICAL: &[&str] = &[
    "assertion failed",
    "integer division by zero",
    "integer division overflow",
    "index out of bounds",
    "range step is zero",
    "non-exhaustive match",
    "numeric runtime trap",
    "invalid Unicode encoding",
    "unhandled OverflowException: arithmetic operation resulted in an overflow",
];

/// The `TrapKind` discriminants of `LOGICAL`.
const LOGICAL_KINDS: &[u64] = &[0, 1, 2, 3, 6, 7, 10, 12, 15];

fn scratch(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("tsuzuri-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn ir_trap_sites_are_covered_by_the_rule() {
    let scratch = scratch("effects-trap");
    let projects = ir_scan::corpus();
    assert!(
        projects.len() >= 100,
        "found only {} projects",
        projects.len()
    );
    let (mut sites, mut functions_checked) = (0, 0);
    let mut missing = Vec::new();
    for directory in &projects {
        let Some((ir, table)) = ir_scan::emit(directory, &scratch) else {
            continue;
        };
        let project = Project::load_for_tests(directory).unwrap();
        let module = project.analyze().unwrap();
        let summary = effects::summarize(
            &module,
            Options {
                sites: true,
                ..Options::new(Scope::All, EffectSet::ALL)
            },
        )
        .unwrap_or_else(|error| panic!("{}: {}", directory.display(), error.message));
        let functions = ir_scan::parse(&ir);
        // (a) The trap sites of the program's own functions, by position (`--trap-info`).
        let real: BTreeSet<u64> = functions
            .iter()
            .filter(|(symbol, _)| ir_scan::user_level(symbol))
            .flat_map(|(_, function)| function.reports.iter().copied())
            .collect();
        let mut exact = BTreeSet::new();
        let mut starts = BTreeSet::new();
        for id in 0..module.functions.len() {
            for site in summary
                .sites(id)
                .iter()
                .filter(|site| site.effect == Effect::Trap)
            {
                let path = project
                    .source_for_span(site.span)
                    .path
                    .display()
                    .to_string();
                exact.insert((path.clone(), site.span.start, site.span.end));
                starts.insert((path, site.span.start));
            }
        }
        let table: serde_json::Value = serde_json::from_str(&table).unwrap_or_default();
        for site in table["sites"].as_array().into_iter().flatten() {
            let id = site["id"].as_u64().unwrap();
            let kind = site["kind"].as_str().unwrap();
            if !real.contains(&id) || !LOGICAL.contains(&kind) {
                continue;
            }
            sites += 1;
            let path = site["path"].as_str().unwrap().to_owned();
            let start = site["span"]["start"].as_u64().unwrap() as usize;
            let end = site["span"]["end"].as_u64().unwrap() as usize;
            if !exact.contains(&(path.clone(), start, end))
                && !starts.contains(&(path.clone(), start))
            {
                missing.push(format!(
                    "{}: {kind} at {path}:{start}..{end}",
                    directory.display()
                ));
            }
        }
        // (b) The functions whose inherited trap context names a logical kind: the program's
        // build and its tests and benches.
        let mut emitted = vec![functions];
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
        for (id, function) in module.functions.iter().enumerate() {
            let symbol = format!("@tz.fn.{}", function.qualified_name());
            let logical = emitted.iter().any(|functions| {
                functions.get(&symbol).is_some_and(|emitted| {
                    emitted
                        .context_kinds
                        .iter()
                        .any(|kind| LOGICAL_KINDS.contains(kind))
                })
            });
            if !logical {
                continue;
            }
            functions_checked += 1;
            if !summary
                .sites(id)
                .iter()
                .any(|site| site.effect == Effect::Trap)
            {
                missing.push(format!(
                    "{}: {symbol} inherits a logical trap kind without a 'trap' site",
                    directory.display()
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    println!(
        "logical trap sites checked: {sites}; functions with logical trap kinds: {functions_checked}"
    );
    assert!(sites > 0 && functions_checked > 0);
    assert!(
        missing.is_empty(),
        "{} logical traps have no 'trap' site of the rule:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";

fn verdict(function: &str) -> Result<(), String> {
    let source = format!("@no_trap\n{function}\n{MAIN}");
    match analyze(&source) {
        Ok(_) => Ok(()),
        Err(error) => {
            assert_eq!(error.code, "E1031", "{source}\n{}", error.message);
            assert_eq!(
                error.kind,
                Some("effect_trap_in_no_trap"),
                "{}",
                error.message
            );
            Err(error.message.clone())
        }
    }
}

fn no_trap(function: &str) {
    if let Err(message) = verdict(function) {
        panic!("expected no trap: {function}\n{message}");
    }
}

fn trap(function: &str, what: &str) {
    match verdict(function) {
        Ok(()) => panic!("expected a trap: {function}"),
        Err(message) => assert!(message.contains(what), "{function}\n{message}"),
    }
}

#[test]
fn integer_division_needs_a_safe_literal_divisor() {
    trap(
        "def f :: i64 -> i64 -> i64 = \\a b -> a / b",
        "divides by a value that may be zero",
    );
    trap(
        "def f :: i64 -> i64 -> i64 = \\a b -> a % b",
        "divides by a value that may be zero",
    );
    no_trap("def f :: i64 -> i64 = \\a -> a / 7 + a % 3");
    // `i64::MIN / -1` overflows; an unsigned all-ones divisor cannot.
    trap("def f :: i64 -> i64 = \\a -> a / -1", "divides by a value");
    no_trap("def f :: i64u -> i64u = \\a -> a / 18446744073709551615i64u");
    no_trap("def f :: f64 -> f64 -> f64 = \\a b -> a / b");
}

#[test]
fn signed_power_needs_a_literal_exponent() {
    trap(
        "def f :: i64 -> i64 -> i64 = \\a b -> a ** b",
        "may be negative or zero",
    );
    no_trap("def f :: i64 -> i64 = \\a -> a ** 3");
    no_trap("def f :: i64u -> i64u -> i64u = \\a b -> a ** b");
}

#[test]
fn indexes_need_a_proof() {
    trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs i -> xs[i]",
        "indexes without a proof",
    );
    no_trap(
        "def f :: ref [i64] -> i64 = \\xs ->\n    let mut sum = 0\n    for i in 0 .. xs.length - 1 do\n        sum = sum + xs[i]\n    sum",
    );
    no_trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs i -> if i >= 0 && i < xs.length then xs[i] else 0",
    );
    no_trap("def f :: i64 -> i64 = \\n ->\n    let xs = [1, 2, 3]\n    xs[2] + n");
    trap(
        "def f :: ref [i64] -> i64 = \\xs -> xs[0]",
        "indexes without a proof",
    );
}

#[test]
fn guard_with_or_proves_the_else_branch() {
    no_trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs i -> if i < 0 || i >= xs.length then 0 else xs[i]",
    );
    trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs i -> if i < 0 || i > xs.length then 0 else xs[i]",
        "indexes without a proof",
    );
    trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs i -> if i < 0 || i >= xs.length then xs[i] else 0",
        "indexes without a proof",
    );
}

#[test]
fn array_try_item_is_trap_free() {
    no_trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs i ->\n    match Array.try_item i xs with\n    | Some x -> x\n    | None -> 0",
    );
    no_trap("def f :: ref [i64] -> i64 = \\xs -> Array.length xs");
    trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs i -> Array.get i xs",
        "this call reaches 'Array.get', which indexes without a proof",
    );
}

#[test]
fn slices_ranges_and_lengths() {
    trap(
        "def f :: ref [i64] -> i64 -> i64 = \\xs n -> (ref xs[0..n]).length",
        "slices without a proof",
    );
    no_trap(
        "def f :: i64 -> i64 = \\n ->\n    let mut sum = 0\n    for i in 0 .. 2 .. n do\n        sum = sum + i\n    sum",
    );
    trap(
        "def f :: i64 -> i64 -> i64 = \\n step ->\n    let mut sum = 0\n    for i in 0 .. step .. n do\n        sum = sum + i\n    sum",
        "may be negative or zero",
    );
    trap(
        "def f :: i64 -> i64 = \\n -> (new [i64](n, \\i -> i)).length",
        "may be negative or zero",
    );
}

#[test]
fn checked_arithmetic_and_assertions_trap() {
    trap(
        "def f :: i64 -> i64 -> i64 = \\a b -> @checked (a + b)",
        "OverflowException",
    );
    trap(
        "def f :: i64 -> i64 = \\a ->\n    assert (a > 0)\n    a",
        "can trap",
    );
}

#[test]
fn matches_do_not_trap() {
    no_trap(
        "def f :: Maybe<i64> -> i64 = \\m ->\n    match m with\n    | Some x -> x\n    | None -> 0",
    );
}
