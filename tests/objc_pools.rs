//! M10: `ObjC.with_autorelease_pool`, the pool of the `@main` of an Objective-C image, and the
//! runtime macro that makes the scheduler run a pool around each run of a tzroutine.
//!
//! The expected shapes are written by hand from the ticket (the order lock, push, call, drop,
//! pop, unlock; the push before the entry and the pop between the wait for the tzroutines and
//! `@tz.main.done`). `tests/objc_pools.mjs` runs the pools against libobjc on a Mac.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::check::CheckedModule;
use tsuzuri::check::effects::{self, Effect, EffectSet};
use tsuzuri::diagnostic::{Diagnostic, DiagnosticKind};
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, llvm};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tsuzuri")
}

fn temp_project(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-m10-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        fs::write(root.join(path), text).unwrap();
    }
    root
}

fn accepts(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message))
}

fn check_err(source: &str) -> Diagnostic {
    analyze(source).expect_err(source)
}

fn apple(module: &CheckedModule, entry: llvm::Entry) -> String {
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let ir = llvm::emit(module, entry).unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // The IR is deterministic.
    assert_eq!(ir, llvm::emit(module, entry).unwrap());
    assert!(
        llvm::duplicate_declarations(&ir).is_empty(),
        "{:?}",
        llvm::duplicate_declarations(&ir)
    );
    ir
}

/// The lines of the definition whose `define` line contains `name`, without the braces.
fn body<'a>(ir: &'a str, name: &str) -> Vec<&'a str> {
    let mut lines = ir.lines().skip_while(|line| {
        !(line.starts_with("define ") && line.contains(name) && line.ends_with('{'))
    });
    assert!(lines.next().is_some(), "no definition of {name}\n{ir}");
    lines.take_while(|line| *line != "}").collect()
}

/// The position of the first line of `lines` that contains `needle`.
fn at(lines: &[&str], needle: &str) -> usize {
    lines
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line with {needle}\n{}", lines.join("\n")))
}

/// A class that is bound to its thread, a class of the main thread and a class that any thread
/// may use, and code that touches each.
const CLASSES: &str = "\
@trusted
extern class NSObject : ObjC.Object

@trusted
extern class TzProbe : NSObject

@trusted
extern class main TzView : NSObject

@trusted
extern \"tzobjc_make\" def make :: unit -> TzProbe

@trusted
extern \"tzobjc_tag\" def tag :: ref TzProbe -> i64

@trusted
extern \"tzobjc_make_view\" def make_view :: unit -> TzView

def touch :: unit -> i64 = \\() ->
    let probe = make ()
    tag (ref probe)

def view :: unit -> unit = \\() ->
    let _view = make_view ()
    ()

@trusted
extern class sync TzShared : NSObject

@trusted
extern \"tzobjc_make_shared\" def make_shared :: unit -> TzShared

@trusted
extern \"tzobjc_shared_tag\" def shared_tag :: ref TzShared -> i64

def shared :: unit -> i64 = \\() ->
    let object = make_shared ()
    shared_tag (ref object)
";

fn with_classes(rest: &str) -> String {
    format!("{CLASSES}\n{rest}")
}

#[test]
fn the_pool_calls_its_function_and_returns_its_result() {
    let module =
        accepts("def f :: unit -> i64 = \\() -> ObjC.with_autorelease_pool (\\() -> 1)\n0\n");
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .unwrap();
    assert_eq!(f.signature.result, tsuzuri::check::Type::I64);
    // The result is whatever the function returns, an owned value too.
    accepts("def f :: unit -> string = \\() -> ObjC.with_autorelease_pool (\\() -> \"text\")\n0\n");
    accepts("def f :: unit -> unit = \\() -> ObjC.with_autorelease_pool (\\() -> ())\n0\n");
    // The pipeline form and the function value.
    accepts(
        "def f :: unit -> i64 = \\() -> (\\() -> 1) |> ObjC.with_autorelease_pool\n\
         def g :: unit -> i64 = \\() ->\n    let pool = ObjC.with_autorelease_pool\n    pool (\\() -> 2)\n0\n",
    );
    // The argument is a function of `unit`.
    for source in [
        "let x = ObjC.with_autorelease_pool 1\n0\n",
        "let x = ObjC.with_autorelease_pool (\\(n: i64) -> n)\n0\n",
        "let x: string = ObjC.with_autorelease_pool (\\() -> 1)\n0\n",
    ] {
        assert_eq!(check_err(source).code, "E1003", "{source}");
    }
}

