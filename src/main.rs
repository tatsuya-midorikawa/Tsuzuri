use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tsuzuri::diagnostic::{Diagnostic, Span, json_string};
use tsuzuri::driver::{self, BuildOptions, Cpu, Emit, Project, Target};

const HELP: &str = "\
Tsuzuri - a statically typed language with ownership, powered by LLVM

Usage:
    tsuzuri lsp
  tsuzuri check source.tz|source.tt|source.tc|directory [--json]
    tsuzuri doc source.tz|source.tt|source.tc|directory -o outdir [--json]
    tsuzuri fmt [--check] source.tz|source.tt|source.tc|directory [--json]
    tsuzuri test source.tz|directory [--list] [--filter TEXT] [--index N] [--json] [-O0|-O1|-O2|-O3]
                             [--target native|wasm32|wasm64] [--wasm-max-memory SIZE] [--wasm-stack-size SIZE]
  tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
  tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]
  tsuzuri toolchain info

Each source file is one module named after its filename:
  .tz  Code (records, unions, functions, and type class instances)
  .tt  Type class declarations (multiple classes per file)
  .tc  One computation expression builder (its operations and helpers)
All .tz, .tt, and .tc files below the project root are loaded recursively.
Subdirectories form dotted modules (Geometry/Point.tz becomes Geometry.Point).
File inputs use their parent as the root; directory inputs use that directory.
Applications start in Main.tz; a directory selects it.
Other source inputs can be checked or built as libraries.

Build options:
  -o, --output PATH       Output path (defaults to the input with a new extension)
  --target native|wasm32|wasm64  Target (default: native; wasm64 uses 64-bit memory)
    --wasm-feature <name>   Opt in to simd128 (WASM build) or threads (wasm32 build)
    --wasm-max-memory SIZE  WASM linear memory limit (WASM build/test; default 16MiB,
                            at most 4GiB-64KiB on wasm32 and 16GiB on wasm64)
    --wasm-stack-size SIZE  WASM main stack size (WASM output/test; default 1MiB)
                            Tsuzuri.toml [wasm] max-memory/stack-size set project defaults
    --emit KIND            exe, object, llvm, header, wasm, or wgsl
                         Default: exe for native, wasm for wasm32 and wasm64
  -O0, -O1, -O2, -O3    LLVM optimization level (default: -O3; no fast-math)
  --cpu generic|native   CPU tuning for native build/run (default: generic)
                         native uses this machine's ISA; not portable to older CPUs
  --json                 Emit machine-readable diagnostics on stderr
    --no-cache             Disable build/run artifact cache reads and writes
    -g, --debug-info        Emit source-level DWARF debug information
    --deny-warnings        Fail check/build/run before code generation on warnings
    --debug-output         Enable WASM Debug output imports (native always writes)
    --trap-info            Report trap locations and emit an output.trap.json table
                                                 Enabled by default for run; disabled by default for build
  --                     Treat remaining arguments as paths
  -h, --help             Show this help
  --version              Show the compiler version

Toolchain:
  TSUZURI_CLANG          Clang executable (default: clang; LLVM 17+)
  TSUZURI_WASM_LD        WebAssembly linker (default: wasm-ld)
  TSUZURI_LLVM_LINK      LLVM IR linker for macOS debug task objects (default: llvm-link)
  TSUZURI_DSYMUTIL       macOS debug symbol linker (default: dsymutil)
  Each tool comes from its variable, then a distribution's bin/, then PATH.

Exports use the tz_ prefix in both C and WebAssembly. Native executables print
the numeric, bool, or UTF-8 string result of Main.tz's top-level code or fn main.
unit results do not print anything.
All UI and I/O belong to the host, not the language.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Lsp,
    Check,
    Doc,
    Build,
    Run,
    Fmt,
    Test,
}

#[derive(Debug)]
struct Arguments {
    action: Action,
    input: PathBuf,
    output: Option<PathBuf>,
    options: BuildOptions,
    json: bool,
    deny_warnings: bool,
    format_check: bool,
    test_filter: Option<String>,
    test_list: bool,
    test_indices: Vec<usize>,
}

