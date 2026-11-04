//! `tzroutine { }`, the `Routine` module and the waits of the boundaries (TZ02).
use std::path::Path;
use std::process::Command;

use tsuzuri::{
    analyze, analyze_modules, formatter::format_source, llvm, parser, syntax::SourceKind,
};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(
        ir,
        llvm::emit(&module, llvm::Entry::Library).unwrap(),
        "{source}: IR is deterministic"
    );
    module
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

fn entry_debug(source: &str) -> String {
    let program = parser::parse(source).unwrap_or_else(|e| panic!("{source}\n{}", e.message));
    format!("{:?}", program.entry)
}

#[test]
fn parses_tzroutine_as_a_contextual_keyword() {
    assert!(entry_debug("tzroutine {\n    ()\n}\n").contains("Routine("));
    assert!(
        entry_debug("let n = 1\ntzroutine {\n    let m = n\n    let _k = m\n}\n")
            .contains("Routine(")
    );
    for source in [
        "let tzroutine = 1\ntzroutine + 1\n",
        "def tzroutine :: i64 -> i64 = \\x -> x\ntzroutine 1\n",
        // On the next line the brace is a block of its own, not the body of a tzroutine.
        "let tzroutine = 1\nlet x = tzroutine\n{ () }\nx\n",
    ] {
        assert!(!entry_debug(source).contains("Routine("), "{source}");
        accepts(source);
    }
    assert!(
        tsuzuri::syntax::CONTEXTUAL_KEYWORDS.contains(&"tzroutine"),
        "the word is contextual, not reserved"
    );
}

#[test]
fn formats_tzroutine() {
    let source = "tzroutine {\n    do! IO.write_line \"x\"\n}\n42\n";
    let formatted = format_source("Main.tz", source, SourceKind::Code).unwrap();
    assert!(
        formatted.formatted.contains("tzroutine {\n"),
        "{}",
        formatted.formatted
    );
    let again = format_source("Main.tz", &formatted.formatted, SourceKind::Code).unwrap();
    assert_eq!(formatted.formatted, again.formatted);
    assert!(!again.changed);
}

#[test]
fn checks_the_body_and_the_captures() {
    accepts("tzroutine { () }\n0\n");
    accepts("tzroutine {\n    do! IO.write_line \"x\"\n}\n0\n");
    accepts("let s = \"a\"\ntzroutine {\n    let _n = String.length (ref s)\n}\n0\n");
    accepts(
        "let shared = Arc.new (Atomic.create 0i64)\nlet mine = Arc.share (ref shared)\ntzroutine {\n    let _old = Atomic.fetch_add (Arc.get (ref mine)) 1i64\n}\nAtomic.load (Arc.get (ref shared))\n",
    );
    // A Copy capture is copied, so the outside keeps it.
    accepts("let n = 5\ntzroutine {\n    let _m = n + 1\n}\nn\n");
    // Inside the body, a borrow of the body's own values is free.
    accepts(
        "tzroutine {\n    let items = [1, 2, 3]\n    let first = ref items\n    let _n = Array.length first\n}\n0\n",
    );
    // `let` ends at a newline, and tzroutines nest.
    accepts(
        "tzroutine {\n    let a = 1\n    let b = a + 1\n    tzroutine {\n        let _c = b\n    }\n}\n0\n",
    );
    rejects("tzroutine { 1 }\n0\n", "E1003");
    rejects("let x = tzroutine { () }\nx + 1\n", "E1003");
    rejects(
        "let x = 5\nlet r = ref x\ntzroutine {\n    let _y = deref r\n}\n0\n",
        "E1013",
    );
    rejects(
        "let r = Rc.new 1\ntzroutine {\n    let _x = Rc.get (ref r)\n}\n0\n",
        "E1013",
    );
    rejects(
        "let s = \"abc\"\ntzroutine {\n    let _n = String.length (ref s)\n}\nString.length (ref s)\n",
        "E1012",
    );
    rejects("let mut n = 0\ntzroutine {\n    n = 1\n}\nn\n", "E1014");
    rejects(
        "for i in 0 ..< 3 do\n    tzroutine {\n        break\n    }\n0\n",
        "E1023",
    );
    rejects(
        "pure def f :: i64 -> i64 = \\x -> {\n    tzroutine { () };\n    x\n}\nf 1\n",
        "E1029",
    );
    rejects("const C: unit = tzroutine { () }\n0\n", "E1026");
}

