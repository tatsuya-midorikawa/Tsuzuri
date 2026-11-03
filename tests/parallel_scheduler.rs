//! `Parallel.*` on the tzroutine scheduler and `Routine.scope` (TZ11): the checks of `Task.scope`
//! carried over, and the generated code. The schedules are in `node tests/features.mjs <bin>
//! parallel_scheduler`, the runtime in `node tests/tzroutine_runtime.mjs`.
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

/// Programs of `Task.scope` from `tests/concurrency.rs`, accepted and refused.
fn task_scope_programs() -> Vec<String> {
    let prelude = "let r = Rc.new 1i64\n";
    let mut programs: Vec<String> = [
        "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 4 (\\shared index -> Atomic.fetch_add shared index)\nArray.length (ref seen)",
        "let table = [1i64, 2i64, 3i64]\nlet sums = Task.scope (ref table) 3 (\\shared index -> Array.sum shared + index)\nArray.sum (ref sums)",
        "let text = \"shared\"\nlet lengths = Task.scope (ref text) 2 (\\shared index -> shared.length + index)\nArray.sum (ref lengths)",
        "let shared = Arc.new (Atomic.create 0i64)\nlet seen = Task.scope (ref shared) 2 (\\arc index -> Atomic.fetch_add (Arc.get arc) index)\nArray.sum (ref seen)",
        "let lock = Mutex.create 0i64\nlet seen = Task.scope (ref lock) 4 (\\shared index -> Mutex.with_lock shared (\\value -> { deref value = deref value + index; deref value }))\nArray.length (ref seen)",
        "let seen = Task.scope (ref 5i64) 0 (\\shared index -> deref shared + index)\nArray.length (ref seen)",
        "let outer = Task.scope (ref 1i64) 2 (\\shared index -> Array.sum (ref (Task.scope shared 2 (\\inner position -> deref inner + position + index))))\nArray.sum (ref outer)",
        "let counted = Rc.new 1\nlet seen = Task.scope (ref counted) 2 (\\_shared index -> index)\n0",
        "let pending = task { return 1 }\nlet seen = Task.scope (ref pending) 2 (\\_shared index -> index)\n0",
        "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 4 (\\shared _index -> shared)\n0",
        "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 4 (\\_shared index -> Rc.new index)\n0",
        "let counter = Atomic.create 0i64\nlet other = Atomic.create 0i64\nlet borrowed = ref other\nlet seen = Task.scope (ref counter) 4 (\\_shared _index -> Atomic.load borrowed)\n0",
        "let counter = Atomic.create 0i64\nlet step = 5i64\nlet borrowed = ref step\nlet seen = Task.scope (ref counter) 4 (\\shared _index -> Atomic.fetch_add shared (deref borrowed))\n0",
        "let counter = Atomic.create 0i64\nlet partial: (ref Atomic<i64> -> i64 -> i64) -> [i64] = Task.scope (ref counter) 4\n0",
        "let base = 10i64\nlet seen = Task.scope (ref (\\x -> x + base)) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)",
        "let lock = Mutex.create 5i64\nlet seen = Task.scope (ref lock) 2 (\\shared index -> {\n    let kept = Mutex.with_lock shared (\\value -> { let view = ref (deref value); \\x -> deref view + x });\n    kept index\n})\nArray.sum (ref seen)",
        "let lock = Mutex.create [1i64, 2i64, 3i64]\nlet seen = Task.scope (ref lock) 2 (\\shared index -> Mutex.with_lock shared (\\values -> { let items = ref (deref values); items[index] }))\nArray.sum (ref seen)",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    programs.extend([
        format!("{prelude}let f = {BORROWING_CLOSURE}\nlet seen = Task.scope (ref f) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)"),
        format!("{prelude}let seen = Task.scope (ref ({BORROWING_CLOSURE})) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)"),
        format!("{prelude}let seen = Task.scope (ref [{BORROWING_CLOSURE}]) 4 (\\shared index -> (shared[0]) index)\nArray.sum (ref seen)"),
    ]);
    programs
}

#[test]
fn routine_scope_has_the_checks_of_task_scope() {
    let mut refused = 0;
    for task in task_scope_programs() {
        let routine = task.replace("Task.scope", "Routine.scope");
        match (analyze(&task), analyze(&routine)) {
            (Ok(_), Ok(_)) => {}
            (Err(expected), Err(found)) => {
                refused += 1;
                assert_eq!(expected.code, found.code, "{routine}\n{}", found.message);
                assert_eq!(expected.kind, found.kind, "{routine}\n{}", found.message);
            }
            (task_result, routine_result) => panic!(
                "{routine}\nTask.scope: {:?}\nRoutine.scope: {:?}",
                task_result.err().map(|e| e.message.clone()),
                routine_result.err().map(|e| e.message.clone())
            ),
        }
    }
    assert!(refused >= 8, "{refused}");
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
fn parallel_runs_on_the_scheduler_of_a_program_that_uses_it() {
    let source =
        "let values = Parallel.init 10 (\\index -> index)\nlet total = Parallel.sum (ref values)\n";
    // Without tzroutines, the pool, as before.
    let module = accepts(&format!("{source}total\n"));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(!ir.contains("tsuzuri_tz_"), "{ir}");
    assert!(
        ir.contains("call void @tsuzuri_task_parallel(ptr @tz.parallel.chunk."),
        "{ir}"
    );
    // With them, the scheduler, and the pool inside a trap boundary (status 3).
    let module = accepts(&format!("{source}tzroutine {{ () }}\ntotal\n"));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    let lines: Vec<&str> = ir.lines().collect();
    let scheduler = lines
        .iter()
        .position(|line| line.contains("call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk."))
        .expect("the scheduler's call");
    let pool = lines
        .iter()
        .position(|line| line.contains("@tsuzuri_task_parallel(ptr @tz.parallel.chunk."))
        .expect("the pool's call");
    assert!(scheduler < pool, "{ir}");
    assert!(lines[scheduler + 1].contains(", 3"), "{ir}");
    // Task.scope stays on the pool until TZ12.
    let module = accepts(
        "tzroutine { () }\nlet counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 2 (\\shared index -> Atomic.fetch_add shared index)\nArray.length (ref seen)\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(!ir.contains("@tsuzuri_tz_parallel("), "{ir}");
    assert!(!ir.contains("@tsuzuri_tz_scope_run("), "{ir}");
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
    // A Channel makes the program use the tzroutine runtime without a spawn of its own: the helper
    // tzroutines of Parallel still get the stack probes, and the pool's call (inside a trap
    // boundary) the blocking wrapper.
    let module = accepts(
        "let ch: Channel<i64> = Channel.make 1\nChannel.send ch 1\nlet values = Parallel.init 10 (\\index -> index)\nParallel.sum (ref values)\n",
    );
    assert!(!llvm::spawns(&module));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(
        ir.contains("call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk."),
        "{ir}"
    );
    assert!(ir.contains("\"probe-stack\"=\"inline-asm\""), "{ir}");
    let lines: Vec<&str> = ir.lines().collect();
    let pool = lines
        .iter()
        .position(|line| line.contains("@tsuzuri_task_parallel(ptr @tz.parallel.chunk."))
        .expect("the pool's call");
    assert!(
        lines[..pool]
            .iter()
            .rev()
            .take(3)
            .any(|line| line.contains("call void @tsuzuri_tz_block_enter()")),
        "{ir}"
    );
}
