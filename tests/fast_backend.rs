#![cfg(feature = "fast-backend")]

//! The translator of the fast debug backend (PB05): what it reads, how it lays out types, and what it
//! refuses. Behavior of the programs it builds is checked by `tests/fast_backend.mjs`.

use std::io::Write;
use std::process::{Command, Stdio};

use tsuzuri::backend::{self, Options};
use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry};

fn clang() -> String {
    std::env::var("TSUZURI_CLANG").unwrap_or_else(|_| "clang".to_owned())
}

/// The constant that `expression` folds to when Clang compiles the module with `definitions`, or
/// `None` where there is no Clang to ask.
fn clang_constant(definitions: &str, expression: &str) -> Option<u64> {
    let ir = format!("{definitions}\ndefine i64 @probe() {{\n  ret i64 {expression}\n}}\n");
    let mut child = Command::new(clang())
        .args([
            "-x",
            "ir",
            "-O1",
            "-S",
            "-emit-llvm",
            "-Wno-override-module",
            "-",
            "-o",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(ir.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    assert!(
        output.status.success(),
        "{ir}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).ok()?;
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("ret i64 "))?;
    line.trim_start()
        .strip_prefix("ret i64 ")?
        .trim()
        .parse()
        .ok()
}

const DEFINITIONS: &str = "\
%tz.string = type { ptr, i64 }
%tz.closure = type { ptr, ptr, ptr, ptr }
%tz.vec = type { ptr, i64, i64 }
%\"tz.union.Result.Result[unit,IO.Error]\" = type { i8, [1 x i8] }
%tz.alias = type i8
%tz.record.Mixed = type { i8, i64, i8, i32, i16 }
%tz.record.Nested = type { %tz.record.Mixed, [3 x %tz.string], i1, double }
%tz.packed = type <{ i8, i32, i16 }>
%tz.wide = type { i8, i128, i8 }
%tz.mask = type { <4 x i1>, <4 x float>, i8 }
%tz.tail = type { i64, [0 x i8] }
%tz.forward = type { %tz.later, i32 }
%tz.later = type { i8, i64 }
";

const TYPES: [&str; 33] = [
    "i1",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "float",
    "double",
    "ptr",
    "{ }",
    "{ i8 }",
    "{ i8, i64 }",
    "{ i64, i8 }",
    "{ i8, i16, i8, i32 }",
    "[0 x i64]",
    "[3 x i8]",
    "[5 x i32]",
    "[2 x { i8, i64 }]",
    "<4 x i32>",
    "<2 x double>",
    "<16 x i8>",
    "<8 x i16>",
    "<32 x i8>",
    "<3 x float>",
    "<4 x i1>",
    "%tz.string",
    "%tz.closure",
    "%tz.vec",
    "%\"tz.union.Result.Result[unit,IO.Error]\"",
    "%tz.record.Nested",
    "%tz.packed",
    "%tz.wide",
    "%tz.mask",
];

#[test]
fn layout_matches_clang() {
    if Command::new(clang()).arg("--version").output().is_err() {
        return;
    }
    let mut checked = 0;
    for ty in
        TYPES
            .iter()
            .copied()
            .chain(["%tz.alias", "%tz.tail", "%tz.forward", "%tz.record.Mixed"])
    {
        let (size, align, offsets) = backend::layout_of(DEFINITIONS, ty)
            .unwrap_or_else(|error| panic!("{ty}: {}", error.message));
        let expected_size = clang_constant(
            DEFINITIONS,
            &format!("ptrtoint (ptr getelementptr ({ty}, ptr null, i32 1) to i64)"),
        )
        .expect("clang folds the size");
        let expected_align = clang_constant(
            DEFINITIONS,
            &format!(
                "ptrtoint (ptr getelementptr ({{ i8, {ty} }}, ptr null, i32 0, i32 1) to i64)"
            ),
        )
        .expect("clang folds the alignment");
        assert_eq!(size, expected_size, "the size of {ty}");
        assert_eq!(u64::from(align), expected_align, "the alignment of {ty}");
        for (index, &offset) in offsets.iter().enumerate() {
            let expected = clang_constant(
                DEFINITIONS,
                &format!(
                    "ptrtoint (ptr getelementptr ({ty}, ptr null, i32 0, i32 {index}) to i64)"
                ),
            )
            .expect("clang folds the offset");
            assert_eq!(offset, expected, "the offset of field {index} of {ty}");
        }
        checked += 1;
    }
    assert!(checked >= 37);
}

fn options(threads: usize) -> Options {
    Options {
        verify: true,
        ..Options::host(threads)
    }
}

fn expect_refusal(ir: &str, construct: &str) {
    let error = backend::compile(ir, &options(1)).expect_err(construct);
    assert_eq!(error.code, "E2002", "{construct}: {}", error.message);
    assert!(
        error.message.contains(construct),
        "expected {construct:?} in {:?}",
        error.message
    );
    assert!(
        error
            .message
            .ends_with("build with --backend llvm and report this program"),
        "{}",
        error.message
    );
}

#[test]
fn rejects_unsupported_constructs_with_e2002() {
    expect_refusal(
        "define half @f(half %x) {\n  ret half %x\n}\n",
        "the type half",
    );
    expect_refusal(
        "define double @f(double %a, double %b) {\n  %r = fadd fast double %a, %b\n  ret double %r\n}\n",
        "the fast-math flag 'fast' in @f",
    );
    expect_refusal(
        "define void @f(ptr %p) {\n  %v = load <vscale x 4 x i32>, ptr %p\n  ret void\n}\n",
        "a scalable vector",
    );
    expect_refusal(
        "define void @f() {\n  call void asm sideeffect \"nop\", \"\"()\n  ret void\n}\n",
        "inline assembly in @f",
    );
    expect_refusal(
        "declare i32 @g()\ndeclare i32 @__gxx_personality_v0(...)\ndefine i32 @f() personality ptr @__gxx_personality_v0 {\n  ret i32 0\n}\n",
        "the function attribute 'personality'",
    );
    expect_refusal(
        "define i32 @f() {\n  %r = invoke i32 @g() to label %a unwind label %b\na:\n  ret i32 %r\nb:\n  ret i32 0\n}\ndeclare i32 @g()\n",
        "the instruction 'invoke' in @f",
    );
    expect_refusal(
        "@target = global i32 0\n@alias = alias i32, ptr @target\n",
        "an alias (@alias)",
    );
    expect_refusal(
        "declare i32 @printf(ptr, ...)\ndefine i32 @f(ptr %p) {\n  %r = call i32 (ptr, ...) @printf(ptr %p)\n  ret i32 %r\n}\n",
        "a call of the variadic function @printf in @f",
    );
    expect_refusal(
        "@g = global i32 0, section \"special\"\n",
        "a section on @g",
    );
    expect_refusal(
        "define i256 @f(i256 %x) {\n  ret i256 %x\n}\n",
        "an integer wider than 128 bits",
    );
    expect_refusal(
        "define void @f(i64 %n) {\n  %p = alloca i8, i64 %n\n  ret void\n}\n",
        "an alloca with a variable count in @f",
    );
    expect_refusal(
        "define void @f() {\nentry:\n  br label %next\nnext:\n  %p = alloca i64\n  ret void\n}\n",
        "an alloca outside the entry block in @f",
    );
    expect_refusal(
        "declare double @llvm.sin.f64(double)\ndefine double @f(double %x) {\n  %r = call double @llvm.sin.f64(double %x)\n  ret double %r\n}\n",
        "the intrinsic llvm.sin.f64 in @f",
    );
    expect_refusal(
        "define void @f(ptr addrspace(1) %p) {\n  ret void\n}\n",
        "a pointer in address space 1",
    );
    expect_refusal(
        "define x86_fp80 @f(x86_fp80 %x) {\n  ret x86_fp80 %x\n}\n",
        "x86_fp80",
    );
    expect_refusal("@g = common global i32 0\n", "the linkage 'common'");
    expect_refusal(
        "define %tz.pair @f() {\n  ret %tz.pair zeroinitializer\n}\n%tz.pair = type { i64, i64 }\n",
        "the result type { i64, i64 } of an external function",
    );
    expect_refusal(
        "define void @f(i128 %x) {\n  ret void\n}\n",
        "a 128-bit integer in the signature of an external function",
    );
    expect_refusal(
        "define void @f() {\n  call void @missing()\n  ret void\n}\n",
        "a call to the unknown function @missing in @f",
    );
    expect_refusal(
        "define void @f() {\n  %x = frobnicate i32 1\n  ret void\n}\n",
        "the instruction 'frobnicate' in @f",
    );
}

/// A program with strings, closures, records, a union, loops and a 128-bit integer, as the compiler
/// emits it with the runtime in the module (numeric and math included).
fn program_ir(source: &str) -> String {
    let module = tsuzuri::analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    llvm::emit_with_options(
        &module,
        EmitOptions {
            entry: Entry::Console,
            wasm: false,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: Allocator::System,
            f16: llvm::F16Path::Soft,
        },
    )
    .unwrap()
}

const PROGRAMS: [&str; 4] = [
    "def main :: unit -> i32 = \\() ->\n    do! IO.write_line \"hello\"\n    0\n",
    "record Point { x: i64, y: i64 }\ndef scale :: Point -> i64 -> Point = \\p k -> Point { x: p.x * k, y: p.y * k }\ndef main :: unit -> i32 = \\() ->\n    let q = scale (Point { x: 3, y: 4 }) 5\n    let words = [\"a\", \"bc\", \"def\"]\n    let total = Array.length (ref words) + (q.x as i64)\n    do! IO.write_line total\n    0\n",
    "def main :: unit -> i32 = \\() ->\n    let a = 170141183460469231731687303715884105727i128\n    let b = a / 7i128 % 1000003i128\n    let f = 2.5 * (b as f64)\n    do! IO.write_line (f as i64)\n    0\n",
    "union Shape = Circle of f64 | Square of f64\ndef area :: Shape -> f64 = \\s -> match s with\n    | Circle r -> 3.14159 * r * r\n    | Square w -> w * w\ndef main :: unit -> i32 = \\() ->\n    let offset = 10\n    let add = x -> x + offset\n    do! IO.write_line (add ((area (Square 4.0)) as i64))\n    do! IO.write_line (Display.display (area (Circle 1.0)))\n    0\n",
];

#[test]
fn reads_and_translates_emitted_programs() {
    for source in PROGRAMS {
        let ir = program_ir(source);
        let stats =
            backend::read_module(&ir).unwrap_or_else(|error| panic!("{source}\n{}", error.message));
        assert!(
            stats.functions > 20 && stats.instructions > 200,
            "{stats:?}"
        );
        assert!(stats.reachable_functions <= stats.functions);
        let object = backend::compile(&ir, &options(1))
            .unwrap_or_else(|error| panic!("{source}\n{}", error.message));
        assert!(object.len() > 1000);
    }
}

#[test]
fn object_is_deterministic() {
    for source in PROGRAMS {
        let ir = program_ir(source);
        let single = backend::compile(&ir, &options(1)).unwrap();
        assert_eq!(single, backend::compile(&ir, &options(1)).unwrap());
        for threads in [2, 5, 8] {
            assert!(
                single == backend::compile(&ir, &options(threads)).unwrap(),
                "{threads} threads change the object of {source}"
            );
        }
    }
}

/// `tsuzuri test --backend fast` (the test runner of the library): the same cases pass, fail and
/// trap, with the same failure text and counterexamples.
#[test]
fn test_runner_reports_the_same_on_both_backends() {
    use tsuzuri::driver::{Backend, TestOptions, run_tests};
    let sources = [
        "test \"passes\" = assert true\ntest \"traps\" = assert false\ntest \"passes after trap\" = assert (2 + 2 == 4)",
        include_str!("fixtures/property/Props.tz"),
        include_str!("fixtures/coverage/Calc.tz"),
    ];
    for source in sources {
        let module = tsuzuri::analyze(source).unwrap_or_else(|error| panic!("{error:?}"));
        let report = |backend| {
            let report = run_tests(
                &module,
                &TestOptions {
                    backend,
                    ..TestOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("{backend:?}: {}", error.message));
            let results: Vec<_> = report
                .results
                .iter()
                .map(|result| {
                    (
                        result.case.index,
                        result.failure.clone(),
                        result.output.clone(),
                    )
                })
                .collect();
            (results, report.ignored)
        };
        let expected = report(Backend::Llvm);
        assert!(!expected.0.is_empty());
        assert_eq!(report(Backend::Fast), expected, "{source}");
    }
}
