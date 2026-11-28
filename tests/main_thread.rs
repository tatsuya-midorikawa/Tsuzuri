//! `MainThread` (M09): `post`, `after` and `is_current`, the reserved module name, the position of
//! the posted function for the thread-bound handles of W19, the target diagnostics and the shape
//! of the IR. The runs on a main dispatch queue are in `tests/main_thread.mjs` (macOS).

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::diagnostic::{Diagnostic, DiagnosticKind};
use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    module
}

fn rejects(source: &str) -> Diagnostic {
    analyze(source).expect_err(source)
}

fn definition<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .find(&format!(" @{name}("))
        .map(|at| ir[..at].rfind("define ").unwrap())
        .unwrap_or_else(|| panic!("no definition of @{name}\n{ir}"));
    let end = start + ir[start..].find("\n}\n").unwrap() + 3;
    &ir[start..end]
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

fn temp_project(label: &str, source: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-m09-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    root
}

/// Runs `tsuzuri build` on `source` with `arguments` and returns its exit status and the JSON of
/// its first diagnostic line.
fn build_json(label: &str, source: &str, arguments: &[&str]) -> (bool, String) {
    let root = temp_project(label, source);
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("build")
        .arg(&root)
        .args(arguments)
        .arg("-o")
        .arg(root.join("out.bin"))
        .arg("--json")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);
    (output.status.success(), text)
}

const POSTS: &str = "\
def work :: unit -> unit = \\() -> ()

def main :: unit -> i32 = \\() ->
    MainThread.post (Owned.function (\\() -> work ()))
    MainThread.after (Time.millisecond() * 5) (Owned.function (\\() -> work ()))
    if MainThread.is_current () then 0 else 1
";

// r1
#[test]
fn the_three_functions_have_the_types_of_the_table() {
    accepts(
        "let post: Owned.Function<unit, unit> -> unit = MainThread.post\n\
         let after: i64 -> Owned.Function<unit, unit> -> unit = MainThread.after\n\
         let current: unit -> bool = MainThread.is_current\n\
         0\n",
    );
    accepts(POSTS);
    // The delay is nanoseconds as an `i64`, before the function (D2).
    assert_eq!(
        rejects("MainThread.after (Owned.function (\\() -> ())) 5\n0\n").code,
        "E1003"
    );
    assert_eq!(rejects("MainThread.post 5\n0\n").code, "E1003");
    assert_eq!(
        rejects("let n: i64 = MainThread.is_current ()\n0\n").code,
        "E1003"
    );
    // The pipeline forms hand over the same owned function.
    accepts(
        "def work :: unit -> unit = \\() -> ()\n\
         Owned.function (\\() -> work ()) |> MainThread.post\n\
         Owned.function (\\() -> work ()) |> MainThread.after 5\n\
         0\n",
    );
}

// r2
#[test]
fn a_posted_function_captures_only_send_values() {
    let posted = rejects(
        "let shared = Rc.new 1i64\n\
         MainThread.post (Owned.function (\\() -> { let _ = Rc.get (ref shared); () }))\n\
         0\n",
    );
    let spawned = rejects(
        "let shared = Rc.new 1i64\n\
         Routine.go (Owned.function (\\() -> { let _ = Rc.get (ref shared); () }))\n\
         0\n",
    );
    assert_eq!(posted.code, "E1013", "{}", posted.message);
    assert_eq!(posted.kind, Some(DiagnosticKind::TaskRequiresSend.id()));
    // The same diagnostic as the function that `Routine.go` takes: the rule is `Owned.function`'s.
    assert_eq!(
        (posted.code, posted.kind),
        (spawned.code, spawned.kind),
        "{} / {}",
        posted.message,
        spawned.message
    );
}

// r3
#[test]
fn pure_code_cannot_post_or_ask_for_the_thread() {
    for body in [
        "MainThread.post (Owned.function (\\() -> ()))\n    n",
        "MainThread.after 1 (Owned.function (\\() -> ()))\n    n",
        "if MainThread.is_current () then n else 0",
    ] {
        let error = rejects(&format!(
            "pure def f :: i64 -> i64 = \\n ->\n    {body}\n\nf 1\n"
        ));
        assert_eq!(error.code, "E1029", "{body}\n{}", error.message);
    }
}

// r4
#[test]
fn the_module_name_is_reserved() {
    let error = analyze_modules(&[
        ("Main.tz", "0\n"),
        ("MainThread.tz", "def post :: unit -> i64 = \\() -> 1\n"),
    ])
    .expect_err("MainThread is a reserved module name");
    assert_eq!(error.code, "E1011", "{}", error.message);
    assert!(tsuzuri::stdlib::is_reserved_module("MainThread"));
}

