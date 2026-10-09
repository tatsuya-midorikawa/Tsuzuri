//! B08: the std `Async` module, its builder, and the borrow rule across suspension points.

use std::path::PathBuf;

use serde_json::json;
use tsuzuri::bindings::{self, JsFlavor};
use tsuzuri::check::{CheckedModule, ModuleOrigin};
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::driver::{self, BuildOptions, Emit, Project, Target};
use tsuzuri::{analyze_modules, llvm};

const ASYNC_BORROW: &str = "async computations cannot keep borrowed values across 'let!', 'do!', or the start of the computation; move or clone the value into the async block instead";
const MISSING: &str = "computation builder 'Async' does not define '";

fn accepts(source: &str) -> String {
    let module = analyze_modules(&[("Main.tz", source)])
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    ir
}

fn rejects(source: &str, code: &str) -> tsuzuri::diagnostic::Diagnostic {
    rejects_modules(&[("Main.tz", source)], code)
}

fn rejects_modules(sources: &[(&str, &str)], code: &str) -> tsuzuri::diagnostic::Diagnostic {
    let error = analyze_modules(sources).expect_err(sources[0].1);
    assert_eq!(error.code, code, "{}\n{}", sources[0].1, error.message);
    error
}

fn rejects_borrow(source: &str) {
    let error = rejects(source, "E1013");
    assert_eq!(error.message, ASYNC_BORROW, "{source}");
}

#[test]
fn reserves_the_async_module_name() {
    let error = rejects_modules(
        &[
            ("Main.tz", "0"),
            ("Async.tz", "def value :: i64\nfn value = 1"),
        ],
        "E1011",
    );
    assert_eq!(
        error.message,
        "module name 'Async' is reserved for the standard library; rename the file"
    );
    accepts("export def one :: i64 -> i64\nfn one _unused = Async.run (Async { return 1 })");
}

#[test]
fn programs_that_do_not_name_async_do_not_load_it() {
    let loaded = |source: &str| {
        analyze_modules(&[("Main.tz", source)])
            .unwrap()
            .records
            .iter()
            .any(|record| record.name == "Async.Async" && record.origin == ModuleOrigin::Std)
    };
    assert!(!loaded("export def add :: i64 -> i64\nfn add n = n + 1"));
    assert!(loaded("// Async\n0"));
}

#[test]
fn yield_names_a_member_but_stays_a_keyword() {
    accepts(
        "export def paused :: i64 -> i64\nfn paused n = Async.run (Async { do! Async.yield (); return n })",
    );
    // A module may declare its own `yield`; it is reached qualified, like `Set.union`.
    let module = analyze_modules(&[
        (
            "Main.tz",
            "export def next :: i64 -> i64\nfn next n = Tools.yield n",
        ),
        ("Tools.tz", "def yield :: i64 -> i64\nfn yield n = n + 1"),
    ])
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    llvm::emit(&module, llvm::Entry::Library).unwrap();
    // Unqualified, `yield` is still the computation statement or a syntax error.
    rejects("let value = yield 1\nvalue", "E0002");
}

#[test]
fn lowers_async_blocks_deterministically_for_both_targets() {
    let ir = accepts(
        "def step :: i64 -> Async<Maybe<i64>>
fn step n = Async {
    do! Async.sleep n
    return if n > 0 then Maybe.Some n else Maybe.None
}

def rec count :: i64 -> Async<i64>
fn rec count n = Async {
    if n <= 0 then return 0
    else
        do! Async.yield ()
        return! count (n - 1)
}

export def total :: i64 -> i64
fn total n = Async.run (Async {
    let! first = Async.Return n
    do! Async.yield ()
    if first > 100 then do! Async.sleep 1
    match! step first with
    | Maybe.Some value -> do! Async.sleep value
    | Maybe.None -> do! Async.yield ()
    let values = [1, 2, 3]
    for value in values do
        do! Async.sleep value
    let! many = Async.all [count 2, count 3]
    let! checked = Async.all_results [Async.Return (Result.Ok 1), Async.Return (Result.Error 2)]
    let failed = match checked with | Result.Ok _ -> 0 | Result.Error error -> error
    let! now = Async.now ()
    return! Async.Return (first + many[0] + many[1] + failed + now)
})",
    );
    assert!(ir.contains("define i64 @tz_total(i64"));
    // A continuation runs once, and resuming it inlines into the caller.
    assert!(ir.contains("nounwind alwaysinline"));
}

