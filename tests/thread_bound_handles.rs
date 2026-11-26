use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::api_diff::{self, Action};
use tsuzuri::diagnostic::{Diagnostic, DiagnosticKind, RelatedRole};
use tsuzuri::driver::Project;
use tsuzuri::formatter::format_source;
use tsuzuri::parser::parse;
use tsuzuri::syntax::{HandleKind, SourceKind};
use tsuzuri::{analyze, llvm};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tsuzuri")
}

fn temp_project(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-w19-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn api_of(path: &Path) -> api_diff::Api {
    let project = Project::load_for_api(path).unwrap();
    api_diff::api(&project).unwrap()
}

fn check_err(source: &str) -> Diagnostic {
    analyze(source).expect_err("expected type/effect error")
}

#[test]
fn parses_and_formats_extern_type_local_and_contextual_names() {
    let source = "\
extern type local Hdc
private extern type local TlsSlot
extern type local copy
extern type copy local
extern type local local
0
";
    let program = parse(source).expect("extern type local declarations must parse");
    assert_eq!(program.extern_types.len(), 5);
    assert_eq!(program.extern_types[0].kind, HandleKind::Local);
    assert_eq!(program.extern_types[0].name.text, "Hdc");
    assert_eq!(program.extern_types[2].kind, HandleKind::Local);
    assert_eq!(program.extern_types[2].name.text, "copy");
    assert_eq!(program.extern_types[3].kind, HandleKind::Copy);
    assert_eq!(program.extern_types[3].name.text, "local");
    assert_eq!(program.extern_types[4].kind, HandleKind::Local);
    assert_eq!(program.extern_types[4].name.text, "local");

    let formatted = format_source("Main.tz", source, SourceKind::Code)
        .expect("formatter must handle extern type local")
        .formatted;
    assert_eq!(formatted, source);
}

#[test]
fn rejects_conflicting_copy_and_local_modifiers() {
    for bad in [
        "extern type copy local Hdc\n0\n",
        "extern type local copy Hdc\n0\n",
        "extern type local local Hdc\n0\n",
    ] {
        let err = parse(bad).expect_err("double modifier must fail");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::ExternTypeModifierConflict.id())
        );
        assert_eq!(err.code, "E0002");
        assert!(
            err.message
                .contains("an extern type takes at most one of 'copy' and 'local'"),
            "unexpected message: {}",
            err.message
        );
    }
}

#[test]
fn api_diff_treats_adding_local_as_breaking_and_removing_as_compatible() {
    let owned_dir = temp_project("api-owned", &[("A.tz", "extern type H\n")]);
    let local_dir = temp_project("api-local", &[("A.tz", "extern type local H\n")]);
    let copy_dir = temp_project("api-copy", &[("A.tz", "extern type copy H\n")]);
    let owned = api_of(&owned_dir);
    let local = api_of(&local_dir);
    let copy = api_of(&copy_dir);

    let added = api_diff::compare(&owned, &local);
    assert_eq!(
        added.breaking(),
        1,
        "adding 'local' must be breaking: {added:?}"
    );
    assert_eq!(added.changes.len(), 1);
    assert_eq!(added.changes[0].action, Action::Changed);
    assert!(added.changes[0].breaking);
    assert_eq!(
        added.changes[0].note,
        "values of the type are now bound to their OS thread"
    );

    let to_owned = api_diff::compare(&local, &owned);
    assert_eq!(
        to_owned.breaking(),
        0,
        "removing 'local' to owned handle must be compatible: {to_owned:?}"
    );
    assert_eq!(to_owned.changes.len(), 1);
    assert!(!to_owned.changes[0].breaking);

    let to_copy = api_diff::compare(&local, &copy);
    assert_eq!(
        to_copy.breaking(),
        0,
        "changing 'local' to 'copy' must be compatible: {to_copy:?}"
    );
    assert_eq!(to_copy.changes.len(), 1);
    assert!(!to_copy.changes[0].breaking);

    let _ = std::fs::remove_dir_all(owned_dir);
    let _ = std::fs::remove_dir_all(local_dir);
    let _ = std::fs::remove_dir_all(copy_dir);
}

