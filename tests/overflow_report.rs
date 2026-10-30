//! A23 Phase 2: the integer overflow report. These tests emit IR inside an `overflow::Scope`
//! (as the driver does for `--overflow-report`) and read the sites that the emission numbered.

use tsuzuri::overflow::{self, Operation, Site, Source};
use tsuzuri::{analyze, analyze_modules, llvm};

/// Emits `source` as the reported `./Main.tz` (the standard library is not in the scope).
fn emit(source: &str, wasm: bool) -> (String, Vec<Site>) {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let scope = overflow::Scope::enter(vec![Source {
        path: "./Main.tz".into(),
        text: source.into(),
        reported: true,
    }]);
    let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    let sites = scope.sites();
    assert!(
        llvm::duplicate_declarations(&ir).is_empty(),
        "{:?}",
        llvm::duplicate_declarations(&ir)
    );
    (ir, sites)
}

fn line_column(source: &str, site: &Site) -> (usize, usize) {
    let prefix = &source[..site.start];
    let line = prefix.matches('\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap().chars().count() + 1;
    (line, column)
}

/// The fixture of `tests/fixtures/overflow_report` (appendix E.1 of the ticket).
const FIXTURE: &str = include_str!("fixtures/overflow_report/Main.tz");

#[test]
fn unproven_arithmetic_is_instrumented_and_proven_arithmetic_is_not() {
    let source = "export def add32 :: i32 -> i32 -> i32 = \\a b -> a + b\n\
                  export def masked :: i32 -> i32 = \\a -> (a &&& 255) + 1\n\
                  export def guarded :: i32 -> i32 = \\a -> if a < 1000 && a > 0 then a + 1 else 0\n";
    for wasm in [false, true] {
        let (ir, sites) = emit(source, wasm);
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert_eq!(line_column(source, &sites[0]), (1, 49));
        assert_eq!(
            (sites[0].operation, sites[0].bits, sites[0].signed),
            (Operation::Add, 32, true)
        );
        assert_eq!(
            ir.matches("declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)")
                .count(),
            1
        );
        assert!(ir.contains("@tsuzuri_overflow_hits = internal global [1 x i8] zeroinitializer"));
        // The operation itself stays, without flags.
        assert!(!ir.contains(" nsw ") && !ir.contains(" nuw "));
    }
}

#[test]
fn each_operation_width_and_sign_uses_the_intrinsic_of_its_sense() {
    for bits in [8, 16, 32, 64, 128] {
        for signed in [true, false] {
            let ty = format!("i{bits}{}", if signed { "" } else { "u" });
            let sense = if signed { "s" } else { "u" };
            // The exports convert from `i64`, which every width can be exported as.
            let mut source = String::new();
            let mut operations = vec![("add", "a + b"), ("sub", "a - b"), ("mul", "a * b")];
            operations.push(("pow", "a ** b"));
            if signed {
                operations.push(("neg", "-a"));
            }
            for (name, body) in operations {
                source.push_str(&format!(
                    "def {name} :: {ty} -> {ty} -> {ty} = \\a b -> {body}\n\
                     export def run_{name} :: i64 -> i64 -> i64 = \\a b -> ({name} (a as {ty}) (b as {ty})) as i64\n"
                ));
            }
            for wasm in [false, true] {
                let (ir, sites) = emit(&source, wasm);
                let expected = if signed { 5 } else { 4 };
                assert_eq!(sites.len(), expected, "{ty}: {sites:?}");
                for operation in ["add", "sub"] {
                    let declaration = format!(
                        "declare {{ i{bits}, i1 }} @llvm.{sense}{operation}.with.overflow.i{bits}(i{bits}, i{bits})"
                    );
                    assert_eq!(ir.matches(&declaration).count(), 1, "{ty}: {declaration}");
                }
                let multiply = format!("@llvm.{sense}mul.with.overflow.i{bits}");
                // `i128` products have no intrinsic: `checked_integer_arithmetic` writes them out.
                assert_eq!(ir.contains(&multiply), bits != 128, "{ty}: {multiply}");
                let operations: Vec<_> = sites.iter().map(|site| site.operation).collect();
                assert!(operations.contains(&Operation::Power), "{ty}");
                assert_eq!(operations.contains(&Operation::Negate), signed, "{ty}");
                assert!(
                    sites
                        .iter()
                        .all(|site| site.bits == bits && site.signed == signed)
                );
            }
        }
    }
}

#[test]
fn sites_are_shared_by_span_operation_and_type() {
    let source = "def twice :: Add<'a> => 'a -> 'a = \\x -> x + x\n\
                  export def a :: i32 -> i32 = \\x -> twice x\n\
                  export def b :: i32 -> i32 = \\x -> twice (x + 1)\n\
                  export def c :: i64 -> i64 = \\x -> twice x\n";
    for wasm in [false, true] {
        let (_, sites) = emit(source, wasm);
        let start = source.find("x + x").unwrap();
        let shared: Vec<_> = sites.iter().filter(|site| site.start == start).collect();
        assert_eq!(shared.len(), 2, "{sites:?}");
        assert_ne!(shared[0].bits, shared[1].bits);
        // `x + 1` of `b` is a site of its own.
        assert_eq!(sites.len(), 3, "{sites:?}");
    }
}

#[test]
fn tail_call_arguments_are_sites_at_their_own_span() {
    for wasm in [false, true] {
        let (_, sites) = emit(FIXTURE, wasm);
        let tail: Vec<_> = sites
            .iter()
            .filter(|site| line_column(FIXTURE, site).0 == 8)
            .map(|site| {
                (
                    line_column(FIXTURE, site),
                    &FIXTURE[site.start..site.end],
                    site.operation,
                )
            })
            .collect();
        assert_eq!(
            tail,
            [
                ((8, 92), "(n - 1)", Operation::Subtract),
                ((8, 100), "(total + 1000000000)", Operation::Add),
            ],
            "wasm: {wasm}"
        );
        let mut all: Vec<_> = sites
            .iter()
            .map(|site| line_column(FIXTURE, site))
            .collect();
        all.sort();
        assert_eq!(
            all,
            [
                (1, 49),
                (2, 53),
                (3, 45),
                (4, 40),
                (5, 49),
                (6, 36),
                (8, 92),
                (8, 100)
            ]
        );
    }
}

#[test]
fn checked_wrapping_saturating_and_shift_forms_are_not_sites() {
    let source = "def safe :: i32 -> i32 -> Result<i32, 'E> = \\x y ->\n\
                  \x20   try\n\
                  \x20       @checked x + y\n\
                  \x20   with\n\
                  \x20   | e is OverflowException -> e\n\
                  export def checked :: i32 -> i32 -> i32 = \\a b ->\n\
                  \x20   match safe a b with\n\
                  \x20   | Result.Ok value -> value\n\
                  \x20   | Result.Error _ -> 0\n\
                  export def wrapping :: i32 -> i32 -> i32 = \\a b -> Int.wrapping_add (Int.wrapping_mul a b) (Int.wrapping_neg a)\n\
                  export def maybe :: i32 -> i32 -> bool = \\a b -> Maybe.is_some (Int.checked_add a b)\n\
                  export def saturating :: i32 -> i32 -> i32 = \\a b -> Int.saturating_add a b\n\
                  export def shifted :: i32 -> i32 = \\a -> (a <<< 1) ^^^ (a >>> 3)\n\
                  export def narrowed :: i32 -> i8 = \\a -> a as i8\n";
    for wasm in [false, true] {
        let (ir, sites) = emit(source, wasm);
        assert!(sites.is_empty(), "{sites:?}");
        assert!(ir.contains("@tsuzuri_overflow_hits = internal global [0 x i8] zeroinitializer"));
    }
}

#[test]
fn std_dependency_and_generated_code_are_not_reported() {
    let main = "export def total :: i64 -> i64 = \\n -> Array.sum [n, n, n]\n\
                export def joined :: i32 -> i32 -> i32 = \\a b -> Other.plus a b\n";
    let other = "def plus :: i32 -> i32 -> i32 = \\a b -> a + b\n";
    let module = analyze_modules(&[("Main", main), ("Other", other)]).unwrap();
    for wasm in [false, true] {
        let scope = overflow::Scope::enter(vec![
            Source {
                path: "./Main.tz".into(),
                text: main.into(),
                reported: true,
            },
            Source {
                path: "./Other.tz".into(),
                text: other.into(),
                reported: false,
            },
        ]);
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(scope.sites().is_empty(), "{:?}", scope.sites());
        assert!(ir.contains("[0 x i8]"));
    }
    // A function whose span has no source, or a source outside the scope, is not reported either.
    let (_, sites) = emit("export def f :: i32 -> i32 = \\a -> a * 3\n", false);
    assert_eq!(sites.len(), 1);
    let module = analyze("export def f :: i32 -> i32 = \\a -> a * 3\n").unwrap();
    let scope = overflow::Scope::enter(Vec::new());
    llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
    assert!(scope.sites().is_empty());
}

#[test]
fn ir_without_a_scope_is_unchanged() {
    let module = analyze(FIXTURE).unwrap();
    for wasm in [false, true] {
        let before = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        {
            let _scope = overflow::Scope::enter(vec![Source {
                path: "./Main.tz".into(),
                text: FIXTURE.into(),
                reported: true,
            }]);
            let instrumented = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_ne!(instrumented, before);
        }
        let after = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(after, before);
        assert!(!after.contains("tsuzuri_overflow") && !after.contains("tz.overflow"));
    }
}

#[test]
fn emission_with_a_scope_is_deterministic() {
    for wasm in [false, true] {
        let first = emit(FIXTURE, wasm);
        let second = emit(FIXTURE, wasm);
        assert_eq!(first, second);
        assert_eq!(first.1.len(), 8);
    }
}

#[test]
fn native_ir_reports_through_write_and_wasm_ir_exports_the_hits() {
    let (native, sites) = emit(FIXTURE, false);
    assert!(native.contains(
        "define internal void @tz.overflow.report(ptr %message, i64 %length) cold nounwind"
    ));
    assert_eq!(
        native.matches("declare i64 @write(i32, ptr, i64)").count(),
        1
    );
    assert!(native.contains("getelementptr inbounds i8, ptr @tsuzuri_overflow_hits, i64 0\n"));
    assert!(native.contains(", i8 1 monotonic\n") && native.contains("= atomicrmw xchg ptr %v"));
    assert_eq!(
        native.matches("declare i1 @llvm.expect.i1(i1, i1)").count(),
        1
    );
    assert!(!native.contains("@tsuzuri_overflow_count"));
    for (index, site) in sites.iter().enumerate() {
        let source = Source {
            path: "./Main.tz".into(),
            text: FIXTURE.into(),
            reported: true,
        };
        let line = format!("warning: {}\n", site.message(&source));
        let bytes: String = line.bytes().map(|byte| format!("\\{byte:02X}")).collect();
        assert!(
            native.contains(&format!(
                "@tz.overflow.message.{index} = private unnamed_addr constant [{} x i8] c\"{bytes}\"",
                line.len()
            )),
            "{line}"
        );
        assert!(native.contains(&format!(
            "call void @tz.overflow.report(ptr @tz.overflow.message.{index}, i64 {})",
            line.len()
        )));
    }
    let (wasm, _) = emit(FIXTURE, true);
    assert!(
        wasm.contains("define i32 @tsuzuri_overflow_count() nounwind {\nentry:\n  ret i32 8\n}")
    );
    assert!(wasm.contains("define i32 @tsuzuri_overflow_hit(i32 %index) nounwind"));
    assert!(wasm.contains("getelementptr inbounds i8, ptr @tsuzuri_overflow_hits, i64 0\n"));
    assert!(wasm.contains("store atomic i8 1, ptr %v") && wasm.contains(" monotonic, align 1\n"));
    assert!(!wasm.contains("@write("));
    assert!(!wasm.contains("atomicrmw"));
    assert!(!wasm.contains("@tz.overflow.report"));
}

#[test]
fn the_message_names_the_line_column_type_and_operation() {
    let (_, sites) = emit(FIXTURE, false);
    let source = Source {
        path: "./Main.tz".into(),
        text: FIXTURE.into(),
        reported: true,
    };
    let mut messages: Vec<_> = sites.iter().map(|site| site.message(&source)).collect();
    messages.sort();
    assert_eq!(
        messages,
        [
            "integer overflow wrapped at ./Main.tz:1:51 (i32 add)",
            "integer overflow wrapped at ./Main.tz:2:55 (i32u sub)",
            "integer overflow wrapped at ./Main.tz:3:47 (i8 mul)",
            "integer overflow wrapped at ./Main.tz:4:40 (i64 neg)",
            "integer overflow wrapped at ./Main.tz:5:51 (i16 pow)",
            "integer overflow wrapped at ./Main.tz:6:47 (i128 add)",
            "integer overflow wrapped at ./Main.tz:8:107 (i32 add)",
            "integer overflow wrapped at ./Main.tz:8:95 (i32 sub)",
        ]
    );
    // The column counts characters: a comment in Japanese before the operation.
    let source = "// 足し算の例\nexport def add :: i32 -> i32 -> i32 = \\a b -> if \"日本\" == \"日本\" then a + b else 0\n";
    let (_, sites) = emit(source, false);
    let text = Source {
        path: "./Main.tz".into(),
        text: source.into(),
        reported: true,
    };
    assert_eq!(
        sites[0].message(&text),
        "integer overflow wrapped at ./Main.tz:2:70 (i32 add)"
    );
}

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-overflow-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn tsuzuri(arguments: &[&str], directory: &std::path::Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        // No build cache notices (`W2001`) in the output; `test` has no cache to turn off.
        .args(matches!(arguments[0], "build" | "run").then_some("--no-cache"))
        .current_dir(directory)
        .output()
        .unwrap()
}

/// The path that the CLI shows for `Main.tz` of the project `.`: `./Main.tz`, or `.\Main.tz` on
/// Windows, which keeps native paths.
fn cli_path() -> String {
    std::path::Path::new(".")
        .join("Main.tz")
        .display()
        .to_string()
}

/// `text` inside a JSON string.
fn json_text(text: &str) -> String {
    text.replace('\\', "\\\\")
}

/// A program whose `mix` wraps once, and which writes a line of its own to the standard error.
const RUN_PROGRAM: &str = "def mix :: i32u -> i32u -> i32u = \\a b -> a + b\n\
                           def main :: unit -> i32 = \\_ ->\n\
                           \x20   Debug.print (to_string (mix 3000000000i32u 3000000000i32u))\n\
                           \x20   0\n";

#[test]
fn run_json_turns_report_lines_into_warning_objects() {
    let root = scratch("run-json");
    std::fs::write(root.join("Main.tz"), RUN_PROGRAM).unwrap();
    let output = tsuzuri(&["run", ".", "--overflow-report", "--json"], &root);
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    let start = RUN_PROGRAM.find("a + b").unwrap();
    let path = json_text(&cli_path());
    let expected = format!(
        "{{\"severity\":\"warning\",\"code\":\"W2001\",\"message\":\"integer overflow wrapped at {path}:1:45 (i32u add)\",\"path\":\"{path}\",\"span\":{{\"start\":{start},\"end\":{},\"line\":1,\"column\":43,\"end_line\":1,\"end_column\":48}},\"kind\":\"integer_overflow_wrapped\"}}",
        start + 5
    );
    let lines: Vec<_> = stderr.lines().collect();
    assert!(lines.contains(&expected.as_str()), "{stderr}");
    // The program's own line stays as it was.
    assert!(lines.contains(&"1705032704"), "{stderr}");
    assert!(!stderr.contains("warning: integer overflow"), "{stderr}");
    // Without the report, nothing is instrumented and nothing is reported.
    let plain = tsuzuri(&["run", ".", "--json"], &root);
    assert!(plain.status.success());
    assert!(!String::from_utf8(plain.stderr).unwrap().contains("W2001"));
    let _ = std::fs::remove_dir_all(&root);
}

/// Review 3 of #67: output that did not end its line comes before a report on the same line;
/// `run --json` keeps the output on a line of its own and turns the report into a warning.
#[test]
fn run_json_finds_a_report_after_unterminated_output() {
    let program = "def mix :: i32u -> i32u -> i32u = \\a b -> a + b\n\
                   def main :: unit -> i32 = \\_ ->\n\
                   \x20   do! IO.write_error \"prefix\"\n\
                   \x20   Debug.print (to_string (mix 3000000000i32u 3000000000i32u))\n\
                   \x20   0\n";
    let root = scratch("run-json-prefix");
    std::fs::write(root.join("Main.tz"), program).unwrap();
    let output = tsuzuri(&["run", ".", "--overflow-report", "--json"], &root);
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    let path = json_text(&cli_path());
    assert!(
        stderr.contains(&format!(
            "prefix\n{{\"severity\":\"warning\",\"code\":\"W2001\",\"message\":\"integer overflow wrapped at {path}:1:45 (i32u add)\",\"path\":\"{path}\""
        )),
        "{stderr}"
    );
    assert!(stderr.ends_with("}\n1705032704\n"), "{stderr}");
    assert!(!stderr.contains("warning: integer overflow"), "{stderr}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn run_without_json_relays_the_report_unchanged() {
    let root = scratch("run-text");
    std::fs::write(root.join("Main.tz"), RUN_PROGRAM).unwrap();
    let output = tsuzuri(&["run", ".", "--overflow-report"], &root);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "warning: integer overflow wrapped at {}:1:45 (i32u add)\n1705032704\n",
            cli_path()
        )
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Two tests: the second adds `127i8 + 1i8` through a function's arguments, which wraps.
const WRAPPING_TESTS: &str = "def bump :: i8 -> i8 -> i8 = \\a b -> a + b\n\
                              \n\
                              test \"adds\" =\n\
                              \x20   assert (bump 1i8 2i8 == 3i8)\n\
                              \n\
                              test \"wraps\" =\n\
                              \x20   assert (bump 127i8 1i8 == -128i8)\n";

fn overflow_tests(source: &str, fail: bool) -> tsuzuri::driver::TestOptions {
    tsuzuri::driver::TestOptions {
        overflow: Some(tsuzuri::driver::OverflowTests {
            fail,
            sources: vec![Source {
                path: "./Main.tz".into(),
                text: source.into(),
                reported: true,
            }],
        }),
        ..tsuzuri::driver::TestOptions::default()
    }
}

/// The report of `bump`'s `+` as the library API, given `./Main.tz`, shows it.
const BUMP_SITE: &str = "integer overflow wrapped at ./Main.tz:1:40 (i8 add)";

/// The same report as the CLI shows it.
fn cli_bump() -> String {
    format!("integer overflow wrapped at {}:1:40 (i8 add)", cli_path())
}

#[test]
fn test_fails_a_test_that_wraps_and_names_the_site() {
    let root = scratch("test-fails");
    std::fs::write(root.join("Main.tz"), WRAPPING_TESTS).unwrap();
    let output = tsuzuri(&["test", "."], &root);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ok 1 - Main adds\n"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "not ok 2 - Main wraps\n  failure: {}\n",
            cli_bump()
        )),
        "{stdout}"
    );
    assert!(stdout.contains("1 passed; 1 failed; 0 ignored"), "{stdout}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn allow_overflow_keeps_the_test_passing_and_reports_it() {
    let root = scratch("test-allow");
    std::fs::write(root.join("Main.tz"), WRAPPING_TESTS).unwrap();
    let output = tsuzuri(&["test", ".", "--allow-overflow"], &root);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("ok 2 - Main wraps\n  warning: {}\n", cli_bump())),
        "{stdout}"
    );
    assert!(stdout.contains("2 passed; 0 failed; 0 ignored"), "{stdout}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn library_run_tests_defaults_to_no_report() {
    let module = analyze(WRAPPING_TESTS).unwrap();
    let report =
        tsuzuri::driver::run_tests(&module, &tsuzuri::driver::TestOptions::default()).unwrap();
    assert_eq!(report.results.len(), 2);
    for result in &report.results {
        assert_eq!(result.failure, None, "{}", result.case.name);
        assert!(result.overflow.is_empty());
    }
    // The same tests with the report: the wrapping one fails with its site.
    let report =
        tsuzuri::driver::run_tests(&module, &overflow_tests(WRAPPING_TESTS, true)).unwrap();
    assert_eq!(report.results[0].failure, None);
    assert_eq!(report.results[1].failure.as_deref(), Some(BUMP_SITE));
    assert_eq!(report.results[1].overflow, [BUMP_SITE]);
    // `fail: false` keeps it passing and keeps the report.
    let report =
        tsuzuri::driver::run_tests(&module, &overflow_tests(WRAPPING_TESTS, false)).unwrap();
    assert_eq!(report.results[1].failure, None);
    assert_eq!(report.results[1].overflow, [BUMP_SITE]);
    assert!(report.results[1].output.is_empty());
}