fn parse_arguments(arguments: &[OsString]) -> Result<Arguments, String> {
    if arguments.first().is_some_and(|argument| argument == "lsp") {
        if arguments.len() != 1 {
            return Err("lsp takes no paths or build options".into());
        }
        return Ok(Arguments {
            action: Action::Lsp,
            input: PathBuf::new(),
            output: None,
            options: BuildOptions::default(),
            json: false,
            deny_warnings: false,
            format_check: false,
            test_filter: None,
            test_list: false,
            test_indices: Vec::new(),
        });
    }
    let mut position = 0;
    let action = match arguments.first().and_then(|value| value.to_str()) {
        Some("check") => {
            position = 1;
            Action::Check
        }
        Some("doc") => {
            position = 1;
            Action::Doc
        }
        Some("build") => {
            position = 1;
            Action::Build
        }
        Some("run") => {
            position = 1;
            Action::Run
        }
        Some("fmt") => {
            position = 1;
            Action::Fmt
        }
        Some("test") => {
            position = 1;
            Action::Test
        }
        _ => Action::Build,
    };
    let mut input = None;
    let mut output = None;
    let mut target = None;
    let mut emit = None;
    let mut optimization = None;
    let mut cpu = None;
    let mut json = false;
    let mut deny_warnings = false;
    let mut format_check = false;
    let mut test_filter = None;
    let mut test_list = false;
    let mut test_indices = Vec::new();
    let mut debug_output = false;
    let mut trap_info = false;
    let mut debug_info = false;
    let mut wasm_simd = false;
    let mut wasm_threads = false;
    let mut wasm_max_memory = None;
    let mut wasm_stack_size = None;
    let mut no_cache = false;
    let mut paths_only = false;
    while position < arguments.len() {
        let argument = &arguments[position];
        position += 1;
        if !paths_only {
            match argument.to_str() {
                Some("--") => {
                    paths_only = true;
                    continue;
                }
                Some("--json") => {
                    json = true;
                    continue;
                }
                Some("--no-cache") => {
                    if no_cache {
                        return Err("no-cache specified more than once".into());
                    }
                    no_cache = true;
                    continue;
                }
                Some("--deny-warnings") => {
                    if deny_warnings {
                        return Err("deny-warnings specified more than once".into());
                    }
                    deny_warnings = true;
                    continue;
                }
                Some("--check") => {
                    if format_check {
                        return Err("check specified more than once".into());
                    }
                    format_check = true;
                    continue;
                }
                Some("--list") => {
                    if test_list {
                        return Err("list specified more than once".into());
                    }
                    test_list = true;
                    continue;
                }
                Some("--index") => {
                    let value = next_value(arguments, &mut position, "--index")?
                        .to_str()
                        .filter(|value| {
                            !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                        })
                        .ok_or("test index must be a nonnegative integer")?
                        .parse::<usize>()
                        .map_err(|_| "test index is too large")?;
                    test_indices.push(value);
                    continue;
                }
                Some("--filter") => {
                    if test_filter.is_some() {
                        return Err("filter specified more than once".into());
                    }
                    test_filter = Some(
                        next_value(arguments, &mut position, "--filter")?
                            .to_str()
                            .ok_or("test filter must be UTF-8")?
                            .to_owned(),
                    );
                    continue;
                }
                Some("--debug-output") => {
                    if debug_output {
                        return Err("debug-output specified more than once".into());
                    }
                    debug_output = true;
                    continue;
                }
                Some("--trap-info") => {
                    if trap_info {
                        return Err("trap-info specified more than once".into());
                    }
                    trap_info = true;
                    continue;
                }
                Some("-g" | "--debug-info") => {
                    if debug_info {
                        return Err("debug information specified more than once".into());
                    }
                    debug_info = true;
                    continue;
                }
                Some("-o" | "--output") => {
                    if output.is_some() {
                        return Err("output specified more than once".into());
                    }
                    output = Some(PathBuf::from(next_value(
                        arguments,
                        &mut position,
                        "--output",
                    )?));
                    continue;
                }
                Some("--target") => {
                    if target.is_some() {
                        return Err("target specified more than once".into());
                    }
                    target = Some(
                        match next_value(arguments, &mut position, "--target")?.to_str() {
                            Some("native") => Target::Native,
                            Some("wasm32") => Target::Wasm32,
                            Some("wasm64") => Target::Wasm64,
                            _ => {
                                return Err("target must be 'native', 'wasm32', or 'wasm64'".into());
                            }
                        },
                    );
                    continue;
                }
                Some("--wasm-feature") => {
                    let feature = match next_value(arguments, &mut position, "--wasm-feature")?.to_str() {
                        Some("simd128") => &mut wasm_simd,
                        Some("threads") => &mut wasm_threads,
                        _ => return Err("supported WASM features are 'simd128' and 'threads'; relaxed SIMD is not supported".into()),
                    };
                    if *feature {
                        return Err("WASM feature specified more than once".into());
                    }
                    *feature = true;
                    continue;
                }
                Some(option @ ("--wasm-max-memory" | "--wasm-stack-size")) => {
                    let (slot, name) = if option == "--wasm-max-memory" {
                        (&mut wasm_max_memory, "maximum memory")
                    } else {
                        (&mut wasm_stack_size, "stack size")
                    };
                    if slot.is_some() {
                        return Err(format!("WASM {name} specified more than once"));
                    }
                    *slot = Some(parse_size(
                        next_value(arguments, &mut position, option)?,
                        option,
                    )?);
                    continue;
                }
                Some("--emit") => {
                    if emit.is_some() {
                        return Err("emit kind specified more than once".into());
                    }
                    emit = Some(
                        match next_value(arguments, &mut position, "--emit")?.to_str() {
                            Some("exe") => Emit::Executable,
                            Some("object") => Emit::Object,
                            Some("llvm") => Emit::Llvm,
                            Some("header") => Emit::Header,
                            Some("wasm") => Emit::Wasm,
                            Some("wgsl") => Emit::Wgsl,
                            _ => {
                                return Err(
                                    "emit kind must be exe, object, llvm, header, wasm, or wgsl"
                                        .into(),
                                );
                            }
                        },
                    );
                    continue;
                }
                Some("-O0" | "-O1" | "-O2" | "-O3") => {
                    if optimization.is_some() {
                        return Err("optimization specified more than once".into());
                    }
                    optimization = Some(argument.to_str().unwrap().as_bytes()[2] - b'0');
                    continue;
                }
                Some("--cpu") => {
                    if cpu.is_some() {
                        return Err("CPU tuning specified more than once".into());
                    }
                    cpu = Some(
                        match next_value(arguments, &mut position, "--cpu")?.to_str() {
                            Some("generic") => Cpu::Generic,
                            Some("native") => Cpu::Native,
                            _ => return Err("CPU tuning must be 'generic' or 'native'".into()),
                        },
                    );
                    continue;
                }
                Some(value) if value.starts_with('-') => {
                    return Err(format!("unknown option '{value}'; use --help"));
                }
                _ => {}
            }
        }
        if input.replace(PathBuf::from(argument)).is_some() {
            return Err(
                "pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively"
                    .into(),
            );
        }
    }
    let input = input.ok_or("missing .tz, .tt, or .tc input or project directory; use --help")?;
    if no_cache && !matches!(action, Action::Build | Action::Run) {
        return Err("--no-cache is only valid with build or run".into());
    }
    if emit == Some(Emit::Wgsl) && (target.is_some() || optimization.is_some() || cpu.is_some()) {
        return Err("WGSL output does not use target, optimization, or CPU options".into());
    }
    if (wasm_simd || wasm_threads) && action != Action::Build {
        return Err("--wasm-feature is only valid with build".into());
    }
    if wasm_max_memory.is_some() || wasm_stack_size.is_some() {
        if !matches!(action, Action::Build | Action::Test) {
            return Err(
                "--wasm-max-memory and --wasm-stack-size are only valid with build or test".into(),
            );
        }
        if action == Action::Test && !target.is_some_and(Target::is_wasm) {
            return Err(
                "--wasm-max-memory and --wasm-stack-size require --target wasm32 or wasm64".into(),
            );
        }
    }
    if debug_info && !matches!(action, Action::Build | Action::Run) {
        return Err("--debug-info is only valid with build or run".into());
    }
    if debug_output && !matches!(action, Action::Build | Action::Run) {
        return Err("--debug-output is only valid with build or run".into());
    }
    if trap_info && !matches!(action, Action::Build | Action::Run) {
        return Err("--trap-info is only valid with build or run".into());
    }
    if format_check && action != Action::Fmt {
        return Err("--check is only valid with fmt".into());
    }
    if action == Action::Fmt && (optimization.is_some() || cpu.is_some() || deny_warnings) {
        return Err(
            "fmt does not use optimization, CPU tuning, or compiler warning options".into(),
        );
    }
    if action != Action::Test && (test_filter.is_some() || test_list || !test_indices.is_empty()) {
        return Err("--filter, --list, and --index are only valid with test".into());
    }
    if action == Action::Test && cpu.is_some() {
        return Err("test does not use CPU tuning".into());
    }
    if output.is_some() && !matches!(action, Action::Build | Action::Doc)
        || emit.is_some() && action != Action::Build
        || target.is_some() && !matches!(action, Action::Build | Action::Test)
    {
        return Err("--output requires build or doc; --target and --emit require a supported build/test action".into());
    }
    if action == Action::Doc {
        if output.is_none() {
            return Err("doc requires -o or --output with an output directory".into());
        }
        if optimization.is_some() || cpu.is_some() {
            return Err("doc does not use optimization or CPU tuning".into());
        }
    }
    if action == Action::Check && optimization.is_some() {
        return Err("check does not use an optimization level".into());
    }
    if action == Action::Check && cpu.is_some() {
        return Err("check does not use CPU tuning".into());
    }
    let target = target.unwrap_or(Target::Native);
    let options = BuildOptions {
        target,
        emit: emit.unwrap_or(if target.is_wasm() {
            Emit::Wasm
        } else {
            Emit::Executable
        }),
        optimization: optimization.unwrap_or(if action == Action::Test { 0 } else { 3 }),
        cpu: cpu.unwrap_or(Cpu::Generic),
        debug_output,
        trap_info: trap_info || action == Action::Run,
        debug_info,
        wasm_simd,
        wasm_threads,
        wasm_max_memory,
        wasm_stack_size,
        cache: !no_cache,
    };
    options.validate().map_err(|error| error.message)?;
    Ok(Arguments {
        action,
        input,
        output,
        options,
        json,
        deny_warnings,
        format_check,
        test_filter,
        test_list,
        test_indices,
    })
}

