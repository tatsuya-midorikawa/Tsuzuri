//! The timers of `Time`, `Timer` and `Ticker` (TZ05): types, privacy, captures, capabilities and the
//! generated code. The schedules are in `node tests/features.mjs <bin> timers`, the programs in
//! `node tests/tzroutine.mjs` and the runtime in `node tests/tzroutine_runtime.mjs`.
use tsuzuri::{analyze, capabilities, llvm, trap::TrapSource};

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
let slept: unit = Time.sleep Time.millisecond()
let after: Channel<i64> = Time.after Time.second()
let tick: Channel<i64> = Time.tick Time.minute()
let created: Timer = Timer.create Time.nanosecond()
let timer: Timer = Time.after_func Time.hour() (Owned.function (\\() -> ()))
let channel: Channel<i64> = Timer.channel timer
let stopped: bool = Timer.stop timer
let again: bool = Timer.reset timer 5
let ticker: Ticker = Ticker.create Time.microsecond()
let ticks: Channel<i64> = Ticker.channel ticker
let halted: unit = Ticker.stop ticker
let restarted: unit = Ticker.reset ticker 7
Time.hour() / Time.minute() + Time.minute() / Time.second() + Time.second() / Time.millisecond() + Time.millisecond() / Time.microsecond() + Time.microsecond() / Time.nanosecond()
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
}

#[test]
fn the_constants_count_nanoseconds() {
    for (expression, value) in [
        ("Time.nanosecond()", 1i64),
        ("Time.microsecond()", 1_000),
        ("Time.millisecond()", 1_000_000),
        ("Time.second()", 1_000_000_000),
        ("Time.minute()", 60_000_000_000),
        ("Time.hour()", 3_600_000_000_000),
    ] {
        accepts(&format!(
            "let check: bool = {expression} == {value}\nassert check\n0\n"
        ));
    }
}

#[test]
fn the_runtime_parts_are_private_and_opaque() {
    for source in [
        "Timer.__create 5\n",
        "Ticker.__create 5\n",
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
        "let timer = Time.after_func Time.hour() (Owned.function (\\() -> ()))\nlet stop = \\() -> Timer.stop timer\nstop ()\n",
        "E1005",
    );
    assert!(message.contains("Timer"), "{message}");
    // A function value takes a borrow of it.
    accepts(
        "def halt :: ref Timer -> bool\nfn halt timer = Timer.stop timer\nlet timer = Time.after_func Time.hour() (Owned.function (\\() -> ()))\nhalt (ref timer)\n",
    );
}

#[test]
fn the_function_of_a_timer_holds_no_borrow() {
    // A30: the timer calls the function after `spawn` freed the string that `g` borrows.
    let head = "def spawn :: i64 -> unit\nfn spawn count =\n    let text = String.repeat count (ref \"abcdefgh\")\n    let borrowed = ref text\n    let g: unit -> unit = \\() -> {\n        do! IO.write_line $\"{borrowed}\"\n    }\n";
    let message = rejects(
        &format!(
            "{head}    let _timer = Time.after_func (5 * Time.millisecond()) (Owned.function g)\n    ()\n\nspawn 4\n0\n"
        ),
        "E1013",
    );
    assert_eq!(
        message,
        "an owned function captures only owned values; this function value holds a borrow"
    );
    // A function value that captures the owned string is the timer's own.
    accepts(
        "def spawn :: i64 -> unit\nfn spawn count =\n    let text = String.repeat count (ref \"abcdefgh\")\n    let g: unit -> unit = \\() -> {\n        do! IO.write_line $\"{text}\"\n    }\n    let _timer = Time.after_func (5 * Time.millisecond()) (Owned.function g)\n    ()\n\nspawn 4\n0\n",
    );
}

#[test]
fn the_period_of_a_tick_must_be_positive() {
    let module = accepts("let ticks = Time.tick 0\nChannel.length (ref ticks)\n");
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("icmp sgt i64 %arg0, 0"), "{ir}");
}

#[test]
fn the_operations_that_read_the_clock_need_time() {
    for name in [
        "Time.sleep",
        "Time.after",
        "Time.tick",
        "Time.after_func",
        "Timer.__create",
        "Ticker.__create",
        "Timer.__reset",
        "Ticker.__reset",
    ] {
        assert_eq!(
            capabilities::primitive(name, "Time"),
            Some(capabilities::Capability::Time),
            "{name}"
        );
    }
    for name in [
        "Timer.__channel",
        "Timer.__stop",
        "Timer.__free",
        "Ticker.__channel",
        "Ticker.__stop",
        "Ticker.__free",
    ] {
        assert!(capabilities::PRIVATE_WITHOUT_CAPABILITY.contains(&name));
    }
}