#[test]
fn a_failing_test_keeps_its_own_failure_reason() {
    let source = "def bump :: i8 -> i8 -> i8 = \\a b -> a + b\n\
                  \n\
                  test \"wraps and fails\" =\n\
                  \x20   assert (bump 127i8 1i8 == 0i8)\n";
    let module = analyze(source).unwrap();
    // Review of #67: a WebAssembly test that traps still reports what wrapped before.
    for target in [
        tsuzuri::driver::Target::Native,
        tsuzuri::driver::Target::Wasm32,
    ] {
        let options = tsuzuri::driver::TestOptions {
            target,
            ..overflow_tests(source, true)
        };
        let report = tsuzuri::driver::run_tests(&module, &options).unwrap();
        let result = &report.results[0];
        let failure = result.failure.as_deref().unwrap();
        assert!(
            !failure.contains("integer overflow"),
            "{target:?}: {failure}"
        );
        assert_eq!(result.overflow, [BUMP_SITE], "{target:?}");
    }
}

#[test]
fn property_tests_report_overflow_inside_the_property() {
    let wrapping = "test \"pair commutes\" = Gen.for_all (Gen.pair (Gen.i32()) (Gen.i32())) (\\pair -> match pair with | (a, b) -> a + b == b + a)\n";
    let module = analyze(wrapping).unwrap();
    let report = tsuzuri::driver::run_tests(&module, &overflow_tests(wrapping, true)).unwrap();
    let failure = report.results[0].failure.as_deref().unwrap();
    assert!(
        failure.starts_with("integer overflow wrapped at ./Main.tz:1:")
            && failure.ends_with("(i32 add)"),
        "{failure}"
    );
    let intended = "test \"pair commutes\" = Gen.for_all (Gen.pair (Gen.i32()) (Gen.i32())) (\\pair -> match pair with | (a, b) -> Int.wrapping_add a b == Int.wrapping_add b a)\n";
    let module = analyze(intended).unwrap();
    let report = tsuzuri::driver::run_tests(&module, &overflow_tests(intended, true)).unwrap();
    assert_eq!(report.results[0].failure, None);
    assert!(report.results[0].overflow.is_empty());
}