const LOCAL_HANDLE: &str = "\
extern type local Window

@trusted
extern \"make_window\" def make_window :: unit -> Window

@trusted
extern \"close_window\" def close_window :: own Window -> unit

def touch :: unit -> unit = \\() ->
    let window = make_window ()
    close_window window
";

// r5, r7
#[test]
fn a_posted_function_may_touch_thread_bound_handles() {
    // The main thread is one fixed thread: a `pinned` function may be posted (D7).
    accepts(&format!(
        "{LOCAL_HANDLE}\nMainThread.post (Owned.function (\\() -> touch ()))\n\
         MainThread.after 5 (Owned.function (\\() -> touch ()))\n\
         Owned.function (\\() -> touch ()) |> MainThread.post\n\
         Owned.function (\\() -> touch ()) |> MainThread.after 5\n0\n"
    ));
    // The edge does not carry `pinned`: a tzroutine that only posts is not pinned code.
    accepts(&format!(
        "{LOCAL_HANDLE}\ntzroutine {{\n    MainThread.post (Owned.function (\\() -> touch ()))\n}}\n0\n"
    ));
    accepts(&format!(
        "{LOCAL_HANDLE}\ndef hand_over :: unit -> unit = \\() ->\n    MainThread.after 5 (Owned.function (\\() -> touch ()))\n\n\
         tzroutine {{\n    hand_over ()\n}}\nRoutine.go (Owned.function (\\() -> hand_over ()))\n0\n"
    ));
}

// r6
#[test]
fn spawning_the_same_function_is_still_rejected() {
    let timer = rejects(&format!(
        "{LOCAL_HANDLE}\nlet _timer = Time.after_func 5 (Owned.function (\\() -> touch ()))\n0\n"
    ));
    assert_eq!(timer.code, "E1013", "{}", timer.message);
    assert_eq!(timer.kind, Some(DiagnosticKind::PinnedCodeSpawned.id()));
    let routine = rejects(&format!(
        "{LOCAL_HANDLE}\nRoutine.go (Owned.function (\\() -> touch ()))\n0\n"
    ));
    assert_eq!(routine.kind, Some(DiagnosticKind::PinnedCodeSpawned.id()));
    // Only the function directly inside `Owned.function` is the main thread's: an owned function
    // made elsewhere is a function value, which may reach any thread.
    let value = rejects(&format!(
        "{LOCAL_HANDLE}\nlet later = Owned.function (\\() -> touch ())\nMainThread.post later\n0\n"
    ));
    assert_eq!(value.code, "E1013", "{}", value.message);
    assert_eq!(value.kind, Some(DiagnosticKind::PinnedFunctionValue.id()));
    // A tzroutine that touches the handle itself is pinned code, posted or not.
    let direct = rejects(&format!(
        "{LOCAL_HANDLE}\ntzroutine {{\n    touch ()\n    MainThread.post (Owned.function (\\() -> touch ()))\n}}\n0\n"
    ));
    assert_eq!(direct.kind, Some(DiagnosticKind::PinnedCodeSpawned.id()));
}

// r8
#[test]
fn webassembly_has_no_main_thread() {
    for target in ["wasm32", "wasm64"] {
        for emit in [&["--emit", "wasm"][..], &["--emit", "llvm"][..]] {
            let mut arguments = vec!["--target", target];
            arguments.extend_from_slice(emit);
            let (ok, text) = build_json("wasm", POSTS, &arguments);
            assert!(!ok, "{target} {emit:?}\n{text}");
            assert!(text.contains("\"code\":\"E2000\""), "{text}");
            assert!(
                text.contains("\"kind\":\"wasm_main_thread_unsupported\""),
                "{text}"
            );
        }
    }
}

// r9
#[test]
fn other_native_targets_refuse_the_build() {
    for target in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
    ] {
        let (ok, text) = build_json("native", POSTS, &["--target", target, "--emit", "object"]);
        assert!(!ok, "{target}\n{text}");
        assert!(text.contains("\"code\":\"E2002\""), "{target}\n{text}");
        assert!(
            text.contains("\"kind\":\"platform_unsupported_api\""),
            "{target}\n{text}"
        );
        assert!(text.contains("MainThread"), "{target}\n{text}");
        // `--emit llvm` writes the IR for any target: it links nothing.
        let (ok, text) = build_json(
            "native-llvm",
            POSTS,
            &["--target", target, "--emit", "llvm"],
        );
        assert!(ok, "{target}\n{text}");
    }
    // `is_current` alone needs the same runtime (D3).
    let (ok, text) = build_json(
        "native-current",
        "def main :: unit -> i32 = \\() -> if MainThread.is_current () then 0 else 1\n",
        &["--target", "x86_64-unknown-linux-gnu", "--emit", "object"],
    );
    assert!(
        !ok && text.contains("\"kind\":\"platform_unsupported_api\""),
        "{text}"
    );
}