#[test]
fn local_handle_is_non_copy_non_send_non_sync_non_capturable_and_nullable() {
    // Move semantics (!Copy): using a local handle twice after moving fails with move error.
    let move_twice = check_err(
        r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
@trusted
extern "rel_dc" def rel_dc :: own Hdc -> unit

let h = get_dc ()
rel_dc h
rel_dc h
0
"#,
    );
    assert_eq!(move_twice.code, "E1012");

    // !Send: Channel.make<Hdc> and Mutex.create<Wrapper> reject with W19 Send diagnostic.
    let send_direct = check_err(
        r#"
extern type local Hdc
let _ch: Channel<Hdc> = Channel.make 1
0
"#,
    );
    assert_eq!(
        send_direct.kind,
        Some(DiagnosticKind::TaskRequiresSend.id())
    );
    assert_eq!(send_direct.code, "E1013");
    assert!(
        send_direct
            .message
            .contains("Hdc holds a thread-bound extern handle ('extern type local')"),
        "unexpected Send message: {}",
        send_direct.message
    );
    assert!(
        send_direct.message.contains("Routine.with_locked_thread"),
        "unexpected Send message: {}",
        send_direct.message
    );

    let send_wrapped = check_err(
        r#"
extern type local Hdc
record Paint { dc: Maybe<Hdc> }
let _m = Mutex.create (Paint { dc: Maybe.None })
0
"#,
    );
    assert_eq!(
        send_wrapped.kind,
        Some(DiagnosticKind::TaskRequiresSend.id())
    );
    assert!(
        send_wrapped
            .message
            .contains("Paint holds a thread-bound extern handle ('extern type local')"),
        "unexpected wrapped Send message: {}",
        send_wrapped.message
    );

    // !Sync: sharing a local handle across `Routine.scope` (or `Arc<Hdc>` across `Send`) is rejected.
    let sync_err = check_err(
        r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
let dc = get_dc ()
let _out = Routine.scope (ref dc) 2 (\_s _i -> 0)
0
"#,
    );
    assert_eq!(sync_err.kind, Some(DiagnosticKind::TaskRequiresSync.id()));
    assert_eq!(sync_err.code, "E1013");

    // !Capture: capturing an owned local handle in a function value produces W19 E1005 wording.
    let capture_err = check_err(
        r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
@trusted
extern "rel_dc" def rel_dc :: own Hdc -> i64
let h = get_dc ()
let _f = \x -> rel_dc h + x
0
"#,
    );
    assert_eq!(
        capture_err.kind,
        Some(DiagnosticKind::CaptureUnsendableInFunctionValue.id())
    );
    assert_eq!(capture_err.code, "E1005");
    assert!(
        capture_err
            .message
            .contains("a thread-bound extern handle ('extern type local') must stay on its OS thread; pass it as an argument"),
        "unexpected Capture message: {}",
        capture_err.message
    );

    // Maybe<Hdc> lowers with NULL-pointer nullable handle ABI and WASM rejects with E1008.
    let nullable_src = r#"
extern type local Hdc
@trusted
extern "try_dc" def try_dc :: Maybe<Hdc> -> Maybe<Hdc>
def main :: unit -> i32 = \() ->
    match try_dc Maybe.None with
    | Maybe.Some _ -> 1i32
    | Maybe.None -> 0i32
"#;
    let module = analyze(nullable_src).expect("Maybe<Hdc> should type-check");
    let ir = llvm::emit(&module, llvm::Entry::Console).expect("LLVM IR should emit for Maybe<Hdc>");
    assert!(ir.contains("@try_dc("), "{ir}");

    let wasm_dir = temp_project("wasm-local", &[("Main.tz", nullable_src)]);
    let wasm_out = wasm_dir.join("out.wasm");
    let wasm_res = Command::new(bin())
        .args([
            "build",
            wasm_dir.to_str().unwrap(),
            "--target",
            "wasm32",
            "-o",
            wasm_out.to_str().unwrap(),
        ])
        .output()
        .expect("run wasm build");
    assert!(!wasm_res.status.success());
    assert!(
        String::from_utf8_lossy(&wasm_res.stderr).contains("E1008"),
        "{}",
        String::from_utf8_lossy(&wasm_res.stderr)
    );
    let _ = std::fs::remove_dir_all(wasm_dir);
}

