//! `tsuzuri test --coverage` (G18 Phase 2). The expected counts are counted by hand from the
//! fixtures, not copied from the compiler's output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CALC: &str = include_str!("fixtures/coverage/Calc.tz");
const SHORTCUTS: &str = include_str!("fixtures/coverage/Shortcuts.tz");
const THROUGH: &str = include_str!("fixtures/coverage/Through.tz");

/// A fresh project root holding `files`, as the compiler names it (no Windows `\\?\` prefix).
fn project(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-coverage-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        fs::write(root.join(name), text).unwrap();
    }
    let root = fs::canonicalize(root).unwrap();
    let plain = root
        .to_str()
        .and_then(|text| text.strip_prefix(r"\\?\"))
        .map(PathBuf::from);
    plain.unwrap_or(root)
}

fn tsuzuri(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("test")
        .arg(root)
        .args(arguments)
        .env("TSUZURI_CACHE_DIR", root.with_extension("cache"))
        .output()
        .unwrap()
}

fn clean(root: &Path) {
    let _ = fs::remove_dir_all(root.with_extension("cache"));
    fs::remove_dir_all(root).unwrap();
}

fn expected_calc(root: &Path) -> String {
    // Two passing tests call `classify` with 5 and 0; the failing third one is left out.
    format!(
        "TN:\nSF:{}\nFN:3,Calc.classify\nFN:9,Calc.unused\nFNDA:2,Calc.classify\nFNDA:0,Calc.unused\nFNF:2\nFNH:1\n\
         DA:3,2\nDA:4,1\nDA:6,1\nDA:9,0\nLF:4\nLH:3\nend_of_record\n",
        root.join("Calc.tz").display()
    )
}

#[test]
fn coverage_reports_exact_lcov_at_o0_and_o3() {
    let root = project("lcov", &[("Calc.tz", CALC)]);
    for optimization in ["-O0", "-O3"] {
        let report = root.join(format!("calc{optimization}.info"));
        let output = tsuzuri(
            &root,
            &["--coverage", report.to_str().unwrap(), optimization],
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert_eq!(fs::read_to_string(&report).unwrap(), expected_calc(&root));
    }
    clean(&root);
}

#[test]
fn coverage_json_and_text_summaries() {
    let root = project("summary", &[("Calc.tz", CALC)]);
    let report = root.join("calc.info");
    let report = report.to_str().unwrap();
    let text = tsuzuri(&root, &["--coverage", report]);
    let stdout = String::from_utf8(text.stdout).unwrap();
    assert!(
        stdout.ends_with(
            "\n2 passed; 1 failed; 0 ignored\ncoverage: 3/4 lines (75.0%), 1/2 functions; excluded 1 failed test\n"
        ),
        "{stdout}"
    );
    let json = tsuzuri(&root, &["--coverage", report, "--json"]);
    let stdout = String::from_utf8(json.stdout).unwrap();
    let path = serde_json::to_string(&root.join("Calc.tz").display().to_string()).unwrap();
    assert_eq!(
        stdout.lines().last().unwrap(),
        format!(
            "{{\"type\":\"coverage\",\"lines\":{{\"hit\":3,\"total\":4}},\"functions\":{{\"hit\":1,\"total\":2}},\"excluded_failed\":1,\"files\":[{{\"path\":{path},\"lines\":{{\"hit\":3,\"total\":4}},\"functions\":{{\"hit\":1,\"total\":2}}}}]}}"
        )
    );
    assert!(
        stdout
            .lines()
            .nth_back(1)
            .unwrap()
            .contains("\"type\":\"summary\"")
    );
    // With only the first test: `classify 5` takes the `then` branch once.
    let filtered = tsuzuri(&root, &["--coverage", report, "--filter", "Calc.positive"]);
    assert!(filtered.status.success(), "{filtered:?}");
    assert!(
        String::from_utf8(filtered.stdout)
            .unwrap()
            .ends_with("coverage: 2/4 lines (50.0%), 1/2 functions\n")
    );
    let lcov = fs::read_to_string(report).unwrap();
    assert!(
        lcov.contains("\nFNDA:1,Calc.classify\n")
            && lcov.contains("\nDA:3,1\nDA:4,1\nDA:6,0\nDA:9,0\n"),
        "{lcov}"
    );
    clean(&root);
}

#[test]
fn coverage_counts_parallel_work_exactly() {
    // 100,000 elements make 25 chunks, which run on several threads; atomic counters lose none.
    let source = "def signs :: [i64] -> [i64]\nfn signs values =\n    let sign = \\x ->\n        if x > 0 then\n            1\n        else\n            0\n    Parallel.map sign (ref values)\n\n\
        test \"parallel\" =\n    let values = new [i64](100000, \\i -> i + 1)\n    let result = signs values\n    Test.is_true (Array.sum (ref result) == 100000)\n";
    let root = project("parallel", &[("Signs.tz", source)]);
    for optimization in ["-O0", "-O3"] {
        let report = root.join(format!("signs{optimization}.info"));
        let output = tsuzuri(
            &root,
            &["--coverage", report.to_str().unwrap(), optimization],
        );
        assert!(output.status.success(), "{output:?}");
        let lcov = fs::read_to_string(&report).unwrap();
        assert!(
            lcov.contains(
                "\nFNDA:1,Signs.signs\nFNF:1\nFNH:1\nDA:3,1\nDA:4,100000\nDA:5,100000\nDA:7,0\nDA:8,1\nLF:5\nLH:4\n"
            ),
            "{lcov}"
        );
    }
    clean(&root);
}

#[test]
fn coverage_counts_calls_that_the_code_generator_shortcuts() {
    // The generated code uses the argument of `same` and an inline `add` in the self tail call of
    // `count`, but both functions ran as far as the program can tell: `same` once, `add` once per
    // recursive call (3), and `count` on entry and on each of its 3 tail calls.
    let root = project("shortcuts", &[("Shortcuts.tz", SHORTCUTS)]);
    let expected = format!(
        "TN:\nSF:{}\nFN:2,Shortcuts.same\nFN:4,Shortcuts.add\nFN:7,Shortcuts.count\n\
         FNDA:1,Shortcuts.same\nFNDA:3,Shortcuts.add\nFNDA:4,Shortcuts.count\nFNF:3\nFNH:3\n\
         DA:2,1\nDA:4,3\nDA:7,4\nDA:8,1\nDA:10,3\nLF:5\nLH:5\nend_of_record\n",
        root.join("Shortcuts.tz").display()
    );
    for optimization in ["-O0", "-O3"] {
        let report = root.join(format!("shortcuts{optimization}.info"));
        let output = tsuzuri(
            &root,
            &["--coverage", report.to_str().unwrap(), optimization],
        );
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.ends_with("\ncoverage: 5/5 lines (100.0%), 3/3 functions\n"),
            "{stdout}"
        );
        assert_eq!(fs::read_to_string(&report).unwrap(), expected);
    }
    clean(&root);
}

#[test]
fn coverage_counts_identity_calls_that_known_closures_look_through() {
    // `pass` is evaluated once in each of the first six tests, though the code generator passes,
    // calls, or captures `double` (or `add 1`) directly. `double` runs once in three tests and
    // once per element in the two three-element arrays; the two instances of `ident` share its body.
    let root = project("through", &[("Through.tz", THROUGH)]);
    let expected = format!(
        "TN:\nSF:{}\nFN:2,Through.pass\nFN:4,Through.double\nFN:6,Through.add\nFN:8,Through.apply\n\
         FN:10,Through.ident\nFNDA:6,Through.pass\nFNDA:9,Through.double\nFNDA:1,Through.add\n\
         FNDA:2,Through.apply\nFNDA:2,Through.ident\nFNF:5\nFNH:5\n\
         DA:2,6\nDA:4,9\nDA:6,1\nDA:8,2\nDA:10,2\nLF:5\nLH:5\nend_of_record\n",
        root.join("Through.tz").display()
    );
    for optimization in ["-O0", "-O3"] {
        let report = root.join(format!("through{optimization}.info"));
        let output = tsuzuri(
            &root,
            &["--coverage", report.to_str().unwrap(), optimization],
        );
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&report).unwrap(), expected);
    }
    clean(&root);
}

