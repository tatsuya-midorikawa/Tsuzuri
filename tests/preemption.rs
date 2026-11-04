//! TZ10 Phase 1: the cooperative preemption checks of a native program that spawns tzroutines.
//! A check is the line `call void @tz.preempt.poll()` (src/llvm_preempt.rs): before the recursive
//! calls of a recursive function, and before the back edge of a loop that makes a heavy call (the
//! runtime, an extern, a recursive function), never in a loop whose calls are small functions,
//! lambdas or allocations. A program without `tzroutine` keeps its IR (S1), and a loop that
//! vectorizes without tzroutines still vectorizes with them (S2).
use std::process::Command;

use tsuzuri::{analyze, llvm};

const POLL: &str = "call void @tz.preempt.poll()";

fn emit(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(
        ir,
        llvm::emit(&module, llvm::Entry::Library).unwrap(),
        "the IR is deterministic"
    );
    ir
}

/// The body of the function `symbol`, from its `define` line to its closing brace.
fn body<'a>(ir: &'a str, symbol: &str) -> &'a str {
    let header = ir
        .lines()
        .find(|line| line.starts_with("define ") && line.contains(&format!("{symbol}(")))
        .unwrap_or_else(|| panic!("no {symbol} in\n{ir}"));
    let start = ir.find(header).unwrap();
    let end = start + ir[start..].find("\n}\n").unwrap();
    &ir[start..end]
}

fn polls(ir: &str, symbol: &str) -> usize {
    body(ir, symbol).matches(POLL).count()
}

/// A spawning program: `go` starts a tzroutine.
const SPAWN: &str = "export def go :: i64 -> i64\nfn go n =\n    tzroutine { () }\n    n\n";

const KERNELS: &str = "def square :: i64 -> i64
fn square x = x * x

def rec fib :: i64 -> i64
fn rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)

export def fibs :: i64 -> i64
fn fibs n = fib n

export def plain_sum :: ref [i64] -> i64
fn plain_sum xs =
    let mut total = 0i64
    for x in xs do
        total = total + x
    total

export def sum_squares :: ref [i64] -> i64
fn sum_squares xs =
    let mut total = 0i64
    for x in xs do
        total = total + square x
    total

def doubled :: [i64] -> [i64]
fn doubled xs = Array.map (\\x -> x * 2) xs

export def doubled_sum :: i64 -> i64
fn doubled_sum n = Array.sum (ref (doubled (Array.init n (\\i -> i))))

export def folded :: ref [i64] -> i64
fn folded xs = Array.fold (\\acc x -> acc + x * 3) 0i64 xs

export def yielding :: i64 -> i64
fn yielding n =
    let mut i = 0i64
    while i < n do
        Routine.yield ()
        i = i + 1
    i
";

#[test]
fn a_program_without_tzroutines_has_no_check() {
    for source in [
        KERNELS.replace("        Routine.yield ()\n", ""),
        // A Channel without a spawn, and Parallel alone: no tzroutine waits for a P behind them.
        "export def channel :: i64 -> i64\nfn channel n =\n    let c: Channel<i64> = Channel.make 1\n    Channel.send c n\n    match Channel.recv c with\n        | Maybe.Some v -> v\n        | Maybe.None -> 0\n".to_owned(),
        "export def total :: i64 -> i64\nfn total n =\n    let values = Parallel.init 1000 (\\i -> i * n)\n    Parallel.sum (ref values)\n".to_owned(),
    ] {
        let ir = emit(&source);
        assert!(!ir.contains("tz.preempt") && !ir.contains("tsuzuri_tz_preempt"), "{source}\n{ir}");
    }
}

#[test]
fn checks_go_to_cycles_and_to_loops_that_call_and_never_to_leaves() {
    let ir = emit(&format!("{KERNELS}{SPAWN}"));
    // A recursive function checks before its recursive calls, not on the path of its base case.
    assert_eq!(polls(&ir, "@tz.fn.Main.fib"), 1);
    let fib = body(&ir, "@tz.fn.Main.fib");
    let site = fib.find(POLL).unwrap();
    assert!(
        fib[site..]
            .lines()
            .nth(1)
            .unwrap()
            .contains("call i64 @tz.fn.Main.fib("),
        "{fib}"
    );
    assert!(
        !fib[..fib.find("\nloop:\n").unwrap()].contains(POLL),
        "{fib}"
    );
    // A loop that calls the runtime checks at its back edge; its function has no other check.
    assert_eq!(polls(&ir, "@tz.fn.Main.yielding"), 1);
    // A leaf and a call-free loop: none. A loop whose calls inline away (a small function, a lambda
    // that a loop calls through a constant closure): none in the loop.
    assert_eq!(polls(&ir, "@tz.fn.Main.square"), 0);
    assert_eq!(polls(&ir, "@tz.fn.Main.plain_sum"), 0);
    assert_eq!(polls(&ir, "@tz.fn.Main.sum_squares"), 0);
    assert_eq!(polls(&ir, "@tz.fn.Main.folded"), 0);
    // A function that calls the runtime once, outside any loop: none.
    assert_eq!(polls(&ir, "@tz.fn.Main.go"), 0);
    // The runtime's symbols, once.
    assert_eq!(
        ir.matches("@tsuzuri_tz_preempt_pending = external hidden global i32")
            .count(),
        1
    );
    assert_eq!(
        ir.matches("declare void @tsuzuri_tz_preempt_check(ptr) nounwind memory(argmem: readwrite, inaccessiblemem: readwrite)")
            .count(),
        1
    );
    assert!(ir.contains("call void @tsuzuri_tz_preempt_check(ptr @tsuzuri_tz_preempt_pending)"));
    assert_eq!(
        ir.matches("define internal void @tz.preempt.poll()")
            .count(),
        1
    );
    assert!(ir.contains("load atomic i32, ptr @tsuzuri_tz_preempt_pending monotonic"));
}