#[test]
fn rejects_spawning_pinned_code_across_all_spawn_forms_with_witnesses() {
    // 1. Direct `tzroutine { ... }` touching a local handle.
    let direct_spawn = check_err(
        r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
@trusted
extern "rel_dc" def rel_dc :: own Hdc -> unit

tzroutine {
    let dc = get_dc ()
    rel_dc dc
}
0
"#,
    );
    assert_eq!(
        direct_spawn.kind,
        Some(DiagnosticKind::PinnedCodeSpawned.id())
    );
    assert_eq!(direct_spawn.code, "E1013");
    assert!(
        direct_spawn
            .message
            .contains("uses the thread-bound extern type 'Hdc' on another tzroutine or thread"),
        "unexpected message: {}",
        direct_spawn.message
    );

    // 2. Indirect call chain (`tzroutine` -> `step_a` -> `step_b` -> `get_dc`) attaches ViaCall + Note + help chain.
    let indirect_spawn = check_err(
        r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
@trusted
extern "rel_dc" def rel_dc :: own Hdc -> unit

def step_b :: unit -> unit = \() ->
    let dc = get_dc ()
    rel_dc dc

def step_a :: unit -> unit = \() -> step_b ()

tzroutine {
    step_a ()
}
0
"#,
    );
    assert_eq!(
        indirect_spawn.kind,
        Some(DiagnosticKind::PinnedCodeSpawned.id())
    );
    assert!(
        indirect_spawn
            .related
            .iter()
            .any(|r| r.role == RelatedRole::ViaCall && r.message.contains("Main.step_b")),
        "expected ViaCall to Main.step_b in {:?}",
        indirect_spawn.related
    );
    assert!(
        indirect_spawn
            .related
            .iter()
            .any(|r| r.role == RelatedRole::Note
                && r.message == "uses thread-bound extern type 'Hdc' here"),
        "expected Note at direct source in {:?}",
        indirect_spawn.related
    );
    let help = indirect_spawn.help.as_deref().unwrap_or("");
    assert!(
        help.contains("'Main.step_a' -> 'Main.step_b'"),
        "expected witness chain in help, got: {help}"
    );

    // 3. Record holding only a `copy` handle whose `Drop` touches a `local` handle cannot be dropped in a `tzroutine`.
    let drop_spawn = check_err(
        r#"
extern type copy Hwnd
extern type local Hdc
@trusted
extern "desktop" def desktop :: unit -> Hwnd
@trusted
extern "get_dc" def get_dc :: Hwnd -> Hdc
@trusted
extern "rel_dc" def rel_dc :: Hwnd -> own Hdc -> unit

record Guard { hwnd: Hwnd }
instance Drop<Guard> {
    fn drop g =
        let dc = get_dc g.hwnd
        rel_dc g.hwnd dc
}

let g = Guard { hwnd: desktop () }
tzroutine {
    let _moved = g
    ()
}
0
"#,
    );
    assert_eq!(
        drop_spawn.kind,
        Some(DiagnosticKind::PinnedCodeSpawned.id())
    );

    // 4. `Routine.go`, `WaitGroup.go`, `ErrGroup.go`, `ErrGroup.try_go`, `Time.after_func`, `Context.after_func`, `Routine.scope`, `Parallel.init`, `Parallel.fold`, and `Anchor<Guard>` drop.
    for snippet in [
        "Routine.go (Owned.function (\\() -> touch ()))",
        "let wg = WaitGroup.create ()\nWaitGroup.go (ref wg) (Owned.function (\\() -> touch ()))",
        "let ctx = Context.background ()\nlet eg: ErrGroup<i64> = ErrGroup.with_context (ref ctx)\nErrGroup.go (ref eg) (Owned.function (\\() -> { touch (); Result.Ok () }))",
        "let ctx = Context.background ()\nlet eg: ErrGroup<i64> = ErrGroup.with_context (ref ctx)\nErrGroup.set_limit (ref eg) 2\nlet _ok = ErrGroup.try_go (ref eg) (Owned.function (\\() -> { touch (); Result.Ok () }))",
        "let _t = Time.after_func 1000 (Owned.function (\\() -> touch ()))",
        "let ctx = Context.background ()\nlet _af = Context.after_func (ref ctx) (Owned.function (\\() -> touch ()))",
        "let shared = 10\nlet _out = Routine.scope (ref shared) 2 (\\s _i -> { touch (); *s })",
        "let _arr = Parallel.init 4 (\\i -> { touch (); i })",
    ] {
        let src = format!(
            r#"
extern type local Hdc
@trusted pure
extern "get_dc" def get_dc :: unit -> Hdc
@trusted pure
extern "rel_dc" def rel_dc :: own Hdc -> unit
pure def touch :: unit -> unit = \() -> rel_dc (get_dc ())
{snippet}
0
"#
        );
        let err = check_err(&src);
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::PinnedCodeSpawned.id()),
            "expected PinnedCodeSpawned for `{snippet}`, got {err:?}"
        );
    }

    // 5. Specialized `std` higher-order spawn (`Parallel.fold` over a `Monoid` whose `combine` touches `Hdc`)!
    let parallel_fold_err = check_err(
        r#"
extern type local Hdc
@trusted pure
extern "get_dc" def get_dc :: unit -> Hdc
@trusted pure
extern "rel_dc" def rel_dc :: own Hdc -> i64

record Job { v: i64 } deriving (Eq)
instance Monoid<Job> {
    let empty = \() -> Job { v: 0 }
    fn combine a b = Job { v: a.v + b.v + rel_dc (get_dc ()) }
}

let jobs = [Job { v: 1 }, Job { v: 2 }]
let _res = Parallel.fold (ref jobs)
0
"#,
    );
    assert_eq!(
        parallel_fold_err.kind,
        Some(DiagnosticKind::PinnedCodeSpawned.id()),
        "{parallel_fold_err:?}"
    );

    // 6. Dropping `Anchor<Guard>` (where `Guard` holds `Hdc`) outside a pinned block in a `tzroutine`!
    let anchor_drop_err = check_err(
        r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
@trusted
extern "rel_dc" def rel_dc :: ref Hdc -> unit

record DcGuard { dc: Hdc }
instance Drop<DcGuard> {
    fn drop g = rel_dc (ref g.dc)
}

tzroutine {
    let _a = Routine.with_locked_thread (\() -> Anchor.create (DcGuard { dc: get_dc () }))
    ()
}
0
"#,
    );
    assert_eq!(
        anchor_drop_err.kind,
        Some(DiagnosticKind::PinnedCodeSpawned.id()),
        "{anchor_drop_err:?}"
    );
}

