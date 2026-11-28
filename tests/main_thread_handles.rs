//! M09 Phase 2: handles bound to the process main thread (`extern type main`, `extern class
//! main`), the effect `main_thread`, the two `E1013` kinds and the entry check of the wrappers
//! that a host calls on a thread of its choice. The runs are in `tests/main_thread.mjs`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::api_diff;
use tsuzuri::check::CheckedModule;
use tsuzuri::check::effects::{self, Effect, EffectSet};
use tsuzuri::diagnostic::{Diagnostic, DiagnosticKind};
use tsuzuri::driver::Project;
use tsuzuri::formatter::format_source;
use tsuzuri::parser::parse;
use tsuzuri::syntax::{HandleKind, SourceKind};
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, llvm};

fn accepts(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message))
}

fn rejects(source: &str, kind: DiagnosticKind) -> Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(
        error.kind,
        Some(kind.id()),
        "{source}\n{}: {}",
        error.code,
        error.message
    );
    assert_eq!(error.code, kind.code(), "{source}");
    error
}

fn definition<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .find(&format!(" @{name}("))
        .map(|at| ir[..at].rfind("define ").unwrap())
        .unwrap_or_else(|| panic!("no definition of @{name}\n{ir}"));
    let end = start + ir[start..].find("\n}\n").unwrap() + 3;
    &ir[start..end]
}

fn temp_project(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-m09h-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        std::fs::write(root.join(path), text).unwrap();
    }
    root
}

fn api_of(path: &Path) -> api_diff::Api {
    api_diff::api(&Project::load_for_api(path).unwrap()).unwrap()
}

const WINDOW: &str = "\
extern type main Window

@trusted
extern \"make_window\" def make_window :: unit -> Window

@trusted
extern \"close_window\" def close_window :: own Window -> unit

@trusted
extern \"run_callback\" def run_callback :: (unit -> unit) -> unit

@trusted
extern \"take_pointer\" def take_pointer :: FnPtr<(unit -> unit)> -> unit

def touch :: unit -> unit = \\() ->
    let window = make_window ()
    close_window window

def idle :: unit -> unit = \\() -> ()
";

#[test]
fn main_is_a_contextual_modifier_of_extern_type() {
    let source = "\
extern type main Window
private extern type main Panel
extern type main
extern type copy main
extern type main local
0
";
    let program = parse(source).unwrap();
    let kinds: Vec<_> = program
        .extern_types
        .iter()
        .map(|decl| (decl.kind, decl.name.text.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (HandleKind::Main, "Window"),
            (HandleKind::Main, "Panel"),
            (HandleKind::Owned, "main"),
            (HandleKind::Copy, "main"),
            (HandleKind::Main, "local"),
        ]
    );
    assert_eq!(program.extern_types[0].header(), "extern type main Window");
    let formatted = format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert_eq!(formatted, source);
    for bad in [
        "extern type main copy H\n0\n",
        "extern type local main H\n0\n",
        "extern type main main H\n0\n",
    ] {
        let error = parse(bad).expect_err(bad);
        assert_eq!(error.code, "E0002", "{bad}");
        assert_eq!(
            error.kind,
            Some(DiagnosticKind::ExternTypeModifierConflict.id())
        );
        assert!(error.message.contains("'main'"), "{}", error.message);
    }
}

#[test]
fn main_is_a_modifier_of_extern_class() {
    let source = "\
@trusted
extern class main Window : ObjC.Object {
    class \"new\" make :: unit -> Self
    \"orderFront\" order_front :: unit -> unit
}

extern class main : ObjC.Object

0
";
    let program = parse(source).unwrap();
    assert!(program.extern_classes[0].main.is_some());
    assert!(program.extern_classes[0].sync.is_none());
    assert_eq!(
        program.extern_classes[0].header(),
        "extern class main Window : ObjC.Object"
    );
    // Without a name after it, `main` is the name of the class.
    assert!(program.extern_classes[1].main.is_none());
    assert_eq!(program.extern_classes[1].name.text, "main");
    let formatted = format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert_eq!(formatted, source);
    for bad in [
        "extern class main sync W : ObjC.Object\n0\n",
        "extern class sync main W : ObjC.Object\n0\n",
        "extern class main local W : ObjC.Object\n0\n",
    ] {
        let error = parse(bad).expect_err(bad);
        assert_eq!(error.code, "E0002", "{bad}");
        assert_eq!(
            error.kind,
            Some(DiagnosticKind::ExternTypeModifierConflict.id())
        );
    }
    // An object of a class is an object of its base.
    let error = rejects(
        "extern class main View : ObjC.Object\nextern class Button : View\n0\n",
        DiagnosticKind::ObjcClassMainBase,
    );
    assert!(
        error.message.contains("extern class main Button"),
        "{}",
        error.message
    );
    accepts("extern class main View : ObjC.Object\nextern class main Button : View\n0\n");
}

