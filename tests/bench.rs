//! `bench` declarations, `Bench.now`/`Bench.consume`, and `tsuzuri bench` (G18 Phase 1).
//! Times are never compared with thresholds; the tests check only their shape.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TOTAL: &str = "def total :: ref [i64] -> i64\nfn total values =\n    let mut sum = 0\n    let mut index = 0\n    while index < values.length do\n        sum = sum + values[index]\n        index = index + 1\n    sum\n";

fn project(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-bench-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        fs::write(root.join(name), text).unwrap();
    }
    root
}

fn tsuzuri(arguments: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .env("TSUZURI_CACHE_DIR", root.with_extension("cache"))
        .output()
        .unwrap()
}

fn clean(root: &Path) {
    let _ = fs::remove_dir_all(root.with_extension("cache"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn parses_formats_and_indexes_bench_declarations() {
    let source = "bench \"same\"=Bench.of (\\_ -> 1)\nbench \"same\" =\n  Bench.of (\\_ -> 2)\ntest \"t\" = ()\n";
    let format = |source: &str| {
        tsuzuri::formatter::format_source("Benches.tz", source, tsuzuri::syntax::SourceKind::Code)
            .unwrap()
            .formatted
    };
    let program = tsuzuri::parser::parse(source).unwrap();
    assert_eq!(program.benches.len(), 2);
    assert_eq!(program.tests.len(), 1);
    assert!(program.functions.is_empty() && program.entry.is_none());
    assert_eq!(program.benches[0].name, "same");
    assert_eq!(
        &source[program.benches[1].name_span.start..program.benches[1].name_span.end],
        "\"same\""
    );
    // A bench formats exactly as a test with the same name and body.
    let formatted = format(source);
    let as_tests = source.replace("bench ", "test ");
    assert_eq!(formatted.replace("bench ", "test "), format(&as_tests));
    assert_ne!(formatted, source);
    assert_eq!(tsuzuri::parser::parse(&formatted).unwrap().benches.len(), 2);
    let module = tsuzuri::analyze(source).unwrap();
    assert_eq!(
        module
            .benches
            .iter()
            .map(|bench| (bench.index, bench.name.as_str(), bench.module.as_str()))
            .collect::<Vec<_>>(),
        [(0, "same", "Main"), (1, "same", "Main")]
    );
    for bench in &module.benches {
        let function = &module.functions[bench.function];
        assert_eq!(function.origin.bench, Some(bench.index));
        assert_eq!(function.origin.test, None);
        assert_eq!(function.signature.parameters, [tsuzuri::check::Type::I64]);
        assert_eq!(function.signature.result, tsuzuri::check::Type::I64);
    }
    let nested =
        tsuzuri::analyze_modules(&[("Geometry/Specs.tz", "bench \"b\" = \\n -> n")]).unwrap();
    assert_eq!(nested.benches[0].module, "Geometry::Specs");
}

#[test]
fn rejects_bench_identifiers_and_malformed_declarations() {
    for (source, code, message) in [
        (
            "let bench = 1\nbench + 1",
            "E0002",
            "expected an identifier",
        ),
        (
            "bench 1 = \\n -> n",
            "E0002",
            "expected a string literal bench name",
        ),
        (
            "bench \"a\" 1",
            "E0002",
            "expected '=' after the bench name",
        ),
        (
            "bench \"\\uD800\" = \\n -> n",
            "E0002",
            "bench names must contain valid Unicode scalars",
        ),
        ("export bench \"a\" = \\n -> n", "E0002", ""),
        ("private bench \"a\" = \\n -> n", "E1022", ""),
        (
            "bench \"a\" = ()",
            "E1003",
            "expected i64 -> i64, found unit",
        ),
        ("bench \"a\" = \\n -> n == 1", "E1003", ""),
    ] {
        let error = tsuzuri::analyze(source).expect_err(source);
        assert_eq!(error.code, code, "{source}: {}", error.message);
        assert!(
            error.message.contains(message),
            "{source}: {}",
            error.message
        );
    }
    let error = tsuzuri::analyze("bench \"a\" = ()").unwrap_err();
    assert_eq!(error.span.start, "bench \"a\" = ".len());
    // Type class files cannot declare benchmarks, as they cannot declare tests.
    assert_eq!(
        tsuzuri::analyze_modules(&[("Traits.tt", "bench \"bad\" = \\n -> n")])
            .unwrap_err()
            .code,
        "E1018"
    );
    // `bench` was an identifier before G18; other spellings still are.
    tsuzuri::analyze("let benchmark = 1\nlet benches = benchmark + 1\nbenches").unwrap();
}

/// `ir` with the numbers of generated symbols (`$lambda.N`, `$instance.N`, `$mono.N`, ...)
/// renumbered in order of first appearance. Declaring and specializing bench functions shifts
/// those numbers, as declaring tests does, without changing any emitted code.
fn renumbered(ir: &str) -> String {
    let mut numbers = std::collections::BTreeMap::new();
    let mut output = String::with_capacity(ir.len());
    let mut rest = ir;
    while let Some(dollar) = rest.find('$') {
        output.push_str(&rest[..=dollar]);
        rest = &rest[dollar + 1..];
        let word = rest
            .find(|character: char| !character.is_ascii_lowercase() && character != '_')
            .unwrap_or(rest.len());
        let digits = rest[word..].strip_prefix('.').map_or(0, |after| {
            after
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(after.len())
        });
        if digits == 0 {
            continue;
        }
        let prefix = rest[..word].to_owned();
        let number = rest[word + 1..word + 1 + digits].to_owned();
        let count = numbers.len();
        let canonical = *numbers.entry((prefix.clone(), number)).or_insert(count);
        output.push_str(&format!("{prefix}.#{canonical}"));
        rest = &rest[word + 1 + digits..];
    }
    output.push_str(rest);
    output
}

#[test]
fn normal_and_test_emission_exclude_benches() {
    // Generic instances, nested lambdas, and Drop glue keep their numbering when benches use
    // the same functions and types.
    let base = format!(
        "{TOTAL}def id :: 'a -> 'a\nfn id value = value\n\
         record Handle<'a> {{ id: i64, value: 'a }}\ninstance Drop<Handle<'a>> {{ fn drop _value = () }}\n\
         def adder :: i64 -> i64\nfn adder x =\n    let add = \\a -> \\b -> a + b\n    add x x\n\
         export def answer :: i64\nfn answer =\n    let handle = Handle {{ id: 1, value: id 2 }}\n    adder (id 20) + handle.id + 1\n\
         test \"total\" = {{ let values = new [1, 2]; assert (total (ref values) == 3 && id true) }}\n"
    );
    let with_benches = format!(
        "{base}bench \"total\" = Bench.with_input (\\_ -> new [i64](1000, \\i -> id i)) (\\values -> total values)\n\
         bench \"fixed\" = Bench.of (\\_ -> {{ let handle = Handle {{ id: 3, value: id true }}; adder (id 1) + handle.id }})\n"
    );
    let plain = tsuzuri::analyze(&base).unwrap();
    let benched = tsuzuri::analyze(&with_benches).unwrap();
    assert_eq!(benched.benches.len(), 2);
    for wasm in [false, true] {
        let library = |module| {
            tsuzuri::llvm::emit_target(module, tsuzuri::llvm::Entry::Library, wasm).unwrap()
        };
        assert_eq!(renumbered(&library(&plain)), renumbered(&library(&benched)));
        let tests = |module| tsuzuri::llvm::emit_test_runner(module, &[0], wasm).unwrap();
        assert_eq!(renumbered(&tests(&plain)), renumbered(&tests(&benched)));
        for ir in [library(&benched), tests(&benched)] {
            assert!(
                !ir.contains("$bench") && !ir.contains("Handle[bool]"),
                "{ir}"
            );
        }
    }
    let main = |source: &str| format!("{source}answer() + 1\n");
    let console = |source: &str| {
        tsuzuri::llvm::emit(
            &tsuzuri::analyze(source).unwrap(),
            tsuzuri::llvm::Entry::Console,
        )
        .unwrap()
    };
    assert_eq!(
        renumbered(&console(&main(&base))),
        renumbered(&console(&main(&with_benches)))
    );
    // Without instances or lambdas in the program, nothing is renumbered.
    let simple = format!("{TOTAL}export def answer :: i64\nfn answer = 42\n");
    let simple_benched = format!("{simple}bench \"sum\" = \\n -> n * 2\n");
    assert_eq!(
        tsuzuri::llvm::emit(
            &tsuzuri::analyze(&simple).unwrap(),
            tsuzuri::llvm::Entry::Library
        )
        .unwrap(),
        tsuzuri::llvm::emit(
            &tsuzuri::analyze(&simple_benched).unwrap(),
            tsuzuri::llvm::Entry::Library
        )
        .unwrap()
    );
    // The bench runner calls the bench functions directly with the iteration count.
    let runner = tsuzuri::llvm::emit_bench_runner(&benched, &[1]).unwrap();
    assert!(runner.contains("define i32 @tsuzuri_bench_count() {\nentry:\n  ret i32 1\n}"));
    assert!(runner.contains("define i64 @tsuzuri_bench_sample(i32 %index, i64 %iterations) {"));
    assert!(runner.contains("call i64 @tz.fn.Main.$bench.1(i64 %iterations)"));
    assert!(!runner.contains("$bench.0") && !runner.contains("@tsuzuri_test_"));
    assert!(runner.contains("declare i64 @tsuzuri_bench_now()"));
    assert_eq!(
        runner,
        tsuzuri::llvm::emit_bench_runner(&benched, &[1]).unwrap()
    );
    assert_eq!(
        tsuzuri::llvm::emit_bench_runner(&benched, &[2])
            .unwrap_err()
            .code,
        "E2000"
    );
}

#[test]
fn bench_now_outside_bench_runner_is_rejected() {
    let message = "Bench.now runs only under tsuzuri bench";
    let program = tsuzuri::analyze("let start = Bench.now()\nstart - start").unwrap();
    let error = tsuzuri::llvm::emit(&program, tsuzuri::llvm::Entry::Console).unwrap_err();
    assert_eq!(error.code, "E1018");
    assert!(error.message.contains(message), "{}", error.message);
    let library = tsuzuri::analyze("export def now :: i64\nfn now = Bench.now()").unwrap();
    for wasm in [false, true] {
        assert_eq!(
            tsuzuri::llvm::emit_target(&library, tsuzuri::llvm::Entry::Library, wasm)
                .unwrap_err()
                .code,
            "E1018"
        );
    }
    let tests = tsuzuri::analyze("test \"clock\" = assert (Bench.now() >= 0)").unwrap();
    assert_eq!(
        tsuzuri::llvm::emit_test_runner(&tests, &[0], false)
            .unwrap_err()
            .code,
        "E1018"
    );
    // A program that names Bench.now only in a bench declaration builds normally.
    let benches =
        tsuzuri::analyze("bench \"clock\" = \\n -> Bench.now() - Bench.now() + n\n42").unwrap();
    tsuzuri::llvm::emit(&benches, tsuzuri::llvm::Entry::Console).unwrap();
    let root = project(
        "now",
        &[("Main.tz", "let start = Bench.now()\nstart - start\n")],
    );
    let output = tsuzuri(&["build", root.to_str().unwrap()], &root);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E1018") && stderr.contains(message),
        "{stderr}"
    );
    clean(&root);
}

#[test]
fn consume_lowers_to_an_asm_barrier_on_native_and_wasm() {
    let module = tsuzuri::analyze(
        "export def consumed :: i64 -> i64\nfn consumed x =\n    let values = new [x, x + 1]\n    Bench.consume values\n    Bench.consume x\n    x + 1\n",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir.matches("call void asm sideeffect \"\", \"r,~{memory}\"(ptr ")
                .count(),
            2,
            "{ir}"
        );
        assert!(!ir.contains("tsuzuri_bench_now"));
    }
    let root = project(
        "consume",
        &[
            (
                "Main.tz",
                "let values = new [1, 2, 3]\nBench.consume values\nBench.consume 41\n42\n",
            ),
            (
                "Lib.tz",
                "export def consumed :: i64 -> i64\nfn consumed x =\n    let values = new [x, x + 1]\n    Bench.consume values\n    Bench.consume x\n    x + 1\n",
            ),
        ],
    );
    let library = root.join("Lib.tz");
    for (target, emit) in [("native", "object"), ("wasm32", "wasm")] {
        for optimization in ["-O0", "-O3"] {
            let output = root.join(format!("lib-{target}{optimization}"));
            let built = tsuzuri(
                &[
                    "build",
                    library.to_str().unwrap(),
                    "--target",
                    target,
                    "--emit",
                    emit,
                    optimization,
                    "-o",
                    output.to_str().unwrap(),
                ],
                &root,
            );
            assert!(built.status.success(), "{target} {optimization}: {built:?}");
        }
    }
    for optimization in ["-O0", "-O3"] {
        let run = tsuzuri(&["run", root.to_str().unwrap(), optimization], &root);
        assert!(run.status.success(), "{run:?}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), "42\n");
    }
    clean(&root);
}

/// The `type: bench` and summary lines of `tsuzuri bench --json`.
fn json_lines(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn cli_runs_lists_filters_and_reports_failures() {
    let source = format!(
        "{TOTAL}\nbench \"total\" = Bench.with_input (\\_ -> new [i64](100000, \\i -> i)) (\\values -> total values)\n\
         bench \"fixed cost\" = Bench.of (\\_ -> 42)\nbench \"traps\" = \\_ -> {{ assert false; 0 }}\n"
    );
    let root = project("cli", &[("Speed.tz", &source)]);
    let path = root.to_str().unwrap();
    let output = tsuzuri(&["bench", path, "--json", "--samples", "3"], &root);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let lines = json_lines(&output);
    assert_eq!(lines.len(), 4, "{lines:?}");
    for line in &lines[..2] {
        assert_eq!(line["type"], "bench");
        assert_eq!(line["status"], "passed");
        assert_eq!(line["metric"], "wall_time");
        assert_eq!(line["unit"], "ms");
        assert_eq!(line["opt"], "O3");
        assert_eq!(line["target"], "native");
        assert_eq!(line["cpu_mode"], "generic");
        assert_eq!(
            line["workload"],
            format!("Speed.{}", line["name"].as_str().unwrap())
        );
        let iterations = line["iterations"].as_u64().unwrap();
        assert!(
            iterations.is_power_of_two() && iterations <= 1 << 30,
            "{line}"
        );
        let samples: Vec<f64> = line["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sample| sample.as_f64().unwrap())
            .collect();
        assert_eq!(samples.len(), 3);
        let (median, min, max) = (
            line["median"].as_f64().unwrap(),
            line["min"].as_f64().unwrap(),
            line["max"].as_f64().unwrap(),
        );
        assert!(0.0 < min && min <= median && median <= max, "{line}");
        assert!(
            samples
                .iter()
                .all(|sample| min <= *sample && *sample <= max)
        );
    }
    assert_eq!(lines[0]["name"], "total");
    assert_eq!(lines[1]["name"], "fixed cost");
    assert_eq!(lines[2]["status"], "failed");
    assert_eq!(lines[2]["failure"], "trapped or terminated by signal");
    assert_eq!(
        lines[3],
        serde_json::json!({"type": "summary", "benchmarks": 3, "failed": 1, "ignored": 0})
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"code\":\"E2005\"")
            && stderr.contains("benchmark 'Speed.traps' trapped or terminated by signal"),
        "{stderr}"
    );
    let listing = tsuzuri(&["bench", path, "--list"], &root);
    assert!(listing.status.success());
    assert_eq!(
        String::from_utf8_lossy(&listing.stdout),
        "0 Speed.total\n1 Speed.fixed cost\n2 Speed.traps\n"
    );
    let filtered = tsuzuri(
        &["bench", path, "--filter", "total", "--samples", "1", "-O0"],
        &root,
    );
    assert!(filtered.status.success(), "{filtered:?}");
    let text = String::from_utf8_lossy(&filtered.stdout);
    assert!(
        text.starts_with("bench 0 Speed.total: median ") && text.contains("; 1 samples of "),
        "{text}"
    );
    assert!(
        text.ends_with("\n1 benchmark; 0 failed; 2 ignored\n"),
        "{text}"
    );
    let indexed = tsuzuri(&["bench", path, "--index", "9"], &root);
    assert_eq!(indexed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&indexed.stderr).contains("E2000"));
    clean(&root);
}

#[test]
fn cli_rejects_wasm_and_invalid_samples() {
    let root = project(
        "rejects",
        &[("Main.tz", "bench \"b\" = Bench.of (\\_ -> 1)\n")],
    );
    let path = root.to_str().unwrap();
    for (arguments, message) in [
        (
            vec!["bench", path, "--target", "wasm32"],
            "tsuzuri bench supports only the native target",
        ),
        (
            vec!["bench", path, "--samples", "0"],
            "bench samples must be an integer between 1 and 1000",
        ),
        (
            vec!["bench", path, "--samples", "1001"],
            "bench samples must be an integer between 1 and 1000",
        ),
        (
            vec!["bench", path, "--samples", "+3"],
            "bench samples must be an integer between 1 and 1000",
        ),
        (
            vec!["test", path, "--samples", "3"],
            "--samples is only valid with bench",
        ),
        (
            vec!["bench", path, "--coverage", "x.info"],
            "--coverage is only valid with test",
        ),
        (
            vec!["bench", path, "--cpu", "native"],
            "bench does not use CPU tuning",
        ),
        (
            vec!["run", path, "--list"],
            "--filter, --list, and --index are only valid with test or bench",
        ),
    ] {
        let output = tsuzuri(&arguments, &root);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("E2000") && stderr.contains(message),
            "{stderr}"
        );
    }
    let error = tsuzuri::driver::run_benches(
        &tsuzuri::analyze("bench \"b\" = \\n -> n").unwrap(),
        &tsuzuri::driver::BenchOptions {
            samples: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "E2000");
    clean(&root);
}