#[test]
fn the_pool_is_a_pinning_boundary() {
    // Code that touches a thread-bound class runs inside the pool, and `pinned` stops there.
    let module = accepts(&with_classes(
        "\
def pooled :: unit -> i64 = \\() -> ObjC.with_autorelease_pool (\\() -> touch ())

def piped :: unit -> i64 = \\() -> (\\() -> touch ()) |> ObjC.with_autorelease_pool

def named :: unit -> i64 = \\() -> ObjC.with_autorelease_pool touch

def main :: unit -> i32 = \\() ->
    tzroutine {
        let _a = pooled ()
        let _b = piped ()
        let _c = named ()
        let _d = ObjC.with_autorelease_pool (\\() -> touch ())
    }
    0
",
    ));
    let summary = effects::summarize(
        &module,
        effects::Options::new(effects::Scope::All, EffectSet::ALL),
    )
    .unwrap();
    let effects_of = |name: &str| {
        let id = module
            .functions
            .iter()
            .position(|function| function.name == name)
            .unwrap_or_else(|| panic!("no function {name}"));
        summary.summary(id).effects
    };
    assert!(effects_of("touch").contains(Effect::Pinned));
    for name in ["pooled", "piped", "named"] {
        assert!(!effects_of(name).contains(Effect::Pinned), "{name}");
        // The pool calls the Objective-C runtime.
        assert!(effects_of(name).contains(Effect::Extern), "{name}");
    }
    // The same code straight in a tzroutine may run on any thread.
    let spawned = check_err(&with_classes(
        "def main :: unit -> i32 = \\() ->\n    tzroutine {\n        let _a = touch ()\n    }\n    0\n",
    ));
    assert_eq!(spawned.code, "E1013", "{}", spawned.message);
    assert_eq!(spawned.kind, Some(DiagnosticKind::PinnedCodeSpawned.id()));
    // A pool keeps some thread, not the main thread: code of the main thread stays out of a
    // tzroutine, and runs in a pool of `main`.
    let main_thread = check_err(&with_classes(
        "def main :: unit -> i32 = \\() ->\n    tzroutine {\n        ObjC.with_autorelease_pool (\\() -> view ())\n    }\n    0\n",
    ));
    assert_eq!(
        main_thread.kind,
        Some(DiagnosticKind::MainThreadCodeSpawned.id()),
        "{}",
        main_thread.message
    );
    accepts(&with_classes(
        "def main :: unit -> i32 = \\() ->\n    ObjC.with_autorelease_pool (\\() -> view ())\n    0\n",
    ));
    // The value of the function is not one that escapes with `pinned` code in it.
    let escaping = check_err(&with_classes(
        "let hold = \\() -> touch ()\nlet _n = ObjC.with_autorelease_pool hold\n0\n",
    ));
    assert_eq!(
        escaping.kind,
        Some(DiagnosticKind::PinnedFunctionValue.id()),
        "{}",
        escaping.message
    );
}

#[test]
fn the_pool_has_the_effect_extern() {
    let impure = check_err(
        "pure def f :: unit -> i64 = \\() -> ObjC.with_autorelease_pool (\\() -> 1)\n0\n",
    );
    assert_eq!(impure.code, "E1029", "{}", impure.message);
    assert!(
        impure.message.contains("ObjC.with_autorelease_pool"),
        "{}",
        impure.message
    );
    // The effect is `extern` in a program that names no class too, and nothing else of the
    // pool: no allocation and no trap.
    let module =
        accepts("def f :: unit -> i64 = \\() -> ObjC.with_autorelease_pool (\\() -> 1)\n0\n");
    let summary = effects::summarize(
        &module,
        effects::Options::new(effects::Scope::All, EffectSet::ALL),
    )
    .unwrap();
    let id = module
        .functions
        .iter()
        .position(|function| function.name == "f")
        .unwrap();
    let found = summary.summary(id).effects;
    assert!(found.contains(Effect::Extern));
    for absent in [Effect::Alloc, Effect::Trap, Effect::Task, Effect::Pinned] {
        assert!(!found.contains(absent), "{absent:?}");
    }
    // As for `ObjC.retain`, a checked attribute does not hold over a call of the runtime.
    let attribute = check_err(
        "@no_alloc\ndef f :: unit -> i64 = \\() -> ObjC.with_autorelease_pool (\\() -> 1)\n0\n",
    );
    assert_eq!(
        attribute.kind,
        Some(DiagnosticKind::EffectExternInAttribute.id()),
        "{}",
        attribute.message
    );
}

