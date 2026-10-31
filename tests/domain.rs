//! B10 Phase 2: `Domain.run` and the rules of its calls (`E1038`). The expected codes and words come from the
//! ticket's table of rules (2.2); the IR checks look for the runtime calls that the design names.
use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry, F16Path};
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::trap::TrapSource;

fn accepts(source: &str) {
    if let Err(error) = tsuzuri::analyze(source) {
        panic!("{source}\n{}: {}", error.code, error.message);
    }
}

fn rejects(source: &str, kind: &str, words: &str) -> String {
    let error = tsuzuri::analyze(source).expect_err(source);
    assert_eq!(error.code, "E1038", "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
    assert!(error.message.contains(words), "{source}\n{}", error.message);
    error.message.clone()
}

#[test]
fn domain_accepts_lambdas_pure_defs_and_plain_captures() {
    accepts("let r = Domain.run (\\() -> 1)\n");
    accepts("let n = 7\nlet r = Domain.run (\\() -> 100 / n)\n");
    accepts("pure def risky :: unit -> i64 = \\() -> 1 / 0\nlet r = Domain.run risky\n");
    // A shared reference to plain data, and a call of a def whose body is pure.
    accepts(
        "def total :: [i64] -> i64 = \\xs -> Array.sum (ref xs)\nlet data = [1, 2, 3]\nlet view = ref data\nlet r = Domain.run (\\() -> view.length + total data)\n",
    );
    // Records, strings and unions of plain data cross both ways.
    accepts(
        "record Point { x: i64, y: i64 }\nlet name = \"p\"\nlet r = Domain.run (\\() -> (Point { x: name.length, y: 2 }, name + \"!\"))\n",
    );
    // A pipe applies `Domain.run` to its one argument as a call does (review of PR #74).
    accepts("let r = (\\() -> 1) |> Domain.run\n");
    accepts("let n = 4\nlet r = (\\() -> 100 / n) |> Domain.run\n");
    // A program that does not name Domain does not load it.
    accepts("let r = 1\n");
}

#[test]
fn domain_rejects_each_rule_with_e1038() {
    // Rule 0: Domain.run as a value.
    rejects(
        "let run = Domain.run\nlet r = run (\\() -> 1)\n",
        "domain_not_called_directly",
        "must be called directly",
    );
    // Rule 1: a variable that holds a lambda.
    rejects(
        "let work = \\() -> 1 / 0\nlet a = Domain.run work\n",
        "domain_argument_not_closure",
        "must be a lambda or the name of a pure def",
    );
    // Rule 1: a def that is not pure.
    rejects(
        "def risky :: unit -> i64 = \\() -> 1 / 0\nlet a = Domain.run risky\n",
        "domain_argument_not_closure",
        "must be a lambda or the name of a pure def",
    );
    // Rule 2: a captured function value.
    let message = rejects(
        "let log = \\(text: string) -> Debug.print text\nlet c = Domain.run (\\() -> { log \"x\"; 1 })\n",
        "domain_capture_not_plain",
        "captures 'log', which holds a function, 'dyn', or a task",
    );
    assert!(message.ends_with("capture plain data or compute the value inside the closure"));
    // Rule 3: debug output, directly and through a function.
    rejects(
        "let b = Domain.run (\\() -> { Debug.print \"x\"; 1 })\n",
        "domain_body_not_pure",
        "must be pure: it prints debug output",
    );
    let message = rejects(
        "def noisy :: i64 -> i64 = \\n -> { Debug.print \"n\"; n }\nlet b = Domain.run (\\() -> noisy 2)\n",
        "domain_body_not_pure",
        "must be pure: it prints debug output (through ",
    );
    assert!(
        message.ends_with("move that work out of the closure"),
        "{message}"
    );
    // Rule 1: an instance of a generic def that is not pure, whatever its type arguments (review of PR #74).
    rejects(
        "def empty :: unit -> ['a] = \\() -> []\nlet r: Result<[i64], Domain.Failure> = Domain.run empty\n",
        "domain_argument_not_closure",
        "must be a lambda or the name of a pure def",
    );
    rejects(
        "def empty :: unit -> [i64] = \\() -> []\nlet r = Domain.run empty\n",
        "domain_argument_not_closure",
        "must be a lambda or the name of a pure def",
    );
    // The rules see a pipe's argument as a call's.
    rejects(
        "let b = (\\() -> { Debug.print \"x\"; 1 }) |> Domain.run\n",
        "domain_body_not_pure",
        "must be pure: it prints debug output",
    );
    rejects(
        "let log = \\(text: string) -> Debug.print text\nlet c = (\\() -> { log \"x\"; 1 }) |> Domain.run\n",
        "domain_capture_not_plain",
        "captures 'log'",
    );
    // Rule 3: a domain inside a domain (D8).
    rejects(
        "let b = Domain.run (\\() -> Domain.run (\\() -> 1))\n",
        "domain_body_not_pure",
        "runs a failure domain ('Domain.run')",
    );
    // Rule 4: a result that is not plain data.
    rejects(
        "let f = Domain.run (\\() -> \\(x: i64) -> x + 1)\n",
        "domain_result_not_plain",
        "the result of 'Domain.run' must be plain data; '",
    );
    // The primitives stay private to the module.
    let error = tsuzuri::analyze("let r = Domain.__message 1\n").expect_err("private");
    assert_eq!(error.code, "E1022", "{}", error.message);
}

fn options(entry: Entry) -> EmitOptions {
    EmitOptions {
        entry,
        wasm: false,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: Allocator::System,
        f16: F16Path::Soft,
    }
}

const PROGRAM: &str = "def main :: unit -> i32 = \\() ->\n    let n = 0\n    match Domain.run (\\() -> 10 / n) with\n    | Result.Ok value -> value as i32\n    | Result.Error failure -> failure.message.length as i32\n";

/// The program's source and the std sources, which the trap sites of `Domain.run` point into.
fn sources(text: &'static str) -> Vec<TrapSource<'static>> {
    std::iter::once(TrapSource {
        path: "Main.tz",
        text,
    })
    .chain(
        tsuzuri::stdlib::sources_for(std::iter::once(text))
            .into_iter()
            .map(|(path, text)| TrapSource { path, text }),
    )
    .collect()
}

#[test]
fn domain_ir_is_deterministic_and_marks_the_runtime() {
    // Failure domains are POSIX's: the IR is the one for x86-64 Linux whatever the host is.
    let _scope = PlatformScope::enter(Platform::new(Triple::parse("x86_64-unknown-linux-gnu")));
    let module = tsuzuri::analyze(PROGRAM).unwrap();
    let sources = sources(PROGRAM);
    let first = llvm::emit_native_build(&module, options(Entry::Console), &sources, None, false)
        .unwrap()
        .ir;
    let second = llvm::emit_native_build(&module, options(Entry::Console), &sources, None, false)
        .unwrap()
        .ir;
    assert_eq!(first, second);
    assert_eq!(first.matches("call i32 @tsuzuri_domain_run(").count(), 1);
    assert_eq!(
        first
            .matches("declare i32 @tsuzuri_domain_run(ptr, ptr, ptr)")
            .count(),
        1
    );
    // The heap is the tracked one, the report raises a trap to the domain, and the sites have texts.
    assert!(first.contains("@tsuzuri_tracked_malloc"));
    assert!(first.contains("call void @tsuzuri_trap_raise("));
    assert!(first.contains("define internal ptr @tz.trap.text(i32 %site, ptr %length)"));
    // The texts are byte strings of `\XX` escapes.
    let text: String = "integer division by zero at Main.tz:3:30"
        .bytes()
        .map(|byte| format!("\\{byte:02X}"))
        .collect();
    assert!(
        first.contains(&format!("c\"{text}\"")),
        "the text of the site"
    );
    // The thunk that the runtime calls takes no trap context.
    let thunk = first
        .lines()
        .find(|line| line.starts_with("define internal void @tz.domain.thunk."))
        .unwrap();
    assert!(thunk.ends_with("(ptr %context) nounwind {"), "{thunk}");
    // A program without Domain keeps the ordinary heap and has no domain runtime.
    let plain = tsuzuri::analyze("def main :: unit -> i32 = \\() -> 10 / 2\n").unwrap();
    let ir = llvm::emit_native_build(&plain, options(Entry::Console), &sources, None, false)
        .unwrap()
        .ir;
    for marker in [
        "tsuzuri_domain_run",
        "tsuzuri_tracked_",
        "tsuzuri_trap_raise",
        "@tz.trap.text",
    ] {
        assert!(!ir.contains(marker), "{marker}");
    }
}

#[cfg(unix)]
fn tsuzuri(arguments: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn project(name: &str, source: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-domain-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    root
}

/// The target rules; a native Windows build is rejected before them, so they are checked on POSIX hosts.
#[cfg(unix)]
#[test]
fn domain_target_errors() {
    let root = project("targets", PROGRAM);
    let main = root.to_str().unwrap();
    let output = |name: &str| root.join(name).to_string_lossy().into_owned();
    for (arguments, why) in [
        (
            vec!["--target", "wasm32"],
            "WebAssembly cannot return from a trap into the module",
        ),
        (
            vec!["--emit", "object", "--freestanding", "--allocator", "host"],
            "--freestanding has no trap runtime",
        ),
        (
            vec!["--allocator", "counting"],
            "the allocator is not the system allocator",
        ),
        (
            vec!["--emit", "llvm"],
            "--emit llvm needs --trap-mode return",
        ),
    ] {
        let mut command = vec!["build", main, "--no-cache", "--json", "-o"];
        let path = output("out");
        command.push(&path);
        command.extend(arguments.iter().copied());
        let result = tsuzuri(&command);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert_eq!(result.status.code(), Some(1), "{arguments:?}\n{stderr}");
        assert!(
            stderr.contains("\"code\":\"E1038\""),
            "{arguments:?}\n{stderr}"
        );
        assert!(
            stderr.contains("\"kind\":\"domain_target_unsupported\""),
            "{arguments:?}\n{stderr}"
        );
        assert!(stderr.contains(why), "{arguments:?}\n{stderr}");
        assert!(stderr.contains("\"line\":3"), "the first call: {stderr}");
    }
    // A host that links the LLVM IR provides src/runtime/trap.c itself, so `--trap-mode return` allows it.
    let ir = output("program.ll");
    let built = tsuzuri(&[
        "build",
        main,
        "--no-cache",
        "--emit",
        "llvm",
        "--trap-mode",
        "return",
        "-o",
        &ir,
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        std::fs::read_to_string(&ir)
            .unwrap()
            .contains("call i32 @tsuzuri_domain_run(")
    );
    // `check` does not look at the target.
    let checked = tsuzuri(&["check", main]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// A program with failure domains has the trap path of `--trap-mode return`, which a callback that the host calls cannot
/// hold (review of PR #74): the builds and the runners reject extern callbacks as `--trap-mode return` does.
#[test]
fn domain_rejects_extern_callbacks_in_builds_and_runners() {
    let _scope = PlatformScope::enter(Platform::new(Triple::parse("x86_64-unknown-linux-gnu")));
    const CALLBACKS: &str = "extern \"host_apply\" def apply :: (i64 -> i64) -> i64 -> i64\n\ndef triple :: i64 -> i64 = \\x -> x * 3\n\ndef main :: unit -> i32 = \\() ->\n    let n = 2\n    let r = Domain.run (\\() -> 10 / n)\n    let k = match r with\n        | Result.Ok v -> v\n        | Result.Error _ -> 0\n    (apply triple k) as i32\n\ntest \"calls back\" =\n    Test.is_true (main () == 15)\n";
    let module = tsuzuri::analyze(CALLBACKS).unwrap();
    let sources = sources(CALLBACKS);
    let check = |result: Result<String, tsuzuri::diagnostic::Diagnostic>, what: &str| {
        let error = result.expect_err(what);
        assert_eq!(error.code, "E2000", "{what}: {}", error.message);
        assert_eq!(error.kind, Some("option_conflict"), "{what}");
        assert!(
            error.message
                == "Domain.run cannot be combined with extern callbacks: a trap would unwind through the host's frames",
            "{what}: {}",
            error.message
        );
        // At the call of Domain.run (line 7).
        let line = CALLBACKS[..error.span.start].matches('\n').count() + 1;
        assert_eq!(line, 7, "{what}");
    };
    for entry in [Entry::Console, Entry::Library] {
        for trap_info in [false, true] {
            check(
                llvm::emit_native_build(&module, options(entry), &sources, None, trap_info)
                    .map(|output| output.ir),
                &format!("{entry:?} trap info {trap_info}"),
            );
        }
    }
    check(
        llvm::emit_test_runner_with(&module, &[0], llvm::TestRunnerOptions::default()),
        "test runner",
    );
    // The same callbacks without Domain build as before.
    let plain = CALLBACKS.replace(
        "let r = Domain.run (\\() -> 10 / n)",
        "let r: Result<i64, i64> = Result.Ok (10 / n)",
    );
    let module = tsuzuri::analyze(&plain).unwrap();
    let ir = llvm::emit_native_build(&module, options(Entry::Console), &sources, None, false)
        .unwrap()
        .ir;
    assert!(ir.contains("@tz.callback."));
}

/// A failure domain frees a trapped closure's blocks past the counting heap's counters, so a runner whose tests read
/// them (`Alloc`, which selects the counting heap by itself) cannot hold one (review of PR #74).
#[test]
fn domain_rejects_the_counting_heap_that_alloc_selects() {
    let _scope = PlatformScope::enter(Platform::new(Triple::parse("x86_64-unknown-linux-gnu")));
    const COUNTED: &str = "test \"counts and contains\" =\n    let start = Alloc.snapshot ()\n    let numbers = [1, 2, 3]\n    let outcome = Domain.run (\\() -> numbers[5])\n    let used = Alloc.since start\n    match outcome with\n    | Result.Error failure -> Test.is_true (failure.kind == 3 && used.frees >= 0)\n    | Result.Ok _ -> Test.is_true false\n";
    let module = tsuzuri::analyze(COUNTED).unwrap();
    let error = llvm::emit_test_runner_with(&module, &[0], llvm::TestRunnerOptions::default())
        .expect_err("a counted runner with a domain");
    assert_eq!(error.code, "E1038", "{}", error.message);
    assert_eq!(error.kind, Some("domain_target_unsupported"));
    assert!(
        error
            .message
            .contains("the allocator is not the system allocator"),
        "{}",
        error.message
    );
    // Without Domain the same test reads the counters as before.
    let plain = "test \"counts\" =\n    let start = Alloc.snapshot ()\n    let numbers = [1, 2, 3]\n    let used = Alloc.since start\n    Test.is_true (numbers[1] == 2 && used.frees >= 0)\n";
    let module = tsuzuri::analyze(plain).unwrap();
    assert!(llvm::emit_test_runner_with(&module, &[0], llvm::TestRunnerOptions::default()).is_ok());
}

#[test]
fn programs_without_domain_keep_their_ir() {
    let _scope = PlatformScope::enter(Platform::new(Triple::parse("x86_64-unknown-linux-gnu")));
    let source =
        "def main :: unit -> i32 = \\() ->\n    let numbers = [1, 2, 3]\n    numbers[1] as i32\n";
    let module = tsuzuri::analyze(source).unwrap();
    let sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    // The word in a comment loads nothing and changes nothing.
    let commented = format!("{source}// Domain\n");
    let with_comment = tsuzuri::analyze(&commented).unwrap();
    for (module, text) in [(&module, source), (&with_comment, commented.as_str())] {
        let sources = [TrapSource {
            path: "Main.tz",
            text,
        }];
        for trap_info in [false, true] {
            let ir =
                llvm::emit_native_build(module, options(Entry::Console), &sources, None, trap_info)
                    .unwrap()
                    .ir;
            for marker in [
                "tsuzuri_domain_run",
                "tsuzuri_tracked_",
                "tsuzuri_trap_raise",
                "@tz.trap.text",
                "Domain",
            ] {
                assert!(!ir.contains(marker), "{marker}");
            }
        }
    }
    let library = llvm::emit_native_build(&module, options(Entry::Library), &sources, None, false)
        .unwrap()
        .ir;
    assert!(!library.contains("tsuzuri_tracked_"));
}
