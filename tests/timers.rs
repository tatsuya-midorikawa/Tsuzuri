//! The timers of `Time`, `Timer` and `Ticker` (TZ05): types, privacy, captures, capabilities and the
//! generated code. The schedules are in `node tests/features.mjs <bin> timers`, the programs in
//! `node tests/tzroutine.mjs` and the runtime in `node tests/tzroutine_runtime.mjs`.
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
let sleep: IO<unit> = Time.sleep Time.millisecond
let after: IO<Channel<i64>> = Time.after Time.second
let tick: IO<Channel<i64>> = Time.tick Time.minute
let created: IO<Timer> = Timer.create Time.nanosecond
let ticking: IO<Ticker> = Ticker.create Time.microsecond
let timer: Timer = Time.after_func Time.hour (Owned.function (\\() -> ()))
let channel: Channel<i64> = Timer.channel timer
let stopped: bool = Timer.stop timer
let again: bool = Timer.reset timer 5
let! ticker = Ticker.create Time.millisecond
let ticks: Channel<i64> = Ticker.channel ticker
let halted: unit = Ticker.stop ticker
let restarted: unit = Ticker.reset ticker 7
Time.hour / Time.minute + Time.minute / Time.second + Time.second / Time.millisecond + Time.millisecond / Time.microsecond + Time.microsecond / Time.nanosecond
";

#[test]
fn the_operations_have_their_types() {
    let module = accepts(EVERY_OPERATION);
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    for declaration in [
        "declare ptr @tsuzuri_tz_timer_new(ptr, ptr, i64, i64, i32, i32, ptr, ptr, ptr)",
        "declare ptr @tsuzuri_tz_timer_chan(ptr)",
        "declare i32 @tsuzuri_tz_timer_stop(ptr)",
        "declare i32 @tsuzuri_tz_timer_reset(ptr, i64)",
        "declare void @tsuzuri_tz_timer_free(ptr)",
    ] {
        assert_eq!(
            ir.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}"
        );
    }
    // The channel's block is the program's allocation, which the runtime frees with @tz.free.
    assert!(ir.contains("call ptr @tz.alloc(i64 136)"), "{ir}");
    assert!(ir.contains("ptr @tz.free, i64"), "{ir}");
    // The function of an after_func is called or dropped by the runtime.
    assert!(ir.contains("define internal void @tz.tzroutine.owned.drop(ptr %box)"));
    // A timer belongs to a boundary: the program waits and stops its timers at the end of main.
    assert!(llvm::spawns(&module));
    assert!(ir.contains("call void @tsuzuri_tz_boundary_leave(ptr %tz.scope)"));
    // 60 + 60 + 1000 + 1000 + 1000
    assert!(EVERY_OPERATION.ends_with("Time.microsecond / Time.nanosecond\n"));
}

#[test]
fn the_constants_count_nanoseconds() {
    for (expression, value) in [
        ("Time.nanosecond", 1i64),
        ("Time.microsecond", 1_000),
        ("Time.millisecond", 1_000_000),
        ("Time.second", 1_000_000_000),
        ("Time.minute", 60_000_000_000),
        ("Time.hour", 3_600_000_000_000),
    ] {
        accepts(&format!(
            "let check: bool = {expression} == {value}\nassert check\n0\n"
        ));
    }
}

#[test]
fn the_runtime_parts_are_private_and_opaque() {
    for source in [
        "Time.__after 5\n",
        "Time.__sleep_ns 5\n",
        "Timer.__create 5\n",
        "let t = Timer { handle: 0 }\n0\n",
        "let t = Ticker { handle: 0 }\n0\n",
    ] {
        rejects(source, "E1022");
    }
    rejects(
        "def peek :: ref Timer -> i64\nfn peek timer =\n    match deref timer with\n    | Timer { handle: h } -> h\n0\n",
        "E1022",
    );
    rejects(
        "def peek :: ref Ticker -> i64\nfn peek ticker = ticker.handle\n0\n",
        "E1022",
    );
}

#[test]
fn a_timer_is_not_captured_by_a_reusable_function() {
    let message = rejects(
        "let timer = Time.after_func Time.hour (Owned.function (\\() -> ()))\nlet stop = \\() -> Timer.stop timer\nstop ()\n",
        "E1005",
    );
    assert!(message.contains("Timer"), "{message}");
    // A function value takes a borrow of it.
    accepts(
        "def halt :: ref Timer -> bool\nfn halt timer = Timer.stop timer\nlet timer = Time.after_func Time.hour (Owned.function (\\() -> ()))\nhalt (ref timer)\n",
    );
}

#[test]
fn the_period_of_a_tick_must_be_positive() {
    let module = accepts("let! ticks = Time.tick 0\nChannel.length ticks\n");
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("icmp sgt i64 %arg0, 0"), "{ir}");
}

#[test]
fn the_operations_that_read_the_clock_need_time() {
    for name in [
        "Time.__sleep_ns",
        "Time.__after",
        "Time.__tick",
        "Time.__after_func",
        "Timer.__create",
        "Ticker.__create",
        "Timer.reset",
        "Ticker.reset",
    ] {
        assert_eq!(
            capabilities::primitive(name, "Time"),
            Some(capabilities::Capability::Time),
            "{name}"
        );
    }
    for name in ["Timer.__free", "Ticker.__free"] {
        assert!(capabilities::PRIVATE_WITHOUT_CAPABILITY.contains(&name));
    }
}

#[test]
fn time_is_an_opt_in_module() {
    let named = |text: &str| -> Vec<&'static str> {
        tsuzuri::stdlib::sources_for([text])
            .iter()
            .filter_map(|(path, _)| tsuzuri::stdlib::module_name(path))
            .collect()
    };
    let without = named("0\n");
    assert!(!without.contains(&"Time") && !without.contains(&"Channel"));
    let with = named("Time.sleep 1\n");
    for module in ["Time", "Timer", "Channel"] {
        assert!(with.contains(&module), "{module}: {with:?}");
    }
}

#[test]
fn a_sleep_alone_does_not_spawn() {
    let module = accepts("do! Time.sleep Time.millisecond\n");
    assert!(!llvm::spawns(&module));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call i32 @tsuzuri_tz_sleep(i64 %arg0)"), "{ir}");
    assert!(!ir.contains("tsuzuri_tz_scope_enter"), "{ir}");
}

#[test]
fn programs_without_timers_keep_their_ir() {
    // Time.sleep_ms is os.c's sleep, whose hook only the tzroutine runtime sets.
    let module = accepts(
        "let! slept = Time.sleep_ms 1\nmatch slept with\n| Result.Ok _ -> 0\n| Result.Error _ -> 1\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(!ir.contains("tsuzuri_tz_"), "{ir}");
    assert!(ir.contains("call i64 @tsuzuri_os_sleep(i64 %arg0)"), "{ir}");
}