#[test]
fn the_pool_needs_the_capability_ffi() {
    let root = temp_project(
        "ffi",
        &[
            (
                "Tsuzuri.toml",
                "[package]\nname = \"pools\"\nversion = \"0.1.0\"\n\n[capabilities]\nallow = []\n",
            ),
            (
                "Main.tz",
                "def main :: unit -> i32 = \\() -> ObjC.with_autorelease_pool (\\() -> 0)\n",
            ),
        ],
    );
    let output = Command::new(bin())
        .arg("check")
        .arg(&root)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains(
            "the root package may not use capability 'ffi' ('ObjC.with_autorelease_pool'); add \"ffi\" to allow in [capabilities]"
        ),
        "{text}"
    );
    // The built-in function has no declaration of the program: no `W1010`.
    assert!(!text.contains("W1010"), "{text}");
    let _ = fs::remove_dir_all(root);
}

const POOL_ONLY: &str =
    "export def run :: own i64 -> i64\nfn run n = ObjC.with_autorelease_pool (\\() -> n + 1)\n";

#[test]
fn pool_builtin_pins_only_where_the_scheduler_runs() {
    // No tzroutine and no `Parallel.*`: every caller is a thread of its own, so the push and
    // the pop are all, and the tzroutine runtime is not linked.
    let ir = apple(&accepts(POOL_ONLY), llvm::Entry::Library);
    assert!(!ir.contains("tsuzuri_tz_"), "{ir}");
    let lines = body(&ir, "@tz.builtin.ObjC.with_autorelease_pool.");
    let push = at(&lines, "call ptr @objc_autoreleasePoolPush()");
    let drop = at(&lines, "call void @tz.closure.drop(");
    let pop = at(&lines, "call void @objc_autoreleasePoolPop(ptr ");
    assert!(push < drop && drop < pop, "{}", lines.join("\n"));
    // The call of the function is between the push and the drop of its closure.
    assert!(
        lines[push + 1..drop]
            .iter()
            .any(|line| line.contains(" = call i64 %")),
        "{}",
        lines.join("\n")
    );
    // A program with only a pool is an Objective-C image: it links libobjc.
    assert!(llvm::uses_objc(&ir), "{ir}");
    for declaration in [
        "declare ptr @objc_autoreleasePoolPush()",
        "declare void @objc_autoreleasePoolPop(ptr)",
    ] {
        assert_eq!(
            ir.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}\n{ir}"
        );
    }

    // A program that starts a tzroutine, and one whose `Parallel.*` runs chunks on helper
    // tzroutines: the thread is kept from before the push to after the pop.
    for program in [
        "def work :: unit -> i64 = \\() -> ObjC.with_autorelease_pool (\\() -> 1)\n\
         def main :: unit -> i32 = \\() ->\n    tzroutine {\n        let _n = work ()\n    }\n    0\n",
        "def main :: unit -> i32 = \\() ->\n    let squares = Parallel.map (\\(n: i64) -> ObjC.with_autorelease_pool (\\() -> n * n)) [1, 2, 3]\n    if squares[0] == 1 then 0 else 1\n",
    ] {
        let ir = apple(&accepts(program), llvm::Entry::Console);
        let lines = body(&ir, "@tz.builtin.ObjC.with_autorelease_pool.");
        let lock = at(&lines, "call void @tsuzuri_tz_lock_thread()");
        let push = at(&lines, "call ptr @objc_autoreleasePoolPush()");
        let drop = at(&lines, "call void @tz.closure.drop(");
        let pop = at(&lines, "call void @objc_autoreleasePoolPop(ptr ");
        let unlock = at(&lines, "call void @tsuzuri_tz_unlock_thread()");
        assert!(
            lock < push && push < drop && drop < pop && pop < unlock,
            "{program}\n{}",
            lines.join("\n")
        );
    }
}

