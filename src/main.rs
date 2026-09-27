use std::env;
use std::ffi::OsString;
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
    tsuzuri test source.tz|directory [--filter TEXT] [--json] [-O0|-O1|-O2|-O3]
                             [--target native|wasm32]
  tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
  tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]

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
  --target native|wasm32  Target (default: native)
    --wasm-feature <name>   Opt in to simd128 or threads (wasm32 build only)
    --emit KIND            exe, object, llvm, header, wasm, or wgsl
                         Default: exe for native, wasm for wasm32
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
    let mut debug_output = false;
    let mut trap_info = false;
    let mut debug_info = false;
    let mut wasm_simd = false;
    let mut wasm_threads = false;
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
                            _ => return Err("target must be 'native' or 'wasm32'".into()),
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
    if action != Action::Test && test_filter.is_some() {
        return Err("--filter is only valid with test".into());
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
        emit: emit.unwrap_or(if target == Target::Wasm32 {
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
        Action::Run => driver::run(module, project, arguments.options),
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
    let report = match driver::run_tests(
        module,
        &driver::TestOptions {
            target: arguments.options.target,
            optimization: arguments.options.optimization,
            filter: arguments.test_filter.clone(),
        },
    ) {
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
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
    }
}
