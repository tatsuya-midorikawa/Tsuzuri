#![cfg(all(feature = "fast-backend", unix))]

//! The translator of the fast debug backend (PB05): what it reads, how it lays out types, and what it
//! refuses. Behavior of the programs it builds is checked by `tests/fast_backend.mjs`. The tests build
//! for the machine they run on, and `--backend fast` is for macOS and Linux hosts only.

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

fn expect_wasm_refusal(ir: &str, construct: &str) {
    let error = backend::compile_wasm(ir, 1, false).expect_err(construct);
    assert_eq!(error.code, "E2002", "{construct}: {}", error.message);
    assert!(
        error.message.contains(construct),
        "expected {construct:?} in {:?}",
        error.message
    );
}

/// What the translators do not implement is refused, and not turned into a call that means something
/// else: `byval` and `sret` follow the platform's convention for a function that other objects see.
/// What they do implement stays: the internal `byval` and `sret` of the runtime, and an `alloca`
/// aligned beyond the stack's 16 bytes. A `musttail` call that they cannot keep as a tail call is an
/// ordinary call with the same meaning (PM09), and the native translator keeps the ones it can.
#[test]
fn refuses_the_conventions_that_it_would_not_implement() {
    let sret = "declare void @g(ptr sret(i64) %p)\ndefine void @f(ptr %p) {\n  call void @g(ptr sret(i64) %p)\n  ret void\n}\n";
    let byval = "declare void @g(ptr byval(i64) %p)\ndefine void @f(ptr %p) {\n  call void @g(ptr byval(i64) %p)\n  ret void\n}\n";
    let exported = "define void @f(ptr byval(i64) %p) {\n  ret void\n}\n";
    let call_only = "declare void @g(ptr)\ndefine void @f(ptr %p) {\n  call void @g(ptr byval(i64) %p)\n  ret void\n}\n";
    let indirect =
        "define void @f(ptr %fn, ptr %p) {\n  call void %fn(ptr sret(i64) %p)\n  ret void\n}\n";
    let musttail = "declare i32 @g(i32)\ndefine i32 @f(i32 %x) {\n  %r = musttail call i32 @g(i32 %x)\n  ret i32 %r\n}\n";
    let refusals = [
        (
            sret,
            "a byval or sret parameter of an external function in @g",
        ),
        (
            byval,
            "a byval or sret parameter of an external function in @g",
        ),
        (
            exported,
            "a byval or sret parameter of an external function in @f",
        ),
        (
            call_only,
            "a byval or sret argument of a call of an external function in @f",
        ),
        (
            indirect,
            "a call through a pointer with a byval or sret argument in @f",
        ),
    ];
    for (ir, construct) in refusals {
        expect_refusal(ir, construct);
        expect_wasm_refusal(ir, construct);
    }
    // The functions of this module are external, so the call is not one that the translators keep.
    backend::compile(musttail, &options(1)).expect("native: an ordinary call");
    backend::compile_wasm(musttail, 1, false).expect("wasm: an ordinary call");
    // A function of the module's own passes them as the translators do, as the runtime's does.
    let internal = "%S = type { i64, i64 }\ndefine internal fastcc void @inner(ptr noalias sret(%S) align 8 %out, ptr byval(%S) align 8 %in) {\n  %a = load i64, ptr %in, align 8\n  store i64 %a, ptr %out, align 8\n  ret void\n}\ndefine i64 @f(ptr %p) {\n  %r = alloca %S, align 8\n  call fastcc void @inner(ptr sret(%S) align 8 %r, ptr byval(%S) align 8 %p)\n  %v = load i64, ptr %r, align 8\n  ret i64 %v\n}\n";
    backend::compile(internal, &options(1)).expect("native");
    backend::compile_wasm(internal, 1, false).expect("wasm");
    // The stack of the native code keeps 16 bytes of alignment, and an `alloca` that asks for more gets
    // it from an address that is rounded up (the front end asks for 32 for a 256-bit vector); a
    // WebAssembly frame is rounded down to the largest alignment that a slot asks for. The operations
    // test runs both and compares the address with Clang's.
    let aligned = "define i64 @f() {\n  %p = alloca i64, align 32\n  %v = ptrtoint ptr %p to i64\n  %m = and i64 %v, 31\n  ret i64 %m\n}\n";
    backend::compile(aligned, &options(1)).expect("native keeps the alignment");
    backend::compile_wasm(aligned, 1, false).expect("wasm keeps the alignment");
    let natural = "define i64 @f() {\n  %p = alloca <8 x float>\n  ret i64 0\n}\n";
    backend::compile(natural, &options(1)).expect("native keeps the natural alignment");
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

const PROGRAMS: [&str; 5] = [
    "def main :: unit -> i32 = \\() ->\n    do! IO.write_line \"hello\"\n    0\n",
    "record Point { x: i64, y: i64 }\ndef scale :: Point -> i64 -> Point = \\p k -> Point { x: p.x * k, y: p.y * k }\ndef main :: unit -> i32 = \\() ->\n    let q = scale (Point { x: 3, y: 4 }) 5\n    let words = [\"a\", \"bc\", \"def\"]\n    let total = Array.length (ref words) + (q.x as i64)\n    do! IO.write_line total\n    0\n",
    "def main :: unit -> i32 = \\() ->\n    let a = 170141183460469231731687303715884105727i128\n    let b = a / 7i128 % 1000003i128\n    let f = 2.5 * (b as f64)\n    do! IO.write_line (f as i64)\n    0\n",
    "union Shape = Circle of f64 | Square of f64\ndef area :: Shape -> f64 = \\s -> match s with\n    | Circle r -> 3.14159 * r * r\n    | Square w -> w * w\ndef main :: unit -> i32 = \\() ->\n    let offset = 10\n    let add = x -> x + offset\n    do! IO.write_line (add ((area (Square 4.0)) as i64))\n    do! IO.write_line (Display.display (area (Circle 1.0)))\n    0\n",
    // A mutual recursion: the calls between the two functions are `musttail` calls (PM09).
    "def rec ping :: i64 -> i64 -> i64\n    def and pong :: i64 -> i64 -> i64\n    fn rec ping n acc = if n == 0 then acc else pong (n - 1) (acc + 1)\n    and pong n acc = if n == 0 then acc else ping (n - 1) (acc + 2)\ndef main :: unit -> i32 = \\() ->\n    do! IO.write_line (ping 1000 0)\n    0\n",
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

/// Links `object` with the C `host` (the object alone when there is none), runs the program, and
/// returns what it printed. `None` where there is no Clang to link with.
fn link_and_run(label: &str, object: &[u8], host: Option<&str>) -> Option<String> {
    Command::new(clang()).arg("--version").output().ok()?;
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-fast-tail-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let (object_path, program) = (directory.join("tail.o"), directory.join("tail"));
    std::fs::write(&object_path, object).unwrap();
    let mut link = Command::new(clang());
    if let Some(host) = host {
        std::fs::write(directory.join("host.c"), host).unwrap();
        link.arg(directory.join("host.c"));
    }
    let linked = link
        .arg(&object_path)
        .arg("-lm")
        .arg("-o")
        .arg(&program)
        .output()
        .expect("Clang starts");
    assert!(
        linked.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&program).output().expect("the program starts");
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        output.status.success(),
        "{label} stopped with {:?}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A `musttail` cycle of two functions, with the C host that calls `run`.
const PING_PONG: &str = "define internal i64 @ping(i64 %n, i64 %acc) {
entry:
  %z = icmp eq i64 %n, 0
  br i1 %z, label %done, label %next
done:
  ret i64 %acc
next:
  %m = sub i64 %n, 1
  %a = add i64 %acc, 1
  %r = musttail call i64 @pong(i64 %m, i64 %a)
  ret i64 %r
}
define internal i64 @pong(i64 %n, i64 %acc) {
entry:
  %z = icmp eq i64 %n, 0
  br i1 %z, label %done, label %next
done:
  ret i64 %acc
next:
  %m = sub i64 %n, 1
  %a = add i64 %acc, 2
  %r = musttail call i64 @ping(i64 %m, i64 %a)
  ret i64 %r
}
define i64 @run(i64 %n) {
  %r = call i64 @ping(i64 %n, i64 0)
  ret i64 %r
}
";

fn print_run(depth: u64) -> String {
    format!(
        "#include <stdint.h>\n#include <stdio.h>\nextern int64_t run(int64_t);\nint main(void) {{ printf(\"%lld\\n\", (long long)run({depth})); return 0; }}\n"
    )
}

/// PM09: a `musttail` call between two functions of the module is one jump, so a cycle of forty
/// million calls runs in the stack of one call (an ordinary call needs at least sixteen bytes each,
/// far more than the megabytes that a program's stack has).
#[test]
fn guaranteed_tail_calls_keep_the_stack_flat() {
    let object = backend::compile(PING_PONG, &options(1)).unwrap();
    // 20,000,000 calls of `ping` add 1 and as many of `pong` add 2.
    if let Some(output) = link_and_run("cycle", &object, Some(&print_run(40_000_000))) {
        assert_eq!(output, "60000000");
    }
    // The translation is the same whatever the number of threads.
    for threads in [2, 4] {
        assert!(object == backend::compile(PING_PONG, &options(threads)).unwrap());
    }
}

/// A function of twelve integers: at least four of them are not in registers. The cycle passes them
/// on rotated, so that an argument written to the wrong slot of the caller's frame changes the sum.
fn rotating_function(name: &str, next: &str, step: u32) -> String {
    let params: Vec<String> = (0..12).map(|index| format!("i64 %p{index}")).collect();
    let mut text = format!(
        "define internal i64 @{name}({}) {{\nentry:\n  %z = icmp eq i64 %p0, 0\n  br i1 %z, label %done, label %next\ndone:\n  %s0 = add i64 0, 0\n",
        params.join(", ")
    );
    for index in 1..12 {
        text.push_str(&format!(
            "  %w{index} = mul i64 %p{index}, {index}\n  %s{index} = add i64 %s{}, %w{index}\n",
            index - 1
        ));
    }
    let mut arguments = vec!["i64 %m".to_owned()];
    arguments.extend((2..12).map(|index| format!("i64 %p{index}")));
    arguments.push("i64 %t".to_owned());
    text.push_str(&format!(
        "  ret i64 %s11\nnext:\n  %m = sub i64 %p0, 1\n  %t = add i64 %p1, {step}\n  %r = musttail call i64 @{next}({})\n  ret i64 %r\n}}\n",
        arguments.join(", ")
    ));
    text
}

/// What `rotating_function` computes for `depth` calls that start from the numbers 1 to 11: the
/// first call is of `ping` (step 3), the next of `pong` (step 5), and so on.
fn rotating_reference(depth: u64) -> i64 {
    let mut slots: Vec<i64> = (1..12).collect();
    for call in 0..depth {
        let first = slots.remove(0);
        slots.push(first + if call % 2 == 0 { 3 } else { 5 });
    }
    slots
        .iter()
        .zip(1..)
        .map(|(value, weight)| value * weight)
        .sum()
}

#[test]
fn guaranteed_tail_calls_pass_arguments_on_the_stack() {
    let mut ir = rotating_function("ping", "pong", 3) + &rotating_function("pong", "ping", 5);
    let start: Vec<String> = (1..12).map(|index| format!("i64 {index}")).collect();
    ir.push_str(&format!(
        "define i64 @run(i64 %n) {{\n  %r = call i64 @ping(i64 %n, {})\n  ret i64 %r\n}}\n",
        start.join(", ")
    ));
    let object = backend::compile(&ir, &options(1)).unwrap();
    // The depth that the C host passes keeps the references in a loop of Rust that is cheap to run.
    for depth in [0, 1, 2, 11, 12, 1000, 3_000_001] {
        if let Some(output) = link_and_run("wide", &object, Some(&print_run(depth))) {
            assert_eq!(output, rotating_reference(depth).to_string(), "{depth}");
        }
    }
}

/// A program of the language: the compiler's own `musttail` calls run in the stack of one call
/// under `--backend fast` too (a cycle of ten million calls). The exit status says whether the
/// answer is right: the object has no C runtime to print with.
#[test]
fn a_mutual_recursion_of_a_program_keeps_the_stack_flat() {
    // 5,000,000 calls of each function add 5,000,000 and 2 * 5,000,000.
    let source = "def rec ping :: i64 -> i64 -> i64\n    def and pong :: i64 -> i64 -> i64\n    fn rec ping n acc = if n == 0 then acc else pong (n - 1) (acc + 1)\n    and pong n acc = if n == 0 then acc else ping (n - 1) (acc + 2)\ndef main :: unit -> i32 = \\() ->\n    if ping 10000000 0 == 15000000 then 0 else 1\n";
    let ir = program_ir(source);
    assert!(ir.contains("musttail call i64 @tz.fn.Main.pong("), "{ir}");
    assert!(ir.contains("musttail call i64 @tz.fn.Main.ping("), "{ir}");
    let object = backend::compile(&ir, &options(2)).unwrap();
    if let Some(output) = link_and_run("program", &object, None) {
        assert_eq!(output, "");
    }
}

/// The functions that cannot have the tail convention (one that other code can call through a
/// pointer, and one with a parameter that travels in memory) make ordinary calls out of the
/// `musttail` calls that enter or leave them, and the program means the same.
#[test]
fn guaranteed_tail_calls_without_the_tail_convention_are_ordinary_calls() {
    let address_taken = format!(
        "@slot = internal global ptr null\n{}",
        PING_PONG.replace(
            "define i64 @run(i64 %n) {",
            "define i64 @run(i64 %n) {\n  store ptr @ping, ptr @slot"
        )
    );
    let large = "define internal i64 @ping([20 x i64] %v, i64 %n) {
entry:
  %e0 = extractvalue [20 x i64] %v, 0
  %z = icmp eq i64 %n, 0
  br i1 %z, label %done, label %next
done:
  ret i64 %e0
next:
  %m = sub i64 %n, 1
  %t = add i64 %e0, 3
  %w = insertvalue [20 x i64] %v, i64 %t, 0
  %r = musttail call i64 @pong([20 x i64] %w, i64 %m)
  ret i64 %r
}
define internal i64 @pong([20 x i64] %v, i64 %n) {
entry:
  %e0 = extractvalue [20 x i64] %v, 0
  %z = icmp eq i64 %n, 0
  br i1 %z, label %done, label %next
done:
  ret i64 %e0
next:
  %m = sub i64 %n, 1
  %t = add i64 %e0, 5
  %w = insertvalue [20 x i64] %v, i64 %t, 0
  %r = musttail call i64 @ping([20 x i64] %w, i64 %m)
  ret i64 %r
}
define i64 @run(i64 %n) {
  %r = call i64 @ping([20 x i64] zeroinitializer, i64 %n)
  ret i64 %r
}
";
    // The depth is small: these two keep an ordinary frame for each call.
    for (label, ir, expected) in [
        ("address", address_taken.as_str(), "1500"),
        ("large", large, "4000"),
    ] {
        let object = backend::compile(ir, &options(1)).unwrap_or_else(|error| {
            panic!("{label}: {}", error.message);
        });
        if let Some(output) = link_and_run(label, &object, Some(&print_run(1000))) {
            assert_eq!(output, expected, "{label}");
        }
    }
}

/// The object format and machine of a relocatable object, read from its header.
fn object_kind(bytes: &[u8]) -> String {
    match bytes {
        [0x7f, b'E', b'L', b'F', 2, 1, ..] => {
            format!("elf {:#x}", u16::from_le_bytes([bytes[18], bytes[19]]))
        }
        [0xcf, 0xfa, 0xed, 0xfe, ..] => format!(
            "macho {:#x}",
            u32::from_le_bytes(bytes[4..8].try_into().unwrap())
        ),
        _ => format!("coff {:#x}", u16::from_le_bytes([bytes[0], bytes[1]])),
    }
}

/// WP14: the object has the format of the machine it is for, whatever machine the compiler runs on,
/// and the one target that cranelift-object cannot write (aarch64 Windows) is refused with E2002.
#[test]
fn objects_for_other_machines_have_their_formats() {
    use tsuzuri::target::{Platform, parse_target};
    let platform = |name: &str| Platform::new(parse_target(name).unwrap().1);
    for (target, expected) in [
        ("x86_64-unknown-linux-gnu", "elf 0x3e"),
        ("aarch64-unknown-linux-gnu", "elf 0xb7"),
        ("aarch64-apple-darwin", "macho 0x100000c"),
        ("x86_64-pc-windows-msvc", "coff 0x8664"),
    ] {
        // The program with a 128-bit division needs the helpers that `windows_abi` adds on Windows.
        for source in [PROGRAMS[0], PROGRAMS[1], PROGRAMS[3]] {
            let options = Options {
                platform: platform(target),
                ..options(2)
            };
            let object = backend::compile(&program_ir(source), &options)
                .unwrap_or_else(|error| panic!("{target}: {}", error.message));
            assert_eq!(object_kind(&object), expected, "{target}");
        }
    }
    let options = Options {
        platform: platform("aarch64-pc-windows-msvc"),
        ..options(1)
    };
    let error = backend::compile(&program_ir(PROGRAMS[0]), &options).unwrap_err();
    assert_eq!(error.code, "E2002");
    assert!(
        error.message.contains("aarch64 on Windows"),
        "{}",
        error.message
    );
}

/// The IR of a `-g` build, as `tsuzuri build -g` has it.
fn debug_program_ir(source: &str) -> String {
    let module = tsuzuri::analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let sources = [tsuzuri::trap::TrapSource {
        path: "proj/Main.tz",
        text: source,
    }];
    llvm::emit_with_debug_info(
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
        &sources,
        false,
        false,
    )
    .unwrap()
    .ir
}

/// WP12: `-g` IR gives an object with a line table and the program's functions, the same whatever
/// the number of threads, and IR without debug information gives none.
#[test]
fn debug_information_adds_a_line_table() {
    let contains =
        |bytes: &[u8], needle: &[u8]| bytes.windows(needle.len()).any(|window| window == needle);
    for source in PROGRAMS {
        let ir = debug_program_ir(source);
        assert!(ir.contains("!DICompileUnit("), "{source}");
        let object = backend::compile(&ir, &options(1))
            .unwrap_or_else(|error| panic!("{source}\n{}", error.message));
        for needle in [
            &b"debug_line"[..],
            b"debug_info",
            b"debug_ranges",
            b"Main.tz",
            b"Main.main",
        ] {
            assert!(
                contains(&object, needle),
                "{:?} is not in the object of {source}",
                String::from_utf8_lossy(needle)
            );
        }
        for threads in [3, 8] {
            assert!(
                object == backend::compile(&ir, &options(threads)).unwrap(),
                "{threads} threads change the -g object of {source}"
            );
        }
        let plain = backend::compile(&program_ir(source), &options(1)).unwrap();
        assert!(!contains(&plain, b"debug_line"), "{source}");
        assert!(!contains(&plain, b"Main.tz"), "{source}");
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
