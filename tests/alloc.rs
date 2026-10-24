//! Allocation counters, opt-in reachability, and the counting CLI (F14).
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry};

const HEAP: &str = include_str!("../src/runtime/heap-counting.ll");
const EXPORT: &str =
    "export def total_allocs :: i64\nfn total_allocs = (Alloc.snapshot ()).allocs\n";
const PROGRAM: &str = "let xs = new [i64](10, \\i -> i)\nlet start = Alloc.snapshot ()\nlet ys = Array.map (\\x -> x + 1) (ref xs)\nlet used = Alloc.since start\nused.bytes + Array.length (ref ys)\n";
const EVENTS: &str = r#"
test "map" =
    let xs = new [i64](10, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.map (\x -> x + 1) (ref xs)
    let used = Alloc.since start
    assert (used.allocs == 1 && used.reallocs == 0 && used.frees == 0 && used.bytes == 80)
    assert (ys[9] == 10)
test "last use" =
    let xs = new [i64](4, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.update_at 0 5 xs
    let used = Alloc.since start
    assert (used.allocs == 0 && used.bytes == 0 && ys[0] == 5)
test "captured owner" =
    let xs = new [i64](4, \i -> i)
    match Alloc.measure (\() -> Array.update_at 0 5 xs) with
    | (used, ys) -> assert (used.allocs == 1 && used.bytes == 32 && ys[0] == 5)
"#;

fn options(wasm: bool, allocator: Allocator) -> EmitOptions {
    EmitOptions {
        entry: Entry::Library,
        wasm,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator,
        f16: llvm::F16Path::Soft,
    }
}

fn project(name: &str, source: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch")
        .join(format!("f14-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Main.tz"), source).unwrap();
    root
}

fn cli(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .env("TSUZURI_CACHE_DIR", root.join("cache"))
        .output()
        .unwrap()
}

#[test]
fn alloc_is_an_opt_in_reserved_std_module() {
    assert!(tsuzuri::stdlib::is_reserved_module("Alloc"));
    for (source, expected) in [("42", false), ("Alloc.snapshot ()", true)] {
        assert_eq!(
            tsuzuri::stdlib::sources_for([source])
                .iter()
                .any(|(name, _)| *name == "std/Alloc.tz"),
            expected
        );
    }
}

#[test]
fn alloc_counter_is_private_to_the_std_module() {
    let error = tsuzuri::analyze("let n = Alloc.__counter 0\nn").unwrap_err();
    assert_eq!(error.code, "E1022");
    assert!(
        error
            .message
            .contains("private to the standard Alloc module")
    );
}

#[test]
fn counting_heap_keeps_the_stats_abi_and_adds_two_counters() {
    let module = tsuzuri::analyze("export def size :: i64\nfn size = 42").unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_with_options(&module, options(wasm, Allocator::Counting)).unwrap();
        for text in [
            "@tz.heap.counts = internal global [6 x i64]",
            "define internal i64 @tz.heap.counter(i64 %index)",
            "define void @tsuzuri_alloc_stats(ptr %stats)",
            "getelementptr inbounds i8, ptr %stats, i64 24",
        ] {
            assert_eq!(ir.matches(text).count(), 1, "{text}");
        }
        assert!(ir.contains(HEAP));
    }
}

#[test]
fn test_runner_links_the_counting_heap_only_when_a_selected_test_reaches_alloc() {
    let source =
        "test \"counts\" = assert ((Alloc.snapshot ()).allocs >= 0)\ntest \"plain\" = assert true";
    let module = tsuzuri::analyze(source).unwrap();
    let plain =
        tsuzuri::analyze("test \"counts\" = assert true\ntest \"plain\" = assert true").unwrap();
    for wasm in [false, true] {
        let counted = llvm::emit_test_runner(&module, &[0], wasm).unwrap();
        assert!(counted.contains(HEAP));
        assert!(counted.contains("call i64 @tz.heap.counter("));
        let uncounted = llvm::emit_test_runner(&module, &[1], wasm).unwrap();
        assert!(!uncounted.contains("tz.heap.counter"));
        assert_eq!(
            uncounted,
            llvm::emit_test_runner(&plain, &[1], wasm).unwrap()
        );
    }
}

#[test]
fn alloc_outside_tests_and_counting_builds_is_rejected() {
    let module = tsuzuri::analyze(EXPORT).unwrap();
    for wasm in [false, true] {
        let error = llvm::emit_target(&module, Entry::Library, wasm).unwrap_err();
        assert_eq!(error.code, "E1018");
        assert!(
            error
                .message
                .contains("Alloc reads the counters of the counting allocator")
        );
        let ir = llvm::emit_with_options(&module, options(wasm, Allocator::Counting)).unwrap();
        assert_eq!(
            ir,
            llvm::emit_with_options(&module, options(wasm, Allocator::Counting)).unwrap()
        );
    }
    let top = tsuzuri::analyze("let s = Alloc.snapshot ()\ns.allocs").unwrap();
    assert_eq!(llvm::emit(&top, Entry::Console).unwrap_err().code, "E1018");
    let bench =
        tsuzuri::analyze("bench \"counters\" = \\n -> (Alloc.snapshot ()).allocs + n").unwrap();
    let error = llvm::emit_bench_runner(&bench, &[0]).unwrap_err();
    assert_eq!(error.code, "E1018");
    assert!(error.message.contains("tsuzuri bench --allocs"));
}

#[test]
fn public_alloc_helpers_stay_out_of_other_builds() {
    let source = "def count_events :: unit -> i64\nfn count_events _unit = (Alloc.snapshot ()).allocs\ntest \"count\" = assert (count_events () >= 0)\nDebug.print \"hi\"";
    let module = tsuzuri::analyze(source).unwrap();
    let private = tsuzuri::analyze(&source.replace("def ", "private def ")).unwrap();
    for wasm in [false, true] {
        for entry in [Entry::Console, Entry::Library] {
            let ir = llvm::emit_target(&module, entry, wasm).unwrap();
            assert!(!ir.contains("tz.heap.counter"));
            assert_eq!(ir, llvm::emit_target(&private, entry, wasm).unwrap());
        }
    }
}

#[test]
fn alloc_reached_from_entry_export_or_drop_is_rejected_by_name() {
    for (source, entry, message, expression) in [
        (
            "def report :: unit -> i64\nfn report _unit =\n    match Alloc.measure (\\() -> 1) with\n    | (used, _) -> used.allocs\nreport ()",
            Entry::Console,
            "the top-level code reaches it through `Main.report`",
            "Alloc.measure",
        ),
        (
            EXPORT,
            Entry::Library,
            "export `Main.total_allocs` reads it",
            "Alloc.snapshot",
        ),
        (
            "record Resource { id: i64 }\ninstance Drop<Resource> { fn drop value = ignore ((Alloc.snapshot ()).allocs + value.id) }\nDebug.print \"hi\"",
            Entry::Console,
            "the Drop instance of `Main.Resource` reads it",
            "Alloc.snapshot",
        ),
    ] {
        let module = tsuzuri::analyze(source).unwrap();
        let error = llvm::emit(&module, entry).unwrap_err();
        assert_eq!(error.code, "E1018");
        assert!(error.message.contains(message), "{}", error.message);
        assert_eq!(&source[error.span.start..error.span.end], expression);
    }
}

#[test]
fn alloc_with_host_or_small_allocator_is_rejected() {
    let module = tsuzuri::analyze(EXPORT).unwrap();
    for allocator in [Allocator::Host, Allocator::Small] {
        let error = llvm::emit_with_options(&module, options(false, allocator)).unwrap_err();
        assert_eq!(error.code, "E2000");
        assert!(error.message.contains("Alloc needs the counting allocator"));
    }
}

#[test]
fn cli_test_counts_events_on_native_and_wasm32() {
    let root = project("events", EVENTS);
    for target in ["native", "wasm32"] {
        for opt in ["-O0", "-O3"] {
            let output = cli(
                &root,
                &["test", root.to_str().unwrap(), "--target", target, opt],
            );
            assert!(output.status.success(), "{target} {opt}: {output:?}");
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("3 passed; 0 failed; 0 ignored")
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_run_and_build_executable_with_counting() {
    let root = project("executable", PROGRAM);
    for opt in ["-O0", "-O3"] {
        let output = cli(
            &root,
            &[
                "run",
                root.to_str().unwrap(),
                "--allocator",
                "counting",
                opt,
            ],
        );
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "90");
        let exe = root.join(if cfg!(windows) {
            "program.exe"
        } else {
            "program"
        });
        let output = cli(
            &root,
            &[
                "build",
                root.to_str().unwrap(),
                "--allocator",
                "counting",
                opt,
                "-o",
                exe.to_str().unwrap(),
            ],
        );
        assert!(output.status.success(), "{output:?}");
        let output = Command::new(exe).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "90");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_build_without_counting_rejects_alloc() {
    let root = project("rejected", PROGRAM);
    for action in ["build", "run"] {
        let output = cli(&root, &[action, root.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("E1018") && stderr.contains("--allocator counting"),
            "{stderr}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_test_with_allocator_counting_runs_programs_that_do_not_use_alloc() {
    let root = project(
        "explicit",
        "test \"one\" = assert true\ntest \"two\" = assert (2 + 2 == 4)",
    );
    let output = cli(
        &root,
        &["test", root.to_str().unwrap(), "--allocator", "counting"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("2 passed"));
    let output = cli(
        &root,
        &["test", root.to_str().unwrap(), "--allocator", "host"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--allocator is only valid with build")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn automatic_counting_also_works_in_coverage_and_debug_test_runners() {
    let root = project("coverage-debug", EVENTS);
    for optimization in ["-O0", "-O3"] {
        let report = root.join("coverage.info");
        let output = cli(
            &root,
            &[
                "test",
                root.to_str().unwrap(),
                "--coverage",
                report.to_str().unwrap(),
                optimization,
            ],
        );
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("3 passed"));
        assert!(report.is_file());

        let executable = root.join(if cfg!(windows) { "debug.exe" } else { "debug" });
        let output = cli(
            &root,
            &[
                "test",
                root.to_str().unwrap(),
                "--index",
                "0",
                "-g",
                "-o",
                executable.to_str().unwrap(),
                optimization,
            ],
        );
        assert!(output.status.success(), "{output:?}");
        let output = Command::new(executable).arg("0").output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    fs::remove_dir_all(root).unwrap();
}
