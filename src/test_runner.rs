use super::*;
use std::process::Stdio;
use std::sync::{Mutex, atomic::AtomicUsize};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct TestOptions {
    pub target: Target,
    pub optimization: u8,
    pub filter: Option<String>,
}

impl Default for TestOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            optimization: 0,
            filter: None,
        }
    }
}

#[derive(Debug)]
pub struct TestResult {
    pub case: crate::check::CheckedTest,
    pub failure: Option<String>,
    pub duration_ms: u128,
}

#[derive(Debug)]
pub struct TestReport {
    pub results: Vec<TestResult>,
    pub ignored: usize,
    pub duration_ms: u128,
    pub messages: Vec<String>,
}

pub fn run_tests(module: &CheckedModule, options: &TestOptions) -> Result<TestReport, Diagnostic> {
    run_with_timeout(module, options, Duration::from_secs(30))
}

fn run_with_timeout(
    module: &CheckedModule,
    options: &TestOptions,
    timeout: Duration,
) -> Result<TestReport, Diagnostic> {
    if options.optimization > 3 {
        return Err(driver_error(
            "E2000",
            "test optimization must be between 0 and 3",
        ));
    }
    let started = Instant::now();
    if options.target == Target::Wasm32 {
        run_tool(
            Command::new("node").arg("--version"),
            "WASM tests require Node.js on PATH",
        )?;
    }
    let selected: Vec<_> = module
        .tests
        .iter()
        .filter(|test| {
            options
                .filter
                .as_ref()
                .is_none_or(|filter| format!("{}.{}", test.module, test.name).contains(filter))
        })
        .cloned()
        .collect();
    let ignored = module.tests.len() - selected.len();
    if selected.is_empty() {
        return Ok(TestReport {
            results: Vec::new(),
            ignored,
            duration_ms: started.elapsed().as_millis(),
            messages: Vec::new(),
        });
    }
    let temporary = TemporaryDirectory::new(&env::temp_dir())?;
    let mut messages = Vec::new();
    let runner = build_runner(
        module,
        &selected.iter().map(|test| test.index).collect::<Vec<_>>(),
        options,
        &temporary.path,
        &mut messages,
    )?;
    let cursor = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(selected.len()));
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(32)
        .min(selected.len());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(case) = selected.get(index) else {
                        break;
                    };
                    let started = Instant::now();
                    let result = execute_test(&runner, index, timeout).map(|failure| TestResult {
                        case: case.clone(),
                        failure,
                        duration_ms: started.elapsed().as_millis(),
                    });
                    results.lock().unwrap().push((case.index, result));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(index, _)| *index);
    let results = results
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Result<_, _>>()?;
    temporary.close()?;
    Ok(TestReport {
        results,
        ignored,
        duration_ms: started.elapsed().as_millis(),
        messages,
    })
}

struct Runner {
    program: OsString,
    arguments: Vec<OsString>,
}

fn execute_test(
    runner: &Runner,
    index: usize,
    timeout: Duration,
) -> Result<Option<String>, Diagnostic> {
    let mut child = Command::new(&runner.program)
        .args(&runner.arguments)
        .arg(index.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| driver_error("E2002", format!("cannot start test runner: {error}")))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(result)) => {
                return Ok((!result.success()).then(|| {
                    result.code().map_or_else(
                        || "trapped or terminated by signal".into(),
                        |code| format!("trapped or exited with code {code}"),
                    )
                }));
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(driver_error(
                    "E2002",
                    format!("cannot wait for test runner: {error}"),
                ));
            }
        }
        if started.elapsed() >= timeout {
            let stopped = child.kill();
            let collected = child.wait();
            collected.map_err(|error| {
                driver_error("E2002", format!("cannot collect timed-out test: {error}"))
            })?;
            stopped.map_err(|error| {
                driver_error("E2002", format!("cannot stop timed-out test: {error}"))
            })?;
            return Ok(Some(format!("timed out after {}ms", timeout.as_millis())));
        }
        std::thread::park_timeout(
            Duration::from_millis(5).min(timeout.saturating_sub(started.elapsed())),
        );
    }
}