#[test]
fn tzroutine_module_functions() {
    accepts("Routine.yield ()\n0\n");
    accepts(
        "let n = Routine.count ()\nlet p = Routine.max_procs ()\nlet c = Routine.cpu_count ()\nn + p + c\n",
    );
    accepts("Routine.go (Owned.function (\\() -> ()))\n0\n");
    accepts("Routine.go (Owned.function (\\() -> {\n    do! IO.write_line \"go\"\n}))\n0\n");
    accepts("let f: unit -> unit = Routine.yield\nf ()\n0\n");
    accepts(
        "let spawn: Owned.Function<unit, unit> -> unit = Routine.go\nspawn (Owned.function (\\() -> ()))\n0\n",
    );
    // Routine.go takes an owned function, not a plain function value.
    rejects("Routine.go (\\() -> ())\n0\n", "E1005");
    rejects(
        "pure def f :: unit -> i64 = \\() -> Routine.count ()\nf ()\n",
        "E1029",
    );
    let error = analyze_modules(&[("Main.tz", "0\n"), ("Routine.tz", "def x :: i64 = 1\n")])
        .expect_err("Routine is a reserved module name");
    assert_eq!(error.code, "E1011", "{}", error.message);
}

#[test]
fn emits_a_spawn_and_declares_the_runtime_once() {
    let module = accepts(
        "let shared = Arc.new (Atomic.create 0i64)\nfor _i in 0 ..< 3 do\n    let mine = Arc.share (ref shared)\n    tzroutine {\n        let _old = Atomic.fetch_add (Arc.get (ref mine)) 1i64\n    }\nRoutine.go (Owned.function (\\() -> ()))\nRoutine.yield ()\nRoutine.count () + Routine.max_procs () + Routine.cpu_count ()\n",
    );
    assert!(llvm::spawns(&module));
    for entry in [llvm::Entry::Library, llvm::Entry::Console] {
        let ir = llvm::emit(&module, entry).unwrap();
        assert!(
            ir.contains("call void @tsuzuri_tz_go(ptr @tz.tzroutine.start.$once."),
            "{ir}"
        );
        assert!(
            ir.contains("define internal void @tz.tzroutine.start.$once."),
            "{ir}"
        );
        assert!(
            ir.contains("call void @tsuzuri_tz_go(ptr @tz.tzroutine.owned, ptr"),
            "{ir}"
        );
        for declaration in [
            "declare void @tsuzuri_tz_go(ptr, ptr)",
            "declare void @tsuzuri_tz_yield()",
            "declare i64 @tsuzuri_tz_count()",
            "declare i64 @tsuzuri_tz_max_procs()",
            "declare i64 @tsuzuri_tz_cpu_count()",
        ] {
            assert_eq!(ir.matches(declaration).count(), 1, "{declaration}\n{ir}");
        }
        assert!(llvm::duplicate_declarations(&ir).is_empty(), "{ir}");
    }
}

/// The definition of `@name` in the IR, from its `define` line to its closing brace.
fn definition<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .lines()
        .position(|line| line.starts_with("define ") && line.contains(&format!("@{name}(")))
        .unwrap_or_else(|| panic!("no @{name} in\n{ir}"));
    let offset: usize = ir.lines().take(start).map(|line| line.len() + 1).sum();
    let rest = &ir[offset..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |end| end + 2)]
}

fn assert_wrapped(function: &str) {
    assert_eq!(
        function
            .matches("call ptr @tsuzuri_tz_scope_enter()")
            .count(),
        1,
        "{function}"
    );
    let returns = function
        .lines()
        .filter(|line| line.trim_start().starts_with("ret "))
        .count();
    assert!(returns > 0, "{function}");
    assert_eq!(
        function
            .matches("call void @tsuzuri_tz_boundary_leave(ptr %tz.scope)")
            .count(),
        returns,
        "{function}"
    );
}

