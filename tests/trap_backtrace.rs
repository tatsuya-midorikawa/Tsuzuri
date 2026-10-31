//! B10 Phase 1: the crash report of a native executable built with `--trap-info` (`src/runtime/backtrace.c`) and the
//! `trap` object that `tsuzuri run` adds to an `E2005`. The expected frames come from the call chain of the fixture,
//! which `nm` and a separate `llvm-symbolizer` run confirmed (ticket appendix A), not from the compiler's output.
use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry, F16Path};
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::trap::TrapSource;

const CHAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/trap_backtrace");
const DECLARATION: &str = "declare void @tsuzuri_trap_backtrace(i32)";

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

fn define_lines(ir: &str) -> Vec<&str> {
    ir.lines()
        .filter_map(|line| {
            line.strip_prefix('}')
                .filter(|rest| rest.starts_with("define "))
                .or(line.starts_with("define ").then_some(line))
        })
        .collect()
}

#[test]
fn backtrace_instrumentation_is_only_for_trap_info_executables() {
    // The crash report is POSIX's: the IR is the one for x86-64 Linux whatever the host is.
    let _scope = PlatformScope::enter(Platform::new(Triple::parse("x86_64-unknown-linux-gnu")));
    let source = std::fs::read_to_string(format!("{CHAIN}/Main.tz")).unwrap();
    let module = tsuzuri::analyze(&source).unwrap();
    let sources = [TrapSource {
        path: "Main.tz",
        text: &source,
    }];
    let executable =
        llvm::emit_native_build(&module, options(Entry::Console), &sources, None, true)
            .unwrap()
            .ir;
    let again = llvm::emit_native_build(&module, options(Entry::Console), &sources, None, true)
        .unwrap()
        .ir;
    assert_eq!(executable, again, "the IR is deterministic");
    let defines = define_lines(&executable);
    assert!(defines.len() > 4, "{defines:?}");
    for line in &defines {
        assert!(line.contains(" \"frame-pointer\"=\"all\""), "{line}");
    }
    assert_eq!(executable.matches(DECLARATION).count(), 1);
    assert!(executable.contains(
        "define internal void @tz.trap.report(i32 %site) cold nounwind noinline \"frame-pointer\"=\"all\" {"
    ));
    assert!(
        executable.contains(
            "done:\n  notail call void @tsuzuri_trap_backtrace(i32 %site)\n  ret void\n}"
        )
    );
    // With debug information the attribute goes before the location of the function.
    let debug = llvm::emit_native_build(
        &module,
        options(Entry::Console),
        &sources,
        Some(false),
        true,
    )
    .unwrap()
    .ir;
    for line in define_lines(&debug) {
        assert!(line.contains(" \"frame-pointer\"=\"all\""), "{line}");
        if let Some((before, _)) = line.split_once(" !dbg ") {
            assert!(before.ends_with(" \"frame-pointer\"=\"all\""), "{line}");
        }
    }
    // A library, a build without trap information, the trap-information IR of the other emitters and a trap
    // boundary have neither the attribute nor the report's call.
    let library = llvm::emit_native_build(&module, options(Entry::Library), &sources, None, true)
        .unwrap()
        .ir;
    let plain = llvm::emit_native_build(&module, options(Entry::Console), &sources, None, false)
        .unwrap()
        .ir;
    let traced = llvm::emit_with_trap_info(&module, options(Entry::Console), &sources)
        .unwrap()
        .ir;
    let boundary = llvm::emit_trap_return(&module, options(Entry::Library), &sources, None, true)
        .unwrap()
        .ir;
    // A Windows executable has no crash report.
    let windows = {
        let _windows = PlatformScope::enter(Platform::new(Triple::parse("x86_64-pc-windows-msvc")));
        llvm::emit_native_build(&module, options(Entry::Console), &sources, None, true)
            .unwrap()
            .ir
    };
    for (name, ir) in [
        ("windows", &windows),
        ("library", &library),
        ("plain", &plain),
        ("trap info", &traced),
        ("trap return", &boundary),
    ] {
        assert!(!ir.contains("frame-pointer"), "{name}");
        assert!(!ir.contains("tsuzuri_trap_backtrace"), "{name}");
    }
}

/// The crash report and the `trap` of `run`: the frames are walked on macOS and Linux only.
#[cfg(unix)]
mod native {
    use super::CHAIN;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::time::{SystemTime, UNIX_EPOCH};