#[test]
fn rejects_pinned_function_values_and_dyn_packaging() {
    for snippet in [
        "let _f = touch",
        "let _f = \\() -> touch ()",
        "record Boxed { f: unit -> i64 }\nlet _b = Boxed { f: touch }",
        "let _fs = [touch]",
        "let _ys = List.map (\\_ -> touch ()) [|1, 2|]",
        "let _m = Maybe.map (\\_ -> touch ()) (Maybe.Some 1)",
        "def apply :: (unit -> i64) -> i64 = \\f -> f ()\nlet _v = apply touch",
        "def add_dc :: i64 -> i64 -> i64 = \\a b -> a + b + touch ()\nlet _partial = add_dc 1",
        "let a = Anchor.create 1\nlet _reader = Anchor.read (\\x -> *x + touch ())",
        "let _comp = touch >> (\\n -> n + 1)",
    ] {
        let src = format!(
            r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
@trusted
extern "rel_dc" def rel_dc :: own Hdc -> i64
def touch :: unit -> i64 = \() -> rel_dc (get_dc ())
{snippet}
0
"#
        );
        let err = check_err(&src);
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::PinnedFunctionValue.id()),
            "expected PinnedFunctionValue for `{snippet}`, got {err:?}"
        );
        assert!(
            err.message.contains(
                "this function value runs code that uses the thread-bound extern type 'Hdc'"
            ),
            "unexpected message for `{snippet}`: {}",
            err.message
        );
    }

    // `Dyn.of` on a type whose class method touches a `local` handle is rejected with `pinned_function_value`.
    let dyn_err = check_err(
        r#"
extern type local Hdc
@trusted
extern "get_dc" def get_dc :: unit -> Hdc
@trusted
extern "rel_dc" def rel_dc :: own Hdc -> i64

class Render<'a> {
    def render :: ref 'a -> i64
}