#[test]
fn intentional_wrapping_with_int_wrapping_passes() {
    let source = "def mix :: i32 -> i32 -> i32 = \\total x -> Int.wrapping_add (Int.wrapping_mul total 31) x\n\
                  \n\
                  test \"hash\" =\n\
                  \x20   assert (mix 2000000000 7 == 1870457863)\n";
    let module = analyze(source).unwrap();
    let report = tsuzuri::driver::run_tests(&module, &overflow_tests(source, true)).unwrap();
    assert_eq!(report.results[0].failure, None);
    assert!(report.results[0].overflow.is_empty());
}

#[test]
fn reports_survive_a_long_standard_error() {
    // The report comes first; 100 KB of output after it push it out of the kept tail.
    let source = "def bump :: i8 -> i8 -> i8 = \\a b -> a + b\n\
                  \n\
                  test \"wraps then talks\" =\n\
                  \x20   let wrapped = bump 127i8 1i8\n\
                  \x20   let line = String.repeat 1000 (ref \"x\")\n\
                  \x20   for _ in 0 ..< 100 do Debug.print line\n\
                  \x20   assert (wrapped == -128i8)\n";
    let module = analyze(source).unwrap();
    let report = tsuzuri::driver::run_tests(&module, &overflow_tests(source, true)).unwrap();
    let result = &report.results[0];
    assert_eq!(result.failure.as_deref(), Some(BUMP_SITE));
    assert!(
        !result.output.contains(BUMP_SITE),
        "the tail no longer holds it"
    );
    assert_eq!(result.overflow, [BUMP_SITE]);
}