#[test]
fn the_timers_load_what_their_types_name() {
    let named = |text: &str| -> Vec<&'static str> {
        tsuzuri::stdlib::sources_for([text])
            .iter()
            .filter_map(|(path, _)| tsuzuri::stdlib::module_name(path))
            .collect()
    };
    // Time stays always loaded; a program of its old functions or of a sleep loads nothing more.
    for text in [
        "0\n",
        "Time.sleep_ms 1\n",
        "Time.sleep 1\n",
        "Time.after_all\n",
    ] {
        let loaded = named(text);
        assert!(loaded.contains(&"Time"), "{text}");
        assert!(
            !loaded.contains(&"Channel") && !loaded.contains(&"Timer"),
            "{text}: {loaded:?}"
        );
    }
    for (text, modules) in [
        ("Time.after 1\n", &["Channel"][..]),
        ("Time . tick 1\n", &["Channel"]),
        ("Time.after_func 1 f\n", &["Timer", "Channel"]),
        ("Ticker.create 1\n", &["Ticker", "Channel"]),
        ("Time /* wait */ .after 1\n", &["Channel"]),
        ("Time // the clock\n    .after 1\n", &["Channel"]),
        ("Time\n    .after_func 1 f\n", &["Timer", "Channel"]),
    ] {
        let loaded = named(text);
        for module in modules {
            assert!(loaded.contains(module), "{text}: {module}: {loaded:?}");
        }
    }
    // A member named inside a string literal loads nothing.
    let loaded = named("\"Time.after\" 1\n");
    assert!(!loaded.contains(&"Channel"), "{loaded:?}");
    // A character literal is not a string, and the hole of an interpolated string is code.
    for (text, module) in [
        ("let quote = '\"'\nTime.after 1\n", "Channel"),
        ("$\"{Time.after 1}\"\n", "Channel"),
        (
            "$\"a {{b}} {Channel.length (ref (Time.tick 1))} c\"\n",
            "Channel",
        ),
        ("$\"{\"x\"}{Time.after_func 1 f}\"\n", "Timer"),
    ] {
        let loaded = named(text);
        assert!(loaded.contains(&module), "{text}: {loaded:?}");
    }
    // The examples of the ticket, which name no Channel.
    accepts("let deadline = Time.after Time.millisecond()\nselect\n| recv _ from deadline -> 1\n");
    accepts("let _timer = Time.after_func Time.millisecond() (Owned.function (\\() -> ()))\n0\n");
}

#[test]
fn a_sleep_alone_does_not_spawn() {
    let module = accepts("Time.sleep Time.millisecond()\n");
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

#[test]
fn a_sleep_ms_in_a_waiting_program_may_hold_a_lock() {
    // On a tzroutine, os.c's sleep waits through the scheduler's hook. Since TZ06 the program's
    // Mutex.with_lock is the runtime's lock of the G, which a tzroutine may hold while it waits,
    // so no thread mark is checked (as for Time.sleep).
    let module = accepts(
        "tzroutine { () }\nlet! slept = Time.sleep_ms 1\nmatch slept with\n| Result.Ok _ -> 0\n| Result.Error _ -> 1\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call i64 @tsuzuri_os_sleep(i64 %arg0)"), "{ir}");
    assert!(!ir.contains("tz.routine.unlocked"), "{ir}");
    assert!(!ir.contains("tz.mutex.locked"), "{ir}");
}

#[test]
fn trap_info_keeps_the_signature_of_the_free_that_the_runtime_calls() {
    // The runtime frees a timer's channel with the `@tz.free` that the program hands it, as
    // `void (*)(void *)`: the trap instrumentation must not give it a context parameter.
    let source = "let fired = Time.after Time.millisecond()\nmatch Channel.recv fired with\n| Maybe.Some _ -> 1\n| Maybe.None -> 0\n";
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
    assert!(output.ir.contains("ptr @tz.free, i64"), "{}", output.ir);
    let definition = output
        .ir
        .lines()
        .find(|line| line.starts_with("define internal void @tz.free("))
        .expect("the definition of @tz.free");
    assert!(!definition.contains("%tz.context"), "{definition}");
    assert!(
        !output.ir.contains("call void @tz.free(ptr %box, i32"),
        "{}",
        output.ir
    );
    // A program without timers keeps its instrumented `@tz.free`.
    let plain = "let values = [1, 2, 3]\nArray.sum values\n";
    let module = accepts(plain);
    let sources = [TrapSource {
        path: "Main.tz",
        text: plain,
    }];
    let output = llvm::emit_with_trap_info(&module, options, &sources).unwrap();
    let definition = output
        .ir
        .lines()
        .find(|line| line.starts_with("define internal void @tz.free("))
        .expect("the definition of @tz.free");
    assert!(definition.contains("%tz.context"), "{definition}");
}