#[test]
fn wraps_every_boundary() {
    let source = "export def f :: i64 -> i64\nfn f n =\n    tzroutine { () }\n    n + 1\n\ntest \"spawns\" =\n    tzroutine { () }\n    assert (1 == 1)\n\ntzroutine { () }\n42\n";
    let module = accepts(source);
    let library = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_wrapped(definition(&library, "tz_f"));
    let console = llvm::emit(&module, llvm::Entry::Console).unwrap();
    let main = definition(&console, "main");
    assert_wrapped(main);
    // The result is printed after the wait.
    let leave = main.find("@tsuzuri_tz_boundary_leave").unwrap();
    assert!(main.find("@tz.fn.").unwrap() < leave, "{main}");
    let tests = llvm::emit(&module, llvm::Entry::TestRunner).unwrap();
    assert_wrapped(definition(&tests, "tsuzuri_test_run"));
    // A def main and an IO entry wait too: @main waits for a def main, @tsuzuri_main for IO.
    let def_main = accepts("def main :: unit -> i32 = \\() ->\n    tzroutine { () }\n    0\n");
    assert_wrapped(definition(
        &llvm::emit(&def_main, llvm::Entry::Console).unwrap(),
        "main",
    ));
    assert_wrapped(definition(
        &llvm::emit(&def_main, llvm::Entry::Library).unwrap(),
        "tsuzuri_main",
    ));
}

#[test]
fn probes_the_stack_only_when_the_program_spawns() {
    // A frame can be larger than a tzroutine's guard: every function of a spawning program probes
    // its frame, and no other program changes.
    let spawning = accepts("tzroutine { () }\n0\n");
    for entry in [llvm::Entry::Library, llvm::Entry::Console] {
        let ir = llvm::emit(&spawning, entry).unwrap();
        let defines: Vec<&str> = ir
            .lines()
            .filter(|line| line.starts_with("define "))
            .collect();
        assert!(!defines.is_empty());
        // Windows probes with __chkstk itself
        for define in defines {
            assert_eq!(
                define.contains(" \"probe-stack\"=\"inline-asm\" "),
                !cfg!(windows),
                "{define}"
            );
        }
    }
    let plain = accepts("Routine.yield ()\n0\n");
    assert!(
        !llvm::emit(&plain, llvm::Entry::Console)
            .unwrap()
            .contains("probe-stack")
    );
    // The entry block's allocas come before its calls: inlining a call would otherwise move an
    // alloca out of the entry block, and a probed dynamic alloca can be misaligned on AArch64.
    let printing = accepts("tzroutine { () }\n42\n");
    let console = llvm::emit(&printing, llvm::Entry::Console).unwrap();
    let main = definition(&console, "main");
    let entry: Vec<&str> = main
        .lines()
        .skip(2)
        .take_while(|line| !line.trim_start().starts_with("ret"))
        .collect();
    let first_call = entry
        .iter()
        .position(|line| line.contains(" call "))
        .unwrap();
    let last_alloca = entry
        .iter()
        .rposition(|line| line.contains(" = alloca "))
        .unwrap();
    assert!(last_alloca < first_call, "{main}");
}