#[test]
fn async_values_are_opaque_and_not_copy() {
    let error = rejects("let a = Async.yield ()\nlet b = a\nAsync.run a", "E1012");
    assert_eq!(error.message, "use of moved or partially moved value 'a'");
    for source in [
        "let start = (Async.now ()).start\n0",
        "let made = Async { start: 1 }\n0",
    ] {
        let error = rejects(source, "E1022");
        assert_eq!(
            error.message, "the representation of 'Async' is opaque; use its module API",
            "{source}"
        );
    }
    let error = rejects("let made = Async.Next { call: \\x -> x }\n0", "E1022");
    assert_eq!(
        error.message,
        "the representation of 'Async.Next' is opaque; use its module API"
    );
    let error = rejects("let call = Async.__resume\n0", "E1022");
    assert_eq!(
        error.message,
        "the continuation primitive is private to the standard Async module; compose computations with the Async builder"
    );
}

#[test]
fn rejects_borrows_across_suspension_points() {
    // A borrow made in the block and used after `do!`.
    rejects_borrow(
        "export def borrowed :: i64 -> i64
fn borrowed n =
    let values = Array.init 3 (\\index -> index)
    Async.run (Async {
        let view = ref values
        do! Async.yield ()
        return Array.length view + n
    })",
    );
    // A borrow of an outer value: the cold start is a suspension point too.
    rejects_borrow(
        "let values = Array.init 3 (\\index -> index)
let view = ref values
let computation = Async { return Array.length view }
0",
    );
    // A borrowed parameter used after `do!`.
    rejects_borrow(
        "def count :: ref [i64] -> Async<i64>
fn count values = Async {
    do! Async.yield ()
    return Array.length values
}
0",
    );
    // A borrow passed as the value.
    rejects_borrow("let values = [1, 2, 3]\nlet computation = Async.Return (ref values)\n0");
    // The placeholder loan of a function parameter is checked against its actual environment
    // at the call site, including when it is stored in a record.
    for prefix in [
        "def deferred :: (unit -> i64) -> Async<i64>\nfn deferred read = Async { do! Async.yield (); return read () }\n",
        "record Reader { read: unit -> i64 }\ndef deferred :: Reader -> Async<i64>\nfn deferred source = Async { do! Async.yield (); return source.read () }\n",
    ] {
        let argument = if prefix.starts_with("record") {
            "Reader { read: \\() -> Array.length view }"
        } else {
            "(\\() -> Array.length view)"
        };
        rejects_borrow(&format!(
            "{prefix}let values = [1, 2, 3]\nlet view = ref values\nAsync.run (deferred {argument})"
        ));
    }
}

#[test]
fn accepts_borrows_that_end_before_suspension_and_moved_values() {
    accepts(
        "def moved :: Async<i64>
fn moved =
    let values = Array.init 3 (\\index -> index)
    Async {
        do! Async.yield ()
        let view = ref values
        return Array.length view
    }

def within :: i64 -> Async<i64>
fn within n =
    let values = Array.init n (\\index -> index)
    let count = Array.length (ref values)
    Async {
        do! Async.yield ()
        return count
    }

def outer_moved :: [i64] -> Async<i64>
fn outer_moved values =
    let owned = values
    Async {
        do! Async.yield ()
        return Array.length (ref owned)
    }

// The block captures its own copy of `values`, so the borrow ends inside the block.
def copied :: Async<i64>
fn copied =
    let values = [1, 2, 3]
    Async { return Array.length (ref values) }

// A function argument may capture anything; the caller checks what it passes.
def twice :: (i64 -> Async<i64>) -> i64 -> Async<i64>
fn twice step n = Async {
    let! first = step n
    let! second = step first
    return second
}

export def total :: i64 -> i64
fn total n =
    let bump = \\x -> Async { do! Async.yield (); return x + 1 }
    Async.run (moved()) + Async.run (within n) + Async.run (outer_moved [4, 5]) + Async.run (copied()) + Async.run (twice bump n)",
    );
}

#[test]
fn for_loops_accept_arrays_of_copy_values() {
    accepts(
        "export def loops :: i64 -> i64
fn loops n = Async.run (Async {
    let values = Array.init n (\\index -> index)
    for _value in values do
        do! Async.yield ()
    return n
})",
    );
    let error = rejects(
        "let words = [\"a\", \"b\"]\nAsync.run (Async { for _word in words do do! Async.yield () })",
        "E1005",
    );
    assert_eq!(
        error.message,
        "no instance for Copy<string>; define an instance or use a supported type"
    );
}