#[test]
fn the_pool_shares_a_declaration_with_the_program() {
    // The program declares the two functions itself, with `i64` for the pool: the IR keeps those
    // lines, and the compiler's calls use its own types.
    let ir = apple(
        &accepts(
            "\
@trusted
extern \"objc_autoreleasePoolPush\" def pool_push :: unit -> i64

@trusted
extern \"objc_autoreleasePoolPop\" def pool_pop :: i64 -> unit

export def run :: own i64 -> i64
fn run n =
    let pool = pool_push ()
    let m = ObjC.with_autorelease_pool (\\() -> n + 1)
    pool_pop pool
    m
",
        ),
        llvm::Entry::Library,
    );
    let declarations: Vec<&str> = ir
        .lines()
        .filter(|line| line.starts_with("declare ") && line.contains("@objc_autoreleasePool"))
        .collect();
    assert_eq!(
        declarations,
        vec![
            "declare i64 @objc_autoreleasePoolPush()",
            "declare void @objc_autoreleasePoolPop(i64)"
        ],
        "{ir}"
    );
    assert!(ir.contains("call ptr @objc_autoreleasePoolPush()"), "{ir}");
    assert!(
        ir.contains("call void @objc_autoreleasePoolPop(ptr %"),
        "{ir}"
    );
}

/// The `@main` of the console entry of `source`.
fn console_main(source: &str) -> (String, Vec<String>) {
    let ir = apple(&accepts(source), llvm::Entry::Console);
    let lines = body(&ir, " @main(")
        .into_iter()
        .map(str::to_owned)
        .collect();
    (ir, lines)
}

#[test]
fn main_pool_only_in_programs_that_use_objc() {
    const PUSH: &str = "  %tz.pool = call ptr @objc_autoreleasePoolPush()";
    const POP: &str = "  call void @objc_autoreleasePoolPop(ptr %tz.pool)";
    let position = |lines: &[String], needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line with {needle}\n{}", lines.join("\n")))
    };

    // `def main` of a program that starts a tzroutine and hands the host a callback: the pop is
    // after the wait for the tzroutines and before the mark that `main` is over.
    let (ir, lines) = console_main(&with_classes(
        "@trusted\nextern \"tzobjc_each\" def each :: (i64 -> i64) -> i64\n\n\
         def bump :: i64 -> i64 = \\n -> n + 1\n\n\
         def main :: unit -> i32 = \\() ->\n    let _n = touch () + each bump\n    tzroutine {\n        let _m = ObjC.with_autorelease_pool (\\() -> touch ())\n    }\n    0\n",
    ));
    let enter = position(&lines, "@tsuzuri_tz_scope_enter()");
    let push = position(&lines, PUSH);
    let call = position(&lines, "%code = call i32 @tz.fn.Main.main(");
    let leave = position(&lines, "@tsuzuri_tz_boundary_leave(");
    let pop = position(&lines, POP);
    let done = position(&lines, "@tz.main.done");
    assert!(
        enter < push && push + 1 == call && call < leave && leave < pop && pop + 1 == done,
        "{}",
        lines.join("\n")
    );
    assert_eq!(lines[push], PUSH);
    assert_eq!(lines[pop], POP);
    assert!(llvm::uses_objc(&ir));

    // `def main` without a tzroutine.
    let (_, lines) = console_main(&with_classes(
        "def main :: unit -> i32 = \\() ->\n    let _n = touch ()\n    0\n",
    ));
    let push = position(&lines, PUSH);
    let call = position(&lines, "%code = call i32 @tz.fn.Main.main(");
    let pop = position(&lines, POP);
    assert!(push + 1 == call && call < pop, "{}", lines.join("\n"));
    assert!(
        !lines.iter().any(|line| line.contains("tsuzuri_tz_")),
        "{}",
        lines.join("\n")
    );

    // The IO entry: the wait is inside `tsuzuri_main`.
    let (_, lines) = console_main(&with_classes(
        "IO {\n    let n = shared ()\n    do! IO.writeln $\"{n}\"\n}\n",
    ));
    let push = position(&lines, PUSH);
    let call = position(&lines, "%result = call i32 @tsuzuri_main()");
    let pop = position(&lines, POP);
    assert!(push + 1 == call && call + 1 == pop, "{}", lines.join("\n"));

    // A top level that returns a value, which `@main` prints after the pop.
    let (_, lines) = console_main(&with_classes("touch ()\n"));
    let push = position(&lines, PUSH);
    let call = position(&lines, "%result = call i64 @tz.fn.");
    let pop = position(&lines, POP);
    let print = position(&lines, "@tz.console.write(");
    assert!(
        push + 1 == call && call < pop && pop < print,
        "{}",
        lines.join("\n")
    );

    // A program whose only use of Objective-C is a pool.
    let (ir, lines) =
        console_main("def main :: unit -> i32 = \\() -> ObjC.with_autorelease_pool (\\() -> 0)\n");
    assert!(lines.iter().any(|line| line == PUSH), "{ir}");
    assert!(lines.iter().any(|line| line == POP), "{ir}");

    // A program without Objective-C, and one that only declares the two functions itself: no
    // pool, no flag, and the `@main` of the same program without the declarations.
    let plain =
        "def main :: unit -> i32 = \\() ->\n    tzroutine {\n        let _n = 1\n    }\n    0\n";
    let declared = format!(
        "@trusted\nextern \"objc_autoreleasePoolPush\" def pool_push :: unit -> i64\n\n\
         @trusted\nextern \"objc_autoreleasePoolPop\" def pool_pop :: i64 -> unit\n\n{plain}"
    );
    let called = "\
@trusted
extern \"objc_autoreleasePoolPush\" def pool_push :: unit -> i64