// r10
#[test]
fn trap_mode_return_and_freestanding_exclude_main_thread() {
    let (ok, text) = build_json(
        "trap-return",
        POSTS,
        &["--trap-mode", "return", "--emit", "object"],
    );
    assert!(!ok, "{text}");
    assert!(text.contains("\"code\":\"E2000\""), "{text}");
    assert!(text.contains("\"kind\":\"option_conflict\""), "{text}");
    assert!(
        text.contains("--trap-mode return cannot be combined with MainThread"),
        "{text}"
    );
    let (ok, text) = build_json(
        "freestanding",
        POSTS,
        &["--freestanding", "--allocator", "host", "--emit", "object"],
    );
    assert!(!ok, "{text}");
    assert!(text.contains("\"code\":\"E2000\""), "{text}");
    assert!(text.contains("\"kind\":\"option_conflict\""), "{text}");
    assert!(
        text.contains("--freestanding cannot use MainThread"),
        "{text}"
    );
}

// r11
#[test]
fn a_program_without_main_thread_names_none_of_it() {
    let source = "\
def work :: unit -> unit = \\() -> ()

test \"spawns\" =
    tzroutine { work () }
    assert (1 == 1)

def main :: unit -> i32 = \\() ->
    tzroutine { work () }
    let _timer = Time.after_func 5 (Owned.function (\\() -> work ()))
    0
";
    let module = accepts(source);
    for ir in [
        llvm::emit(&module, llvm::Entry::Library).unwrap(),
        llvm::emit(&module, llvm::Entry::Console).unwrap(),
        llvm::emit(&module, llvm::Entry::TestRunner).unwrap(),
        llvm::emit_target(&module, llvm::Entry::Library, true).unwrap(),
    ] {
        assert!(!ir.contains("mainthread"), "{ir}");
    }
    assert!(!llvm::uses_main_thread(&module));
}

// r12
#[test]
fn the_ir_posts_through_one_trampoline_and_drops_at_the_end_of_a_boundary() {
    let module = accepts(POSTS);
    assert!(llvm::uses_main_thread(&module));
    let console = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(console, llvm::emit(&module, llvm::Entry::Console).unwrap());
    assert_eq!(
        count(
            &console,
            "declare void @tsuzuri_mainthread_post(i64, ptr, ptr, ptr)\n"
        ),
        1,
        "{console}"
    );
    assert_eq!(
        count(&console, "declare i32 @tsuzuri_mainthread_is_current()\n"),
        1
    );
    assert_eq!(
        count(
            &console,
            "declare void @tsuzuri_mainthread_boundary_done()\n"
        ),
        1
    );
    assert_eq!(
        count(&console, "define internal void @tz.mainthread.run("),
        1
    );
    assert_eq!(
        count(&console, "define internal void @tz.tzroutine.owned.drop("),
        1
    );
    assert_eq!(
        count(
            &console,
            "ptr @tz.mainthread.run, ptr @tz.tzroutine.owned.drop, ptr "
        ),
        2,
        "{console}"
    );
    assert!(console.contains("call void @tsuzuri_mainthread_post(i64 0, ptr @tz.mainthread.run"));
    // The trampoline is no callback of an `extern`: nothing treats it as one.
    assert!(!console.contains("@tz.callback."), "{console}");
    // This program starts no tzroutine: no scope, and the drop is right after the body.
    let main = definition(&console, "main");
    let body = main.find("call i32 @tz.fn.").unwrap();
    let done = main
        .find("call void @tsuzuri_mainthread_boundary_done()")
        .unwrap();
    assert!(body < done, "{main}");
    assert!(!main.contains("tsuzuri_tz_"), "{main}");
    assert!(!definition(&console, "tz.mainthread.run").contains("tsuzuri_tz_"));

    // A library keeps what was posted for the main thread of its host (D10).
    let library = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(library.contains("call void @tsuzuri_mainthread_post("));
    assert!(!library.contains("boundary_done"), "{library}");
    assert!(!library.contains("boundary_open"), "{library}");
}