/// Review 3 of #67: a WebAssembly test that exhausts the engine's stack (a `RangeError` in
/// Node) fails and still reports what wrapped before.
#[test]
fn wasm_tests_report_overflow_after_the_stack_is_exhausted() {
    let source = "def bump :: i8 -> i8 -> i8 = \\a b -> a + b\n\
                  \n\
                  def rec down :: i64 -> i64 = \\n -> if n == 0 then 0 else 1 + down (n - 1)\n\
                  \n\
                  test \"wraps then recurses\" =\n\
                  \x20   let wrapped = bump 127i8 1i8\n\
                  \x20   assert (wrapped == -128i8 && down 100000000 > 0)\n";
    let module = analyze(source).unwrap();
    let options = tsuzuri::driver::TestOptions {
        target: tsuzuri::driver::Target::Wasm32,
        ..overflow_tests(source, true)
    };
    let report = tsuzuri::driver::run_tests(&module, &options).unwrap();
    let result = &report.results[0];
    let failure = result.failure.as_deref().unwrap();
    assert!(!failure.contains("integer overflow"), "{failure}");
    assert_eq!(result.overflow, [BUMP_SITE], "{}", result.output);
    assert!(
        result
            .output
            .contains("the stack was probably exhausted by deep recursion"),
        "{}",
        result.output
    );
}