#[test]
fn a_main_handle_is_moved_and_stays_on_its_thread() {
    let twice = analyze(&format!(
        "{WINDOW}\nlet w = make_window ()\nclose_window w\nclose_window w\n0\n"
    ))
    .expect_err("moved twice");
    assert_eq!(twice.code, "E1012");
    rejects(
        "extern type main Window\nlet _ch: Channel<Window> = Channel.make 1\n0\n",
        DiagnosticKind::TaskRequiresSend,
    );
    rejects(
        &format!(
            "{WINDOW}\nlet w = make_window ()\nlet _out = Routine.scope (ref w) 2 (\\_s _i -> 0)\n0\n"
        ),
        DiagnosticKind::TaskRequiresSync,
    );
    rejects(
        &format!(
            "{WINDOW}\n@trusted\nextern \"use_window\" def use_window :: own Window -> i64\n\
             let w = make_window ()\nlet _f = \\x -> use_window w + x\n0\n"
        ),
        DiagnosticKind::CaptureUnsendableInFunctionValue,
    );
}

#[test]
fn the_effect_reaches_whoever_calls_the_code_but_not_through_an_entry() {
    let module = accepts(&format!(
        "{WINDOW}\n\
         def locked :: unit -> unit = \\() -> Routine.with_locked_thread (\\() -> touch ())\n\n\
         def posts :: unit -> unit = \\() -> MainThread.post (Owned.function (\\() -> touch ()))\n\n\
         def registers :: unit -> unit = \\() ->\n    run_callback touch\n    take_pointer (FnPtr.of touch)\n\n\
         def main :: unit -> i32 = \\() ->\n    touch ()\n    locked ()\n    posts ()\n    registers ()\n    idle ()\n    0\n"
    ));
    assert!(module.main_bound && module.thread_bound);
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
    // The code that touches the handle, and what calls it.
    for name in ["touch", "main"] {
        assert!(effects_of(name).contains(Effect::MainThread), "{name}");
        assert!(effects_of(name).contains(Effect::Pinned), "{name}");
    }
    // `Routine.with_locked_thread` fixes a thread, not the main thread: `pinned` stops there and
    // `main_thread` goes on.
    assert!(effects_of("locked").contains(Effect::MainThread));
    assert!(!effects_of("locked").contains(Effect::Pinned));
    // A posted function runs on the main thread; a callback and a function pointer check it.
    for name in ["posts", "registers", "idle"] {
        assert!(!effects_of(name).contains(Effect::MainThread), "{name}");
        assert!(!effects_of(name).contains(Effect::Pinned), "{name}");
    }
    assert_eq!(Effect::from_name("main_thread"), Some(Effect::MainThread));
    assert_eq!(Effect::ALL.len(), 16);
}

#[test]
fn code_of_the_main_thread_cannot_be_started_elsewhere() {
    for snippet in [
        "tzroutine {\n    touch ()\n}",
        // The pin is to a worker thread: accepted for a `local` handle, never for a `main` one.
        "tzroutine {\n    Routine.with_locked_thread (\\() -> touch ())\n}",
        "Routine.go (Owned.function (\\() -> touch ()))",
        "let wg = WaitGroup.create ()\nWaitGroup.go (ref wg) (Owned.function (\\() -> touch ()))",
        "let _timer = Time.after_func 5 (Owned.function (\\() -> touch ()))",
        "let shared = 10\nlet _out = Routine.scope (ref shared) 2 (\\s _i -> { touch (); *s })",
        "let _arr = Parallel.init 4 (\\i -> { touch (); i })",
    ] {
        let error = rejects(
            &format!("{WINDOW}\n{snippet}\n0\n"),
            DiagnosticKind::MainThreadCodeSpawned,
        );
        assert!(
            error
                .message
                .contains("the main-thread extern type 'Window'")
                && error.message.contains("MainThread.post"),
            "{snippet}\n{}",
            error.message
        );
    }
    for snippet in [
        "let f = \\() -> touch ()\nf ()",
        "let later = Owned.function (\\() -> touch ())\nMainThread.post later",
        "record Hook { run: unit -> unit }\nlet _hook = Hook { run: touch }",
    ] {
        let error = rejects(
            &format!("{WINDOW}\n{snippet}\n0\n"),
            DiagnosticKind::MainThreadFunctionValue,
        );
        assert!(
            error
                .message
                .contains("the main-thread extern type 'Window'"),
            "{snippet}\n{}",
            error.message
        );
    }
    // The same forms with a `local` handle keep their kinds and their wording (W19).
    let local = WINDOW.replace("extern type main Window", "extern type local Window");
    rejects(
        &format!("{local}\ntzroutine {{\n    touch ()\n}}\n0\n"),
        DiagnosticKind::PinnedCodeSpawned,
    );
    accepts(&format!(
        "{local}\ntzroutine {{\n    Routine.with_locked_thread (\\() -> touch ())\n}}\n0\n"
    ));
    rejects(
        &format!("{local}\nlet f = \\() -> touch ()\nf ()\n0\n"),
        DiagnosticKind::PinnedFunctionValue,
    );
}

