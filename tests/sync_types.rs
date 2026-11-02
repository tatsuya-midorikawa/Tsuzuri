//! The sync types of TZ06 (`WaitGroup`, `Once`, `OnceValue`, `RwMutex`, `Cond`, `Semaphore`,
//! `SyncMap`) and the runtime's `Mutex`: types, privacy, captures, the borrows of the results and the
//! generated code. The schedules are in `node tests/features.mjs <bin> sync_types` and `sync_single`.
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
fn the_mutex_of_a_program_without_the_tzroutine_runtime_keeps_its_ir() {
    let old = ir("let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\value -> deref value)\n");
    assert!(old.contains("call i32 @tsuzuri_mutex_lock(ptr"), "{old}");
    assert!(!old.contains("tsuzuri_tz_"), "{old}");
    // A program that uses the runtime takes the runtime's lock, and the thread's mark is gone.
    let new = ir(
        "let lock = Mutex.create 0i64\nlet group = WaitGroup.create ()\nMutex.with_lock (ref lock) (\\value -> deref value)\n",
    );
    assert!(new.contains("call i32 @tsuzuri_tz_mutex32_lock(ptr"), "{new}");
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
    assert!(message.contains("RwMutex.with_read must be fully applied directly"), "{message}");
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