#[test]
fn scope_children_are_waited_for_with_their_tzroutines() {
    // TZ02 made the function that a pool worker ran (a child of Task.scope) a boundary of its own. A
    // child of Routine.scope is a tzroutine of the scope (TZ11): tsuzuri_tz_scope_run returns after
    // every child and every tzroutine that they spawned (run in tests/tzroutine.mjs), so the child's
    // function is called as it is, and the status of the wait is checked.
    let module = accepts(
        "let counter = Arc.new (Atomic.create 0i64)\nlet _seen = Routine.scope (ref counter) 4 (\\shared index -> {\n    let mine = Arc.share shared;\n    tzroutine {\n        let _n = Atomic.fetch_add (Arc.get (ref mine)) 1i64\n    };\n    index\n})\n0\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    let call = ir
        .lines()
        .find(|line| line.contains("@tsuzuri_tz_scope_run(ptr @"))
        .unwrap_or_else(|| panic!("{ir}"));
    let callback: String = call[call.find("(ptr @").unwrap() + 6..]
        .chars()
        .take_while(|c| !matches!(c, ',' | ')' | ' '))
        .collect();
    let child = definition(&ir, &callback);
    assert!(!child.contains("@tsuzuri_tz_scope_enter"), "{child}");
    assert!(!ir.contains("tsuzuri_task_parallel"), "{ir}");
}

#[test]
fn a_tzroutine_may_yield_inside_a_mutex() {
    // TZ06: a program that spawns takes the runtime's lock, which belongs to the G, so a tzroutine
    // may switch away inside Mutex.with_lock; the thread's mark of TZ02 is gone.
    let module = accepts(
        "let lock = Mutex.create 0i64\ntzroutine { () }\nMutex.with_lock (ref lock) (\\value -> {\n    Routine.yield ();\n    deref value\n})\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call i32 @tsuzuri_tz_mutex32_lock(ptr"), "{ir}");
    assert!(
        ir.contains("call void @tsuzuri_tz_mutex32_unlock(ptr"),
        "{ir}"
    );
    assert!(
        !ir.contains("tz.mutex.locked") && !ir.contains("tz.routine.unlocked"),
        "{ir}"
    );
    // Without a spawn the lock is the same: every native program takes the runtime's lock (TZ12).
    let plain = accepts(
        "let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\value -> deref value)\n",
    );
    let ir = llvm::emit(&plain, llvm::Entry::Console).unwrap();
    assert!(
        ir.contains("call i32 @tsuzuri_tz_mutex32_lock(ptr") && !ir.contains("tsuzuri_mutex_"),
        "{ir}"
    );
}

/// The program of the fixture `name` of tests/fixtures, checked.
fn fixture(name: &str) -> tsuzuri::check::CheckedModule {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join(name)).unwrap() {
        let path = entry.unwrap().path();
        if matches!(path.extension().and_then(|e| e.to_str()), Some("tz" | "tt")) {
            let file = path.file_name().unwrap().to_str().unwrap().to_owned();
            files.push((file, std::fs::read_to_string(&path).unwrap()));
        }
    }
    files.sort();
    let sources: Vec<(&str, &str)> = files
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    analyze_modules(&sources).unwrap_or_else(|e| panic!("{name}: {}", e.message))
}

#[test]
fn programs_without_tzroutine_keep_their_ir() {
    // Natively a Mutex takes the scheduler's lock (TZ12): data_channel names tsuzuri_tz_ for it, and
    // waits for no tzroutine either.
    for name in ["data_channel", "control", "net_sockets"] {
        let module = fixture(name);
        assert!(!llvm::spawns(&module), "{name}");
        for entry in [
            llvm::Entry::Library,
            llvm::Entry::Console,
            llvm::Entry::TestRunner,
        ] {
            if entry == llvm::Entry::Console && module.entry.is_none() {
                continue;
            }
            let ir = llvm::emit(&module, entry).unwrap();
            if name != "data_channel" {
                assert!(!ir.contains("tsuzuri_tz_"), "{name}: {ir}");
            }
            assert!(!ir.contains("%tz.scope"), "{name}");
            assert!(!ir.contains("probe-stack"), "{name}");
        }
    }
    // Native parallel work starts helper tzroutines without a spawn of the program (TZ11, TZ12):
    // the boundaries wait for them and the frames are probed, natively only.
    let module = fixture("pure_parallel");
    assert!(!llvm::spawns(&module));
    let native = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(
        native.contains("call ptr @tsuzuri_tz_scope_enter()")
            && native.contains("probe-stack") != cfg!(windows),
        "{native}"
    );
    let wasm = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    assert!(
        !wasm.contains("%tz.scope") && !wasm.contains("probe-stack"),
        "{wasm}"
    );
    // A program that only yields or counts does not spawn: its boundaries do not wait.
    let module = accepts("Routine.yield ()\nRoutine.count ()\n");
    assert!(!llvm::spawns(&module));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(!ir.contains("@tsuzuri_tz_scope_enter"), "{ir}");
}

