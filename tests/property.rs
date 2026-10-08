//! Property tests with `Gen` (G18 Phase 3). The expected counterexamples are the simplest
//! failing values, worked out by hand from each generator's order of simplicity (smaller
//! choices, then fewer); they are not copied from the compiler's output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROPS: &str = include_str!("fixtures/property/Props.tz");
const BOUNDS: &str = include_str!("fixtures/property/Bounds.tz");
const DEFAULT_SEED: &str = "11400714819323198485";

/// Each failing property and its simplest counterexample, as `Display` shows it.
const SIMPLEST: [(&str, &str); 20] = [
    ("i64 below 100", "100"),
    ("i64 above -100", "-100"),
    ("i32 below 1000", "1000"),
    ("range below 500", "500"),
    ("negative range above -30", "-30"),
    ("bool is false", "true"),
    // `Display` writes a whole f64 without a fraction (docs/language.md).
    ("f64 below 2", "2"),
    ("char before c", "c"),
    ("unicode char before brace", "{"),
    ("string shorter than 3", "aaa"),
    ("array shorter than 3", "[0, 0, 0]"),
    ("array values below 5", "[5]"),
    ("maybe below 50", "Some 50"),
    ("error below 3", "Error 3"),
    ("pair corner", "(10, 20)"),
    ("map doubles", "100"),
    ("bind sizes", "[7, 7]"),
    ("one of", "15"),
    ("element", "4"),
    ("seven cases", "0"),
];

fn project(test: &str) -> PathBuf {
    project_with(test, "Props.tz", PROPS)
}

fn project_with(test: &str, file: &str, text: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-property-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(file), text).unwrap();
    root
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

/// The `type: test` lines of `tsuzuri test --json`, without their timings.
fn results(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|line| line["type"] == "test")
        .map(|mut line| {
            line.as_object_mut().unwrap().remove("duration_ms");
            line
        })
        .collect()
}

