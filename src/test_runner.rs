use super::*;
use std::process::Stdio;
use std::sync::{Mutex, atomic::AtomicUsize};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct TestOptions {
    pub target: Target,
    pub optimization: u8,
    pub filter: Option<String>,
    pub indices: Vec<usize>,
    /// `None` selects [`DEFAULT_WASM_MAX_MEMORY`]; only valid for WASM targets.
    pub wasm_max_memory: Option<u64>,
    /// `None` selects [`DEFAULT_WASM_STACK_SIZE`]; only valid for WASM targets.
    pub wasm_stack_size: Option<u64>,
}

impl Default for TestOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            optimization: 0,
            filter: None,
            indices: Vec::new(),
            wasm_max_memory: None,
            wasm_stack_size: None,
        }
    }
}

impl TestOptions {
    pub fn includes(&self, test: &crate::check::CheckedTest) -> bool {
        (self.indices.is_empty() || self.indices.contains(&test.index))
            && self
                .filter
                .as_ref()
                .is_none_or(|filter| format!("{}.{}", test.module, test.name).contains(filter))
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
    run_tests_linked(module, options, &LinkInputs::default())
}

/// Like [`run_tests`], linking the host `links` into the native test executable.
pub fn run_tests_linked(
    module: &CheckedModule,
    options: &TestOptions,
    links: &LinkInputs,
) -> Result<TestReport, Diagnostic> {
    run_with_timeout(module, options, links, Duration::from_secs(30))
}

fn run_with_timeout(
    module: &CheckedModule,
    options: &TestOptions,
    links: &LinkInputs,
    timeout: Duration,
) -> Result<TestReport, Diagnostic> {
    if options.optimization > 3 {
        return Err(driver_error(
            "E2000",
            "test optimization must be between 0 and 3",
        ));
    }
    if !links.is_empty() {
        if options.target.is_wasm() {
            return Err(driver_error(
                "E2000",
                "link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe",
            ));
        }
        links.check_shape()?;
        links.check_readable()?;
    }
    if !options.target.is_wasm()
        && (options.wasm_max_memory.is_some() || options.wasm_stack_size.is_some())
    {
        return Err(driver_error(
            "E2000",
            "--wasm-max-memory and --wasm-stack-size require --target wasm32 or wasm64",
        ));
    }
    wasm_memory_limits(
        options.target,
        options.wasm_max_memory,
        options.wasm_stack_size,
    )?;
    let started = Instant::now();
    if options.target.is_wasm() {
        run_tool(
            Command::new("node").arg("--version"),
            "WASM tests require Node.js on PATH",
        )?;
    }
    if options.target == Target::Wasm64 {
        // Validates an empty module with one 64-bit memory.
        run_tool(
            Command::new("node").args([
                "-e",
                "process.exit(WebAssembly.validate(new Uint8Array([0,97,115,109,1,0,0,0,5,3,1,4,0])) ? 0 : 1)",
            ]),
            "WASM64 tests require Node.js with memory64 support (Node.js 24 or newer)",
        )?;
    }
    let selected: Vec<_> = module
        .tests
        .iter()
        .filter(|test| options.includes(test))
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
        links,
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
                return Ok((!result.success()).then(|| termination_reason(&result)));
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

fn termination_reason(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("trapped or exited with code {code}");
    }
    #[cfg(unix)]
    if probable_stack_exhaustion(status) {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!(
                "terminated by signal {signal}; the stack was probably exhausted by deep recursion"
            );
        }
    }
    "trapped or terminated by signal".into()
}

fn build_runner(
    module: &CheckedModule,
    selected: &[usize],
    options: &TestOptions,
    links: &LinkInputs,
    directory: &Path,
    messages: &mut Vec<String>,
) -> Result<Runner, Diagnostic> {
    let wasm = options.target.is_wasm();
    let memory64 = options.target == Target::Wasm64;
    let (max_memory, stack_size) = wasm_memory_limits(
        options.target,
        options.wasm_max_memory,
        options.wasm_stack_size,
    )?;
    let mut text = llvm::emit_test_runner_for(module, selected, wasm, memory64)?;
    if wasm {
        text = llvm::with_wasm_heap_limit(text, max_memory);
        if crate::driver::wasm_stack_checks(options.target, false, max_memory) {
            text = llvm::with_stack_checks(text, false);
        }
        text.push_str(include_str!("runtime/wasm.ll"));
    } else if cfg!(windows) {
        text = llvm::windows_abi(text, module);
    }
    let task_runtime = !wasm && text.contains("declare void @tsuzuri_task_parallel(");
    if task_runtime && !cfg!(any(unix, windows)) {
        return Err(driver_error(
            "E2002",
            "native parallel tasks require POSIX or Windows threads",
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
            .arg(if memory64 {
                "--target=wasm64-unknown-unknown"
            } else {
                "--target=wasm32-unknown-unknown"
            })
            .args(["-mbulk-memory", "-c"])
            .arg("-o")
            .arg(&object);
    } else {
        let main = directory.join("main.c");
        clang.args(native_compile_args(cfg!(windows), env::consts::ARCH));
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
            fs::write(&runtime, crate::driver::task_runtime_source())
                .map_err(|error| io_error("write task runtime", &runtime, error))?;
            clang.arg(&runtime);
            if !cfg!(windows) {
                clang.arg("-pthread");
            }
        }
        links.add_to(&mut clang);
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
                &format!("stack-size={stack_size}"),
                &format!("--max-memory={max_memory}"),
                "--export=tsuzuri_test_count",
                "--export=tsuzuri_test_run",
            ])
            .args(memory64.then_some("-mwasm64"))
            .arg(&object)
            .arg("-o")
            .arg(&artifact);
        collect_message(
            messages,
            run_tool(
                &mut linker,
                &wasm_link_hint(
                    "WASM tests require wasm-ld or TSUZURI_WASM_LD",
                    options.wasm_max_memory,
                    options.wasm_stack_size,
                ),
            )?,
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
            &LinkInputs::default(),
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

    #[cfg(unix)]
    #[test]
    fn execute_test_names_stack_exhaustion_signals() {
        for (script, expected) in [
            (
                "kill -SEGV $$",
                "terminated by signal 11; the stack was probably exhausted by deep recursion",
            ),
            ("kill -TRAP $$", "trapped or terminated by signal"),
            ("exit 1", "trapped or exited with code 1"),
        ] {
            let runner = Runner {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), script.into()],
            };
            assert_eq!(
                execute_test(&runner, 0, Duration::from_secs(5))
                    .unwrap()
                    .as_deref(),
                Some(expected),
                "{script}"
            );
        }
    }
}
