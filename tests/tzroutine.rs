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
            ir.contains("call void @tsuzuri_tz_go(ptr @tz.tzroutine.start.$task."),
            "{ir}"
        );
        assert!(
            ir.contains("define internal void @tz.tzroutine.start.$task."),
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
        for define in defines {
            assert!(
                define.contains(" \"probe-stack\"=\"inline-asm\" "),
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
fn pool_workers_wait_for_their_tzroutines() {
    // A child of Task.scope (and a task of Task.parallel) runs on a pool worker, outside every
    // boundary: the function that the worker runs waits for the tzroutines it spawned.
    let module = accepts(
        "let counter = Arc.new (Atomic.create 0i64)\nlet _seen = Task.scope (ref counter) 4 (\\shared index -> {\n    let mine = Arc.share shared;\n    tzroutine {\n        let _n = Atomic.fetch_add (Arc.get (ref mine)) 1i64\n    };\n    index\n})\n0\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    let call = ir
        .lines()
        .find(|line| line.contains("@tsuzuri_task_parallel(ptr @"))
        .unwrap_or_else(|| panic!("{ir}"));
    let callback: String = call[call.find("(ptr @").unwrap() + 6..]
        .chars()
        .take_while(|c| !matches!(c, ',' | ')' | ' '))
        .collect();
    assert_wrapped(definition(&ir, &callback));
}

#[test]
fn a_tzroutine_cannot_yield_inside_a_mutex() {
    // A program that spawns marks the thread inside Mutex.with_lock; Routine.yield traps there.
    let module = accepts(
        "let lock = Mutex.create 0i64\ntzroutine { () }\nMutex.with_lock (ref lock) (\\value -> {\n    Routine.yield ();\n    deref value\n})\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(
        ir.contains("@tz.mutex.locked = internal thread_local global i32 0"),
        "{ir}"
    );
    assert!(ir.contains("store i32 1, ptr @tz.mutex.locked"), "{ir}");
    assert!(ir.contains("call i1 @tz.routine.unlocked()"), "{ir}");
    // Without a spawn, nothing changes for Mutex.
    let plain = accepts(
        "let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\value -> deref value)\n",
    );
    assert!(
        !llvm::emit(&plain, llvm::Entry::Console)
            .unwrap()
            .contains("tz.mutex.locked")
    );
}

#[test]
fn programs_without_tzroutine_keep_their_ir() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for fixture in ["tasks", "concurrency", "channel", "control", "async"] {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(root.join(fixture)).unwrap() {
            let path = entry.unwrap().path();
            if matches!(path.extension().and_then(|e| e.to_str()), Some("tz" | "tt")) {
                let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                files.push((name, std::fs::read_to_string(&path).unwrap()));
            }
        }
        files.sort();
        let sources: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect();
        let module =
            analyze_modules(&sources).unwrap_or_else(|e| panic!("{fixture}: {}", e.message));
        assert!(!llvm::spawns(&module), "{fixture}");
        for entry in [
            llvm::Entry::Library,
            llvm::Entry::Console,
            llvm::Entry::TestRunner,
        ] {
            if entry == llvm::Entry::Console && module.entry.is_none() {
                continue;
            }
            let ir = llvm::emit(&module, entry).unwrap();
            assert!(!ir.contains("tsuzuri_tz_"), "{fixture}: {ir}");
            assert!(!ir.contains("%tz.scope"), "{fixture}");
        }
    }
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
    for (args, kind) in [
        (
            vec!["--target", "wasm32", "-o", output],
            "option_requires_option",
        ),
        (
            vec!["--target", "wasm32", "--wasm-feature", "jspi", "-o", output],
            "option_conflict",
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