#[test]
fn the_main_thread_and_its_entries_may_run_the_code() {
    accepts(&format!(
        "{WINDOW}\n\
         export def refresh :: i64 -> i64 = \\n ->\n    touch ()\n    n\n\n\
         test \"a test body\" =\n    touch ()\n    assert (1 == 1)\n\n\
         def main :: unit -> i32 = \\() ->\n\
         \x20   touch ()\n\
         \x20   Routine.with_locked_thread (\\() -> touch ())\n\
         \x20   MainThread.post (Owned.function (\\() -> touch ()))\n\
         \x20   MainThread.after 5 (Owned.function (\\() -> touch ()))\n\
         \x20   run_callback touch\n\
         \x20   take_pointer (FnPtr.of touch)\n\
         \x20   let lock = Mutex.create 1\n\
         \x20   Mutex.with_lock (ref lock) (\\_value -> touch ())\n\
         \x20   tzroutine {{\n        MainThread.post (Owned.function (\\() -> touch ()))\n        run_callback touch\n    }}\n\
         \x20   0\n"
    ));
}

#[test]
fn only_the_wrappers_of_main_thread_code_check_their_thread() {
    let source = format!(
        "{WINDOW}\n\
         export def refresh :: i64 -> i64 = \\n ->\n    touch ()\n    n\n\n\
         export def plain :: i64 -> i64 = \\n -> n + 1\n\n\
         test \"touches\" =\n    touch ()\n    assert (1 == 1)\n\n\
         test \"plain\" =\n    assert (1 == 1)\n\n\
         def main :: unit -> i32 = \\() ->\n    touch ()\n    run_callback touch\n    run_callback idle\n    0\n"
    );
    let module = accepts(&source);
    const CHECK: &str = "call void @tz.main_thread.check()";
    let console = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(console, llvm::emit(&module, llvm::Entry::Console).unwrap());
    assert!(definition(&console, "tz.callback.Main.touch").contains(CHECK));
    assert!(!definition(&console, "tz.callback.Main.idle").contains(CHECK));
    assert!(definition(&console, "tz_refresh").contains(CHECK));
    assert!(!definition(&console, "tz_plain").contains(CHECK));
    // The `main` of an executable is on the main thread.
    assert!(!definition(&console, "main").contains(CHECK));
    assert_eq!(
        console
            .matches("define internal void @tz.main_thread.check(")
            .count(),
        1
    );
    assert_eq!(
        console
            .matches("declare i32 @tsuzuri_mainthread_is_current()\n")
            .count(),
        1
    );
    assert!(
        console.contains("a callback that uses main-thread handles was called on another thread")
            || console.contains("@tz.main_thread.wrong")
    );
    // A library's host chooses the thread that calls `tsuzuri_main`.
    let library = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(
        definition(&library, "tsuzuri_main").contains(CHECK),
        "{library}"
    );
    // A runner calls the test of the effect through the check.
    let tests = llvm::emit(&module, llvm::Entry::TestRunner).unwrap();
    let run = definition(&tests, "tsuzuri_test_run");
    assert_eq!(run.matches(CHECK).count(), 1, "{run}");

    // A program whose handles stay in `main` needs no check and no runtime: it builds anywhere.
    let portable = accepts(&format!(
        "{WINDOW}\ndef main :: unit -> i32 = \\() ->\n    touch ()\n    0\n"
    ));
    for entry in [llvm::Entry::Console, llvm::Entry::TestRunner] {
        let ir = llvm::emit(&portable, entry).unwrap();
        assert!(
            !ir.contains("main_thread") && !ir.contains("mainthread"),
            "{ir}"
        );
    }
    // A program without a `main` handle computes nothing and emits nothing of this.
    let plain = accepts("extern type local Hdc\ndef main :: unit -> i32 = \\() -> 0\n");
    assert!(!plain.main_bound);
    assert!(
        !llvm::emit(&plain, llvm::Entry::Console)
            .unwrap()
            .contains("main_thread")
    );
}

