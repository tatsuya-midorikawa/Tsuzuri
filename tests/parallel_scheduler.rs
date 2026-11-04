//! `Parallel.*` on the tzroutine scheduler and `Routine.scope` (TZ11; every native program since
//! TZ12): the checks that `Task.scope` had, and the generated code. The schedules are in `node
//! tests/features.mjs <bin> parallel_scheduler`, the runtime in `node tests/tzroutine_runtime.mjs`.
use tsuzuri::{analyze, llvm};

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

const BORROWING_CLOSURE: &str = "\\x -> { let again = Rc.share r; deref (Rc.get (ref again)) + x }";

/// Programs of `Routine.scope`, with the code and the kind that refuse each one (`None`: accepted).
/// They are the programs of `Task.scope` from `tests/concurrency.rs` that TZ11 compared with
/// `Routine.scope`, with what `Task.scope` answered (an owned function stands for the task value).
fn scope_programs() -> Vec<(String, Option<(&'static str, &'static str)>)> {
    let prelude = "let r = Rc.new 1i64\n";
    let sync = Some(("E1013", "task_requires_sync"));
    let borrows = Some(("E1013", "parallel_callback_borrows_environment"));
    let captures_rc = Some(("E1005", "capture_rc_in_function_value"));
    let mut programs: Vec<(String, Option<(&str, &str)>)> = [
        ("let counter = Atomic.create 0i64\nlet seen = Routine.scope (ref counter) 4 (\\shared index -> Atomic.fetch_add shared index)\nArray.length (ref seen)", None),
        ("let table = [1i64, 2i64, 3i64]\nlet sums = Routine.scope (ref table) 3 (\\shared index -> Array.sum shared + index)\nArray.sum (ref sums)", None),
        ("let text = \"shared\"\nlet lengths = Routine.scope (ref text) 2 (\\shared index -> shared.length + index)\nArray.sum (ref lengths)", None),
        ("let shared = Arc.new (Atomic.create 0i64)\nlet seen = Routine.scope (ref shared) 2 (\\arc index -> Atomic.fetch_add (Arc.get arc) index)\nArray.sum (ref seen)", None),
        ("let lock = Mutex.create 0i64\nlet seen = Routine.scope (ref lock) 4 (\\shared index -> Mutex.with_lock shared (\\value -> { deref value = deref value + index; deref value }))\nArray.length (ref seen)", None),
        ("let seen = Routine.scope (ref 5i64) 0 (\\shared index -> deref shared + index)\nArray.length (ref seen)", None),
        ("let outer = Routine.scope (ref 1i64) 2 (\\shared index -> Array.sum (ref (Routine.scope shared 2 (\\inner position -> deref inner + position + index))))\nArray.sum (ref outer)", None),
        ("let counted = Rc.new 1\nlet seen = Routine.scope (ref counted) 2 (\\_shared index -> index)\n0", sync),
        ("let pending = Owned.function (\\() -> 1)\nlet seen = Routine.scope (ref pending) 2 (\\_shared index -> index)\n0", sync),
        ("let counter = Atomic.create 0i64\nlet seen = Routine.scope (ref counter) 4 (\\shared _index -> shared)\n0", Some(("E1013", "task_requires_owned_value"))),
        ("let counter = Atomic.create 0i64\nlet seen = Routine.scope (ref counter) 4 (\\_shared index -> Rc.new index)\n0", Some(("E1013", "task_requires_send"))),
        ("let counter = Atomic.create 0i64\nlet other = Atomic.create 0i64\nlet borrowed = ref other\nlet seen = Routine.scope (ref counter) 4 (\\_shared _index -> Atomic.load borrowed)\n0", borrows),
        ("let counter = Atomic.create 0i64\nlet step = 5i64\nlet borrowed = ref step\nlet seen = Routine.scope (ref counter) 4 (\\shared _index -> Atomic.fetch_add shared (deref borrowed))\n0", borrows),
        ("let counter = Atomic.create 0i64\nlet partial: (ref Atomic<i64> -> i64 -> i64) -> [i64] = Routine.scope (ref counter) 4\n0", Some(("E1013", "must_apply_directly"))),
        ("let base = 10i64\nlet seen = Routine.scope (ref (\\x -> x + base)) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)", None),
        ("let lock = Mutex.create 5i64\nlet seen = Routine.scope (ref lock) 2 (\\shared index -> {\n    let kept = Mutex.with_lock shared (\\value -> { let view = ref (deref value); \\x -> deref view + x });\n    kept index\n})\nArray.sum (ref seen)", Some(("E1013", "mutex_result_retains_borrow"))),
        ("let lock = Mutex.create [1i64, 2i64, 3i64]\nlet seen = Routine.scope (ref lock) 2 (\\shared index -> Mutex.with_lock shared (\\values -> { let items = ref (deref values); items[index] }))\nArray.sum (ref seen)", None),
    ]
    .into_iter()
    .map(|(source, expected)| (source.to_owned(), expected))
    .collect();
    programs.extend([
        (format!("{prelude}let f = {BORROWING_CLOSURE}\nlet seen = Routine.scope (ref f) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)"), captures_rc),
        (format!("{prelude}let seen = Routine.scope (ref ({BORROWING_CLOSURE})) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)"), captures_rc),
        (format!("{prelude}let seen = Routine.scope (ref [{BORROWING_CLOSURE}]) 4 (\\shared index -> (shared[0]) index)\nArray.sum (ref seen)"), captures_rc),
    ]);
    programs
}