@trusted
extern \"objc_autoreleasePoolPop\" def pool_pop :: i64 -> unit

def main :: unit -> i32 = \\() ->
    let pool = pool_push ()
    pool_pop pool
    0
";
    let (plain_ir, plain_main) = console_main(plain);
    let (declared_ir, declared_main) = console_main(&declared);
    assert_eq!(plain_main, declared_main);
    // The declarations that nothing calls leave no line (only the numbers of the lifted
    // functions differ between the two programs).
    assert!(!declared_ir.contains("objc_"), "{declared_ir}");
    let (called_ir, called_main) = console_main(called);
    for (ir, lines) in [(&plain_ir, &plain_main), (&called_ir, &called_main)] {
        assert!(!llvm::uses_objc(ir), "{ir}");
        assert!(
            !lines.iter().any(|line| line.contains("objc_")),
            "{}",
            lines.join("\n")
        );
    }
    // A library's `tsuzuri_main`, which its host calls, has no pool of its own.
    let library = apple(
        &accepts(&with_classes(
            "def main :: unit -> i32 = \\() ->\n    let _n = touch ()\n    0\n",
        )),
        llvm::Entry::Library,
    );
    assert!(!library.contains("objc_autoreleasePool"), "{library}");
}

#[test]
fn a_builtin_body_that_releases_declares_the_release() {
    // The body of `ignore` for an object releases it. It is emitted apart from the functions of
    // the program, and what it uses of Objective-C still reaches the module.
    let ir = apple(
        &accepts(&with_classes(
            "export def run :: i64 = \\() ->\n    ignore (make ())\n    0\n",
        )),
        llvm::Entry::Library,
    );
    assert!(ir.contains("call void @objc_release(ptr "), "{ir}");
    assert_eq!(
        ir.lines()
            .filter(|line| *line == "declare void @objc_release(ptr)")
            .count(),
        1,
        "{ir}"
    );
    assert!(llvm::uses_objc(&ir));
}