#[test]
fn wasm_tests_report_overflow_after_a_stack_check_traps() {
    // Above 2 GiB of memory, wasm32 functions check their stack pointer. Frames of 1,000 values
    // reach the check before the engine's own limit, and it traps with the stack pointer below
    // the guard, where the runner still reads the hits.
    let values: Vec<String> = (0..1000).map(|index| format!("n + {index}")).collect();
    let source = format!(
        "def bump :: i8 -> i8 -> i8 = \\a b -> a + b\n\
         \n\
         def rec deep :: i64 -> i64 = \\n ->\n\
         \x20   let values = [{}]\n\
         \x20   if n == 0 then 0 else values[deep (n - 1) % 1000]\n\
         \n\
         test \"wraps then recurses\" =\n\
         \x20   let wrapped = bump 127i8 1i8\n\
         \x20   assert (wrapped == -128i8 && deep 1000 >= 0)\n",
        values.join(", ")
    );
    let module = analyze(&source).unwrap();
    let options = tsuzuri::driver::TestOptions {
        target: tsuzuri::driver::Target::Wasm32,
        wasm_max_memory: Some(3 << 30),
        wasm_stack_size: Some(64 << 10),
        ..overflow_tests(&source, true)
    };
    let report = tsuzuri::driver::run_tests(&module, &options).unwrap();
    let result = &report.results[0];
    assert_eq!(result.overflow, [BUMP_SITE], "{}", result.output);
    assert!(
        !result.output.contains("RangeError"),
        "the engine's limit came first: {}",
        result.output
    );
}