record Brush { width: i64 }
instance Render<Brush> {
    fn render b = b.width + rel_dc (get_dc ())
}

let b = Brush { width: 3 }
let _d: dyn Render = Dyn.of b
0
"#,
    );
    assert_eq!(dyn_err.kind, Some(DiagnosticKind::PinnedFunctionValue.id()));
    assert!(dyn_err.message.contains("'Hdc'"), "{}", dyn_err.message);
}

#[test]
fn allows_pinning_boundaries_inline_sync_locks_mutual_recursion_and_effect_attributes() {
    let source = r#"
extern type local Hdc
@trusted no_alloc
extern "get_dc" def get_dc :: unit -> Hdc
@trusted no_alloc
extern "rel_dc" def rel_dc :: own Hdc -> i64
@trusted
extern "run_cb" def run_cb :: (unit -> i64) -> i64
@trusted
extern "take_fp" def take_fp :: FnPtr<(unit -> i64)> -> i64

@no_alloc
def touch :: unit -> i64 = \() -> rel_dc (get_dc ())

def cb_entry :: unit -> i64 = \() -> touch ()

// Mutual recursion across a pinning boundary: `outer` pins the OS thread before calling `inner`,
// and `inner` touches `Hdc` and recurses back to `outer`. `outer` itself is NOT `pinned`, so
// `tzroutine { let _ = outer 2; () }` is sound!
def rec outer :: i64 -> i64 = \n ->
    if n <= 0 then 0
    else Routine.with_locked_thread (\() -> inner (n - 1))
and inner :: i64 -> i64 = \n -> touch () + outer n

def main :: unit -> i32 = \() ->
    // 1. Direct call and pinned tzroutine (call + pipeline forms).
    let a = touch ()
    tzroutine {
        let _ = Routine.with_locked_thread (\() -> touch ())
        let _ = (\() -> touch ()) |> Routine.with_locked_thread
        let _ = outer 2
        ()
    }
    // 2. Callback extern and FnPtr.of (including moving FnPtr across tzroutines).
    let b = run_cb cb_entry
    let fp = FnPtr.of cb_entry
    tzroutine {
        let _ = take_fp fp
        ()
    }
    // 3. Synchronous inline lock / cell / anchor helpers.
    let mtx = Mutex.create 10
    let c = Mutex.with_lock (ref mtx) (\v -> *v + touch ())
    let rw = RwMutex.create 20
    let d = RwMutex.with_read (ref rw) (\v -> *v + touch ())
    let e = RwMutex.with_write (ref rw) (\v -> { *v = *v + touch (); *v })
    let ov = OnceValue.create ()
    let f = *(OnceValue.get (ref ov) (\() -> touch ()))
    let cond = Cond.create ()
    let g = Cond.wait_with (ref cond) (ref mtx) (\v -> *v > 0) (\v -> *v + touch ())
    let anc = Anchor.create 5
    let tok = Anchor.token (ref anc)
    let _ = Anchor.read (\v -> *v + touch ()) tok
    let _ = tok |> Anchor.read (\v -> *v + touch ())
    let _ = Anchor.update (\v -> v + touch ()) tok
    (a + b + c + d + e + f + g) as i32
"#;
    let module = analyze(source).expect("all pinning and inline positions must be accepted");
    let ir = llvm::emit_target(&module, llvm::Entry::Console, false).expect("IR should emit");
    assert!(ir.contains("@tz.callback.Main.cb_entry"), "{ir}");

    // Conversely, `tzroutine { let _ = inner 1; () }` in the same mutual recursion MUST be rejected!
    let bad_inner = format!("{source}\n");
    let bad_source = bad_inner.replace("let _ = outer 2", "let _ = inner 2");
    let err = check_err(&bad_source);
    assert_eq!(err.kind, Some(DiagnosticKind::PinnedCodeSpawned.id()));
}