#[test]
fn only_an_apple_target_reaches_the_pool() {
    let called = accepts(POOL_ONLY);
    let value = accepts(
        "export def run :: own i64 -> i64\nfn run n =\n    let pool = ObjC.with_autorelease_pool\n    pool (\\() -> n + 1)\n",
    );
    for module in [&called, &value] {
        let wasm = llvm::emit_target(module, llvm::Entry::Library, true).unwrap_err();
        assert_eq!(wasm.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
        assert_eq!(wasm.code, "E1008");
        for triple in [
            Triple::X86_64LinuxGnu,
            Triple::Aarch64LinuxGnu,
            Triple::X86_64WindowsMsvc,
        ] {
            let _guard = PlatformScope::enter(Platform::new(Some(triple)));
            let other = llvm::emit(module, llvm::Entry::Library).unwrap_err();
            assert_eq!(other.kind, Some(DiagnosticKind::AbiAppleOnly.id()));
            assert_eq!(other.code, "E1008");
        }
    }
}

#[test]
fn a_plain_retain_is_inside_the_blocking_call() {
    // One symbol declared with an `i64` result and with a class result (W14): the canonical C
    // result is `i64`, which cannot carry the attached-call bundle, so the +0 result is retained
    // by a plain call after its `inttoptr`. Until that retain the autorelease pool alone holds
    // the object, and the scheduler of an Objective-C image pops the pool of a tzroutine that
    // gives its thread back: the retain is inside the blocking call, where the tzroutine is not
    // preempted and keeps its thread.
    let source = with_classes(
        "\
@trusted
extern \"tzobjc_raw\" def raw :: unit -> i64

@trusted
extern \"tzobjc_raw\" def typed :: unit -> TzShared

def work :: unit -> i64 = \\() ->
    let object = typed ()
    raw () + shared_tag (ref object)

def main :: unit -> i32 = \\() ->
    tzroutine {
        let _n = work ()
    }
    0
",
    );
    let ir = apple(&accepts(&source), llvm::Entry::Console);
    let lines: Vec<&str> = ir.lines().collect();
    let retain = at(&lines, "call ptr @objc_retainAutoreleasedReturnValue(ptr %");
    // enter, the call, its `inttoptr`, the retain, exit: one blocking call.
    assert!(
        lines[retain - 3].contains("call void @tsuzuri_tz_blocking_enter()"),
        "{}",
        lines[retain - 3..=retain + 1].join("\n")
    );
    assert!(
        lines[retain - 2].contains("call i64 @tzobjc_raw()"),
        "{}",
        lines[retain - 2]
    );
    assert!(
        lines[retain - 1].contains("inttoptr i64 "),
        "{}",
        lines[retain - 1]
    );
    assert!(
        lines[retain + 1].contains("call void @tsuzuri_tz_blocking_exit()"),
        "{}",
        lines[retain + 1]
    );
    // The pass that brackets extern calls did not bracket the call a second time.
    assert!(
        !lines[retain - 4].contains("@tsuzuri_tz_blocking_enter"),
        "{}",
        lines[retain - 4]
    );
    for declaration in [
        "declare void @tsuzuri_tz_blocking_enter()",
        "declare void @tsuzuri_tz_blocking_exit()",
    ] {
        assert_eq!(
            ir.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}"
        );
    }
    // A program whose code never runs on a tzroutine of the scheduler has no bracket at all.
    let library = apple(
        &accepts(&with_classes(
            "\
@trusted
extern \"tzobjc_raw\" def raw :: unit -> i64

@trusted
extern \"tzobjc_raw\" def typed :: unit -> TzShared

export def work :: i64 = \\() ->
    let object = typed ()
    raw () + shared_tag (ref object)
",
        )),
        llvm::Entry::Library,
    );
    assert!(
        library.contains("call ptr @objc_retainAutoreleasedReturnValue(ptr %"),
        "{library}"
    );
    assert!(!library.contains("tsuzuri_tz_blocking"), "{library}");
}

#[test]
fn trap_mode_return_excludes_the_pool() {
    // A trap that returns to the host would skip the pop and leave the pool, with what it holds,
    // on the host's thread. The driver refuses the combination before it looks at the target.
    for source in [
        POOL_ONLY,
        "export def run :: own i64 -> i64\nfn run n =\n    let pool = ObjC.with_autorelease_pool\n    pool (\\() -> n + 1)\n",
    ] {
        let root = temp_project("trap-return", &[("Main.tz", source)]);
        let output = Command::new(bin())
            .arg("build")
            .arg(&root)
            .args(["--trap-mode", "return", "--emit", "object", "--json", "-o"])
            .arg(root.join("out.o"))
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.status.success(), "{text}");
        assert!(text.contains("\"code\":\"E2000\""), "{text}");
        assert!(text.contains("\"kind\":\"option_conflict\""), "{text}");
        assert!(
            text.contains("--trap-mode return cannot be combined with ObjC.with_autorelease_pool"),
            "{text}"
        );
        let _ = fs::remove_dir_all(root);
    }
}
