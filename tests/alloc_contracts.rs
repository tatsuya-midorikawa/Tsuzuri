//! Rule-derived structural contracts, independent of time or optimization (F14 Phase 3).
use std::process::Command;
use tsuzuri::driver::{self, Target, TestOptions};
use tsuzuri::syntax::Edition;

const CONTRACTS: &str = include_str!("fixtures/alloc_contracts/Main.tz");

#[test]
fn std_allocation_contracts_hold_on_native_and_wasm32_at_o0_and_o3() {
    // The byte contracts specify UTF-16, independent of the surrounding compact-string suite.
    if std::env::var(tsuzuri::llvm::STRINGS_VARIABLE).as_deref() != Ok("utf16") {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "std_allocation_contracts_hold_on_native_and_wasm32_at_o0_and_o3",
                "--nocapture",
            ])
            .env(tsuzuri::llvm::STRINGS_VARIABLE, "utf16")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_eq!(
        CONTRACTS
            .lines()
            .filter(|line| line.starts_with("test \""))
            .count(),
        42
    );
    for edition in [Edition::E2026, Edition::E2027] {
        let module = tsuzuri::analyze_with_edition(CONTRACTS, edition).unwrap();
        for target in [Target::Native, Target::Wasm32] {
            for optimization in [0, 3] {
                let report = driver::run_tests(
                    &module,
                    &TestOptions {
                        target,
                        optimization,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(report.results.len(), 42);
                let failures: Vec<_> = report
                    .results
                    .iter()
                    .filter_map(|result| {
                        result.failure.as_ref().map(|failure| {
                            format!("{}: {failure}\n{}", result.case.name, result.output)
                        })
                    })
                    .collect();
                assert!(
                    failures.is_empty(),
                    "{edition:?} {target:?} -O{optimization}:\n{}",
                    failures.join("\n")
                );
            }
        }
    }
}

#[test]
fn a_changed_expectation_fails_the_contract() {
    let header = CONTRACTS.split("// --- Array ---").next().unwrap();
    let test = CONTRACTS
        .split_once("test \"Vec.push_owned: 100 pushes realloc five times\"")
        .unwrap()
        .1;
    let test = test.split("// Rule:").next().unwrap();
    assert!(test.contains("1 5 0 2016"));
    let source = format!(
        "{header}\ntest \"the mutant expects six resizes\"{}",
        test.replace("1 5 0 2016", "1 6 0 2016")
    );
    let module = tsuzuri::analyze(&source).unwrap();
    let report = driver::run_tests(&module, &TestOptions::default()).unwrap();
    assert_eq!(report.results.len(), 1);
    assert!(report.results[0].failure.is_some());
    assert!(
        report.results[0].output.contains("reallocs 5/6"),
        "{:?}",
        report.results[0]
    );
}