#[test]
fn all_accepts_values_that_are_not_copy() {
    accepts(
        "def label :: i64 -> Async<string>
fn label n = Async {
    do! Async.sleep n
    return to_string n
}

export def lengths :: i64 -> i64
fn lengths n =
    let texts = Async.run (Async.all [label n, Async.Return \"abc\"])
    let checked: Result<[string], string> = Async.run (Async.all_results [Async.Return (Result.Ok \"x\")])
    match checked with
    | Result.Ok more -> texts[0].length + texts[1].length + more[0].length
    | Result.Error text -> text.length",
    );
}

#[test]
fn unsupported_operations_report_missing_builder_operations() {
    for (source, operation) in [
        (
            "Async.run (Async {\n    let! a = Async.Return 1\n    and! b = Async.Return 2\n    return a + b\n})",
            "MergeSources",
        ),
        (
            "Async.run (Async { while false do do! Async.yield () })",
            "While",
        ),
        ("Async.run (Async { yield 1 })", "Yield"),
    ] {
        let error = rejects(source, "E1018");
        assert!(
            error.message.starts_with(MISSING),
            "{source}\n{}",
            error.message
        );
        assert_eq!(error.message, format!("{MISSING}{operation}'"), "{source}");
    }
}

#[test]
fn implicit_async_bodies_select_the_async_builder() {
    accepts(
        "def worker :: i64 -> Async<i64>
fn worker n =
    do! Async.yield ()
    return n

export def doubled :: i64 -> i64
fn doubled n = Async.run (worker n) * 2",
    );
}

#[test]
fn tasks_can_run_and_receive_async_computations() {
    accepts(
        "export def in_task :: i64 -> i64
fn in_task n =
    let computation = Async { do! Async.sleep 2; let! t = Async.now (); return t + n }
    Task.run (task { return Async.run computation })",
    );
}

const HOST: &str = r"extern def start_operation :: i64 -> unit
extern def cancel_operation :: i64 -> unit
extern def report :: i64 -> unit

def wait_host :: i64 -> Async<i64>
fn wait_host request = Async.host (Async.Operation {
    start: \operation -> start_operation (operation + request),
    cancel: \operation -> cancel_operation operation
})
";

const STARTED: &str = r"
export def begin :: i64 -> unit
fn begin n =
    do! Async.start (Async {
        let! value = wait_host n
        report value
    })
";

const BLOCKING: &str = r"
export def wait :: i64 -> i64
fn wait n =
    let! value = Async.block_on (wait_host n)
    value
";

fn project(source: &str) -> (Project, CheckedModule) {
    let project = Project::single_main(PathBuf::from("Main.tz"), source.to_owned());
    let module = project
        .analyze()
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    (project, module)
}

/// The driver rejects `options` before it writes `name` or anything else.
fn build_error(source: &str, name: &str, options: BuildOptions) -> Diagnostic {
    let (project, module) = project(source);
    let root = std::env::temp_dir().join(format!("tsuzuri-async-rejected-{}", std::process::id()));
    let output = root.join(name);
    let error = driver::build(&module, &project, &output, options).expect_err(source);
    assert!(!root.exists(), "{source}");
    assert_eq!(error.code, "E2000", "{source}\n{}", error.message);
    error
}

#[test]
fn the_executor_exports_its_entry_points_only_when_reachable() {
    for (source, posts) in [
        (format!("{HOST}{STARTED}"), false),
        (format!("{HOST}{BLOCKING}"), true),
    ] {
        let (_, module) = project(&source);
        let header = llvm::header(&module);
        assert!(
            header.contains("int64_t tsuzuri_async_poll(int64_t now);\n"),
            "{header}"
        );
        assert!(
            header.contains("void tsuzuri_async_complete(int64_t operation, int64_t value);\n")
        );
        assert_eq!(
            header.contains("int32_t tsuzuri_async_post(int64_t operation, int64_t value);\n"),
            posts
        );
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert!(ir.contains("define i64 @tsuzuri_async_poll(i64 %arg0) nounwind {"));
            assert!(
                ir.contains("define void @tsuzuri_async_complete(i64 %arg0, i64 %arg1) nounwind {")
            );
            assert_eq!(llvm::uses_reactor(&ir), posts);
            // Each thread has its own executor, and an operation id names the thread that began it.
            if wasm {
                assert!(ir.contains("@tz.async.thread = internal global i64 1, align 8"));
                assert!(ir.contains("@tz.async.next_id = internal global i64 0, align 8"));
            } else {
                assert!(
                    ir.contains("@tz.async.thread = internal thread_local global i64 0, align 8")
                );
                assert!(
                    ir.contains("@tz.async.next_id = internal thread_local global i64 0, align 8")
                );
            }
            assert!(ir.contains("%high = shl i64 %thread, 39"));
            assert!(!ir.contains("atomicrmw xchg"));
            // WebAssembly waits and reads the clock through the imports of JSPI.
            assert_eq!(
                ir.contains(
                    "declare void @tsuzuri_async_wait(i64) \"wasm-import-module\"=\"tsuzuri_async\" \"wasm-import-name\"=\"wait\""
                ),
                posts && wasm
            );
            // Natively a thread waits for, and takes, the completions posted to it.
            assert_eq!(
                ir.contains("declare void @tsuzuri_async_wait(i64, i64)"),
                posts && !wasm
            );
            assert_eq!(
                ir.contains("declare i32 @tsuzuri_async_take(i64, ptr, ptr)"),
                posts && !wasm
            );
        }
    }
    // `Async.run` drives its computation itself.
    let (_, module) = project(&format!(
        "{HOST}export def run_host :: i64 -> i64\nfn run_host n = Async.run (wait_host n)\n"
    ));
    assert!(llvm::host_entries(&module).is_empty());
    assert!(!llvm::header(&module).contains("tsuzuri_async_"));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(!ir.contains("@tsuzuri_async_") && !llvm::uses_reactor(&ir));
}

#[test]
fn the_driver_rejects_executors_that_the_target_cannot_drive() {
    let trapping = "--trap-mode return cannot be combined with Async.start or Async.block_on: a trap inside the executor would leave its state half updated";
    for source in [format!("{HOST}{STARTED}"), format!("{HOST}{BLOCKING}")] {
        for (emit, name) in [
            (Emit::Object, "api.o"),
            (Emit::Header, "api.h"),
            (Emit::BindingsCs, "api.cs"),
        ] {
            let options = BuildOptions {
                emit,
                trap_return: true,
                ..Default::default()
            };
            assert_eq!(build_error(&source, name, options).message, trapping);
        }
        for (emit, name) in [
            (Emit::Wasm, "api.wasm"),
            (Emit::Object, "api.o"),
            (Emit::BindingsJs, "api.mjs"),
        ] {
            let options = BuildOptions {
                target: Target::Wasm32,
                emit,
                wasm_threads: true,
                ..Default::default()
            };
            assert_eq!(
                build_error(&source, name, options).message,
                "--wasm-feature threads cannot be combined with Async.start or Async.block_on: their executor keeps per-thread state that WebAssembly threads do not set up"
            );
        }
    }
    let blocking = format!("{HOST}{BLOCKING}");
    for emit in [Emit::Wasm, Emit::Object, Emit::Llvm] {
        let options = BuildOptions {
            target: Target::Wasm32,
            emit,
            ..Default::default()
        };
        assert_eq!(
            build_error(&blocking, "api.wasm", options).message,
            "Async.block_on on WebAssembly needs --wasm-feature jspi: it suspends the WebAssembly stack with JavaScript Promise Integration while it waits"
        );
    }
}

#[test]
fn javascript_bindings_drive_the_executor_and_return_promises_under_jspi() {
    let (_, started) = project(&format!("{HOST}{STARTED}"));
    let table = bindings::table(&started);
    assert_eq!(table["async"], json!({ "driven": true, "jspi": false }));
    let declarations = bindings::declarations(&started);
    assert!(declarations.contains(
        "export interface AsyncHost {\n  complete(operation: bigint, value: bigint): void;\n  settled(): Promise<void>;\n}\n"
    ));
    assert!(declarations.contains("  begin(arg0: bigint): void;\n"));
    let (_, blocking) = project(&format!("{HOST}{BLOCKING}"));
    let javascript = bindings::javascript_for(&blocking, JsFlavor::Jspi);
    assert!(javascript.contains("\"async\":{\"driven\":true,\"jspi\":true}"));
    assert_eq!(
        javascript,
        bindings::javascript_for(&blocking, JsFlavor::Jspi)
    );
    let declarations = bindings::declarations_for(&blocking, JsFlavor::Jspi);
    assert!(declarations.contains("  wait(arg0: bigint): Promise<bigint>;\n"));
    assert!(declarations.contains("export interface AsyncHost {"));
    // A program without the executor keeps the synchronous glue and its table.
    let (_, plain) = project("export def add :: i64 -> i64\nfn add n = n + 1\n");
    assert!(
        bindings::table(&plain)
            .get("async")
            .is_none_or(|value| value.is_null())
    );
    assert!(!bindings::declarations(&plain).contains("AsyncHost"));
}
