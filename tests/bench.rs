//! `bench` declarations, `Bench.now`/`Bench.consume`, and `tsuzuri bench` (G18 Phase 1).
//! Times are never compared with thresholds; the tests check only their shape.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// How a trapped program is reported: `llvm.trap` raises a signal on Unix, and on Windows it ends the process
/// with STATUS_ILLEGAL_INSTRUCTION (0xC000001D, or -1073741795 as the signed exit code).
const TRAPPED: &str = if cfg!(windows) {
    "trapped or exited with code -1073741795"
} else {
    "trapped or terminated by signal"
};

const TOTAL: &str = "def total :: ref [i64] -> i64\nfn total values =\n    let mut sum = 0\n    let mut index = 0\n    while index < values.length do\n        sum = sum + values[index]\n        index = index + 1\n    sum\n";
const ALLOCATION_BENCHES: &str = r#"
def push_five :: unit -> i64
fn push_five _unit =
    let mut v: Vec<i64> = Vec.empty()
    for i in 1i64 .. 5i64 do v = Vec.push_owned i v
    Vec.length (ref v)
def sum_all :: ref [i64] -> i64
fn sum_all values = Array.sum values
bench "nothing allocated" = Bench.of (\_ -> 42)
bench "one array of ten" = Bench.of (\_ -> new [i64](10, \i -> i))
bench "map over a shared input" = Bench.with_input (\_ -> new [i64](10, \i -> i)) (\values -> Array.map (\x -> x + 1) values)
bench "sum over a shared input" = Bench.with_input (\_ -> new [i64](100, \i -> i)) (\values -> sum_all values)
bench "five pushes" = Bench.of (\_ -> push_five ())
"#;

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
fn bench_allocation_runner_ir_has_the_counts_function_and_the_default_runner_is_unchanged() {
    use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry, F16Path};
    let module = tsuzuri::analyze(ALLOCATION_BENCHES).unwrap();
    let counted = llvm::emit_with_options(
        &module,
        EmitOptions {
            entry: Entry::BenchRunner,
            wasm: false,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: Allocator::Counting,
            f16: F16Path::Soft,
        },
    )
    .unwrap();
    assert!(counted.contains("define void @tsuzuri_bench_counts(ptr %out)"));
    assert!(counted.contains(include_str!("../src/runtime/heap-counting.ll")));
    assert_eq!(
        counted,
        llvm::emit_bench_allocation_runner(&module, &[0, 1, 2, 3, 4], F16Path::Soft).unwrap()
    );
    let counters =
        tsuzuri::analyze("bench \"reads\" = \\n -> (Alloc.snapshot ()).allocs + n").unwrap();
    llvm::emit_bench_allocation_runner(&counters, &[0], F16Path::Soft).unwrap();
    let timed = llvm::emit_bench_runner(&module, &[0, 1, 2, 3, 4]).unwrap();
    assert!(!timed.contains("tsuzuri_bench_counts") && !timed.contains("tz.heap."));
}