#[test]
fn an_extern_call_in_a_loop_is_checked() {
    let source = format!(
        "extern def tick :: i64 -> i64\n\
         export def ticks :: i64 -> i64\nfn ticks n =\n    let mut total = 0i64\n    for i in 1 .. n do\n        total = total + tick i\n    total\n{SPAWN}"
    );
    let ir = emit(&source);
    assert_eq!(
        polls(&ir, "@tz.fn.Main.ticks"),
        1,
        "{}",
        body(&ir, "@tz.fn.Main.ticks")
    );
    // TZ08 made the call blocking; no check is inside a blocking section, where the G has no P.
    let mut blocking = false;
    for line in ir.lines() {
        if line.contains("call void @tsuzuri_tz_blocking_enter()") {
            blocking = true;
        } else if line.contains("call void @tsuzuri_tz_blocking_exit()") {
            blocking = false;
        }
        assert!(!(blocking && line.contains(POLL)), "{ir}");
    }
    assert!(ir.contains("call void @tsuzuri_tz_blocking_exit()"));
}

#[test]
fn recursion_through_a_closure_is_checked() {
    let source = format!(
        "def rec countdown :: i64 -> i64\nfn rec countdown n =\n    let step = \\k -> countdown k\n    if n == 0 then 0 else step (n - 1)\n\
         export def run :: i64 -> i64\nfn run n = countdown n\n{SPAWN}"
    );
    let ir = emit(&source);
    assert!(
        polls(&ir, "@tz.fn.Main.countdown") >= 1,
        "{}",
        body(&ir, "@tz.fn.Main.countdown")
    );
}

#[test]
fn a_loop_that_only_allocates_is_not_checked() {
    let source = format!(
        "export def built :: i64 -> i64\nfn built n =\n    let mut values: Vec<i64> = Vec.empty()\n    for i in 1 ..= n do\n        values = Vec.push (i * 2) values\n    Vec.length (ref values)\n{SPAWN}"
    );
    let ir = emit(&source);
    assert_eq!(
        polls(&ir, "@tz.fn.Main.built"),
        0,
        "{}",
        body(&ir, "@tz.fn.Main.built")
    );
}

/// The functions of the optimized IR that hold a vector loop.
fn vectorized(ir: &str, name: &str) -> Vec<String> {
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-preemption-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let input = directory.join("input.ll");
    let output = directory.join("output.ll");
    std::fs::write(&input, ir).unwrap();
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let status = Command::new(clang)
        .args([
            "-O3",
            "-S",
            "-emit-llvm",
            "-Wno-override-module",
            "-x",
            "ir",
        ])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .status()
        .expect("clang runs");
    assert!(status.success());
    let text = std::fs::read_to_string(&output).unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    let mut found = Vec::new();
    let mut function = String::new();
    for line in text.lines() {
        if line.starts_with("define ") {
            function = line
                .split('@')
                .nth(1)
                .unwrap()
                .split('(')
                .next()
                .unwrap()
                .to_owned();
        } else if line.starts_with("vector.body") && !found.contains(&function) {
            found.push(function.clone());
        }
    }
    found.sort();
    found
}

#[test]
fn a_loop_that_vectorizes_without_tzroutines_still_vectorizes_with_them() {
    let plain = emit(KERNELS);
    let spawning = emit(&format!("{KERNELS}{SPAWN}"));
    let without = vectorized(&plain, "plain");
    let with = vectorized(&spawning, "spawning");
    assert!(!without.is_empty(), "no vector loop at all");
    for name in &without {
        assert!(
            with.contains(name),
            "{name} lost its vector loop: {without:?} -> {with:?}"
        );
    }
}