    const DEEP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/trap_backtrace_deep"
    );

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-trap-backtrace-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn tsuzuri(arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(arguments)
            .output()
            .unwrap()
    }

    /// Builds `fixture` with `--trap-info` at `-O0` into `root`.
    fn build_chain(fixture: &str, root: &Path, name: &str) -> PathBuf {
        let executable = root.join(name);
        let built = tsuzuri(&[
            "build",
            fixture,
            "--trap-info",
            "-O0",
            "--no-cache",
            "-o",
            executable.to_str().unwrap(),
        ]);
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        executable
    }

    /// The fields of the one line of a report file.
    fn report_fields(report: &Path) -> Vec<String> {
        let text = fs::read_to_string(report).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(
            text.ends_with('\n') && text.starts_with("tz-trap/1 "),
            "{text}"
        );
        text.trim_end().split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn report_file_lists_the_frames_of_a_trapping_chain() {
        use std::os::unix::process::ExitStatusExt;
        let root = scratch("chain");
        let executable = build_chain(CHAIN, &root, "chain");
        let report = root.join("report");
        let run = Command::new(&executable)
            .env("TSUZURI_TRAP_REPORT", &report)
            .output()
            .unwrap();
        assert!(run.status.signal().is_some(), "{:?}", run.status);
        let fields = report_fields(&report);
        // level3, level2, level1, main and tsuzuri_main at least (S2).
        assert!(fields.len() - 4 >= 5, "{fields:?}");
        assert!(fields[1].parse::<u32>().unwrap() > 0, "{fields:?}");
        assert!(fields[2].starts_with("0x"), "{fields:?}");
        assert_eq!(fields[3], "0");
        assert!(fields[4..].iter().all(|address| address.starts_with("0x")));
        let parsed =
            tsuzuri::trap::parse_report_line(&fs::read_to_string(&report).unwrap()).unwrap();
        assert_eq!(parsed.addresses.len(), fields.len() - 4);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unset_report_variable_changes_nothing() {
        let root = scratch("unset");
        let executable = build_chain(CHAIN, &root, "chain");
        let report = root.join("report");
        let run = Command::new(&executable)
            .env_remove("TSUZURI_TRAP_REPORT")
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(!run.status.success());
        assert!(!report.exists());
        let entries = || {
            let mut names: Vec<String> = fs::read_dir(&root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        assert_eq!(
            entries(),
            ["chain", "chain.trap.json"],
            "only the build's files"
        );
        let stderr = String::from_utf8(run.stderr).unwrap();
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert!(
            stderr.starts_with("trap: integer division by zero at "),
            "{stderr}"
        );
        assert!(stderr.ends_with("Main.tz:2:15\n"), "{stderr}");
        // An empty variable is unset too.
        let empty = Command::new(&executable)
            .env("TSUZURI_TRAP_REPORT", "")
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(empty.stderr).unwrap(), stderr);
        assert_eq!(entries(), ["chain", "chain.trap.json"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_file_caps_a_deep_recursion_at_32_frames() {
        let root = scratch("deep");
        let executable = build_chain(DEEP, &root, "deep");
        let report = root.join("report");
        let run = Command::new(&executable)
            .env("TSUZURI_TRAP_REPORT", &report)
            .output()
            .unwrap();
        assert!(!run.status.success());
        let fields = report_fields(&report);
        assert_eq!(fields[3], "1", "{fields:?}");
        assert_eq!(fields.len() - 4, 32, "{fields:?}");
        // The same return address repeats: every frame but the first is `depth (n - 1) + 1`.
        assert!(
            fields[5..]
                .windows(2)
                .take(20)
                .all(|pair| pair[0] == pair[1])
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// The walk reads every frame record through a pipe, so a chain that runs into memory the process cannot read
    /// (unmapped or `PROT_NONE`, which `mincore` reports as mapped) ends there instead of faulting (review of PR #74).
    #[test]
    fn the_walk_stops_at_unreadable_records_without_a_fault() {
        let root = scratch("walk");
        let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
        for optimization in ["-O0", "-O2"] {
            let program = root.join(format!("backtrace-runtime{optimization}"));
            let built = Command::new(&clang)
                .args([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-fno-omit-frame-pointer",
                ])
                .arg(optimization)
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/backtrace_runtime.c"
                ))
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            let report = root.join(format!("report{optimization}"));
            let run = Command::new(&program).arg(&report).output().unwrap();
            assert!(
                run.status.success(),
                "{:?}\n{}",
                run.status,
                String::from_utf8_lossy(&run.stderr)
            );
            assert_eq!(
                String::from_utf8(run.stdout).unwrap(),
                "backtrace runtime: 20 checks passed\n"
            );
            let fields = report_fields(&report);
            assert_eq!(&fields[..2], ["tz-trap/1", "7"]);
            assert_eq!(fields[3], "0");
        }
        fs::remove_dir_all(root).unwrap();
    }

    fn symbolizer() -> bool {
        let tool =
            std::env::var_os("TSUZURI_LLVM_SYMBOLIZER").unwrap_or_else(|| "llvm-symbolizer".into());
        Command::new(tool)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    /// The one JSON line of a `run --json` that failed.
    fn run_json(arguments: &[&str], symbolizer: Option<&str>) -> serde_json::Value {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command.args(arguments);
        if let Some(tool) = symbolizer {
            command.env("TSUZURI_LLVM_SYMBOLIZER", tool);
        }
        let run = command.output().unwrap();
        assert_eq!(run.status.code(), Some(1));
        let stderr = String::from_utf8(run.stderr).unwrap();
        let line = stderr
            .lines()
            .find(|line| line.contains("\"code\":\"E2005\""))
            .unwrap_or_else(|| panic!("{stderr}"));
        serde_json::from_str(line).unwrap()
    }

    fn functions(trap: &serde_json::Value) -> Vec<String> {
        trap["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| frame["function"].as_str().unwrap_or("").to_owned())
            .collect()
    }

    #[test]
    fn run_reports_frames_with_a_symbolizer() {
        if !symbolizer() {
            eprintln!("skipped: no llvm-symbolizer");
            return;
        }
        let diagnostic = run_json(&["run", CHAIN, "-O0", "--no-cache", "--json"], None);
        let keys: Vec<&str> = diagnostic
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        for key in ["severity", "code", "message", "path", "span", "trap"] {
            assert!(keys.contains(&key), "{keys:?}");
        }
        assert_eq!(diagnostic["span"]["line"], 2);
        assert_eq!(diagnostic["span"]["column"], 15);
        let trap = &diagnostic["trap"];
        assert_eq!(trap["kind"], "integer division by zero");
        assert!(trap["site"].as_u64().unwrap() > 0);
        assert_eq!(
            functions(trap),
            ["Main.level3", "Main.level2", "Main.level1", "Main.main"]
        );
        for frame in trap["frames"].as_array().unwrap() {
            assert!(frame["address"].as_str().unwrap().starts_with("0x"));
            assert!(frame.get("path").is_none(), "{frame}");
        }
        assert!(trap.get("truncated").is_none());
        // With -g the callers get their lines; the innermost frame's line is the diagnostic's own span.
        let debug = run_json(&["run", CHAIN, "-O0", "-g", "--no-cache", "--json"], None);
        let trap = &debug["trap"];
        assert_eq!(
            functions(trap),
            ["Main.level3", "Main.level2", "Main.level1", "Main.main"]
        );
        let level2 = &trap["frames"][1];
        assert!(
            level2["path"].as_str().unwrap().ends_with("Main.tz"),
            "{level2}"
        );
        assert_eq!(level2["line"], 5);
        assert_eq!(level2["column"], 15);
        assert_eq!(trap["frames"][2]["line"], 8);
        assert_eq!(trap["frames"][3]["line"], 11);
        // The human form puts one line under the quote for each frame.
        let human = tsuzuri(&["run", CHAIN, "-O0", "--no-cache"]);
        let stderr = String::from_utf8(human.stderr).unwrap();
        assert!(
            stderr.contains(
                "    |               ^\n  at Main.level3\n  at Main.level2\n  at Main.level1\n  at Main.main\n"
            ),
            "{stderr}"
        );
        // At -O3 the four functions are inlined into the entry, which is not shown: no frame.
        let optimized = run_json(&["run", CHAIN, "-O3", "--no-cache", "--json"], None);
        assert_eq!(optimized["trap"]["kind"], "integer division by zero");
        assert_eq!(optimized["trap"]["frames"], serde_json::json!([]));
        assert_eq!(optimized["span"]["line"], 2);
    }

    #[test]
    fn run_reports_raw_frames_without_a_symbolizer() {
        let missing = "/nonexistent/llvm-symbolizer";
        let diagnostic = run_json(
            &["run", CHAIN, "-O0", "--no-cache", "--json"],
            Some(missing),
        );
        let trap = &diagnostic["trap"];
        assert_eq!(trap["kind"], "integer division by zero");
        let frames = trap["frames"].as_array().unwrap();
        assert!(frames.len() >= 5, "{trap}");
        for frame in frames {
            let keys: Vec<&String> = frame.as_object().unwrap().keys().collect();
            assert_eq!(keys, ["address"], "{frame}");
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        let human = command
            .args(["run", CHAIN, "-O0", "--no-cache"])
            .env("TSUZURI_LLVM_SYMBOLIZER", missing)
            .output()
            .unwrap();
        let stderr = String::from_utf8(human.stderr).unwrap();
        assert!(
            stderr.contains(
                "\n  note: frames are not resolved; install llvm-symbolizer or set TSUZURI_LLVM_SYMBOLIZER"
            ),
            "{stderr}"
        );
        assert!(!stderr.contains("  at "), "{stderr}");
    }
}