#[test]
fn the_entry_check_builds_only_for_apple_targets() {
    let source =
        format!("{WINDOW}\nexport def refresh :: i64 -> i64 = \\n ->\n    touch ()\n    n\n");
    let root = temp_project("targets", &[("Main.tz", &source)]);
    let build = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("build")
            .arg(&root)
            .args(arguments)
            .arg("-o")
            .arg(root.join("out.bin"))
            .arg("--json")
            .output()
            .unwrap();
        (
            output.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    };
    let (ok, text) = build(&["--target", "x86_64-unknown-linux-gnu", "--emit", "object"]);
    assert!(!ok, "{text}");
    assert!(
        text.contains("\"kind\":\"platform_unsupported_api\""),
        "{text}"
    );
    assert!(text.contains("main-thread extern type"), "{text}");
    let (ok, text) = build(&["--target", "wasm32", "--emit", "wasm"]);
    assert!(!ok, "{text}");
    assert!(
        text.contains("\"kind\":\"wasm_main_thread_unsupported\""),
        "{text}"
    );
    let (ok, text) = build(&["--target", "x86_64-unknown-linux-gnu", "--emit", "llvm"]);
    assert!(ok, "{text}");
    let _ = std::fs::remove_dir_all(&root);
}

const APP: &str = "\
@trusted
extern class main App : ObjC.Object {
    class \"sharedApp\" shared :: unit -> Self
    class \"windowCount\" window_count :: unit -> i64
    \"activate\" activate :: unit -> unit
}

@trusted
extern class Plain : ObjC.Object {
    class \"count\" count :: unit -> i64
}
";

#[test]
fn a_message_to_a_main_class_is_sent_on_the_main_thread() {
    accepts(&format!(
        "{APP}\ndef main :: unit -> i32 = \\() ->\n    let app = App.shared ()\n    App.activate (ref app)\n    (App.window_count ()) as i32\n"
    ));
    // A class method whose types name no object is a message to the class all the same.
    rejects(
        &format!("{APP}\ntzroutine {{\n    let _ = App.window_count ()\n    ()\n}}\n0\n"),
        DiagnosticKind::MainThreadCodeSpawned,
    );
    rejects(
        &format!(
            "{APP}\ndef activate :: unit -> unit = \\() ->\n    let app = App.shared ()\n    App.activate (ref app)\n\n\
             tzroutine {{\n    activate ()\n}}\n0\n"
        ),
        DiagnosticKind::MainThreadCodeSpawned,
    );
    // In a program that spawns, an extern call is a blocking call, but not a message that the
    // main thread sends: that thread holds no P (D15).
    let module = accepts(&format!(
        "{APP}\ndef main :: unit -> i32 = \\() ->\n    tzroutine {{\n        let _ = Plain.count ()\n        ()\n    }}\n    (App.window_count ()) as i32\n"
    ));
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    let lines: Vec<&str> = ir.lines().collect();
    let sends: Vec<usize> = (0..lines.len())
        .filter(|&at| lines[at].contains("@objc_msgSend(") && !lines[at].starts_with("declare"))
        .collect();
    assert_eq!(sends.len(), 2, "{ir}");
    for at in sends {
        let bracketed = lines[at - 1].contains("@tsuzuri_tz_blocking_enter()")
            && lines[at + 1].contains("@tsuzuri_tz_blocking_exit()");
        let marked = lines[at].contains("\"tz-main-thread\"");
        assert_ne!(bracketed, marked, "{}", lines[at]);
    }
}

#[test]
fn api_diff_orders_the_thread_modifiers() {
    let dirs: Vec<PathBuf> = [
        "extern type H\n",
        "extern type local H\n",
        "extern type main H\n",
    ]
    .iter()
    .enumerate()
    .map(|(index, text)| temp_project(&format!("api{index}"), &[("A.tz", text)]))
    .collect();
    let apis: Vec<_> = dirs.iter().map(|dir| api_of(dir)).collect();
    // Binding the values more tightly breaks the callers; loosening it does not.
    for (old, new, breaking) in [(0, 2, 1), (1, 2, 1), (2, 1, 0), (2, 0, 0)] {
        let changes = api_diff::compare(&apis[old], &apis[new]);
        assert_eq!(changes.changes.len(), 1, "{old} -> {new}");
        assert_eq!(changes.breaking(), breaking, "{old} -> {new}: {changes:?}");
    }
    assert_eq!(
        api_diff::compare(&apis[0], &apis[2]).changes[0].note,
        "values of the type are now bound to the process main thread"
    );
    for dir in dirs {
        let _ = std::fs::remove_dir_all(dir);
    }
}