#[test]
fn coverage_rejects_wasm_list_and_source_output() {
    let root = project("rejects", &[("Calc.tz", CALC)]);
    for (arguments, message) in [
        (
            vec!["--coverage", "calc.info", "--target", "wasm32"],
            "test coverage supports only the native target",
        ),
        (
            vec!["--coverage", "calc.info", "--list"],
            "coverage cannot be combined with --list",
        ),
        (
            vec!["--coverage", "a.info", "--coverage", "b.info"],
            "coverage specified more than once",
        ),
    ] {
        let output = tsuzuri(&root, &arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("E2000") && stderr.contains(message),
            "{stderr}"
        );
    }
    for action in ["build", "run", "check"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                action,
                root.join("Calc.tz").to_str().unwrap(),
                "--coverage",
                "x.info",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--coverage is only valid with test")
        );
    }
    let source = root.join("Calc.tz");
    let output = tsuzuri(&root, &["--coverage", source.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("E2003"));
    assert_eq!(fs::read_to_string(&source).unwrap(), CALC);
    // The library rejects WASM coverage the same way.
    let module = tsuzuri::analyze(CALC).unwrap();
    let error = tsuzuri::driver::run_tests(
        &module,
        &tsuzuri::driver::TestOptions {
            target: tsuzuri::driver::Target::Wasm32,
            coverage: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "E2000");
    clean(&root);
}

#[test]
fn runner_ir_without_coverage_has_no_counters() {
    let module = tsuzuri::analyze(CALC).unwrap();
    for wasm in [false, true] {
        let runner = tsuzuri::llvm::emit_test_runner(&module, &[0, 1, 2], wasm).unwrap();
        assert!(
            !runner.contains("tsuzuri_coverage") && !runner.contains("atomicrmw"),
            "{runner}"
        );
        let library =
            tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
        assert!(!library.contains("tsuzuri_coverage"));
    }
    // classify's body and branches are counted; unused has a counter but no code in the runner.
    let plan = tsuzuri::coverage::plan(&module);
    assert_eq!(plan.len(), 4);
    let covered = tsuzuri::llvm::emit_test_runner_covered(&module, &[0, 1, 2], &plan).unwrap();
    assert_eq!(
        covered,
        tsuzuri::llvm::emit_test_runner_covered(&module, &[0, 1, 2], &plan).unwrap()
    );
    assert!(covered.contains(
        "@tsuzuri_coverage_counters = global [4 x i64] zeroinitializer\n@tsuzuri_coverage_count = constant i64 4\n"
    ));
    let increments: Vec<&str> = covered
        .lines()
        .filter(|line| line.contains("atomicrmw add ptr getelementptr inbounds ([4 x i64], ptr @tsuzuri_coverage_counters"))
        .collect();
    assert_eq!(increments.len(), 3, "{covered}");
    assert!(
        increments
            .iter()
            .all(|line| line.ends_with("i64 1 monotonic"))
    );
}