/// Review 7 of #67: the checker makes a test body a generated function, which the report covers too.
#[test]
fn arithmetic_in_a_test_body_is_reported() {
    let source = "def id :: i8 -> i8 = \\a -> a\n\
                  \n\
                  test \"wraps in the body\" =\n\
                  \x20   assert (id 127i8 + 1i8 == -128i8)\n";
    let module = analyze(source).unwrap();
    let site = "integer overflow wrapped at ./Main.tz:4:22 (i8 add)";
    for target in [
        tsuzuri::driver::Target::Native,
        tsuzuri::driver::Target::Wasm32,
    ] {
        let options = tsuzuri::driver::TestOptions {
            target,
            ..overflow_tests(source, true)
        };
        let report = tsuzuri::driver::run_tests(&module, &options).unwrap();
        let result = &report.results[0];
        assert_eq!(result.failure.as_deref(), Some(site), "{target:?}");
        assert_eq!(result.overflow, [site], "{target:?}");
    }
}

#[test]
fn wasm_tests_report_overflow_like_native_tests() {
    let module = analyze(WRAPPING_TESTS).unwrap();
    let options = tsuzuri::driver::TestOptions {
        target: tsuzuri::driver::Target::Wasm32,
        ..overflow_tests(WRAPPING_TESTS, true)
    };
    let report = tsuzuri::driver::run_tests(&module, &options).unwrap();
    assert_eq!(report.results[0].failure, None);
    assert_eq!(report.results[1].failure.as_deref(), Some(BUMP_SITE));
    assert_eq!(report.results[1].overflow, [BUMP_SITE]);
    let options = tsuzuri::driver::TestOptions {
        target: tsuzuri::driver::Target::Wasm32,
        ..overflow_tests(WRAPPING_TESTS, false)
    };
    let report = tsuzuri::driver::run_tests(&module, &options).unwrap();
    assert_eq!(report.results[1].failure, None);
    assert_eq!(report.results[1].overflow, [BUMP_SITE]);
}