/// The report of a failed property: the case, the seed, and the counterexample lines.
fn report(result: &serde_json::Value) -> Vec<String> {
    result["output"]
        .as_str()
        .unwrap_or_else(|| panic!("{result}"))
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn properties_shrink_to_the_simplest_counterexample() {
    let root = project("simplest");
    let output = tsuzuri(&root, &["--json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 22);
    for name in ["i64 holds", "pair commutes"] {
        let result = results
            .iter()
            .find(|result| result["name"] == name)
            .unwrap();
        assert_eq!(result["status"], "passed", "{result}");
    }
    for (name, simplest) in SIMPLEST {
        let result = results
            .iter()
            .find(|result| result["name"] == name)
            .unwrap_or_else(|| panic!("{name}"));
        assert_eq!(result["status"], "failed", "{result}");
        assert_eq!(
            result["failure"], "trapped or terminated by signal",
            "{result}"
        );
        let lines = report(result);
        assert_eq!(lines.len(), 3, "{name}: {lines:?}");
        let cases = if name == "seven cases" { 7 } else { 100 };
        let case: u32 = lines[0]
            .strip_prefix("property failed at case ")
            .and_then(|rest| rest.strip_suffix(&format!(" of {cases} (seed {DEFAULT_SEED})")))
            .and_then(|case| case.parse().ok())
            .unwrap_or_else(|| panic!("{name}: {}", lines[0]));
        assert!((1..=cases).contains(&case), "{name}: {}", lines[0]);
        assert_eq!(lines[1], format!("counterexample: {simplest}"), "{name}");
        let shrinks = lines[2]
            .strip_prefix("shrunk ")
            .and_then(|rest| rest.split_once(" times from "))
            .unwrap_or_else(|| panic!("{name}: {}", lines[2]));
        assert!(shrinks.0.parse::<u32>().is_ok(), "{name}: {}", lines[2]);
    }
    // The text report puts the property's lines under the failure.
    let text = tsuzuri(&root, &["--filter", "Props.i64 below 100"]);
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        stdout.contains(
            "not ok 3 - Props i64 below 100\n  failure: trapped or terminated by signal\n  property failed at case "
        ) && stdout.contains("\n  counterexample: 100\n  shrunk "),
        "{stdout}"
    );
    clean(&root);
}

#[test]
fn integer_generators_reach_their_bounds_and_shrink_from_them() {
    // Each property fails only at a bound of its type, except the two that fail for every value
    // up to 1,000 above the minimum; their simplest counterexample is that value, whatever the
    // search found first. 1 in 16 proposals of `Gen.i64()` and `Gen.i32()` is a bound, so the
    // default seed's 100 cases meet both bounds.
    let expected = [
        (
            "negation overflows only at the minimum",
            "-9223372036854775808",
        ),
        ("i64 reaches the maximum", "9223372036854775807"),
        ("i64 shrinks from the minimum", "-9223372036854774808"),
        ("i32 reaches the minimum", "-2147483648"),
        ("i32 reaches the maximum", "2147483647"),
        ("i32 shrinks from the minimum", "-2147482648"),
    ];
    let root = project_with("bounds", "Bounds.tz", BOUNDS);
    let output = tsuzuri(&root, &["--json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let found = results(&output);
    assert_eq!(found.len(), expected.len());
    let mut cases = Vec::new();
    for (name, simplest) in expected {
        let result = found
            .iter()
            .find(|result| result["name"] == name)
            .unwrap_or_else(|| panic!("{name}"));
        let lines = report(result);
        let case: u32 = lines[0]
            .strip_prefix("property failed at case ")
            .and_then(|rest| rest.strip_suffix(&format!(" of 100 (seed {DEFAULT_SEED})")))
            .and_then(|case| case.parse().ok())
            .unwrap_or_else(|| panic!("{name}: {}", lines[0]));
        cases.push(case);
        assert_eq!(lines[1], format!("counterexample: {simplest}"), "{name}");
        if let Some(minimum) = [
            ("i64 shrinks from the minimum", "-9223372036854775808"),
            ("i32 shrinks from the minimum", "-2147483648"),
        ]
        .into_iter()
        .find_map(|(shrinking, minimum)| (shrinking == name).then_some(minimum))
        {
            // The search meets the minimum first: other proposals of the lower half are random
            // distances, which fall within 1,000 of it with a chance of about 2^-21 (i32) or
            // 2^-53 (i64) each.
            let (shrinks, start) = lines[2]
                .strip_prefix("shrunk ")
                .and_then(|rest| rest.split_once(" times from "))
                .unwrap_or_else(|| panic!("{name}: {}", lines[2]));
            assert!(shrinks.parse::<u32>().unwrap() > 0, "{name}: {}", lines[2]);
            assert_eq!(start, minimum, "{name}");
        } else {
            // Only the bound fails, and no simpler choices make it.
            assert_eq!(
                lines[2],
                format!("shrunk 0 times from {simplest}"),
                "{name}"
            );
        }
    }
    // The same random choices decide the side and the bound for both widths, so i64 and i32 meet
    // their minimum (and their maximum) at the same case.
    assert_eq!(
        (cases[2], cases[3], cases[5]),
        (cases[0], cases[0], cases[0])
    );
    assert_eq!(cases[1], cases[4]);
    let reports = |output: &Output| -> Vec<(String, String)> {
        results(output)
            .iter()
            .map(|result| {
                (
                    result["name"].as_str().unwrap().to_owned(),
                    result["output"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    };
    let native = reports(&output);
    assert_eq!(native, reports(&tsuzuri(&root, &["--json", "-O3"])));
    assert_eq!(
        native,
        reports(&tsuzuri(&root, &["--json", "--target", "wasm32"]))
    );
    clean(&root);
}

#[test]
fn invalid_generator_domains_trap_before_any_case() {
    // A generator with an empty domain is an error in the test, not a property that holds.
    let root = project_with(
        "domains",
        "Domains.tz",
        concat!(
            "test \"zero limit\" = Gen.for_all (Gen.array_up_to 0 (Gen.i64())) (\\values -> values.length == 0)\n",
            "test \"negative limit\" = Gen.for_all (Gen.array_up_to (-1) (Gen.i64())) (\\values -> values.length == 0)\n",
            "test \"empty range\" = Gen.for_all (Gen.range 5 1) (\\x -> x > 0)\n",
        ),
    );
    let output = tsuzuri(&root, &["--json"]);
    let statuses: Vec<_> = results(&output)
        .iter()
        .map(|result| {
            (
                result["name"].as_str().unwrap().to_owned(),
                result["status"].as_str().unwrap().to_owned(),
                result.get("output").is_none(),
            )
        })
        .collect();
    assert_eq!(
        statuses,
        [
            ("zero limit".to_owned(), "passed".to_owned(), true),
            ("negative limit".to_owned(), "failed".to_owned(), true),
            ("empty range".to_owned(), "failed".to_owned(), true),
        ]
    );
    clean(&root);
}

#[test]
fn property_runs_are_reproducible_across_runs_optimizations_and_targets() {
    let root = project("reproducible");
    let first = results(&tsuzuri(&root, &["--json"]));
    assert_eq!(first, results(&tsuzuri(&root, &["--json"])));
    assert_eq!(first, results(&tsuzuri(&root, &["--json", "-O3"])));
    // The generator is integer arithmetic on PCG, so WASM finds the same values; only the
    // failure reason (a WASM trap ends Node with code 1) differs.
    let reports = |results: &[serde_json::Value]| -> Vec<(String, String)> {
        results
            .iter()
            .map(|result| {
                (
                    result["name"].as_str().unwrap().to_owned(),
                    result["output"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    };
    let wasm = results(&tsuzuri(&root, &["--json", "--target", "wasm32"]));
    assert_eq!(reports(&first), reports(&wasm));
    assert!(
        wasm.iter()
            .filter(|result| result["status"] == "failed")
            .all(|result| result["failure"] == "trapped or exited with code 1")
    );
    clean(&root);
}

#[test]
fn seed_option_sets_the_search_and_is_validated() {
    let root = project("seed");
    let filter = ["--json", "--seed", "7", "--filter", "Props.i64 below 100"];
    let seeded = results(&tsuzuri(&root, &filter));
    assert_eq!(seeded.len(), 1);
    let lines = report(&seeded[0]);
    assert!(lines[0].ends_with(" of 100 (seed 7)"), "{lines:?}");
    // The simplest counterexample does not depend on where the search started.
    assert_eq!(lines[1], "counterexample: 100");
    assert_eq!(seeded, results(&tsuzuri(&root, &filter)));
    let largest = [
        "--json",
        "--seed",
        "18446744073709551615",
        "--filter",
        "Props.element",
    ];
    let lines = report(&results(&tsuzuri(&root, &largest))[0]);
    assert!(
        lines[0].ends_with(" (seed 18446744073709551615)"),
        "{lines:?}"
    );
    assert_eq!(lines[1], "counterexample: 4");
    for (arguments, message) in [
        (
            vec!["--seed", "18446744073709551616"],
            "property seed must be an integer between 0 and 18446744073709551615",
        ),
        (
            vec!["--seed", "-1"],
            "property seed must be an integer between 0 and 18446744073709551615",
        ),
        (
            vec!["--seed", "1", "--seed", "2"],
            "seed specified more than once",
        ),
    ] {
        let output = tsuzuri(&root, &arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("E2000") && stderr.contains(message),
            "{stderr}"
        );
    }
    let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args([
            "build",
            root.join("Props.tz").to_str().unwrap(),
            "--seed",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(build.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&build.stderr).contains("--seed is only valid with test"));
    clean(&root);
}

#[test]
fn seed_reaches_only_the_generator_and_gen_internals_are_private() {
    assert_eq!(tsuzuri::llvm::DEFAULT_PROPERTY_SEED, 11400714819323198485);
    let module = tsuzuri::analyze(
        "test \"p\" = Gen.for_all (Gen.range 0 9) (\\x -> x < 10)\ntest \"plain\" = assert true",
    )
    .unwrap();
    let runner = |seed| {
        tsuzuri::llvm::emit_test_runner_with(
            &module,
            &[0],
            tsuzuri::llvm::TestRunnerOptions {
                seed,
                ..Default::default()
            },
        )
        .unwrap()
    };
    // 11400714819323198485 is -7046029254386353131 as a signed 64-bit constant.
    assert!(runner(None).contains("ret i64 -7046029254386353131\n"));
    assert!(runner(Some(7)).contains("ret i64 7\n"));
    // A test that does not reach a property is unaffected by the seed.
    let plain = |seed| {
        tsuzuri::llvm::emit_test_runner_with(
            &module,
            &[1],
            tsuzuri::llvm::TestRunnerOptions {
                seed,
                ..Default::default()
            },
        )
        .unwrap()
    };
    assert_eq!(plain(None), plain(Some(7)));
    assert_eq!(
        plain(None),
        tsuzuri::llvm::emit_test_runner(&module, &[1], false).unwrap()
    );
    for source in [
        "let seed = Gen.__seed()\n0",
        "let generator: Gen<i64> = Gen { run: \\source -> (1, source) }\n0",
        "let generator = Gen.i64()\nlet run = generator.run\n0",
    ] {
        let error = tsuzuri::analyze(source).expect_err(source);
        assert_eq!(error.code, "E1022", "{source}: {}", error.message);
    }
    // Gen is an opt-in module: a program that does not name it does not load it.
    assert!(
        !tsuzuri::stdlib::sources_for(["test \"t\" = assert true"])
            .iter()
            .any(|(path, _)| *path == "std/Gen.tz")
    );
}