#[test]
fn routine_scope_has_the_checks_of_task_scope() {
    for (source, expected) in scope_programs() {
        match (analyze(&source), expected) {
            (Ok(_), None) => {}
            (Err(found), Some((code, kind))) => {
                assert_eq!(found.code, code, "{source}\n{}", found.message);
                assert_eq!(found.kind, Some(kind), "{source}\n{}", found.message);
            }
            (result, expected) => panic!(
                "{source}\nexpected {expected:?}, found {:?}",
                result.err().map(|e| (e.code, e.message.clone()))
            ),
        }
        // TZ12 removed Task.scope: an accepted call written with it is E1040, whose help names
        // Routine.scope.
        if expected.is_none() {
            let removed = source.replace("Routine.scope", "Task.scope");
            let error = analyze(&removed).expect_err(&removed);
            assert_eq!(error.code, "E1040", "{removed}\n{}", error.message);
            assert_eq!(error.kind, Some("removed_task_api"), "{removed}");
        }
    }
}

#[test]
fn routine_scope_runs_each_child_on_a_tzroutine() {
    let module = accepts(
        "let numbers = [1i64, 2i64, 3i64]\nlet sums = Routine.scope (ref numbers) 3 (\\shared index -> shared[index] * 2)\nArray.sum (ref sums)\n",
    );
    assert!(llvm::spawns(&module));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(
        ir.contains("call i32 @tsuzuri_tz_scope_run(ptr @tz.parallel.chunk."),
        "{ir}"
    );
    assert!(!ir.contains("@tsuzuri_task_parallel("), "{ir}");
}

#[test]
fn parallel_runs_on_the_scheduler_in_every_native_program() {
    let source =
        "let values = Parallel.init 10 (\\index -> index)\nlet total = Parallel.sum (ref values)\n";
    // With tzroutines and without them (TZ12; the task runtime's pool before), one call of the
    // scheduler for each parallel operation. Inside a trap boundary the scheduler runs the chunks on
    // the calling thread, so no other call follows.
    for program in [
        format!("{source}total\n"),
        format!("{source}tzroutine {{ () }}\ntotal\n"),
    ] {
        let module = accepts(&program);
        let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
        assert_eq!(
            ir.matches("call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk.")
                .count(),
            2,
            "{ir}"
        );
        assert!(!ir.contains("tsuzuri_task_parallel"), "{ir}");
        // WebAssembly keeps its own entry: in order, or on the workers of `--wasm-feature threads`.
        let wasm = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
        assert!(
            wasm.contains("call void @tsuzuri_task_parallel(ptr @tz.parallel.chunk.")
                && !wasm.contains("@tsuzuri_tz_parallel("),
            "{wasm}"
        );
    }
}

#[test]
fn routine_scope_is_refused_where_tzroutines_are() {
    // A tzroutine of each child: like `tzroutine { }`, never on the default WebAssembly target (TZ09)
    // and never in a pure function.
    let error = analyze(
        "pure def spread :: ref [i64] -> [i64]\nfn spread values = Routine.scope values 2 (\\shared index -> shared[index])\n0\n",
    )
    .expect_err("a pure function cannot spawn");
    assert!(error.message.contains("Routine.scope"), "{}", error.message);
}

#[test]
fn helpers_without_a_spawn_are_instrumented() {
    // Without a spawn of its own, alone or with a Channel, the helper tzroutines of Parallel still
    // get the stack probes; a wait for them is no blocking call (TZ12: the pool and its blocking
    // wrapper are gone).
    for source in [
        "let values = Parallel.init 10 (\\index -> index)\nParallel.sum (ref values)\n",
        "let ch: Channel<i64> = Channel.make 1\nChannel.send ch 1\nlet values = Parallel.init 10 (\\index -> index)\nParallel.sum (ref values)\n",
    ] {
        let module = accepts(source);
        assert!(!llvm::spawns(&module));
        let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
        assert!(
            ir.contains("call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk."),
            "{ir}"
        );
        assert!(ir.contains("\"probe-stack\"=\"inline-asm\""), "{ir}");
        assert!(!ir.contains("@tsuzuri_tz_block_enter"), "{ir}");
    }
}
