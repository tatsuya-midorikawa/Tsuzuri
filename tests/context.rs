//! `Context` and `ErrGroup` (TZ07): types, privacy, captures, capabilities and the generated code.
//! The programs are in `node tests/tzroutine.mjs` and `node tests/features.mjs <bin> context`, the
//! runtime in `node tests/tzroutine_runtime.mjs`.
use tsuzuri::{analyze, capabilities, llvm};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(
        ir,
        llvm::emit(&module, llvm::Entry::Console).unwrap(),
        "{source}: IR is deterministic"
    );
    module
}

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.message.clone()
}

const EVERY_OPERATION: &str = "\
let root: Context = Context.background ()
let child: Context = Context.with_cancel root
let timed: Context = Context.with_timeout child Time.hour()
let dated: Context = Context.with_deadline child 5
let done: Channel<unit> = Context.done timed
let why: Maybe<Context.Error> = Context.err dated
let ended: bool = Context.is_done dated
let deadline: Maybe<i64> = Context.deadline timed
let other: Context = Context.share child
let cancelled: unit = Context.cancel other
let after: Context.AfterFunc = Context.after_func child (Owned.function (\\() -> ()))
let stopped: bool = Context.stop_after after
let group: ErrGroup<string> = ErrGroup.create ()
let grouped: ErrGroup<string> = ErrGroup.with_context root
let context: Context = ErrGroup.context group
let started: unit = ErrGroup.go group (Owned.function (\\() -> Result.Ok ()))
let tried: Result<unit, Owned.Function<unit, Result<unit, string>>> = ErrGroup.try_go group (Owned.function (\\() -> Result.Error \"no\"))
let limited: unit = ErrGroup.set_limit grouped 2
let waited: Result<unit, string> = ErrGroup.wait group
let shared: ErrGroup<string> = ErrGroup.share group
0
";

#[test]
fn the_operations_have_their_types() {
    let module = accepts(EVERY_OPERATION);
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    for declaration in [
        "declare ptr @tsuzuri_tz_ctx_new(ptr, i64, ptr, ptr)",
        "declare ptr @tsuzuri_tz_ctx_timeout(ptr, i64, ptr, ptr)",
        "declare ptr @tsuzuri_tz_ctx_done(ptr)",
        "declare i32 @tsuzuri_tz_ctx_err(ptr)",
        "declare i64 @tsuzuri_tz_ctx_deadline(ptr)",
        "declare void @tsuzuri_tz_ctx_share(ptr)",
        "declare void @tsuzuri_tz_ctx_cancel(ptr)",
        "declare void @tsuzuri_tz_ctx_release(ptr)",
        "declare ptr @tsuzuri_tz_ctx_after(ptr, ptr, ptr, ptr, ptr, ptr)",
        "declare i32 @tsuzuri_tz_ctx_stop_after(ptr)",
        "declare void @tsuzuri_tz_ctx_free_after(ptr)",
    ] {
        assert_eq!(
            ir.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}"
        );
    }
    // The done channel is the program's allocation, which the runtime frees with @tz.free.
    assert!(ir.contains("call ptr @tz.alloc(i64 128)"), "{ir}");
    assert!(ir.contains("ptr @tz.free)"), "{ir}");
    // The function of an after_func is called or dropped by the runtime.
    assert!(ir.contains("define internal void @tz.tzroutine.owned.drop(ptr %box)"));
    // A deadline is a timer of a boundary: the program waits and stops its timers at the end.
    assert!(llvm::spawns(&module));
    assert!(ir.contains("call void @tsuzuri_tz_boundary_leave(ptr %tz.scope)"));
}

#[test]
fn the_runtime_parts_are_private_and_opaque() {
    for source in [
        "let root = Context.__background ()\n0\n",
        "let c = Context { handle: 0 }\n0\n",
        "let a = Context.AfterFunc { handle: 0 }\n0\n",
    ] {
        rejects(source, "E1022");
    }
    rejects(
        "def peek :: ref Context -> i64\nfn peek context = context.handle\n0\n",
        "E1022",
    );
    rejects(
        "def peek :: ref ErrGroup<string> -> Context\nfn peek group = Context.share (ref group.context)\n0\n",
        "E1022",
    );
}

#[test]
fn a_context_is_not_captured_by_a_reusable_function() {
    let message = rejects(
        "let root = Context.background ()\nlet ctx = Context.with_cancel root\nlet stop = \\() -> Context.cancel ctx\nstop ()\n0\n",
        "E1005",
    );
    assert!(message.contains("Context"), "{message}");
    // A tzroutine takes its own handle.
    accepts(
        "let root = Context.background ()\nlet ctx = Context.with_cancel root\nlet mine = Context.share ctx\ntzroutine {\n    Context.cancel mine\n}\nlet done = Context.done ctx\nlet _closed = Channel.recv done\n0\n",
    );
}

#[test]
fn the_deadlines_need_time() {
    for name in ["Context.__with_timeout", "Context.__with_deadline"] {
        assert_eq!(
            capabilities::primitive(name, "Context"),
            Some(capabilities::Capability::Time),
            "{name}"
        );
    }
    for name in [
        "Context.__background",
        "Context.__with_cancel",
        "Context.__cancel",
        "Context.__done",
        "Context.__err",
        "Context.__deadline",
        "Context.__share",
        "Context.__release",
        "Context.__after_func",
        "Context.__stop_after",
        "Context.__free_after",
    ] {
        assert!(
            capabilities::PRIVATE_WITHOUT_CAPABILITY.contains(&name),
            "{name}"
        );
        assert_eq!(capabilities::primitive(name, "Context"), None, "{name}");
    }
}

#[test]
fn a_context_needs_the_scheduler() {
    // Like a timer, a context belongs to a boundary, which waits for it: the default WebAssembly
    // target has no scheduler (TZ09).
    let module = accepts("let root = Context.background ()\nContext.is_done root\n");
    assert!(llvm::spawns(&module));
}

#[test]
fn programs_without_contexts_keep_their_ir() {
    // A program that names neither module loads neither (and so not Channel through them).
    let named = |text: &str| -> Vec<&'static str> {
        tsuzuri::stdlib::sources_for([text])
            .iter()
            .filter_map(|(path, _)| tsuzuri::stdlib::module_name(path))
            .collect()
    };
    let loaded = named("0\n");
    assert!(!loaded.contains(&"Context") && !loaded.contains(&"ErrGroup"));
    let loaded = named("ErrGroup.create ()\n");
    for module in ["ErrGroup", "Context", "Channel", "WaitGroup", "Mutex", "Cond"] {
        assert!(loaded.contains(&module), "{module}: {loaded:?}");
    }
}