#[test]
fn a_program_that_spawns_drops_the_posted_functions_before_the_wait() {
    let source = "\
def work :: unit -> unit = \\() -> ()

def main :: unit -> i32 = \\() ->
    tzroutine {
        MainThread.post (Owned.function (\\() -> work ()))
    }
    0
";
    let module = accepts(source);
    let console = llvm::emit(&module, llvm::Entry::Console).unwrap();
    let main = definition(&console, "main");
    let body = main.find("call i32 @tz.fn.").unwrap();
    let done = main
        .find("call void @tsuzuri_mainthread_boundary_done()")
        .unwrap();
    let leave = main.find("call void @tsuzuri_tz_boundary_leave(").unwrap();
    assert!(body < done && done < leave, "{main}");
    // The posted function runs under the boundary rule of a static callback (D5).
    let run = definition(&console, "tz.mainthread.run");
    let enter = run
        .find("call ptr @tsuzuri_tz_callback_scope_enter()")
        .unwrap();
    let call = run.find("call i8 %code(").unwrap();
    let drop = run.find("call void @tz.closure.drop(").unwrap();
    let scope_leave = run
        .find("call void @tsuzuri_tz_callback_scope_leave(")
        .unwrap();
    assert!(enter < call && call < drop && drop < scope_leave, "{run}");
    assert_eq!(
        count(&console, "declare ptr @tsuzuri_tz_callback_scope_enter()\n"),
        1
    );
    assert_eq!(
        count(
            &console,
            "declare void @tsuzuri_tz_callback_scope_leave(ptr)\n"
        ),
        1
    );
}

#[test]
fn the_other_entries_drop_at_the_end_of_their_boundaries() {
    // An IO entry: `@main` only calls `tsuzuri_main`, whose returns are the boundary's.
    let io = accepts(
        "def work :: unit -> unit = \\() -> ()\n\n\
         MainThread.post (Owned.function (\\() -> work ()))\nIO.writeln \"start\"\n",
    );
    let console = llvm::emit(&io, llvm::Entry::Console).unwrap();
    let entry = definition(&console, "tsuzuri_main");
    assert!(
        count(entry, "call void @tsuzuri_mainthread_boundary_done()") >= 1,
        "{entry}"
    );
    for (at, _) in entry.match_indices("  ret ") {
        let before = entry[..at].trim_end();
        assert!(
            before.ends_with("call void @tsuzuri_mainthread_boundary_done()"),
            "{entry}"
        );
    }
    assert!(!definition(&console, "main").contains("boundary_done"));
    assert!(
        !llvm::emit(&io, llvm::Entry::Library)
            .unwrap()
            .contains("boundary_done")
    );

    // A top-level expression: the drop comes before the result is printed.
    let expression = accepts(
        "def work :: unit -> unit = \\() -> ()\n\nMainThread.post (Owned.function (\\() -> work ()))\n42\n",
    );
    let console = llvm::emit(&expression, llvm::Entry::Console).unwrap();
    let main = definition(&console, "main");
    let done = main
        .find("call void @tsuzuri_mainthread_boundary_done()")
        .unwrap();
    assert!(main.find(" @tz.fn.").unwrap() < done, "{main}");
    assert!(done < main.find("@tz.console.write").unwrap(), "{main}");

    // A test and a bench sample are boundaries too; a sample opens what the one before closed.
    let runners = accepts(
        "def work :: unit -> unit = \\() -> ()\n\n\
         test \"posts\" =\n    MainThread.post (Owned.function (\\() -> work ()))\n    assert (1 == 1)\n\n\
         test \"plain\" =\n    assert (1 == 1)\n\n\
         bench \"posts\" = Bench.of (\\_ -> MainThread.post (Owned.function (\\() -> work ())))\n\n0\n",
    );
    let tests = llvm::emit(&runners, llvm::Entry::TestRunner).unwrap();
    let run = definition(&tests, "tsuzuri_test_run");
    assert_eq!(
        count(run, "call void @tsuzuri_mainthread_boundary_done()"),
        2,
        "{run}"
    );
    assert!(!tests.contains("boundary_open"), "{tests}");
    let benches = llvm::emit_bench_runner(&runners, &[0]).unwrap();
    let sample = definition(&benches, "tsuzuri_bench_sample");
    let open = sample
        .find("call void @tsuzuri_mainthread_boundary_open()")
        .unwrap();
    assert!(open < sample.find("switch i32 %index").unwrap(), "{sample}");
    let body = sample.find("call i64 @tz.fn.").unwrap();
    let done = sample
        .find("call void @tsuzuri_mainthread_boundary_done()")
        .unwrap();
    assert!(body < done, "{sample}");
    assert_eq!(
        count(
            &benches,
            "declare void @tsuzuri_mainthread_boundary_open()\n"
        ),
        1
    );
    assert_eq!(benches, llvm::emit_bench_runner(&runners, &[0]).unwrap());
}