#[test]
fn programs_without_local_handles_compute_nothing() {
    let plain_dir = temp_project(
        "time-plain",
        &[("Main.tz", "def add :: i64 -> i64 = \\x -> x + 1\nadd 41\n")],
    );
    let plain = plain_dir.join("Main.tz");

    let out_plain = Command::new(bin())
        .args(["check", plain.to_str().unwrap()])
        .env("TSUZURI_TIME_PASSES", "1")
        .output()
        .expect("run tsuzuri check on plain Main.tz");
    assert!(
        out_plain.status.success(),
        "{}",
        String::from_utf8_lossy(&out_plain.stderr)
    );
    let stderr_plain = String::from_utf8_lossy(&out_plain.stderr);
    assert!(
        !stderr_plain.contains("\"pinned\""),
        "programs without extern type local must not run the 'pinned' phase: {stderr_plain}"
    );

    let local_dir = temp_project(
        "time-local",
        &[(
            "Main.tz",
            "extern type local Hdc\n@trusted\nextern \"noop\" def noop :: Maybe<Hdc> -> i64\nnoop Maybe.None\n",
        )],
    );
    let local = local_dir.join("Main.tz");
    let out_local = Command::new(bin())
        .args(["check", local.to_str().unwrap()])
        .env("TSUZURI_TIME_PASSES", "1")
        .output()
        .expect("run tsuzuri check on local Main.tz");
    assert!(
        out_local.status.success(),
        "{}",
        String::from_utf8_lossy(&out_local.stderr)
    );
    let stderr_local = String::from_utf8_lossy(&out_local.stderr);
    assert!(
        stderr_local.contains("\"pinned\""),
        "programs with extern type local must record the 'pinned' phase: {stderr_local}"
    );
    let _ = std::fs::remove_dir_all(plain_dir);
    let _ = std::fs::remove_dir_all(local_dir);
}

#[test]
fn main_thread_advice_in_thread_bound_messages() {
    let spawned_src = "\
extern type local Window

@trusted
extern \"host_window_create\" def window_create :: unit -> Window

@trusted
extern \"host_window_close\" def window_close :: own Window -> unit

def open_window :: unit -> unit = \\() ->
    let w = window_create ()
    window_close w

def main :: unit -> i32 = \\() ->
    tzroutine {
        open_window ()
    }
    0
";
    let spawned = check_err(spawned_src);
    assert_eq!(spawned.code, "E1013");
    assert_eq!(spawned.kind, Some(DiagnosticKind::PinnedCodeSpawned.id()));
    assert_eq!(
        spawned.message,
        "this starts code that uses the thread-bound extern type 'Window' on another tzroutine or thread, which may run on any OS thread; do that work inside 'Routine.with_locked_thread' in the new tzroutine, or run it on the calling thread; an API that needs the process main thread (AppKit on macOS) must run in 'main' or in a callback that the main thread calls"
    );
    assert_eq!(spawned.span.start, spawned_src.find("tzroutine").unwrap());
    assert_eq!(spawned.related.len(), 2);

    let send_src = "\
extern type local Window

@trusted
extern \"host_window_create\" def window_create :: unit -> Window

def main :: unit -> i32 = \\() ->
    let _w = window_create ()
    let _windows: Channel<Window> = Channel.make 1
    0
";
    let send_err = check_err(send_src);
    assert_eq!(send_err.code, "E1013");
    assert_eq!(send_err.kind, Some(DiagnosticKind::TaskRequiresSend.id()));
    assert_eq!(
        send_err.message,
        "values that move to another tzroutine or thread must be Send; Main.Window holds a thread-bound extern handle ('extern type local'), which must stay on the OS thread that received it; use it there, or do the work inside 'Routine.with_locked_thread'; an API that needs the process main thread (AppKit on macOS) must run in 'main' or in a callback that the main thread calls"
    );

    let fn_val = check_err(
        "\
extern type local Window
@trusted
extern \"host_window_create\" def window_create :: unit -> Window
@trusted
extern \"host_window_close\" def window_close :: own Window -> unit
def open_window :: unit -> unit = \\() -> window_close (window_create ())
let _f = open_window
0
",
    );
    assert_eq!(fn_val.kind, Some(DiagnosticKind::PinnedFunctionValue.id()));
    assert!(
        !fn_val.message.contains("main thread"),
        "{}",
        fn_val.message
    );
}