#[test]
fn test_command_text_and_json_show_the_report() {
    let root = scratch("test-json");
    std::fs::write(root.join("Main.tz"), WRAPPING_TESTS).unwrap();
    let output = tsuzuri(&["test", ".", "--json"], &root);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let line = stdout
        .lines()
        .find(|line| line.contains("\"name\":\"wraps\""))
        .unwrap();
    let site = json_text(&cli_bump());
    let prefix = format!(
        "{{\"type\":\"test\",\"index\":1,\"module\":\"Main\",\"name\":\"wraps\",\"status\":\"failed\",\"failure\":\"{site}\",\"output\":\"warning: {site}\\n\",\"overflow\":[\"{site}\"],\"duration_ms\":"
    );
    assert!(line.starts_with(&prefix), "{line}");
    let passed = stdout
        .lines()
        .find(|line| line.contains("\"name\":\"adds\""))
        .unwrap();
    assert!(!passed.contains("overflow"), "{passed}");
    let allowed = tsuzuri(&["test", ".", "--json", "--allow-overflow"], &root);
    assert_eq!(allowed.status.code(), Some(0), "{allowed:?}");
    let stdout = String::from_utf8(allowed.stdout).unwrap();
    let line = stdout
        .lines()
        .find(|line| line.contains("\"name\":\"wraps\""))
        .unwrap();
    assert!(
        line.contains(&format!(
            "\"status\":\"passed\",\"overflow\":[\"{site}\"],\"duration_ms\":"
        )),
        "{line}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Review of #67: the sites of `a + b` and `a + b + c` start together, so their messages name
/// the operator, and `run --json` finds the one that wrapped.
#[test]
fn nested_sites_name_their_operators() {
    let source = "export def sum3 :: i32 -> i32 -> i32 -> i32 = \\a b c -> a + b + c\n";
    let (_, sites) = emit(source, false);
    assert_eq!(sites.len(), 2, "{sites:?}");
    assert_eq!(sites[0].start, sites[1].start);
    let text = Source {
        path: "./Main.tz".into(),
        text: source.into(),
        reported: true,
    };
    let messages: Vec<_> = sites.iter().map(|site| site.message(&text)).collect();
    assert_ne!(messages[0], messages[1]);
    let outer = sites
        .iter()
        .find(|site| site.end == source.len() - 1)
        .unwrap();
    let line = format!("warning: {}\n", outer.message(&text));
    let json = String::from_utf8(overflow::json_lines(line.as_bytes(), &sites, &[text])).unwrap();
    assert!(json.contains(&format!("\"end\":{},", outer.end)), "{json}");
    assert!(
        json.contains(&format!("\"column\":{}", outer.start + 1)),
        "{json}"
    );
}

/// Review of #67: under the report a helper of the program stays a call in a native tail call,
/// so its arithmetic is reported at its own body, not at the argument.
#[test]
fn helper_calls_in_tail_arguments_keep_their_own_sites() {
    let source = "def plus :: i32 -> i32 -> i32 = \\a b -> a + b\n\
                  def rec spin :: i32 -> i32 -> i32 = \\n acc -> if n == 0 then acc else spin (n - 1) (plus acc 1000000000)\n\
                  export def run :: i32 -> i32 = \\n -> spin n 0\n";
    let (_, sites) = emit(source, false);
    let texts: Vec<_> = sites
        .iter()
        .map(|site| &source[site.start..site.end])
        .collect();
    assert!(texts.contains(&"a + b"), "{texts:?}");
    assert!(texts.contains(&"(n - 1)"), "{texts:?}");
    assert!(!texts.iter().any(|text| text.contains("plus")), "{texts:?}");
}

/// Review of #67: a block comment between the operands, nested or not, is not the operator.
#[test]
fn block_comments_between_the_operands_are_skipped() {
    let source = "export def mul :: i32 -> i32 -> i32 = \\a b -> a /* x * y /* * */ */ * b\n";
    let (_, sites) = emit(source, false);
    let [site] = sites.as_slice() else {
        panic!("{sites:?}");
    };
    assert_eq!(site.anchor, source.rfind('*').unwrap(), "{site:?}");
}

/// Review of #67: the top-level code of a file (the checker's `$entry`) is the program's own code.
#[test]
fn top_level_code_is_reported() {
    let root = scratch("entry");
    let program = "def value :: unit -> i32 = \\_ -> 2147483647\nDebug.print (value () + 1)\n";
    std::fs::write(root.join("Main.tz"), program).unwrap();
    let output = tsuzuri(&["run", ".", "--overflow-report"], &root);
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with(&format!(
            "warning: integer overflow wrapped at {}:2:23 (i32 add)\n",
            cli_path()
        )),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
