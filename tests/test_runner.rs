use tsuzuri::{parser, syntax::SourceKind};

#[test]
fn native_and_wasm_tests_are_isolated_filtered_and_ordered() {
    let module = tsuzuri::analyze("test \"passes\" = assert true\ntest \"traps\" = assert false\ntest \"passes after trap\" = assert (2 + 2 == 4)").unwrap();
    for target in [
        tsuzuri::driver::Target::Native,
        tsuzuri::driver::Target::Wasm32,
    ] {
        for optimization in [0, 3] {
            let options = tsuzuri::driver::TestOptions {
                target,
                optimization,
                filter: None,
                indices: Vec::new(),
                wasm_max_memory: None,
                wasm_stack_size: None,
            };
            let report = tsuzuri::driver::run_tests(&module, &options).unwrap();
            assert_eq!(
                report
                    .results
                    .iter()
                    .map(|result| result.case.index)
                    .collect::<Vec<_>>(),
                vec![0, 1, 2]
            );
            assert_eq!(
                report
                    .results
                    .iter()
                    .map(|result| result.failure.is_some())
                    .collect::<Vec<_>>(),
                vec![false, true, false]
            );
            let filtered = tsuzuri::driver::run_tests(
                &module,
                &tsuzuri::driver::TestOptions {
                    filter: Some("passes".into()),
                    ..options
                },
            )
            .unwrap();
            assert_eq!(filtered.results.len(), 2);
            assert_eq!(filtered.ignored, 1);
            assert!(
                filtered
                    .results
                    .iter()
                    .all(|result| result.failure.is_none())
            );
        }
    }
}

