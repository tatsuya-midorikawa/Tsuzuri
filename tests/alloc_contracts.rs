//! Rule-derived structural contracts, independent of time or optimization (F14 Phase 3).
use tsuzuri::driver::{self, Target, TestOptions};

const CONTRACTS: &str = include_str!("fixtures/alloc_contracts/Main.tz");

#[test]
fn std_allocation_contracts_hold_on_native_and_wasm32_at_o0_and_o3() {
    assert_eq!(
        CONTRACTS
            .lines()
            .filter(|line| line.starts_with("test \""))
            .count(),
        42
    );
    let module = tsuzuri::analyze(CONTRACTS).unwrap();
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
                "{target:?} -O{optimization}:\n{}",
                failures.join("\n")
            );
        }
    }
}

#[test]
fn a_changed_expectation_fails_the_contract() {
    let header = CONTRACTS.split("// --- Array ---").next().unwrap();
    let test = CONTRACTS
        .split_once("test \"Vec.push: 100 pushes realloc five times\"")
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