fn build_runner(
    module: &CheckedModule,
    selected: &[usize],
    options: &TestOptions,
    directory: &Path,
    messages: &mut Vec<String>,
) -> Result<Runner, Diagnostic> {
    let wasm = options.target == Target::Wasm32;
    let mut text = llvm::emit_test_runner(module, selected, wasm)?;
    if wasm {
        text.push_str(include_str!("runtime/wasm.ll"));
    }
    let task_runtime = !wasm && text.contains("declare void @tsuzuri_task_parallel(");
    if task_runtime && !cfg!(unix) {
        return Err(driver_error(
            "E2002",
            "native parallel tasks require POSIX pthreads",
        ));
    }
    let ir = directory.join("tests.ll");
    let object = directory.join("tests.o");
    let artifact = directory.join(if wasm {
        "tests.wasm"
    } else if cfg!(windows) {
        "tests.exe"
    } else {
        "tests"
    });
    fs::write(&ir, text).map_err(|error| io_error("write test IR", &ir, error))?;
    let mut clang = Command::new(tool("TSUZURI_CLANG", "clang"));
    clang
        .args(["-x", "ir", "-Wno-override-module"])
        .arg(format!("-O{}", options.optimization))
        .arg(&ir);
    if wasm {
        clang
            .args(["--target=wasm32-unknown-unknown", "-mbulk-memory", "-c"])
            .arg("-o")
            .arg(&object);
    } else {
        let main = directory.join("main.c");
        fs::write(&main, include_str!("runtime/test-runner.c"))
            .map_err(|error| io_error("write test entry", &main, error))?;
        clang
            .args(["-x", "c", "-std=c11"])
            .arg(&main)
            .arg("-o")
            .arg(&artifact);
        if !cfg!(windows) {
            clang.arg("-lm");
        }
        if task_runtime {
            let runtime = directory.join("task.c");
            fs::write(&runtime, include_str!("runtime/task.c"))
                .map_err(|error| io_error("write task runtime", &runtime, error))?;
            clang.arg(&runtime).arg("-pthread");
        }
    }
    collect_message(
        messages,
        run_tool(&mut clang, "tests require LLVM/Clang 17+ or TSUZURI_CLANG")?,
    );
    if wasm {
        let mut linker = Command::new(tool("TSUZURI_WASM_LD", "wasm-ld"));
        linker
            .args([
                "--no-entry",
                "--strip-all",
                "--stack-first",
                "-z",
                "stack-size=1048576",
                "--max-memory=16777216",
                "--export=tsuzuri_test_count",
                "--export=tsuzuri_test_run",
            ])
            .arg(&object)
            .arg("-o")
            .arg(&artifact);
        collect_message(
            messages,
            run_tool(&mut linker, "WASM tests require wasm-ld or TSUZURI_WASM_LD")?,
        );
        let script = directory.join("run.mjs");
        fs::write(&script, include_str!("runtime/test-runner.mjs"))
            .map_err(|error| io_error("write Node test runner", &script, error))?;
        Ok(Runner {
            program: "node".into(),
            arguments: vec![script.into_os_string(), artifact.into_os_string()],
        })
    } else {
        Ok(Runner {
            program: artifact.into_os_string(),
            arguments: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_rejects_malformed_indices_and_reaps_timed_out_children() {
        let module =
            crate::analyze("test \"passes\" = ()\ntest \"loops\" = while true do ()").unwrap();
        let temporary = TemporaryDirectory::new(&env::temp_dir()).unwrap();
        let runner = build_runner(
            &module,
            &[0, 1],
            &TestOptions::default(),
            &temporary.path,
            &mut Vec::new(),
        )
        .unwrap();
        for arguments in [
            vec![],
            vec!["0", "1"],
            vec![""],
            vec!["-1"],
            vec!["+0"],
            vec![" 0"],
            vec!["0x"],
            vec!["18446744073709551616"],
            vec!["2"],
        ] {
            assert_eq!(
                Command::new(&runner.program)
                    .args(arguments)
                    .status()
                    .unwrap()
                    .code(),
                Some(2)
            );
        }
        assert!(
            execute_test(&runner, 0, Duration::from_secs(1))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            execute_test(&runner, 1, Duration::from_millis(30))
                .unwrap()
                .as_deref(),
            Some("timed out after 30ms")
        );
        assert!(
            execute_test(&runner, 0, Duration::from_secs(1))
                .unwrap()
                .is_none()
        );
        temporary.close().unwrap();
    }
}