/// The last JSON diagnostic that `tsuzuri build --json` printed.
fn build_error(directory: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("build")
        .arg(directory)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{args:?} should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rfind(|line| line.starts_with('{'))
        .unwrap_or_else(|| panic!("{args:?}: no JSON diagnostic in\n{stderr}"));
    serde_json::from_str(line).unwrap()
}

#[test]
fn rejects_unsupported_targets() {
    let directory = std::env::temp_dir().join(format!("tsuzuri-tzroutine-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("Main.tz"),
        "export def f :: i64 -> i64\nfn f n =\n    tzroutine { () }\n    n\n",
    )
    .unwrap();
    let output = directory.join("out.bin");
    let output = output.to_str().unwrap();
    let glue = directory.join("out.mjs");
    let glue = glue.to_str().unwrap();
    for (args, kind) in [
        (
            vec!["--target", "wasm32", "-o", output],
            "option_requires_option",
        ),
        // TZ09: jspi runs tzroutines on wasm32 only, and not with the WebGPU imports.
        (
            vec!["--target", "wasm64", "--wasm-feature", "jspi", "-o", output],
            "option_conflict",
        ),
        (
            vec![
                "--target",
                "wasm32",
                "--wasm-feature",
                "jspi",
                "--wasm-feature",
                "webgpu",
                "-o",
                output,
            ],
            "option_conflict",
        ),
        (
            vec!["--target", "wasm32", "--emit", "bindings-js", "-o", glue],
            "option_requires_option",
        ),
        (
            vec![
                "--target",
                "wasm32",
                "--wasm-feature",
                "threads",
                "-o",
                output,
            ],
            "option_conflict",
        ),
        (
            vec![
                "--freestanding",
                "--allocator",
                "host",
                "--emit",
                "object",
                "-o",
                output,
            ],
            "option_conflict",
        ),
        (
            vec!["--trap-mode", "return", "--emit", "object", "-o", output],
            "option_conflict",
        ),
    ] {
        let error = build_error(&directory, &args);
        assert_eq!(error["code"], "E2000", "{args:?}: {error}");
        assert_eq!(error["kind"], kind, "{args:?}: {error}");
        assert!(
            error["message"].as_str().unwrap().contains("tzroutine"),
            "{args:?}: {error}"
        );
    }
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn routine_yield_is_not_a_trap_of_the_effect_rules() {
    // TZ06: a yield inside `Mutex.with_lock` is allowed (the lock is the G's), so `@no_trap`
    // accepts one.
    accepts("@no_trap\ndef pause :: unit -> unit = \\() -> Routine.yield ()\n\n0\n");
}

#[test]
fn the_lock_list_of_the_g_replaces_the_thread_mark() {
    // TZ06: the runtime keeps the locks that a G holds, so no thread-local mark is read across a
    // yield (R2); a failure domain releases the locks that a trap left with the depth it saved.
    let module = accepts(
        "def work :: i64 -> i64\nfn work n = n + 1\nlet lock = Mutex.create 0i64\nlet held = Mutex.with_lock (ref lock) (\\value -> deref value)\nlet failed = match Domain.run (\\() -> work held) with\n    | Result.Ok n -> n\n    | Result.Error _ -> 1\ntzroutine { () }\nRoutine.yield ()\nfailed\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(!ir.contains("tz.mutex.locked"), "{ir}");
    assert!(ir.contains("call i32 @tsuzuri_tz_held_depth()"), "{ir}");
    assert!(
        ir.contains("call void @tsuzuri_tz_held_restore(i32"),
        "{ir}"
    );
}

#[test]
fn a_failure_domain_restores_the_spawn_scope_around_parallel_work() {
    // Natively parallel work runs on the scheduler (TZ12), so a tzroutine that waits for it makes no
    // blocking call; a trap in a chunk inside a failure domain skips the leaves of what the chunk
    // entered, and the domain restores the spawn scope that it was entered with.
    let module = accepts(
        "def joined :: i64 -> i64\nfn joined n =\n    let results = Parallel.init 2 (\\index -> index + n)\n    results[0]\n\ntzroutine { () }\nlet failed = match Domain.run (\\() -> joined 1) with\n    | Result.Ok value -> value\n    | Result.Error _ -> 0\nfailed\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(
        ir.contains("call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk."),
        "{ir}"
    );
    assert!(
        !ir.contains("@tsuzuri_tz_block_enter") && !ir.contains("tsuzuri_task_parallel"),
        "{ir}"
    );
    let run = ir.find("@tsuzuri_domain_run(ptr").expect("a domain");
    let before = &ir[..run];
    assert!(
        before[before.rfind("define ").unwrap()..].contains("call ptr @tsuzuri_tz_scope_current()")
    );
    assert!(ir[run..].contains("call void @tsuzuri_tz_scope_reset(ptr"));
}

/// The IR of `tsuzuri build --emit llvm` of a one-file project with `args`.
fn emitted_ir(name: &str, source: &str, args: &[&str]) -> String {
    let directory = std::env::temp_dir().join(format!(
        "tsuzuri-tzroutine-ir-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("Main.tz"), source).unwrap();
    let output = directory.join("out.ll");
    let result = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("build")
        .arg(&directory)
        .args(["--emit", "llvm", "-o"])
        .arg(&output)
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{args:?}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let ir = std::fs::read_to_string(&output).unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    ir
}

const JSPI: [&str; 4] = ["--target", "wasm32", "--wasm-feature", "jspi"];

#[test]
fn jspi_modules_that_spawn_get_the_stack_switch() {
    // TZ09: a wasm32 module of --wasm-feature jspi that spawns has the stack switch, the per-G stack
    // checks, the allocator of the runtime, the scheduler's DataChannel and Time.sleep.
    let source = "export def f :: i64 -> i64\nfn f n =\n    match DataChannel.bounded 1 with\n    | (sender, receiver) ->\n        tzroutine {\n            Time.sleep Time.millisecond()\n            let _sent = DataChannel.send (ref sender) n\n        }\n        match DataChannel.recv (ref receiver) with\n        | Maybe.Some value -> value\n        | Maybe.None -> 0\n";
    let ir = emitted_ir("spawn", source, &JSPI);
    assert_eq!(
        ir,
        emitted_ir("spawn-again", source, &JSPI),
        "deterministic"
    );
    for line in [
        "define void @tsuzuri_tz_entry(i32 %g) nounwind {",
        "define void @tsuzuri_tz_fire() nounwind {",
        "define hidden void @tzr_jspi_suspend(ptr %g) noinline nounwind {",
        "declare void @tz.jspi.import.suspend(ptr) \"wasm-import-module\"=\"tsuzuri_tz\" \"wasm-import-name\"=\"suspend\"",
        "@__stack_pointer = external addrspace(1) global ptr",
        "@tsuzuri_stack_top = addrspace(1) global i32 0",
        "declare i32 @tsuzuri_channel_send(ptr, ptr)",
        "declare i32 @tsuzuri_tz_sleep(i64)",
    ] {
        assert_eq!(
            ir.lines().filter(|text| *text == line).count(),
            1,
            "{line}\n{ir}"
        );
    }
    assert!(ir.contains("define weak ptr @tsuzuri_alloc(i64 %size)"));
    assert!(ir.contains("call void @tz.stack.check()"));
    for absent in [
        "define internal i32 @tsuzuri_channel_send(",
        "@tsuzuri_mutex_wait_ok",
        "@tsuzuri_os_sleep",
        // the stack switch writes the bounds, so they are not invariant (Copilot on #90)
        "!invariant.load",
    ] {
        assert!(!ir.contains(absent), "{absent}\n{ir}");
    }
    // A module that spawns nothing keeps the IR that it has without jspi: no stack switch, no import.
    let single = "export def g :: i64 -> i64\nfn g n =\n    let values: Channel<i64> = Channel.make 1\n    Channel.send values n\n    match Channel.recv values with\n    | Maybe.Some value -> value\n    | Maybe.None -> 0\n";
    let with_jspi = emitted_ir("single-jspi", single, &JSPI);
    assert_eq!(
        with_jspi,
        emitted_ir("single", single, &["--target", "wasm32"])
    );
    for absent in [
        "tsuzuri_tz_entry",
        "tz.jspi",
        "tsuzuri_stack_top",
        "wasm-import-module",
    ] {
        assert!(!with_jspi.contains(absent), "{absent}\n{with_jspi}");
    }
}

#[test]
fn jspi_net_imports_keep_the_stack_pointer_of_the_g() {
    // TZ09: under --wasm-feature net a socket call is an import that suspends the G alone: its
    // wrapper keeps the G's stack pointer and bounds across the call.
    let source = "export def f :: i64 -> i64\nfn f port =\n    tzroutine { () }\n    let address = Maybe.get (Net.parse_ip port (ref \"127.0.0.1\"))\n    let! connected = Net.connect address Maybe.None\n    if Result.is_ok (ref connected) then 1 else 0\n";
    let ir = emitted_ir(
        "net",
        source,
        &[
            "--target",
            "wasm32",
            "--wasm-feature",
            "jspi",
            "--wasm-feature",
            "net",
        ],
    );
    assert!(
        ir.contains("declare i64 @tz.jspi.net.open(ptr, i32, i64, i64, i64, i64) \"wasm-import-module\"=\"tsuzuri_net\" \"wasm-import-name\"=\"open\""),
        "{ir}"
    );
    let wrapper = &ir[ir
        .find("define hidden i64 @tsuzuri_net_open(")
        .expect("the wrapper of open")..];
    let wrapper = &wrapper[..wrapper.find("\n}\n").unwrap()];
    assert!(wrapper.contains("noinline"), "{wrapper}");
    let order = [
        "load volatile ptr, ptr addrspace(1) @__stack_pointer",
        "call void @tz.jspi.leave(ptr %sp)",
        "call i64 @tz.jspi.net.open(",
        "store volatile ptr %sp, ptr addrspace(1) @__stack_pointer",
    ];
    let mut at = 0;
    for step in order {
        at += wrapper[at..]
            .find(step)
            .unwrap_or_else(|| panic!("{step}\n{wrapper}"));
    }
    assert!(!ir.contains("declare i64 @tsuzuri_net_open("), "{ir}");
}

#[test]
fn the_webassembly_test_runner_refuses_tzroutines() {
    // TZ09 D6: the runner of `tsuzuri test --target wasm32` does not drive JSPI.
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-tzroutine-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("Main.tz"),
        "test \"spawns\" =\n    tzroutine { () }\n    assert true\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("test")
        .arg(&directory)
        .args(["--target", "wasm32", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rfind(|line| line.starts_with('{'))
        .unwrap_or_else(|| panic!("no JSON diagnostic in\n{stderr}"));
    let error: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(error["code"], "E2000", "{error}");
    assert_eq!(error["kind"], "option_conflict", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("the WebAssembly test runner cannot run tzroutines"),
        "{error}"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn jspi_bindings_of_a_module_that_spawns_run_its_tzroutines() {
    // TZ09: the table of the glue says so, and only for a module that spawns.
    let spawning = accepts("export def f :: i64 -> i64\nfn f n =\n    tzroutine { () }\n    n\n");
    let glue = tsuzuri::bindings::javascript_for(&spawning, tsuzuri::bindings::JsFlavor::Jspi);
    assert!(glue.contains("\"tzroutine\":true"), "{glue}");
    let single = tsuzuri::bindings::javascript_for(&spawning, tsuzuri::bindings::JsFlavor::Single);
    assert!(!single.contains("\"tzroutine\""));
    let plain = accepts("export def f :: i64 -> i64\nfn f n = n + 1\n");
    let glue = tsuzuri::bindings::javascript_for(&plain, tsuzuri::bindings::JsFlavor::Jspi);
    assert!(!glue.contains("\"tzroutine\""));
}