#[test]
fn wasm_memory_options_reach_the_wasm_test_link() {
    let module = tsuzuri::analyze(include_str!("fixtures/wasm_memory/Main.tz")).unwrap();
    let failures = |wasm_max_memory| {
        tsuzuri::driver::run_tests(
            &module,
            &tsuzuri::driver::TestOptions {
                target: tsuzuri::driver::Target::Wasm32,
                wasm_max_memory,
                ..Default::default()
            },
        )
        .unwrap()
        .results
        .iter()
        .filter(|result| result.failure.is_some())
        .map(|result| result.case.name.clone())
        .collect::<Vec<_>>()
    };
    assert_eq!(failures(None), ["allocates 32 MiB"]);
    assert!(failures(Some(67108864)).is_empty());
    // Above 2 GiB the heap compares the remaining room and functions check the stack.
    assert!(failures(Some(4294901760)).is_empty());
    let wasm64 = tsuzuri::driver::run_tests(
        &module,
        &tsuzuri::driver::TestOptions {
            target: tsuzuri::driver::Target::Wasm64,
            ..Default::default()
        },
    );
    match wasm64 {
        Ok(report) => assert_eq!(
            report
                .results
                .iter()
                .filter(|result| result.failure.is_some())
                .map(|result| result.case.name.as_str())
                .collect::<Vec<_>>(),
            ["allocates 32 MiB"]
        ),
        // Node.js before 24 has no memory64; tests/wasm64.mjs covers the run on Node.js 24.
        Err(error) => assert!(
            error.code == "E2002" && error.message.contains("memory64 support"),
            "{error:?}"
        ),
    }
    let native = tsuzuri::driver::run_tests(
        &module,
        &tsuzuri::driver::TestOptions {
            wasm_max_memory: Some(67108864),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(native.code, "E2000");
}

#[test]
fn cli_reports_json_filters_and_failures_without_main() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-tests-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Specs.tz"), "test \"passes\" = { let expected = \"ok\"; let actual = \"ok\"; Test.equal expected actual; Test.is_true (expected.length == actual.length) }\ntest \"fails\" = Test.is_true false\ntest \"passes later\" = { let expected = 1; let actual = 2; Test.not_equal expected actual }").unwrap();
    for target in ["native", "wasm32"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("test")
            .arg(&root)
            .args(["--json", "--target", target])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let lines: Vec<_> = stdout.lines().collect();
        assert_eq!(
            lines.len(),
            4,
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(lines[0].contains("\"index\":0") && lines[0].contains("\"status\":\"passed\""));
        assert!(lines[1].contains("\"index\":1") && lines[1].contains("\"status\":\"failed\""));
        assert!(lines[2].contains("\"index\":2") && lines[2].contains("\"status\":\"passed\""));
        assert!(lines[3].contains("\"passed\":2,\"failed\":1,\"ignored\":0"));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("\"code\":\"E2006\"") && stderr.contains("Specs.tz"),
            "{stderr}"
        );
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("test")
            .arg(&root)
            .args(["--filter", "Specs.passes", "--target", target, "-O3"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("2 passed; 0 failed; 1 ignored"));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("test")
        .arg(&root)
        .args(["--target", "wasm32"])
        .env("PATH", root.join("missing-tools"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("E2002"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn normal_emission_excludes_test_only_code_and_specializations() {
    let module = tsuzuri::analyze("private def helper :: i64 -> i64\nfn helper value = value + 1\n\
        def id :: 'a -> 'a\nfn id value = value\n\
        test \"first\" = { let values = new [1, 2]; let apply = value -> id (helper value); assert (apply 0 == 1 && values.length == 2) }\n\
        test \"second\" = assert false\n\
        export def answer :: i64\nfn answer = 42").unwrap();
    for wasm in [false, true] {
        let normal =
            tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
        assert!(normal.contains("@tz_answer"));
        for excluded in [
            "$test",
            "$lambda",
            "Main.id.$mono",
            "Main.helper",
            "@tsuzuri_test_",
            "@malloc",
        ] {
            assert!(!normal.contains(excluded), "{excluded}\n{normal}");
        }
        let runner = tsuzuri::llvm::emit_test_runner(&module, &[0], wasm).unwrap();
        assert!(runner.contains("@tsuzuri_test_count"));
        assert!(runner.contains("@tsuzuri_test_run"));
        assert!(runner.contains("Main.$test.0"));
        assert!(!runner.contains("Main.$test.1"));
        assert!(!runner.contains("@tz_answer"));
        assert!(runner.contains("Main.id.$mono"));
        assert!(runner.contains("Main.helper"));
        assert_eq!(
            runner,
            tsuzuri::llvm::emit_test_runner(&module, &[0], wasm).unwrap()
        );
    }
}

#[test]
fn cli_discovers_without_tools_and_selects_duplicate_names_by_index() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-discovery-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Specs.tz"), "/* test \"not a test\" = () */\ntest \"same \u{1f600}\" = assert false\ntest \"same \u{1f600}\" = assert true").unwrap();
    let listing = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("test")
        .arg(&root)
        .args(["--list", "--json"])
        .env("PATH", root.join("missing-tools"))
        .env("TSUZURI_CLANG", root.join("missing-clang"))
        .output()
        .unwrap();
    assert!(listing.status.success(), "{listing:?}");
    let cases: Vec<serde_json::Value> = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0]["index"], 0);
    assert_eq!(cases[1]["index"], 1);
    assert_eq!(cases[0]["name"], cases[1]["name"]);
    assert_eq!(
        cases[0]["range"]["start"],
        serde_json::json!({"line": 1, "character": 5})
    );
    assert_eq!(cases[0]["range"]["end"]["character"], 14);
    assert_eq!(cases[0]["path"], root.join("Specs.tz").to_str().unwrap());
    for target in ["native", "wasm32"] {
        for optimization in ["-O0", "-O3"] {
            let selected = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
                .arg("test")
                .arg(&root)
                .args(["--json", "--index", "1", "--target", target, optimization])
                .output()
                .unwrap();
            assert!(selected.status.success(), "{selected:?}");
            let output = String::from_utf8(selected.stdout).unwrap();
            assert!(output.contains("\"index\":1"), "{output}");
            assert!(!output.contains("\"index\":0"), "{output}");
            assert!(
                output.contains("\"passed\":1,\"failed\":0,\"ignored\":1"),
                "{output}"
            );
        }
    }
    let invalid = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("test")
        .arg(&root)
        .args(["--list", "--index", "2", "--json"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("E2000"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tests_are_checked_and_keep_private_dependencies_reachable() {
    let module = tsuzuri::analyze("private def helper :: i64 -> i64\nfn helper value = value + 1\n\
        test \"uses private and lambda\" = { let calculate = value -> helper value; assert (calculate 1 == 2) }\n\
        test \"builder\" = { let value = Option { let! number = Some 2; return number }; assert (Option.get value == 2) }").unwrap();
    assert_eq!(module.tests.len(), 2);
    assert!(module.warnings.is_empty(), "{:?}", module.warnings);
    for test in &module.tests {
        assert_eq!(
            module.functions[test.function].origin.test,
            Some(test.index)
        );
        assert_eq!(
            module.functions[test.function].signature.result,
            tsuzuri::check::Type::Unit
        );
    }
    assert_eq!(
        tsuzuri::analyze("test \"bad type\" = 42").unwrap_err().code,
        "E1003"
    );
    assert_eq!(
        tsuzuri::analyze(
            "test \"moved\" = { let value = \"x\"; let _used = value; assert (value.length == 1) }"
        )
        .unwrap_err()
        .code,
        "E1012"
    );
    tsuzuri::analyze_modules(&[("Specs.tz", "test \"ok\" = assert true")]).unwrap();
    tsuzuri::analyze_modules(&[(
        "Builder.tc",
        "def Return :: unit -> unit\nfn Return value = value\ntest \"ok\" = assert true",
    )])
    .unwrap();
    assert_eq!(
        tsuzuri::analyze_modules(&[("Traits.tt", "test \"bad\" = assert true")])
            .unwrap_err()
            .code,
        "E1018"
    );
}

#[test]
fn shared_specializations_work_in_normal_and_selected_test_roots() {
    let module = tsuzuri::analyze("def identity :: 'a -> 'a\nfn identity value = value\n\
        export def answer :: i64\nfn answer = identity 42\n\
        test \"one\" = assert (identity 1 == 1)\ntest \"two\" = assert (identity 2 == 2)\n\
        test \"tasks\" = { let tasks = new [task { return identity 3 }]; let results = Task.run (Task.parallel tasks); assert (results[0] == 3) }").unwrap();
    for target in [
        tsuzuri::driver::Target::Native,
        tsuzuri::driver::Target::Wasm32,
    ] {
        let normal = tsuzuri::llvm::emit_target(
            &module,
            tsuzuri::llvm::Entry::Library,
            target == tsuzuri::driver::Target::Wasm32,
        )
        .unwrap();
        assert!(normal.contains("Main.identity.$mono"));
        assert!(!normal.contains("$test"));
        let report = tsuzuri::driver::run_tests(
            &module,
            &tsuzuri::driver::TestOptions {
                target,
                optimization: 3,
                filter: None,
                indices: Vec::new(),
                wasm_max_memory: None,
                wasm_stack_size: None,
            },
        )
        .unwrap();
        assert_eq!(report.results.len(), 3);
        assert!(
            report.results.iter().all(|result| result.failure.is_none()),
            "{report:?}"
        );
    }
}

#[test]
fn parser_keeps_tests_separate_and_preserves_names_and_spans() {
    let source = "test \"same\"=assert true\ntest \"same\"=\n  let value=1\n  assert(value==1)\ntest \"\" = ()";
    let program = parser::parse(source).unwrap();
    assert_eq!(program.tests.len(), 3);
    assert!(program.functions.is_empty());
    assert!(program.entry.is_none());
    assert_eq!(program.tests[0].name, "same");
    assert_eq!(program.tests[1].name, "same");
    assert_eq!(program.tests[2].name, "");
    assert_eq!(
        &source[program.tests[0].name_span.start..program.tests[0].name_span.end],
        "\"same\""
    );
    let formatted =
        tsuzuri::formatter::format_source("Specs.tz", source, SourceKind::Code).unwrap();
    assert_eq!(parser::parse(&formatted.formatted).unwrap().tests.len(), 3);
    for invalid in [
        "let test = 1",
        "test 1 = ()",
        "test \"bad\"",
        "private test \"bad\" = ()",
        "export test \"bad\" = ()",
        "{ test \"bad\" = () }",
        "test \"\\uD800\" = ()",
    ] {
        assert!(parser::parse(invalid).is_err(), "{invalid}");
    }
}
