//! The sync types of TZ06 (`WaitGroup`, `Once`, `OnceValue`, `RwMutex`, `Cond`, `Semaphore`,
//! `SyncMap`) and the runtime's `Mutex`: types, privacy, captures, the borrows of the results and the
//! generated code. The schedules are in `node tests/features.mjs <bin> sync_types` and `sync_single`.
use tsuzuri::{
    analyze, llvm,
    trap::{TrapKind, TrapSource},
};

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

fn ir(source: &str) -> String {
    llvm::emit(&accepts(source), llvm::Entry::Console).unwrap()
}

#[test]
fn the_operations_have_their_types() {
    let text = ir("\
let group: WaitGroup = WaitGroup.create ()
WaitGroup.add (ref group) 1
let other: WaitGroup = WaitGroup.share (ref group)
WaitGroup.done (ref other)
WaitGroup.wait (ref group)
WaitGroup.go (ref group) (Owned.function (\\() -> ()))
let once: Once = Once.create ()
Once.call (ref once) (\\() -> ())
let finished: bool = Once.done (ref once)
let cell: OnceValue<string> = OnceValue.create ()
let value: ref string = OnceValue.get (ref cell) (\\() -> \"made\")
let length = String.length value
let lock: RwMutex<i64> = RwMutex.create 1
let read: i64 = RwMutex.with_read (ref lock) (\\x -> deref x)
RwMutex.with_write (ref lock) (\\x -> deref x = read + length)
let mutex = Mutex.create 0i64
let cond: Cond = Cond.create ()
Cond.signal (ref cond)
Cond.broadcast (ref cond)
let got: i64 = Cond.wait_with (ref cond) (ref mutex) (\\x -> deref x >= 0i64) (\\x -> deref x)
let slots: Semaphore = Semaphore.create 2
Semaphore.acquire (ref slots) 1
let tried: bool = Semaphore.try_acquire (ref slots) 1
Semaphore.release (ref slots) 1
let map: SyncMap<i64, i64> = SyncMap.create ()
SyncMap.store (ref map) 1 2
let inner: i64 = RwMutex.into_inner lock
let kept: Maybe<string> = OnceValue.into_inner cell
if finished && tried then got + inner else 0
");
    for declaration in [
        "declare i32 @tsuzuri_tz_waitgroup_add(ptr, i64)",
        "declare i32 @tsuzuri_tz_waitgroup_wait(ptr)",
        "declare i32 @tsuzuri_tz_once_begin(ptr)",
        "declare void @tsuzuri_tz_once_end(ptr)",
        "declare i32 @tsuzuri_tz_rw_read_lock(ptr)",
        "declare i32 @tsuzuri_tz_rw_write_lock(ptr)",
        "declare i32 @tsuzuri_tz_mutex32_lock(ptr)",
        "declare i32 @tsuzuri_tz_cond_wait(ptr, ptr, i64)",
        "declare void @tsuzuri_tz_cond_wake(ptr, i64)",
        "declare i32 @tsuzuri_tz_sem_acquire(ptr, i64, i32)",
        "declare i32 @tsuzuri_tz_sem_release(ptr, i64, i64)",
    ] {
        assert_eq!(
            text.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}"
        );
    }
    // The handles are blocks that the generated code counts; the runtime only waits.
    assert!(text.contains("atomicrmw add ptr"), "{text}");
    assert!(text.contains("atomicrmw sub ptr"), "{text}");
    // OnceValue publishes with release and reads with acquire.
    assert!(text.contains("acquire, align 8"), "{text}");
    // WaitGroup.go starts its tzroutine through the runtime's start function.
    assert!(text.contains("define internal void @tz.waitgroup.go(ptr %box)"));
}

#[test]
fn the_mutex_of_every_native_program_is_the_runtime_lock() {
    // TZ12: a program that starts no tzroutine takes the tzroutine runtime's lock too, and only it.
    let alone =
        ir("let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\value -> deref value)\n");
    assert!(
        alone.contains("call i32 @tsuzuri_tz_mutex32_lock(ptr"),
        "{alone}"
    );
    assert!(!alone.contains("@tsuzuri_mutex_lock("), "{alone}");
    assert!(!alone.contains("@tsuzuri_tz_go"), "{alone}");
    // A program that uses the runtime takes the same lock, and the thread's mark is gone.
    let new = ir(
        "let lock = Mutex.create 0i64\nlet group = WaitGroup.create ()\nMutex.with_lock (ref lock) (\\value -> deref value)\n",
    );
    assert!(
        new.contains("call i32 @tsuzuri_tz_mutex32_lock(ptr"),
        "{new}"
    );
    assert!(!new.contains("@tsuzuri_mutex_lock("), "{new}");
    assert!(!new.contains("tz.mutex.mark"), "{new}");
    let waiting = ir(
        "let lock = Mutex.create 0i64\nlet ch: Channel<i64> = Channel.make 1\nlet borrowed = ref ch\nMutex.with_lock (ref lock) (\\value -> {\n    Channel.send borrowed 1;\n    deref value\n})\n",
    );
    assert!(waiting.contains("call i32 @tsuzuri_tz_mutex32_lock(ptr"));
    assert!(!waiting.contains("tz.routine.unlocked"), "{waiting}");
}

#[test]
fn the_handles_are_opaque_and_the_primitives_private() {
    for source in [
        "let g = WaitGroup { block: 0 }\n0\n",
        "let o = Once { block: 0 }\n0\n",
        "let c = Cond { block: 0 }\n0\n",
        "let s = Semaphore { block: 0 }\n0\n",
        "def peek :: ref Once -> i64\nfn peek once = once.block\n0\n",
        "let o = Once.create ()\nlet _b = Once.__begin (ref o)\n0\n",
        "let o = Once.create ()\nOnce.__end (ref o)\n0\n",
        "let g = WaitGroup.create ()\nWaitGroup.__drop (ref g)\n0\n",
        "let mut x = 1\nlet _old = SyncMap.__replace 2 (ref mut x)\n0\n",
    ] {
        rejects(source, "E1022");
    }
}

#[test]
fn a_handle_or_a_cell_is_not_captured_by_a_reusable_function() {
    for source in [
        "let g = WaitGroup.create ()\nlet f = \\() -> WaitGroup.wait (ref g)\nf ()\n",
        "let l = RwMutex.create 1i64\nlet f = \\() -> RwMutex.with_read (ref l) (\\x -> deref x)\nf ()\n",
        "let c: OnceValue<i64> = OnceValue.create ()\nlet f = \\() -> deref (OnceValue.get (ref c) (\\() -> 1))\nf ()\n",
    ] {
        rejects(source, "E1005");
    }
}

#[test]
fn a_result_of_a_lock_holds_no_borrow_of_the_locked_value() {
    // As `Mutex.with_lock` (tests/concurrency.rs): a function that reads the locked value after the
    // lock is gone.
    for (source, name) in [
        (
            "let lock = RwMutex.create 5i64\nlet kept = RwMutex.with_write (ref lock) (\\value -> { let view = ref (deref value); \\x -> deref view + x })\n0",
            "RwMutex.with_write",
        ),
        (
            "let lock = RwMutex.create 5i64\nlet kept = RwMutex.with_read (ref lock) (\\value -> \\x -> deref value + x)\n0",
            "RwMutex.with_read",
        ),
        (
            "let lock = Mutex.create 5i64\nlet cond = Cond.create ()\nlet kept = Cond.wait_with (ref cond) (ref lock) (\\_ -> true) (\\value -> { let view = ref (deref value); \\x -> deref view + x })\n0",
            "Cond.wait_with",
        ),
    ] {
        let message = rejects(source, "E1013");
        assert!(
            message.contains(&format!(
                "{name} results must be proven free of borrowed environments"
            )),
            "{message}"
        );
    }
    // The call has to be direct when its result may hold a borrow.
    let message = rejects(
        "let lock = RwMutex.create 5i64\nlet reader = RwMutex.with_read (ref lock)\nlet kept = reader (\\value -> \\x -> deref value + x)\n0",
        "E1013",
    );
    assert!(
        message.contains("RwMutex.with_read must be fully applied directly"),
        "{message}"
    );
}

#[test]
fn a_once_value_lends_for_as_long_as_its_cell() {
    rejects(
        "def escape :: unit -> ref i64\nfn escape _ =\n    let cell: OnceValue<i64> = OnceValue.create ()\n    OnceValue.get (ref cell) (\\() -> 1)\n0\n",
        "E1013",
    );
    rejects(
        "let cell: OnceValue<Rc<i64>> = OnceValue.create ()\nlet value = OnceValue.get (ref cell) (\\() -> Rc.new 1)\n0\n",
        "E1013",
    );
}

#[test]
fn a_once_value_keeps_no_borrow_in_its_cell() {
    // The cell outlives the data that an initializer's function may borrow.
    let message = rejects(
        "let cell: OnceValue<i64 -> i64> = OnceValue.create ()\nlet first = {\n    let data = [1i64, 2i64];\n    let view = ref data;\n    let f = OnceValue.get (ref cell) (\\() -> \\x -> x + view[0]);\n    (deref f) 1\n}\nlet g = OnceValue.get (ref cell) (\\() -> \\x -> x)\nfirst + (deref g) 5\n",
        "E1013",
    );
    assert!(
        message.contains(
            "OnceValue.get initializers must return values proven free of borrowed environments"
        ),
        "{message}"
    );
    // A function made inside from values of its own is kept.
    accepts(
        "let cell: OnceValue<i64 -> i64> = OnceValue.create ()\nlet f = OnceValue.get (ref cell) (\\_unit -> {\n    let step = 3i64;\n    \\x -> x + step\n})\n(deref f) 1\n",
    );
    // A partial application cannot be checked where the value is made.
    let message = rejects(
        "let cell: OnceValue<i64 -> i64> = OnceValue.create ()\nlet getter = OnceValue.get (ref cell)\n0\n",
        "E1013",
    );
    assert!(
        message.contains("OnceValue.get must be fully applied directly"),
        "{message}"
    );
}

#[test]
fn a_once_value_lends_only_its_cell() {
    // The reference points into the cell: what the initializer borrowed may go before it.
    accepts(
        "let cell: OnceValue<i64> = OnceValue.create ()\nlet kept = {\n    let data = [7i64];\n    let view = ref data;\n    OnceValue.get (ref cell) (\\() -> view[0])\n}\nderef kept\n",
    );
}

#[test]
fn a_sync_map_stores_values_that_no_function_may_capture() {
    accepts(
        "let map: SyncMap<i64, Mutex<i64>> = SyncMap.create ()\nSyncMap.store (ref map) 1 (Mutex.create 5i64)\nSyncMap.with_value (ref map) (ref 1) (\\lock -> Mutex.with_lock lock (\\value -> deref value))\n",
    );
}

#[test]
fn another_handle_is_no_allocation() {
    for (create, share) in [
        ("WaitGroup.create ()", "WaitGroup.share"),
        ("Once.create ()", "Once.share"),
        ("Cond.create ()", "Cond.share"),
        ("Semaphore.create 2", "Semaphore.share"),
    ] {
        let module = create.split('.').next().unwrap();
        accepts(&format!(
            "@no_alloc\ndef again :: ref {module} -> {module}\nfn again handle = {share} handle\nlet first = {create}\nlet _second = again (ref first)\n0\n"
        ));
    }
}

#[test]
fn a_length_alone_uses_no_scheduler() {
    // `Time.second()` is a constant: the program calls the runtime only for its lock (TZ12) and
    // starts no tzroutine and no timer, so --trap-mode return stays allowed.
    let text = ir(
        "let lock = Mutex.create 0i64\nlet wait = Time.second()\nMutex.with_lock (ref lock) (\\value -> deref value + wait)\n",
    );
    let called: std::collections::BTreeSet<&str> = text
        .match_indices("@tsuzuri_tz_")
        .map(|(start, _)| {
            let name = &text[start + 1..];
            &name[..name
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap()]
        })
        .collect();
    assert_eq!(
        called,
        std::collections::BTreeSet::from([
            "tsuzuri_tz_mutex32_lock",
            "tsuzuri_tz_mutex32_unlock",
            "tsuzuri_tz_report"
        ]),
        "{text}"
    );
}

#[test]
fn the_types_load_their_modules() {
    let named = |text: &str| -> Vec<&'static str> {
        tsuzuri::stdlib::sources_for([text])
            .iter()
            .filter_map(|(path, _)| tsuzuri::stdlib::module_name(path))
            .collect()
    };
    let none = named("0\n");
    for module in [
        "WaitGroup",
        "Once",
        "OnceValue",
        "RwMutex",
        "Cond",
        "Semaphore",
        "SyncMap",
    ] {
        assert!(!none.contains(&module), "{module}");
        assert!(named(&format!("{module}.create ()\n")).contains(&module));
    }
    assert!(named("SyncMap.create ()\n").contains(&"RwMutex"));
}

#[test]
fn a_negative_count_after_go_is_an_assertion_at_the_call() {
    // The child takes its one off after the call has returned: with trap information, its trap
    // names the call of WaitGroup.go, as an assertion.
    let source = "let group = WaitGroup.create ()\nWaitGroup.go (ref group) (Owned.function (\\() -> ()))\nWaitGroup.wait (ref group)\n0\n";
    let module = accepts(source);
    let sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Console,
        wasm: false,
        wasm_simd128: false,
        debug_output: false,
        runtime_objects: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    let output = llvm::emit_with_trap_info(&module, options, &sources).unwrap();
    assert!(
        output
            .ir
            .contains("call void @tsuzuri_tz_go(ptr @tz.waitgroup.go.0, ptr"),
        "{}",
        output.ir
    );
    let call = source.find("WaitGroup.go").unwrap();
    assert!(
        output
            .trap_sites
            .iter()
            .any(|site| site.kind == TrapKind::Assert && site.span.start == call),
        "{:?}",
        output.trap_sites
    );
}