fn next_value<'a>(
    arguments: &'a [OsString],
    position: &mut usize,
    option: &str,
) -> Result<&'a OsString, String> {
    let value = arguments
        .get(*position)
        .ok_or_else(|| format!("{option} needs a value"))?;
    *position += 1;
    Ok(value)
}

/// Parses a byte count with an optional binary `KiB`, `MiB`, or `GiB` suffix.
fn parse_size(value: &OsStr, option: &str) -> Result<u64, String> {
    value
        .to_str()
        .and_then(tsuzuri::package::parse_size)
        .ok_or_else(|| {
            let example = if option == "--wasm-stack-size" {
                "4194304 or 4MiB"
            } else {
                "67108864 or 64MiB"
            };
            format!(
                "{option} must be a byte count or a number followed by KiB, MiB, or GiB, such as {example}"
            )
        })
}

fn print_diagnostic(error: &Diagnostic, input: &Path, source: &str, json: bool) {
    print_with_severity(error.severity.as_str(), error, input, source, json);
}

fn print_diagnostics(errors: &tsuzuri::diagnostic::DiagnosticSet, project: &Project, json: bool) {
    for (index, error) in errors.diagnostics.iter().enumerate() {
        if index != 0 && !json {
            eprintln!();
        }
        let source = project.source_for(error);
        print_diagnostic(error, &source.path, &source.text, json);
    }
    if let Some(note) = errors.omission_note() {
        if json {
            eprintln!(
                "{{\"severity\":\"note\",\"message\":{}}}",
                json_string(&note)
            );
        } else {
            let severity = if errors
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == tsuzuri::diagnostic::Severity::Error)
            {
                "error"
            } else {
                "warning"
            };
            eprintln!("\n{severity}: {note}");
        }
    }
}

