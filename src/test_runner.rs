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
    /// Count the regions of the user's code that passing tests run (`--coverage`, native only).
    pub coverage: bool,
    /// The property-test seed (`--seed`) instead of `llvm::DEFAULT_PROPERTY_SEED`.
    pub seed: Option<u64>,
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
            coverage: false,
            seed: None,
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
    /// The end of the standard error of a failed test, such as a property's counterexample.
    pub output: String,
    pub duration_ms: u128,
}

#[derive(Debug)]
pub struct TestReport {
    pub results: Vec<TestResult>,
    pub ignored: usize,
    pub duration_ms: u128,
    pub messages: Vec<String>,
    /// The merged counters of the passing tests, with `TestOptions::coverage`.
    pub coverage: Option<CoverageRun>,
}

/// What `tsuzuri test --coverage` counted: one counter per region of `plan`.
#[derive(Debug)]
pub struct CoverageRun {
    pub plan: crate::coverage::CoveragePlan,
    pub counts: Vec<u64>,
    /// The failed tests, whose counts are left out because they end before writing them.
    pub excluded_failed: usize,
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
    if options.coverage && options.target.is_wasm() {
        return Err(driver_error(
            "E2000",
            "test coverage supports only the native target",
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
    let plan = options.coverage.then(|| crate::coverage::plan(module));
    if selected.is_empty() {
        return Ok(TestReport {
            results: Vec::new(),
            ignored,
            duration_ms: started.elapsed().as_millis(),
            messages: Vec::new(),
            coverage: plan.map(|plan| CoverageRun {
                counts: vec![0; plan.len()],
                plan,
                excluded_failed: 0,
            }),
        });
    }
    let temporary = TemporaryDirectory::new(&env::temp_dir())?;
    let mut messages = Vec::new();
    let runner = build_runner(
        module,
        &selected.iter().map(|test| test.index).collect::<Vec<_>>(),
        options,
        links,
        plan.as_ref(),
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
                    let result =
                        execute_test(&runner, index, timeout).map(|(failure, output)| TestResult {
                            case: case.clone(),
                            output: if failure.is_some() {
                                output
                            } else {
                                String::new()
                            },
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
    let results: Vec<TestResult> = results
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Result<_, _>>()?;
    let coverage = match plan {
        Some(plan) => Some(merge_coverage(plan, &results, &runner)?),
        None => None,
    };
    temporary.close()?;
    Ok(TestReport {
        results,
        ignored,
        duration_ms: started.elapsed().as_millis(),
        messages,
        coverage,
    })
}

/// Adds up the counter files that the passing tests wrote (D12: failed tests write none).
fn merge_coverage(
    plan: crate::coverage::CoveragePlan,
    results: &[TestResult],
    runner: &Runner,
) -> Result<CoverageRun, Diagnostic> {
    let directory = runner
        .coverage
        .as_ref()
        .expect("a covered runner writes counter files");
    let mut counts = vec![0; plan.len()];
    let mut excluded_failed = 0;
    for (position, result) in results.iter().enumerate() {
        if result.failure.is_some() {
            excluded_failed += 1;
            continue;
        }
        let path = coverage_file(directory, position);
        let bytes =
            fs::read(&path).map_err(|error| io_error("read coverage counters", &path, error))?;
        crate::coverage::merge(&mut counts, &bytes).map_err(|message| {
            driver_error(
                "E2002",
                format!(
                    "invalid coverage counters in '{}': {message}",
                    path.display()
                ),
            )
        })?;
    }
    Ok(CoverageRun {
        plan,
        counts,
        excluded_failed,
    })
}

/// The counter file of the selected test at `position`.
fn coverage_file(directory: &Path, position: usize) -> PathBuf {
    directory.join(format!("coverage-{position}.bin"))
}

/// Refuses a `--coverage` output that would overwrite one of the project's sources (E2003).
pub fn check_coverage_output(project: &Project, output: &Path) -> Result<(), Diagnostic> {
    protect_sources(project, output)
}

/// Writes the lcov report of `coverage` for the project's user sources to `output` and
/// returns its files.
pub fn write_coverage(
    project: &Project,
    coverage: &CoverageRun,
    output: &Path,
) -> Result<Vec<crate::coverage::FileCoverage>, Diagnostic> {
    protect_sources(project, output)?;
    let paths: Vec<Option<String>> = project
        .sources
        .iter()
        .map(|source| {
            (source.origin == ModuleOrigin::User).then(|| {
                crate::cache::real_path(&source.path)
                    .unwrap_or_else(|_| source.path.clone())
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .collect();
    let sources: Vec<Option<crate::coverage::CoverageSource<'_>>> = paths
        .iter()
        .zip(&project.sources)
        .map(|(path, source)| {
            path.as_deref().map(|path| crate::coverage::CoverageSource {
                path,
                text: &source.text,
            })
        })
        .collect();
    let files = crate::coverage::files(&coverage.plan, &coverage.counts, &sources);
    fs::write(output, crate::coverage::render_lcov(&files))
        .map_err(|error| io_error("write coverage report", output, error))?;
    Ok(files)
}

struct Runner {
    program: OsString,
    arguments: Vec<OsString>,
    /// The directory of the counter files of a covered native runner.
    coverage: Option<PathBuf>,
}

/// A native test runner with debug information that runs one test when a debugger starts
/// `program` with `arguments` (G16 Phase 2).
#[derive(Debug)]
pub struct DebugRunner {
    pub case: crate::check::CheckedTest,
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub messages: Vec<String>,
}

/// Builds, without running it, the native runner of the one test that `options.indices`
/// selects, with debug information, at `output`; on macOS the DWARF goes to `<output>.dwarf`, as
/// for `build -g`. The runner holds only that test, so its argument is `0`. Like the C runtime,
/// the runner's entry has no debug information, so stepping stays in Tsuzuri code.
pub fn build_debug_runner(
    module: &CheckedModule,
    project: &Project,
    options: &TestOptions,
    links: &LinkInputs,
    output: &Path,
) -> Result<DebugRunner, Diagnostic> {
    let [index] = options.indices[..] else {
        return Err(driver_error(
            "E2000",
            "debugging a test needs exactly one --index",
        ));
    };
    let Some(case) = module.tests.get(index).cloned() else {
        return Err(driver_error(
            "E2000",
            "test index is out of range; refresh the test list",
        ));
    };
    if options.target != Target::Native {
        return Err(driver_error(
            "E2000",
            "debugging a test requires the native target",
        ));
    }
    if options.optimization > 3 {
        return Err(driver_error(
            "E2000",
            "test optimization must be between 0 and 3",
        ));
    }
    if !links.is_empty() {
        links.check_shape()?;
        links.check_readable()?;
    }
    let dwarf = cfg!(target_os = "macos").then(|| {
        let mut path = output.as_os_str().to_owned();
        path.push(".dwarf");
        PathBuf::from(path)
    });
    let pdb = msvc_linker().then(|| pdb_path(output));
    for path in std::iter::once(output)
        .chain(dwarf.as_deref())
        .chain(pdb.as_deref())
    {
        protect_sources(project, path)?;
        protect_links(links, path)?;
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create output directory", parent, error))?;
    let mut temporary = TemporaryDirectory::new(parent)?;
    let mut text = project.with_trap_sources(|sources| {
        llvm::emit_test_runner_with(
            module,
            &[index],
            llvm::TestRunnerOptions {
                seed: options.seed,
                debug: Some((sources, options.optimization != 0)),
                ..llvm::TestRunnerOptions::default()
            },
        )
    })?;
    if cfg!(windows) {
        text = llvm::windows_abi(text, module);
    }
    if pdb.is_some() {
        text = llvm::with_codeview(text);
    }
    let sources = native_runtime_sources(&text)?;
    let ir = temporary.path.join("tests.ll");
    let object = temporary.path.join("tests.o");
    let artifact = temporary
        .path
        .join(if cfg!(windows) { "tests.exe" } else { "tests" });
    fs::write(&ir, text).map_err(|error| io_error("write test IR", &ir, error))?;
    let mut messages = Vec::new();
    let optimization = format!("-O{}", options.optimization);
    let mut compile = Command::new(tool("TSUZURI_CLANG", "clang"));
    compile
        .args(["-x", "ir", "-Wno-override-module", "-g", "-c"])
        .arg(&optimization)
        .args(native_compile_args(cfg!(windows), env::consts::ARCH))
        .arg(&ir)
        .arg("-o")
        .arg(&object);
    collect_message(
        &mut messages,
        run_tool(
            &mut compile,
            "tests require LLVM/Clang 17+ or TSUZURI_CLANG",
        )?,
    );
    // The entry and the runtime have no debug information; `-g` only makes the link keep the
    // program's (and on Windows write the PDB).
    let mut link = Command::new(tool("TSUZURI_CLANG", "clang"));
    link.args(["-g", &optimization])
        .args(native_compile_args(cfg!(windows), env::consts::ARCH));
    for (name, source) in sources {
        let path = temporary.path.join(name);
        let runtime = path.with_extension("o");
        fs::write(&path, source).map_err(|error| io_error("write runtime", &path, error))?;
        let mut compile = Command::new(tool("TSUZURI_CLANG", "clang"));
        compile
            .args(["-x", "c", "-std=c11", "-c", &optimization])
            .args(native_compile_args(cfg!(windows), env::consts::ARCH));
        if !cfg!(windows) {
            compile.arg("-pthread");
        }
        compile.arg(&path).arg("-o").arg(&runtime);
        collect_message(
            &mut messages,
            run_tool(
                &mut compile,
                "tests require LLVM/Clang 17+ or TSUZURI_CLANG",
            )?,
        );
        link.arg(runtime);
    }
    link.arg(&object).arg("-o").arg(&artifact);
    if !cfg!(windows) {
        link.args(["-lm", "-pthread"]);
    }
    links.add_to(&mut link);
    let staged_pdb = temporary.path.join("tests.pdb");
    if let Some(pdb) = &pdb {
        link.args(pdb_link_args(&staged_pdb, pdb, &temporary.path)?);
    }
    collect_message(
        &mut messages,
        run_tool(&mut link, "tests require LLVM/Clang 17+ or TSUZURI_CLANG")?,
    );
    let staged = temporary.path.join("tests.dwarf");
    if dwarf.is_some() {
        let mut symbols = Command::new(tool("TSUZURI_DSYMUTIL", "dsymutil"));
        symbols.arg("--flat").arg(&artifact).arg("-o").arg(&staged);
        collect_message(
            &mut messages,
            run_tool(
                &mut symbols,
                "macOS debug executables require dsymutil; set TSUZURI_DSYMUTIL",
            )?,
        );
    }
    let sidecars: Vec<_> = dwarf
        .iter()
        .map(|path| (&staged, path))
        .chain(pdb.iter().map(|path| (&staged_pdb, path)))
        .collect();
    publish_outputs(project, &artifact, output, &sidecars, &mut temporary)?;
    temporary.close()?;
    let program = crate::cache::real_path(output)
        .map_err(|error| io_error("resolve test runner", output, error))?;
    Ok(DebugRunner {
        case,
        program,
        arguments: vec!["0".into()],
        messages,
    })
}

/// The C sources of a native test runner for its IR `text`: the entry and the runtimes that the
/// IR declares. A test may build IO actions without running them; their primitives still need
/// the runtime.
fn native_runtime_sources(text: &str) -> Result<Vec<(&'static str, String)>, Diagnostic> {
    let mut sources = vec![("main.c", include_str!("runtime/test-runner.c").to_owned())];
    if text.contains("declare void @tsuzuri_task_parallel(") {
        if !cfg!(any(unix, windows)) {
            return Err(driver_error(
                "E2002",
                "native parallel tasks require POSIX or Windows threads",
            ));
        }
        sources.push(("task.c", crate::driver::task_runtime_source()));
    }
    if text.contains("declare i64 @tsuzuri_os_") {
        if cfg!(windows) {
            return Err(driver_error("E2002", crate::driver::OS_WINDOWS_MESSAGE));
        }
        sources.push(("os.c", include_str!("runtime/os.c").to_owned()));
    }
    if text.contains("declare i32 @tsuzuri_io_") {
        sources.push(("io.c", include_str!("runtime/io.c").to_owned()));
    }
    if text.contains("declare i32 @tsuzuri_gpu_") {
        sources.push(("gpu.c", include_str!("runtime/gpu.c").to_owned()));
    }
    if llvm::uses_reactor(text) {
        if !crate::driver::ASYNC_NATIVE_SUPPORTED {
            return Err(driver_error("E2002", crate::driver::ASYNC_NATIVE_MESSAGE));
        }
        sources.push(("async.c", include_str!("runtime/async.c").to_owned()));
    }
    Ok(sources)
}

/// Runs selected test `index`: why it failed, if it did, and the end of its stderr.
fn execute_test(
    runner: &Runner,
    index: usize,
    timeout: Duration,
) -> Result<(Option<String>, String), Diagnostic> {
    let mut command = Command::new(&runner.program);
    command.args(&runner.arguments).arg(index.to_string());
    if let Some(directory) = &runner.coverage {
        command.env("TSUZURI_COVERAGE_FILE", coverage_file(directory, index));
    }
    let finished = run_captured(&mut command, timeout, false, "test")?;
    let failure = match finished.status {
        Some(status) => (!status.success()).then(|| termination_reason(&status)),
        None => Some(format!("timed out after {}ms", timeout.as_millis())),
    };
    Ok((failure, finished.stderr))
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
    coverage: Option<&crate::coverage::CoveragePlan>,
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
    // A property test reports its counterexample with Debug.print, so a WASM runner of a program
    // that uses Gen imports the write; other programs keep no imports.
    let debug_output = module
        .functions
        .iter()
        .any(|function| function.module == "Gen");
    let mut text = llvm::emit_test_runner_with(
        module,
        selected,
        llvm::TestRunnerOptions {
            wasm,
            memory64,
            coverage,
            seed: options.seed,
            debug_output,
            debug: None,
        },
    )?;
    if !wasm {
        let artifact = compile_native_runner(
            module,
            text,
            &NativeRunner {
                stem: "tests",
                kind: "test",
                tools_hint: "tests require LLVM/Clang 17+ or TSUZURI_CLANG",
                entry: include_str!("runtime/test-runner.c"),
                defines: if coverage.is_some() {
                    &["-DTSUZURI_COVERAGE"]
                } else {
                    &[]
                },
                optimization: options.optimization,
            },
            links,
            directory,
            messages,
        )?;
        return Ok(Runner {
            program: artifact.into_os_string(),
            arguments: Vec::new(),
            coverage: coverage.map(|_| directory.to_owned()),
        });
    }
    text = llvm::with_wasm_heap_limit(text, max_memory);
    if crate::driver::wasm_stack_checks(options.target, false, max_memory) {
        text = llvm::with_stack_checks(text, false);
    }
    text = llvm::with_wasm_gpu_host(text, false);
    text.push_str(include_str!("runtime/wasm.ll"));
    if text.contains("declare i64 @tsuzuri_os_") {
        return Err(driver_error("E2000", crate::driver::OS_WASM_MESSAGE));
    }
    if text.contains("define i64 @tsuzuri_async_poll(") || llvm::uses_reactor(&text) {
        return Err(driver_error(
            "E2000",
            "the WebAssembly test runner cannot drive Async.start or Async.block_on; use Async.run, or build a module with an asynchronous host",
        ));
    }
    let ir = directory.join("tests.ll");
    let object = directory.join("tests.o");
    let artifact = directory.join("tests.wasm");
    fs::write(&ir, text).map_err(|error| io_error("write test IR", &ir, error))?;
    let mut clang = Command::new(tool("TSUZURI_CLANG", "clang"));
    clang
        .args(["-x", "ir", "-Wno-override-module"])
        .arg(format!("-O{}", options.optimization))
        .arg(&ir)
        .arg(if memory64 {
            "--target=wasm64-unknown-unknown"
        } else {
            "--target=wasm32-unknown-unknown"
        })
        .args(["-mbulk-memory", "-c"])
        .arg("-o")
        .arg(&object);
    collect_message(
        messages,
        run_tool(&mut clang, "tests require LLVM/Clang 17+ or TSUZURI_CLANG")?,
    );
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
        // The host's `tsuzuri_debug.write` reads the text from the exported memory. wasm-ld
        // exports it by default; like `build --debug-output`, do not rely on that.
        .args(debug_output.then_some("--export-memory"))
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
        coverage: None,
    })
}

/// The C entry and file names of a native runner executable.
struct NativeRunner<'a> {
    /// The file stem of its IR and executable in the temporary directory.
    stem: &'a str,
    /// `test` or `bench`, for I/O error messages.
    kind: &'a str,
    tools_hint: &'a str,
    entry: &'a str,
    defines: &'a [&'a str],
    optimization: u8,
}

/// Compiles the runner IR `text` with its C entry, the runtimes the IR declares, and the host
/// link inputs into an executable in `directory`.
fn compile_native_runner(
    module: &CheckedModule,
    mut text: String,
    runner: &NativeRunner<'_>,
    links: &LinkInputs,
    directory: &Path,
    messages: &mut Vec<String>,
) -> Result<PathBuf, Diagnostic> {
    if cfg!(windows) {
        text = llvm::windows_abi(text, module);
    }
    let task_runtime = text.contains("declare void @tsuzuri_task_parallel(");
    if task_runtime && !cfg!(any(unix, windows)) {
        return Err(driver_error(
            "E2002",
            "native parallel tasks require POSIX or Windows threads",
        ));
    }
    // A test may build IO actions without running them; their primitives still need the runtime.
    let os_runtime = text.contains("declare i64 @tsuzuri_os_");
    let io_runtime = text.contains("declare i32 @tsuzuri_io_");
    let async_runtime = llvm::uses_reactor(&text);
    let gpu_runtime = text.contains("declare i32 @tsuzuri_gpu_");
    if os_runtime && cfg!(windows) {
        return Err(driver_error("E2002", crate::driver::OS_WINDOWS_MESSAGE));
    }
    if async_runtime && !crate::driver::ASYNC_NATIVE_SUPPORTED {
        return Err(driver_error("E2002", crate::driver::ASYNC_NATIVE_MESSAGE));
    }
    let kind = runner.kind;
    let ir = directory.join(format!("{}.ll", runner.stem));
    let artifact = directory.join(if cfg!(windows) {
        format!("{}.exe", runner.stem)
    } else {
        runner.stem.to_owned()
    });
    fs::write(&ir, text).map_err(|error| io_error(&format!("write {kind} IR"), &ir, error))?;
    let mut clang = Command::new(tool("TSUZURI_CLANG", "clang"));
    clang
        .args(["-x", "ir", "-Wno-override-module"])
        .arg(format!("-O{}", runner.optimization))
        .arg(&ir);
    let main = directory.join("main.c");
    clang.args(native_compile_args(cfg!(windows), env::consts::ARCH));
    fs::write(&main, runner.entry)
        .map_err(|error| io_error(&format!("write {kind} entry"), &main, error))?;
    clang
        .args(["-x", "c", "-std=c11"])
        .args(runner.defines)
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
    for (needed, name, source) in [
        (os_runtime, "os.c", include_str!("runtime/os.c")),
        (io_runtime, "io.c", include_str!("runtime/io.c")),
        (async_runtime, "async.c", include_str!("runtime/async.c")),
        (gpu_runtime, "gpu.c", include_str!("runtime/gpu.c")),
    ] {
        if needed {
            let runtime = directory.join(name);
            fs::write(&runtime, source)
                .map_err(|error| io_error("write runtime", &runtime, error))?;
            clang.arg(&runtime);
        }
    }
    if (async_runtime || gpu_runtime) && !task_runtime && !cfg!(windows) {
        clang.arg("-pthread");
    }
    // gpu.c loads a WebGPU library with dlopen, which older glibc keeps in libdl.
    if gpu_runtime && cfg!(target_os = "linux") {
        clang.arg("-ldl");
    }
    links.add_to(&mut clang);
    collect_message(messages, run_tool(&mut clang, runner.tools_hint)?);
    Ok(artifact)
}

/// The time that one bench sample aims for: the runner doubles the iteration count up to it.
pub const BENCH_SAMPLE_NS: u64 = 10_000_000;
/// The longest one bench process may run.
pub const BENCH_TIMEOUT: Duration = Duration::from_secs(300);
/// The samples per bench without `--samples`.
pub const DEFAULT_BENCH_SAMPLES: usize = 11;
/// The most iterations per sample.
pub const MAX_BENCH_ITERATIONS: u64 = 1 << 30;

/// What `tsuzuri bench` measures (G18 Phase 1).
#[derive(Clone, Debug)]
pub struct BenchOptions {
    pub target: Target,
    pub optimization: u8,
    pub filter: Option<String>,
    pub indices: Vec<usize>,
    /// Between 1 and 1000.
    pub samples: usize,
}

impl Default for BenchOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            optimization: 3,
            filter: None,
            indices: Vec::new(),
            samples: DEFAULT_BENCH_SAMPLES,
        }
    }
}

impl BenchOptions {
    pub fn includes(&self, bench: &crate::check::CheckedBench) -> bool {
        (self.indices.is_empty() || self.indices.contains(&bench.index))
            && self
                .filter
                .as_ref()
                .is_none_or(|filter| format!("{}.{}", bench.module, bench.name).contains(filter))
    }
}

/// One measured bench: the iteration count of every sample and the time per iteration of
/// each sample in milliseconds, or why it failed.
#[derive(Debug)]
pub struct BenchResult {
    pub case: crate::check::CheckedBench,
    pub failure: Option<String>,
    /// The end of the standard error of a failed bench.
    pub output: String,
    pub iterations: u64,
    pub samples_ms: Vec<f64>,
}

impl BenchResult {
    /// The median (of the two middle samples for an even count), minimum, and maximum
    /// milliseconds per iteration; `None` for a failed bench.
    pub fn statistics(&self) -> Option<(f64, f64, f64)> {
        if self.failure.is_some() || self.samples_ms.is_empty() {
            return None;
        }
        let mut sorted = self.samples_ms.clone();
        sorted.sort_by(f64::total_cmp);
        let middle = sorted.len() / 2;
        let median = if sorted.len() % 2 == 1 {
            sorted[middle]
        } else {
            (sorted[middle - 1] + sorted[middle]) / 2.0
        };
        Some((median, sorted[0], sorted[sorted.len() - 1]))
    }
}

#[derive(Debug)]
pub struct BenchReport {
    pub results: Vec<BenchResult>,
    pub ignored: usize,
    pub messages: Vec<String>,
}

pub fn run_benches(
    module: &CheckedModule,
    options: &BenchOptions,
) -> Result<BenchReport, Diagnostic> {
    run_benches_linked(module, options, &LinkInputs::default())
}

/// Measures the selected benches one at a time, each in its own process, so that they do not
/// disturb one another. The report has no pass or fail threshold.
pub fn run_benches_linked(
    module: &CheckedModule,
    options: &BenchOptions,
    links: &LinkInputs,
) -> Result<BenchReport, Diagnostic> {
    if options.target.is_wasm() {
        return Err(driver_error(
            "E2000",
            "tsuzuri bench supports only the native target",
        ));
    }
    if options.optimization > 3 {
        return Err(driver_error(
            "E2000",
            "bench optimization must be between 0 and 3",
        ));
    }
    if !(1..=1000).contains(&options.samples) {
        return Err(driver_error(
            "E2000",
            "bench samples must be an integer between 1 and 1000",
        ));
    }
    if options
        .indices
        .iter()
        .any(|index| *index >= module.benches.len())
    {
        return Err(driver_error(
            "E2000",
            "bench index is out of range; refresh the bench list",
        ));
    }
    if !links.is_empty() {
        links.check_shape()?;
        links.check_readable()?;
    }
    let selected: Vec<_> = module
        .benches
        .iter()
        .filter(|bench| options.includes(bench))
        .cloned()
        .collect();
    let ignored = module.benches.len() - selected.len();
    let mut messages = Vec::new();
    if selected.is_empty() {
        return Ok(BenchReport {
            results: Vec::new(),
            ignored,
            messages,
        });
    }
    let temporary = TemporaryDirectory::new(&env::temp_dir())?;
    let text = llvm::emit_bench_runner(
        module,
        &selected.iter().map(|bench| bench.index).collect::<Vec<_>>(),
    )?;
    let artifact = compile_native_runner(
        module,
        text,
        &NativeRunner {
            stem: "benches",
            kind: "bench",
            tools_hint: "benchmarks require LLVM/Clang 17+ or TSUZURI_CLANG",
            entry: include_str!("runtime/bench-runner.c"),
            defines: &[],
            optimization: options.optimization,
        },
        links,
        &temporary.path,
        &mut messages,
    )?;
    let mut results = Vec::with_capacity(selected.len());
    for (position, case) in selected.into_iter().enumerate() {
        let (outcome, output) = execute_bench(&artifact, position, options.samples, BENCH_TIMEOUT)?;
        results.push(match outcome {
            Ok((iterations, samples)) => BenchResult {
                case,
                failure: None,
                output: String::new(),
                iterations,
                samples_ms: samples
                    .iter()
                    .map(|nanoseconds| *nanoseconds as f64 / iterations as f64 / 1e6)
                    .collect(),
            },
            Err(reason) => BenchResult {
                case,
                failure: Some(reason),
                output,
                iterations: 0,
                samples_ms: Vec::new(),
            },
        });
    }
    temporary.close()?;
    Ok(BenchReport {
        results,
        ignored,
        messages,
    })
}

/// The iteration count and the nanoseconds of each sample, or why the bench failed.
type BenchOutcome = Result<(u64, Vec<u64>), String>;

/// Runs bench `position` of a bench runner; returns its outcome and the end of its stderr.
fn execute_bench(
    runner: &Path,
    position: usize,
    samples: usize,
    timeout: Duration,
) -> Result<(BenchOutcome, String), Diagnostic> {
    let mut command = Command::new(runner);
    command.args([
        position.to_string(),
        samples.to_string(),
        BENCH_SAMPLE_NS.to_string(),
    ]);
    let finished = run_captured(&mut command, timeout, true, "bench")?;
    let outcome = match finished.status {
        None => Err(format!("timed out after {}ms", timeout.as_millis())),
        Some(status) if status.code() == Some(3) => Err("returned a negative duration".into()),
        Some(status) if !status.success() => Err(termination_reason(&status)),
        Some(_) => parse_bench_output(&finished.stdout, samples),
    };
    Ok((outcome, finished.stderr))
}

/// Parses `iterations N` and `samples` lines `sample NS` exactly.
fn parse_bench_output(output: &str, samples: usize) -> Result<(u64, Vec<u64>), String> {
    let malformed = || "produced malformed output".to_owned();
    let number = |text: &str| -> Option<u64> {
        (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| text.parse().ok())
            .flatten()
    };
    let mut lines = output.lines();
    let iterations = lines
        .next()
        .and_then(|line| line.strip_prefix("iterations "))
        .and_then(number)
        .filter(|count| count.is_power_of_two() && *count <= MAX_BENCH_ITERATIONS)
        .ok_or_else(malformed)?;
    let values = lines
        .map(|line| line.strip_prefix("sample ").and_then(number))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(malformed)?;
    if values.len() != samples {
        return Err(malformed());
    }
    Ok((iterations, values))
}

/// How a child ended: `None` after a timeout, which kills it.
struct Captured {
    status: Option<std::process::ExitStatus>,
    stdout: String,
    stderr: String,
}

/// The most bytes of a child's standard error that a report keeps (the last ones).
const CAPTURED_STDERR: usize = 64 * 1024;

/// Runs `command` with null stdin, waiting at most `timeout`. Readers drain the pipes while the
/// child runs, so a full pipe never blocks it; stdout is kept only with `stdout`. `kind` is
/// `test` or `bench`, for the messages.
fn run_captured(
    command: &mut Command,
    timeout: Duration,
    stdout: bool,
    kind: &str,
) -> Result<Captured, Diagnostic> {
    command
        .stdin(Stdio::null())
        .stdout(if stdout {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| driver_error("E2002", format!("cannot start {kind} runner: {error}")))?;
    let out = child.stdout.take().map(|pipe| {
        std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = std::io::Read::read_to_end(&mut { pipe }, &mut text);
            text
        })
    });
    let error_pipe = child.stderr.take().expect("stderr is piped");
    let err = std::thread::spawn(move || read_tail(error_pipe, CAPTURED_STDERR));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(driver_error(
                    "E2002",
                    format!("cannot wait for {kind} runner: {error}"),
                ));
            }
        }
        if started.elapsed() >= timeout {
            let stopped = child.kill();
            let collected = child.wait();
            collected.map_err(|error| {
                driver_error("E2002", format!("cannot collect timed-out {kind}: {error}"))
            })?;
            stopped.map_err(|error| {
                driver_error("E2002", format!("cannot stop timed-out {kind}: {error}"))
            })?;
            break None;
        }
        std::thread::park_timeout(
            Duration::from_millis(5).min(timeout.saturating_sub(started.elapsed())),
        );
    };
    let stdout = out
        .map(|reader| reader.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Ok(Captured {
        status,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr,
    })
}

/// Reads `reader` to its end and keeps its last `limit` bytes, noting what it left out.
fn read_tail(mut reader: impl std::io::Read, limit: usize) -> String {
    let mut kept: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
    let mut omitted = 0usize;
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                kept.extend(&buffer[..count]);
                while kept.len() > limit {
                    kept.pop_front();
                    omitted += 1;
                }
            }
        }
    }
    let text = String::from_utf8_lossy(kept.make_contiguous()).into_owned();
    if omitted == 0 {
        text
    } else {
        format!("[{omitted} earlier bytes omitted]\n{text}")
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
            None,
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
                .0
                .is_none()
        );
        assert_eq!(
            execute_test(&runner, 1, Duration::from_millis(30))
                .unwrap()
                .0
                .as_deref(),
            Some("timed out after 30ms")
        );
        assert!(
            execute_test(&runner, 0, Duration::from_secs(1))
                .unwrap()
                .0
                .is_none()
        );
        temporary.close().unwrap();
    }

    fn bench_result(samples_ms: Vec<f64>) -> BenchResult {
        BenchResult {
            case: crate::check::CheckedBench {
                module: "Main".into(),
                name: "b".into(),
                index: 0,
                function: 0,
                span: Span::default(),
            },
            failure: None,
            output: String::new(),
            iterations: 1,
            samples_ms,
        }
    }

    #[test]
    fn bench_statistics_use_median_min_max() {
        assert_eq!(
            bench_result(vec![3.0, 1.0, 2.0]).statistics(),
            Some((2.0, 1.0, 3.0))
        );
        assert_eq!(
            bench_result(vec![4.0, 1.0, 3.0, 2.0]).statistics(),
            Some((2.5, 1.0, 4.0))
        );
        assert_eq!(bench_result(vec![5.0]).statistics(), Some((5.0, 5.0, 5.0)));
        let mut failed = bench_result(vec![1.0]);
        failed.failure = Some("timed out after 1ms".into());
        assert_eq!(failed.statistics(), None);
        assert_eq!(bench_result(Vec::new()).statistics(), None);
    }

    #[test]
    fn bench_output_parsing_is_strict() {
        assert_eq!(
            parse_bench_output("iterations 8\nsample 10\nsample 0\n", 2),
            Ok((8, vec![10, 0]))
        );
        for (output, samples) in [
            ("sample 10\n", 1),
            ("iterations 8\nsample 10\n", 2),
            ("iterations 8\nsample 10\nsample 11\n", 1),
            ("iterations 8\nsample ten\n", 1),
            ("iterations 8\nsample -1\n", 1),
            ("iterations 8\nsample +1\n", 1),
            ("iterations 6\nsample 1\n", 1),
            ("iterations 0\nsample 1\n", 1),
            ("iterations 2147483648\nsample 1\n", 1),
            ("iterations 8\nsample 1\nextra\n", 1),
            ("", 1),
        ] {
            assert_eq!(
                parse_bench_output(output, samples),
                Err("produced malformed output".into()),
                "{output:?}"
            );
        }
    }

    #[test]
    fn bench_runner_rejects_malformed_arguments() {
        let module = crate::analyze("bench \"twice\" = \\n -> n * 2").unwrap();
        let temporary = TemporaryDirectory::new(&env::temp_dir()).unwrap();
        let runner = compile_native_runner(
            &module,
            llvm::emit_bench_runner(&module, &[0]).unwrap(),
            &NativeRunner {
                stem: "benches",
                kind: "bench",
                tools_hint: "benchmarks require LLVM/Clang 17+ or TSUZURI_CLANG",
                entry: include_str!("runtime/bench-runner.c"),
                defines: &[],
                optimization: 0,
            },
            &LinkInputs::default(),
            &temporary.path,
            &mut Vec::new(),
        )
        .unwrap();
        for arguments in [
            vec![],
            vec!["0", "1"],
            vec!["0", "1", "1", "1"],
            vec!["1", "1", "1"],
            vec!["-0", "1", "1"],
            vec!["0", "0", "1"],
            vec!["0", "1001", "1"],
            vec!["0", "+1", "1"],
            vec!["0", "1", "0"],
            vec!["0", "1", "x"],
            vec!["0", "1", "99999999999999999999"],
        ] {
            assert_eq!(
                Command::new(&runner)
                    .args(&arguments)
                    .status()
                    .unwrap()
                    .code(),
                Some(2),
                "{arguments:?}"
            );
        }
        // A one-nanosecond target stops at the first iteration count: the body returns 2n.
        let output = Command::new(&runner)
            .args(["0", "2", "1"])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "iterations 1\nsample 2\nsample 2\n"
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
                coverage: None,
            };
            assert_eq!(
                execute_test(&runner, 0, Duration::from_secs(5))
                    .unwrap()
                    .0
                    .as_deref(),
                Some(expected),
                "{script}"
            );
        }
    }
}