#[test]
fn bench_allocation_runner_counts_through_the_driver() {
    let module = tsuzuri::analyze(ALLOCATION_BENCHES).unwrap();
    let report = tsuzuri::driver::run_benches(
        &module,
        &tsuzuri::driver::BenchOptions {
            allocs: true,
            samples: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let counts: Vec<_> = report
        .results
        .iter()
        .map(|result| {
            assert!(result.failure.is_none(), "{result:?}");
            (result.alloc_calls.unwrap(), result.alloc_bytes.unwrap())
        })
        .collect();
    assert_eq!(counts, [(0, 0), (1, 80), (1, 80), (0, 0), (2, 96)]);
}

#[test]
fn bench_allocs_reports_deterministic_per_iteration_counts() {
    let root = project("alloc-counts", &[("Main.tz", ALLOCATION_BENCHES)]);
    let mut previous = None;
    for _ in 0..2 {
        let output = tsuzuri(
            &[
                "bench",
                root.to_str().unwrap(),
                "--allocs",
                "--json",
                "--samples",
                "3",
            ],
            &root,
        );
        assert!(output.status.success(), "{output:?}");
        let rows = json_lines(&output);
        let counts: Vec<_> = rows
            .iter()
            .filter(|row| row["type"] == "bench")
            .map(|row| {
                assert_eq!(row["status"], "passed");
                (
                    row["alloc_calls"].as_u64().unwrap(),
                    row["alloc_bytes"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(counts, [(0, 0), (1, 80), (1, 80), (0, 0), (2, 96)]);
        if let Some(previous) = previous.as_ref() {
            assert_eq!(&counts, previous);
        }
        previous = Some(counts);
        assert!(rows.last().unwrap().get("alloc_calls").is_none());
    }
    let output = tsuzuri(
        &[
            "bench",
            root.to_str().unwrap(),
            "--allocs",
            "--samples",
            "1",
        ],
        &root,
    );
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains(", alloc_calls 1, alloc_bytes 80 per iteration")
    );
    clean(&root);
}

#[test]
fn bench_allocs_is_opt_in_and_keeps_the_json_keys() {
    let root = project("alloc-opt-in", &[("Main.tz", ALLOCATION_BENCHES)]);
    let output = tsuzuri(
        &["bench", root.to_str().unwrap(), "--json", "--samples", "1"],
        &root,
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("alloc_"));
    for line in stdout
        .lines()
        .filter(|line| line.contains("\"type\":\"bench\""))
    {
        let positions: Vec<_> = [
            "type",
            "index",
            "module",
            "name",
            "status",
            "workload",
            "target",
            "opt",
            "cpu_mode",
            "metric",
            "unit",
            "iterations",
            "samples",
            "median",
            "min",
            "max",
        ]
        .iter()
        .map(|key| line.find(&format!("\"{key}\":")).unwrap())
        .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{line}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            positions.len()
        );
    }
    let output = tsuzuri(&["bench", root.to_str().unwrap(), "--samples", "1"], &root);
    assert!(output.status.success(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("alloc_calls"));
    clean(&root);
}

#[test]
fn bench_allocs_rejects_other_commands_and_reports_failures() {
    let root = project(
        "alloc-failures",
        &[("Main.tz", "bench \"trap\" = Bench.of (\\_ -> assert false)")],
    );
    for action in ["run", "test", "check", "build"] {
        let output = tsuzuri(&[action, root.to_str().unwrap(), "--allocs"], &root);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--allocs is only valid with bench")
        );
    }
    let output = tsuzuri(
        &[
            "bench",
            root.to_str().unwrap(),
            "--allocs",
            "--json",
            "--samples",
            "1",
        ],
        &root,
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let rows = json_lines(&output);
    assert_eq!(rows[0]["status"], "failed");
    assert!(rows[0].get("alloc_calls").is_none());
    fs::write(
        root.join("Main.tz"),
        r#"
bench "negative allocation delta" = \n ->
    if n == 1 then
        let xs = new [i64](10, \i -> i)
        Bench.consume (ref xs)
    else ()
    0
"#,
    )
    .unwrap();
    let output = tsuzuri(
        &[
            "bench",
            root.to_str().unwrap(),
            "--allocs",
            "--json",
            "--samples",
            "1",
        ],
        &root,
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let rows = json_lines(&output);
    assert_eq!(rows[0]["status"], "failed");
    assert!(
        rows[0]["failure"]
            .as_str()
            .unwrap()
            .contains("allocation run: returned a negative allocation count")
    );
    assert!(rows[0].get("alloc_calls").is_none());
    clean(&root);
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

/// `ir` with the numbers of generated symbols (`$lambda.N`, `$instance.N`, `$mono.N`,
/// `$intrinsic.Display.display.N`, ...) renumbered in order of first appearance. Declaring and
/// specializing bench functions shifts those numbers, as declaring tests does, without changing
/// any emitted code.
fn renumbered(ir: &str) -> String {
    let mut numbers = std::collections::BTreeMap::new();
    let mut output = String::with_capacity(ir.len());
    let mut rest = ir;
    while let Some(dollar) = rest.find('$') {
        output.push_str(&rest[..=dollar]);
        rest = &rest[dollar + 1..];
        // `word(.word)*.digits`: the words name the kind of symbol, the digits number it.
        let mut end = 0;
        let digits = loop {
            let word = rest[end..]
                .find(|character: char| !character.is_ascii_alphabetic() && character != '_')
                .unwrap_or(rest.len() - end);
            if word == 0 {
                break 0;
            }
            end += word;
            let Some(after) = rest[end..].strip_prefix('.') else {
                break 0;
            };
            let digits = after
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(after.len());
            if digits > 0 {
                break digits;
            }
            end += 1;
        };
        if digits == 0 {
            continue;
        }
        let kind = &rest[..end];
        let number = &rest[end + 1..end + 1 + digits];
        let count = numbers.len();
        let canonical = *numbers
            .entry((kind.to_owned(), number.to_owned()))
            .or_insert(count);
        output.push_str(&format!("{kind}.#{canonical}"));
        rest = &rest[end + 1 + digits..];
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

const MEASURE: &str = "def measure :: i64 -> i64\nfn measure n =\n    let start = Bench.now()\n    let mut index = 0\n    while index < n do\n        Bench.consume index\n        index = index + 1\n    Bench.now() - start\n";
const TIMED: &str = "def timed :: i64 -> i64\nfn timed n = Bench.of (\\() -> 1) n\n";
const ADVICE: &str =
    "; call it only from bench declarations and from functions that only benchmarks use";

#[test]
fn public_bench_helpers_stay_out_of_other_builds() {
    // `measure`, `timed` (through `Bench.of`), and `report` (through both) are public, so every
    // build used to root them and reject the program with E1018 though only benches call them.
    let program = format!(
        "{MEASURE}{TIMED}def report :: i64 -> i64\nfn report n = measure n + timed n\n\
         bench \"measure\" = measure\nbench \"report\" = report\nDebug.print \"hi\"\n"
    );
    // With private helpers, which builds have never rooted, the program built before G18 D3's
    // revision; the analysis is the same, so the output is the same byte for byte.
    let private = program.replace("def ", "private def ");
    let plain = "Debug.print \"hi\"\n";
    let console = |source: &str, wasm| {
        tsuzuri::llvm::emit_target(
            &tsuzuri::analyze(source).unwrap(),
            tsuzuri::llvm::Entry::Console,
            wasm,
        )
        .unwrap()
    };
    for wasm in [false, true] {
        let ir = console(&program, wasm);
        for name in ["tsuzuri_bench", "$bench", "measure", "timed", "report"] {
            assert!(!ir.contains(name), "{name}: {ir}");
        }
        assert_eq!(ir, console(&private, wasm));
        // Only the numbers of generated symbols differ from the program without the helpers and
        // benches, as their analysis and that of the std `Bench` module add instances.
        assert_eq!(renumbered(&ir), renumbered(&console(plain, wasm)));
    }
    let root = project("helpers", &[("Main.tz", program.as_str())]);
    let run = tsuzuri(&["run", root.to_str().unwrap()], &root);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stderr), "hi\n");
    let bench = tsuzuri(&["bench", root.to_str().unwrap(), "--samples", "1"], &root);
    assert!(bench.status.success(), "{bench:?}");
    assert!(
        String::from_utf8_lossy(&bench.stdout).ends_with("\n2 benchmarks; 0 failed; 0 ignored\n")
    );
    clean(&root);
}

#[test]
fn clock_reached_from_entry_export_test_or_drop_is_rejected_by_name() {
    // The error points at the use that leads to the clock in the last user function on the way.
    let rejected =
        |source: &str, error: tsuzuri::diagnostic::Diagnostic, text: &str, message: &str| {
            assert_eq!(error.code, "E1018");
            assert_eq!(
                error.message,
                format!("Bench.now runs only under tsuzuri bench, but {message}{ADVICE}")
            );
            assert_eq!(&source[error.span.start..error.span.end], text, "{error:?}");
        };
    let entry = format!("{MEASURE}bench \"m\" = measure\nDebug.print (measure 5)\n");
    let module = tsuzuri::analyze(&entry).unwrap();
    rejected(
        &entry,
        tsuzuri::llvm::emit(&module, tsuzuri::llvm::Entry::Console).unwrap_err(),
        "Bench.now",
        "the top-level code reaches it through `Main.measure`",
    );
    let top = "let start = Bench.now()\nstart - start\n";
    rejected(
        top,
        tsuzuri::llvm::emit(
            &tsuzuri::analyze(top).unwrap(),
            tsuzuri::llvm::Entry::Console,
        )
        .unwrap_err(),
        "Bench.now",
        "the top-level code reads it",
    );
    let export = format!("{TIMED}export def run :: i64\nfn run = timed 3\nbench \"t\" = timed\n");
    let module = tsuzuri::analyze(&export).unwrap();
    for wasm in [false, true] {
        rejected(
            &export,
            tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap_err(),
            "Bench.of",
            "export `Main.run` reaches it through `Main.timed`",
        );
    }
    let tests =
        format!("{MEASURE}test \"t\" = assert (measure 1 >= 0)\ntest \"plain\" = assert true\n");
    let module = tsuzuri::analyze(&tests).unwrap();
    for wasm in [false, true] {
        rejected(
            &tests,
            tsuzuri::llvm::emit_test_runner(&module, &[0], wasm).unwrap_err(),
            "Bench.now",
            "test \"t\" reaches it through `Main.measure`",
        );
        // A test that does not reach the clock runs.
        let ir = tsuzuri::llvm::emit_test_runner(&module, &[1], wasm).unwrap();
        assert!(
            !ir.contains("tsuzuri_bench") && !ir.contains("measure"),
            "{ir}"
        );
    }
    // Drop glue runs wherever a value is dropped, so a Drop instance that reads the clock is
    // part of every build.
    let drop = "record Resource { id: i64 }\ninstance Drop<Resource> { fn drop value = Bench.consume (Bench.now() + value.id) }\nDebug.print \"hi\"\n";
    rejected(
        drop,
        tsuzuri::llvm::emit(
            &tsuzuri::analyze(drop).unwrap(),
            tsuzuri::llvm::Entry::Console,
        )
        .unwrap_err(),
        "Bench.now",
        "the Drop instance of `Main.Resource` reads it",
    );
    let root = project("named", &[("Main.tz", entry.as_str())]);
    let output = tsuzuri(&["run", root.to_str().unwrap()], &root);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "Main.tz:3:17: error[E1018]: Bench.now runs only under tsuzuri bench, but the top-level code reaches it through `Main.measure`{ADVICE}"
        )),
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
    assert_eq!(lines[2]["failure"], TRAPPED);
    assert_eq!(
        lines[3],
        serde_json::json!({"type": "summary", "benchmarks": 3, "failed": 1, "ignored": 0})
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"code\":\"E2005\"")
            && stderr.contains(&format!("benchmark 'Speed.traps' {TRAPPED}")),
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