fn print_with_severity(
    severity: &str,
    diagnostic: &Diagnostic,
    input: &Path,
    source: &str,
    json: bool,
) {
    let path = input.to_string_lossy();
    eprintln!(
        "{}",
        if json {
            diagnostic.json_with_severity(severity, &path, source)
        } else {
            diagnostic.render_with_severity(severity, &path, source)
        }
    );
}

fn run_action(
    arguments: &Arguments,
    project: &Project,
    module: &tsuzuri::check::CheckedModule,
) -> Result<Vec<String>, Diagnostic> {
    match arguments.action {
        Action::Lsp => unreachable!("LSP runs without a build project"),
        Action::Check => Ok(Vec::new()),
        Action::Doc => driver::document(
            project,
            arguments.output.as_ref().expect("doc requires output"),
        ),
        Action::Fmt => unreachable!("formatting runs before compilation"),
        Action::Test => unreachable!("tests use an isolated runner"),
        Action::Build => driver::build(
            module,
            project,
            &arguments
                .output
                .clone()
                .unwrap_or_else(|| arguments.options.output_path(project.input())),
            arguments.options,
        ),
        Action::Run => {
            driver::run_with_diagnostics(module, project, arguments.options, arguments.json)
        }
    }
}

fn run_formatter(arguments: &Arguments) -> ExitCode {
    match driver::format_sources(&arguments.input, arguments.format_check) {
        Ok(changed) => {
            if arguments.format_check {
                for path in &changed {
                    if arguments.json {
                        eprintln!(
                            "{{\"severity\":\"error\",\"code\":\"E2000\",\"message\":\"file is not formatted\",\"path\":{}}}",
                            json_string(&path.to_string_lossy())
                        );
                    } else {
                        eprintln!("{}: not formatted", path.display());
                    }
                }
                if !changed.is_empty() {
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            let source = driver::read_source(&error.path).unwrap_or_default();
            print_diagnostic(&error.diagnostic, &error.path, &source, arguments.json);
            ExitCode::FAILURE
        }
    }
}

fn run_test_action(
    arguments: &Arguments,
    project: &Project,
    module: &tsuzuri::check::CheckedModule,
) -> ExitCode {
    let options = driver::TestOptions {
        target: arguments.options.target,
        optimization: arguments.options.optimization,
        filter: arguments.test_filter.clone(),
        indices: arguments.test_indices.clone(),
        wasm_max_memory: arguments.options.wasm_max_memory,
        wasm_stack_size: arguments.options.wasm_stack_size,
    };
    if options
        .indices
        .iter()
        .any(|index| *index >= module.tests.len())
    {
        print_diagnostic(
            &Diagnostic::new(
                "E2000",
                "test index is out of range; refresh the test list",
                Span::default(),
            ),
            project.input(),
            "",
            arguments.json,
        );
        return ExitCode::FAILURE;
    }
    if arguments.test_list {
        for case in module.tests.iter().filter(|case| options.includes(case)) {
            if arguments.json {
                let source = project.source_for(&Diagnostic::new("E2000", "", case.span));
                let mapper = tsuzuri::lsp::PositionMapper::new(
                    &source.text,
                    tsuzuri::lsp::PositionEncoding::Utf16,
                );
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "test", "index": case.index, "module": case.module,
                        "name": case.name, "path": source.path,
                        "range": mapper.range(&source.text, case.span),
                    })
                );
            } else {
                println!(
                    "{} {}.{}",
                    case.index,
                    case.module,
                    case.name.escape_debug()
                );
            }
        }
        return ExitCode::SUCCESS;
    }
    let report = match driver::run_tests(module, &options) {
        Ok(report) => report,
        Err(error) => {
            let source = project.source_for(&error);
            print_diagnostic(&error, &source.path, &source.text, arguments.json);
            return ExitCode::FAILURE;
        }
    };
    for message in &report.messages {
        if arguments.json {
            eprintln!(
                "{{\"severity\":\"warning\",\"code\":\"W2001\",\"message\":{}}}",
                json_string(message.trim())
            );
        } else {
            eprintln!("{}", message.trim());
        }
    }
    for result in &report.results {
        let case = &result.case;
        if arguments.json {
            let failure = result.failure.as_ref().map_or_else(String::new, |failure| {
                format!(",\"failure\":{}", json_string(failure))
            });
            println!(
                "{{\"type\":\"test\",\"index\":{},\"module\":{},\"name\":{},\"status\":\"{}\"{},\"duration_ms\":{}}}",
                case.index,
                json_string(&case.module),
                json_string(&case.name),
                if result.failure.is_some() {
                    "failed"
                } else {
                    "passed"
                },
                failure,
                result.duration_ms
            );
        } else {
            println!(
                "{} {} - {} {}",
                if result.failure.is_some() {
                    "not ok"
                } else {
                    "ok"
                },
                case.index + 1,
                case.module,
                case.name.escape_debug()
            );
            if let Some(failure) = &result.failure {
                println!("  failure: {failure}");
            }
        }
    }
    let failed = report
        .results
        .iter()
        .filter(|result| result.failure.is_some())
        .count();
    let passed = report.results.len() - failed;
    if arguments.json {
        println!(
            "{{\"type\":\"summary\",\"passed\":{passed},\"failed\":{failed},\"ignored\":{},\"duration_ms\":{}}}",
            report.ignored, report.duration_ms
        );
    } else {
        println!(
            "\n{passed} passed; {failed} failed; {} ignored",
            report.ignored
        );
    }
    if let Some(first) = report
        .results
        .iter()
        .find(|result| result.failure.is_some())
    {
        let diagnostic = Diagnostic::new(
            "E2006",
            format!("{failed} test{} failed", if failed == 1 { "" } else { "s" }),
            first.case.span,
        );
        let source = project.source_for(&diagnostic);
        print_diagnostic(&diagnostic, &source.path, &source.text, arguments.json);
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn toolchain_info() -> String {
    let mut info = format!("tsuzuri {}\n", env!("CARGO_PKG_VERSION"));
    match driver::distribution_root() {
        Some(root) => {
            let id = std::fs::read_to_string(root.join("manifest.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|manifest| manifest["id"].as_str()?.get(..12).map(str::to_owned));
            info += &format!(
                "distribution: {} (id {})\n",
                root.display(),
                id.as_deref().unwrap_or("unavailable")
            );
        }
        None => info += "distribution: none\n",
    }
    for (variable, fallback) in [
        ("TSUZURI_CLANG", "clang"),
        ("TSUZURI_WASM_LD", "wasm-ld"),
        ("TSUZURI_LLVM_LINK", "llvm-link"),
        ("TSUZURI_DSYMUTIL", "dsymutil"),
    ] {
        let (source, tool) = driver::resolve_tool(variable, fallback);
        info += &tool_line(variable, source, &tool);
    }
    info + &tool_line("node", driver::ToolSource::Path, "node".as_ref())
}

fn tool_line(name: &str, source: driver::ToolSource, tool: &std::ffi::OsStr) -> String {
    let source = match source {
        driver::ToolSource::Env => "env",
        driver::ToolSource::Bundled => "bundled",
        driver::ToolSource::Path => "path",
    };
    match tsuzuri::cache::executable_path(tool) {
        Ok(path) => format!(
            "{name}: {source} {} ({})\n",
            path.display(),
            version_line(&path)
        ),
        Err(_) => format!("{name}: {source} {} (not found)\n", tool.to_string_lossy()),
    }
}

/// The first `--version` line naming a version, or else its first nonempty line.
fn version_line(tool: &Path) -> String {
    // Some Clang drivers write files into the working directory, so probe from a scratch one.
    let scratch = env::temp_dir().join(format!("tsuzuri-version-{}", std::process::id()));
    let output = std::fs::create_dir_all(&scratch).and_then(|()| {
        std::process::Command::new(tool)
            .arg("--version")
            .current_dir(&scratch)
            .stdin(std::process::Stdio::null())
            .output()
    });
    let _ = std::fs::remove_dir_all(&scratch);
    let output = match output {
        Ok(output) if output.status.success() => output,
        _ => return "unavailable".into(),
    };
    let bytes = if output.stdout.iter().all(u8::is_ascii_whitespace) {
        output.stderr
    } else {
        output.stdout
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .find(|line| line.to_ascii_lowercase().contains("version"))
        .or(lines.first())
        .map_or_else(|| "unavailable".into(), |line| (*line).to_owned())
}

fn main() -> ExitCode {
    let raw: Vec<_> = env::args_os().skip(1).collect();
    if raw.is_empty() {
        eprintln!("{HELP}");
        return ExitCode::from(2);
    }
    let flags: Vec<_> = raw
        .iter()
        .take_while(|argument| *argument != "--")
        .collect();
    if flags
        .iter()
        .any(|argument| *argument == "--help" || *argument == "-h")
    {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if raw.len() == 1 && raw[0] == "--version" {
        println!("tsuzuri {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if raw.len() == 2 && raw[0] == "toolchain" && raw[1] == "info" {
        print!("{}", toolchain_info());
        return ExitCode::SUCCESS;
    }
    let json = flags.iter().any(|argument| *argument == "--json");
    let arguments = match parse_arguments(&raw) {
        Ok(arguments) => arguments,
        Err(message) => {
            print_diagnostic(
                &Diagnostic::new("E2000", message, Span::default()),
                Path::new("<command line>"),
                "",
                json,
            );
            return ExitCode::from(2);
        }
    };
    if arguments.action == Action::Lsp {
        return ExitCode::from(tsuzuri::lsp::serve(
            std::io::stdin(),
            std::io::stdout().lock(),
            std::io::stderr().lock(),
        ) as u8);
    }
    if arguments.action == Action::Fmt {
        return run_formatter(&arguments);
    }
    let loaded = if arguments.action == Action::Test {
        Project::load_for_tests(&arguments.input)
    } else if arguments.action == Action::Doc {
        Project::load_for_docs(&arguments.input)
    } else {
        Project::load(&arguments.input)
    };
    let project = match loaded {
        Ok(project) => project,
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", arguments.json);
            return ExitCode::FAILURE;
        }
    };
    let mut arguments = arguments;
    arguments.options = arguments.options.with_manifest_wasm(project.wasm);
    // Command-line values were validated during parsing, so only manifest values can fail here.
    if let Err(error) = arguments.options.validate() {
        print_diagnostic(
            &Diagnostic::new(
                error.code,
                format!(
                    "{} (after applying the root package's [wasm])",
                    error.message
                ),
                Span::default(),
            ),
            Path::new("Tsuzuri.toml"),
            "",
            arguments.json,
        );
        return ExitCode::FAILURE;
    }
    let module = match project.analyze_all() {
        Ok(module) => module,
        Err(errors) => {
            print_diagnostics(&errors, &project, arguments.json);
            return ExitCode::FAILURE;
        }
    };
    let warnings =
        tsuzuri::diagnostic::DiagnosticSet::from_diagnostics(module.warnings.iter().cloned(), 0);
    print_diagnostics(&warnings, &project, arguments.json);
    if arguments.deny_warnings && !warnings.is_empty() {
        return ExitCode::FAILURE;
    }
    if arguments.action == Action::Test {
        return run_test_action(&arguments, &project, &module);
    }
    let result = run_action(&arguments, &project, &module);
    match result {
        Ok(messages) => {
            for message in messages {
                if arguments.json {
                    eprintln!(
                        "{{\"severity\":\"warning\",\"code\":\"W2001\",\"message\":{}}}",
                        json_string(message.trim())
                    );
                } else {
                    eprintln!("{}", message.trim());
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            let source = project.source_for(&error);
            print_diagnostic(&error, &source.path, &source.text, arguments.json);
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[&str]) -> Result<Arguments, String> {
        parse_arguments(&values.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn test_discovery_and_index_arguments_are_scoped() {
        let arguments =
            parse(&["test", "Specs.tz", "--list", "--index", "0", "--index", "2"]).unwrap();
        assert!(arguments.test_list);
        assert_eq!(arguments.test_indices, vec![0, 2]);
        for values in [
            vec!["check", "Specs.tz", "--list"],
            vec!["run", "Main.tz", "--index", "0"],
            vec!["test", "Specs.tz", "--list", "--list"],
            vec!["test", "Specs.tz", "--index", "-1"],
            vec!["test", "Specs.tz", "--index", ""],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
    }

    #[test]
    fn docs_requires_output_and_rejects_build_options() {
        let arguments = parse(&["doc", "Library.tz", "-o", "docs", "--json"]).unwrap();
        assert_eq!(arguments.action, Action::Doc);
        assert_eq!(arguments.output.as_deref(), Some(Path::new("docs")));
        assert!(arguments.json);
        assert!(parse(&["doc", "Library.tz"]).is_err());
        for option in [
            vec!["--target", "native"],
            vec!["--emit", "llvm"],
            vec!["--cpu", "generic"],
            vec!["-O0"],
            vec!["-g"],
            vec!["--debug-output"],
            vec!["--trap-info"],
            vec!["--wasm-feature", "simd128"],
        ] {
            let mut values = vec!["doc", "Library.tz", "-o", "docs"];
            values.extend(option);
            assert!(parse(&values).is_err(), "{values:?}");
        }
    }

    #[test]
    fn selects_target_defaults_and_honors_path_separator() {
        assert!(
            !parse(&["build", "Main.tz", "--no-cache"])
                .unwrap()
                .options
                .cache
        );
        assert!(
            !parse(&["run", "Main.tz", "--no-cache"])
                .unwrap()
                .options
                .cache
        );
        assert!(parse(&["check", "Main.tz", "--no-cache"]).is_err());
        assert!(parse(&["build", "Main.tz", "--no-cache", "--no-cache"]).is_err());
        assert_eq!(
            parse(&["build", "Kernel.tz", "--emit", "wgsl"])
                .unwrap()
                .options
                .emit,
            Emit::Wgsl
        );
        assert!(parse(&["build", "Kernel.tz", "--emit", "wgsl", "-O3"]).is_err());
        assert!(parse(&["build", "Kernel.tz", "--emit", "wgsl", "--target", "wasm32"]).is_err());
        let threads = parse(&[
            "build",
            "Main.tz",
            "--target",
            "wasm32",
            "--wasm-feature",
            "threads",
            "--wasm-feature",
            "simd128",
        ])
        .unwrap();
        assert!(threads.options.wasm_threads && threads.options.wasm_simd);
        for values in [
            vec!["build", "Main.tz", "--wasm-feature", "threads"],
            vec!["check", "Main.tz", "--wasm-feature", "threads"],
            vec!["run", "Main.tz", "--wasm-feature", "threads"],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--emit",
                "llvm",
                "--wasm-feature",
                "threads",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--emit",
                "header",
                "--wasm-feature",
                "threads",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "threads",
                "--wasm-feature",
                "threads",
            ],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
        assert!(
            parse(&[
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "simd128"
            ])
            .unwrap()
            .options
            .wasm_simd
        );
        assert!(
            parse(&["build", "Main.tz", "-g"])
                .unwrap()
                .options
                .debug_info
        );
        assert!(
            parse(&["run", "Main.tz", "--debug-info"])
                .unwrap()
                .options
                .debug_info
        );
        for values in [
            ["check", "Main.tz", "-g"],
            ["fmt", "Main.tz", "-g"],
            ["test", "Main.tz", "-g"],
        ] {
            assert!(parse(&values).is_err());
        }
        assert_eq!(parse(&["lsp"]).unwrap().action, Action::Lsp);
        for options in [["lsp", "-O0"], ["lsp", "--json"], ["lsp", "Main.tz"]] {
            assert!(parse(&options).is_err());
        }
        let tests = parse(&[
            "test", "Sources", "--filter", "name", "--target", "wasm32", "--json",
        ])
        .unwrap();
        assert_eq!(tests.action, Action::Test);
        assert_eq!(tests.options.optimization, 0);
        assert_eq!(tests.test_filter.as_deref(), Some("name"));
        let formatting = parse(&["fmt", "--check", "Sources", "--json"]).unwrap();
        assert_eq!(formatting.action, Action::Fmt);
        assert!(formatting.format_check);
        for action in ["check", "build", "run"] {
            assert!(
                parse(&[action, "Main.tz", "--deny-warnings", "--json"])
                    .unwrap()
                    .deny_warnings
            );
        }
        let arguments = parse(&["build", "A.tz", "--target", "wasm32", "-O0"]).unwrap();
        assert_eq!(arguments.options.emit, Emit::Wasm);
        assert_eq!(arguments.options.optimization, 0);
        let arguments = parse(&["--", "-project/Main.tz"]).unwrap();
        assert_eq!(arguments.input, Path::new("-project/Main.tz"));
        assert!(parse(&["run", "Main.tz", "--target", "wasm32"]).is_err());
        assert_eq!(parse(&["run", "app"]).unwrap().input, Path::new("app"));
        assert_eq!(
            parse(&["run", "app", "--cpu", "native"])
                .unwrap()
                .options
                .cpu,
            Cpu::Native
        );
        assert_eq!(
            parse(&["build", "Main.tz"]).unwrap().options.cpu,
            Cpu::Generic
        );
    }

    #[test]
    fn rejects_ambiguous_or_unused_arguments() {
        for values in [
            vec!["build", "Main.tz", "--wasm-feature", "simd128"],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "relaxed-simd",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "simd128",
                "--wasm-feature",
                "simd128",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--emit",
                "header",
                "--wasm-feature",
                "simd128",
            ],
            vec!["check", "Main.tz", "--wasm-feature", "simd128"],
            vec!["run", "Main.tz", "--wasm-feature", "simd128"],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
        for values in [
            vec!["check"],
            vec!["check", "Main.tz", "--trap-info"],
            vec!["test", "Main.tz", "--trap-info"],
            vec!["build", "Main.tz", "--emit", "header", "--trap-info"],
            vec!["build", "Main.tz", "--trap-info", "--trap-info"],
            vec!["check", "Main.tz", "--debug-output"],
            vec!["test", "Main.tz", "--debug-output"],
            vec!["build", "Main.tz", "--emit", "header", "--debug-output"],
            vec!["check", "Main.tz", "--filter", "name"],
            vec!["test", "Main.tz", "--filter"],
            vec!["test", "Main.tz", "--filter", "a", "--filter", "b"],
            vec!["test", "Main.tz", "--cpu", "native"],
            vec!["test", "Main.tz", "--emit", "llvm"],
            vec!["test", "Main.tz", "-o", "output"],
            vec!["check", "Main.tz", "--check"],
            vec!["fmt", "Main.tz", "--check", "--check"],
            vec!["fmt", "Main.tz", "--deny-warnings"],
            vec!["fmt", "Main.tz", "-O0"],
            vec!["fmt", "Main.tz", "--cpu", "native"],
            vec!["fmt", "Main.tz", "-o", "Other.tz"],
            vec!["check", "Main.tz", "--deny-warnings", "--deny-warnings"],
            vec!["A.tz", "B.tz"],
            vec!["build", "Main.tz", "-O9"],
            vec!["build", "Main.tz", "--output"],
            vec!["build", "Main.tz", "--emit", "wasm"],
            vec!["check", "A.tz", "-O0"],
            vec!["run", "Main.tz", "-o", "app"],
            vec!["build", "Main.tz", "-O0", "-O3"],
            vec!["build", "Main.tz", "--cpu"],
            vec!["build", "Main.tz", "--cpu", "unsupported"],
            vec!["build", "Main.tz", "--cpu", "generic", "--cpu", "native"],
            vec!["check", "Main.tz", "--cpu", "generic"],
            vec!["build", "Main.tz", "--target", "wasm32", "--cpu", "native"],
            vec!["build", "Main.tz", "--emit", "llvm", "--cpu", "native"],
            vec!["build", "Main.tz", "--emit", "header", "--cpu", "native"],
            vec!["run", "Main.tz", "--wasm-max-memory", "64MiB"],
            vec!["check", "Main.tz", "--wasm-stack-size", "2MiB"],
            vec!["doc", "Main.tz", "-o", "docs", "--wasm-max-memory", "64MiB"],
            vec!["fmt", "Main.tz", "--wasm-stack-size", "2MiB"],
            vec!["test", "Main.tz", "--wasm-max-memory", "64MiB"],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-max-memory",
                "64MiB",
                "--wasm-max-memory",
                "64MiB",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-max-memory=64MiB",
            ],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
        let unused = "--wasm-max-memory and --wasm-stack-size are only valid with build or test";
        let untargeted =
            "--wasm-max-memory and --wasm-stack-size require --target wasm32 or wasm64";
        let output = "--wasm-max-memory requires wasm32 or wasm64 object, LLVM IR, or WASM output";
        let stack_output = "--wasm-stack-size requires WASM output; link object and LLVM IR output with wasm-ld -z stack-size";
        let memory_range =
            "--wasm-max-memory must be a multiple of 64 KiB and at most 4 GiB - 64 KiB on wasm32";
        let memory64_range =
            "--wasm-max-memory must be a multiple of 64 KiB and at most 16 GiB on wasm64";
        let stack_range = "--wasm-stack-size must be a multiple of 16 bytes and at least 64 KiB";
        let relation = "--wasm-max-memory must be at least the stack size plus 64 KiB; raise the memory limit or lower --wasm-stack-size";
        for (values, message) in [
            (vec!["run", "app", "--wasm-max-memory", "64MiB"], unused),
            (vec!["check", "app", "--wasm-stack-size", "2MiB"], unused),
            (
                vec!["doc", "app", "-o", "docs", "--wasm-max-memory", "64MiB"],
                unused,
            ),
            (vec!["fmt", "app", "--wasm-stack-size", "2MiB"], unused),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-max-memory",
                    "64MiB",
                    "--wasm-max-memory",
                    "128MiB",
                ],
                "WASM maximum memory specified more than once",
            ),
            (
                vec![
                    "test",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-stack-size",
                    "2MiB",
                    "--wasm-stack-size",
                    "2MiB",
                ],
                "WASM stack size specified more than once",
            ),
            (
                vec!["test", "app", "--wasm-max-memory", "64MiB"],
                untargeted,
            ),
            (
                vec![
                    "test",
                    "app",
                    "--target",
                    "native",
                    "--wasm-stack-size",
                    "2MiB",
                ],
                untargeted,
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-max-memory=64MiB",
                ],
                "unknown option '--wasm-max-memory=64MiB'; use --help",
            ),
            (
                vec!["build", "app", "--target", "wasm32", "--wasm-stack-size"],
                "--wasm-stack-size needs a value",
            ),
            (vec!["build", "app", "--wasm-max-memory", "64MiB"], output),
            (
                vec![
                    "build",
                    "app",
                    "--emit",
                    "wgsl",
                    "--wasm-max-memory",
                    "64MiB",
                ],
                output,
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--emit",
                    "header",
                    "--wasm-max-memory",
                    "64MiB",
                ],
                output,
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--emit",
                    "object",
                    "--wasm-stack-size",
                    "2MiB",
                ],
                stack_output,
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-max-memory",
                    "100000",
                ],
                memory_range,
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-max-memory",
                    "4GiB",
                ],
                memory_range,
            ),
            (
                vec![
                    "test",
                    "app",
                    "--target",
                    "wasm64",
                    "--wasm-max-memory",
                    "17GiB",
                ],
                memory64_range,
            ),
            (
                vec!["build", "app", "--target", "wasm128"],
                "target must be 'native', 'wasm32', or 'wasm64'",
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm64",
                    "--wasm-feature",
                    "threads",
                ],
                "--wasm-feature threads requires wasm32 object or WASM output",
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-max-memory",
                    "1MiB",
                ],
                relation,
            ),
            (
                vec![
                    "test",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-stack-size",
                    "16MiB",
                ],
                relation,
            ),
            (
                vec![
                    "build",
                    "app",
                    "--target",
                    "wasm32",
                    "--wasm-stack-size",
                    "1000",
                ],
                stack_range,
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
    }

    #[test]
    fn parses_wasm_memory_sizes() {
        for (text, bytes) in [
            ("0", 0),
            ("65536", 65536),
            ("4KiB", 4096),
            ("64MiB", 67108864),
            ("1GiB", 1073741824),
            ("17179869183GiB", 17179869183 << 30),
        ] {
            assert_eq!(
                parse_size(OsStr::new(text), "--wasm-max-memory"),
                Ok(bytes),
                "{text}"
            );
        }
        for text in [
            "64MB",
            "64 MiB",
            "",
            "0x10",
            "1.5MiB",
            "+64MiB",
            "-1",
            "MiB",
            "64mib",
            "64MiB ",
            "18446744073709551616",
            "17179869184GiB",
        ] {
            assert_eq!(
                parse_size(OsStr::new(text), "--wasm-stack-size").unwrap_err(),
                "--wasm-stack-size must be a byte count or a number followed by KiB, MiB, or GiB, such as 4194304 or 4MiB",
                "{text}"
            );
        }
        assert_eq!(
            parse(&[
                "build",
                "app",
                "--target",
                "wasm32",
                "--wasm-max-memory",
                "64MB"
            ])
            .unwrap_err(),
            "--wasm-max-memory must be a byte count or a number followed by KiB, MiB, or GiB, such as 67108864 or 64MiB"
        );
        let build = parse(&[
            "build",
            "app",
            "--target",
            "wasm32",
            "--wasm-max-memory",
            "256MiB",
            "--wasm-stack-size",
            "4MiB",
        ])
        .unwrap();
        assert_eq!(
            (build.options.wasm_max_memory, build.options.wasm_stack_size),
            (Some(268435456), Some(4194304))
        );
        let test = parse(&[
            "test",
            "app",
            "--wasm-stack-size",
            "65536",
            "--target",
            "wasm32",
            "--wasm-max-memory",
            "2GiB",
        ])
        .unwrap();
        assert_eq!(test.action, Action::Test);
        assert_eq!(
            (test.options.wasm_max_memory, test.options.wasm_stack_size),
            (Some(2147483648), Some(65536))
        );
        for emit in ["object", "llvm"] {
            let arguments = parse(&[
                "build",
                "app",
                "--target",
                "wasm32",
                "--emit",
                emit,
                "--wasm-max-memory",
                "64MiB",
            ])
            .unwrap();
            assert_eq!(arguments.options.wasm_max_memory, Some(67108864));
        }
        let threads = parse(&[
            "build",
            "app",
            "--target",
            "wasm32",
            "--wasm-feature",
            "threads",
            "--wasm-max-memory",
            "1GiB",
        ])
        .unwrap();
        assert_eq!(threads.options.wasm_max_memory, Some(1073741824));
        let defaults = parse(&["build", "app", "--target", "wasm32"]).unwrap();
        assert_eq!(
            (
                defaults.options.wasm_max_memory,
                defaults.options.wasm_stack_size
            ),
            (None, None)
        );
        let wasm64 = parse(&[
            "build",
            "app",
            "--target",
            "wasm64",
            "--wasm-max-memory",
            "16GiB",
        ])
        .unwrap();
        assert_eq!(
            (
                wasm64.options.target,
                wasm64.options.emit,
                wasm64.options.wasm_max_memory
            ),
            (Target::Wasm64, Emit::Wasm, Some(17179869184))
        );
        let near_4_gib = parse(&[
            "test",
            "app",
            "--target",
            "wasm32",
            "--wasm-max-memory",
            "4194240KiB",
        ])
        .unwrap();
        assert_eq!(near_4_gib.options.wasm_max_memory, Some(4294901760));
    }
}
