use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tsuzuri::diagnostic::{Diagnostic, Span, json_string};
use tsuzuri::driver::{self, BuildOptions, Cpu, Emit, Project, Target};

const HELP: &str = "\
Tsuzuri - a statically typed language with ownership, powered by LLVM

Usage:
  tsuzuri check source.tz|source.tt|source.tc|directory [--json]
  tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
  tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]

Each source file is one module named after its filename:
  .tz  Code (records, unions, functions, and type class instances)
  .tt  Type class declarations (multiple classes per file)
  .tc  One computation expression builder (its operations and helpers)
All sibling .tz, .tt, and .tc files are loaded together.
Applications start in Main.tz; a directory selects it.
Other source inputs can be checked or built as libraries.

Build options:
  -o, --output PATH       Output path (defaults to the input with a new extension)
  --target native|wasm32  Target (default: native)
  --emit KIND            exe, object, llvm, header, or wasm
                         Default: exe for native, wasm for wasm32
  -O0, -O1, -O2, -O3    LLVM optimization level (default: -O3; no fast-math)
  --cpu generic|native   CPU tuning for native build/run (default: generic)
                         native uses this machine's ISA; not portable to older CPUs
  --json                 Emit machine-readable diagnostics on stderr
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
    Check,
    Build,
    Run,
}

#[derive(Debug)]
struct Arguments {
    action: Action,
    input: PathBuf,
    output: Option<PathBuf>,
    options: BuildOptions,
    json: bool,
}

fn parse_arguments(arguments: &[OsString]) -> Result<Arguments, String> {
    let mut position = 0;
    let action = match arguments.first().and_then(|value| value.to_str()) {
        Some("check") => {
            position = 1;
            Action::Check
        }
        Some("build") => {
            position = 1;
            Action::Build
        }
        Some("run") => {
            position = 1;
            Action::Run
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
                            _ => {
                                return Err(
                                    "emit kind must be exe, object, llvm, header, or wasm".into()
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
                "pass one .tz, .tt, or .tc file or project directory; sibling modules are loaded automatically"
                    .into(),
            );
        }
    }
    let input = input.ok_or("missing .tz, .tt, or .tc input or project directory; use --help")?;
    if action != Action::Build && (output.is_some() || target.is_some() || emit.is_some()) {
        return Err("--output, --target, and --emit are build-only options".into());
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
        optimization: optimization.unwrap_or(3),
        cpu: cpu.unwrap_or(Cpu::Generic),
    };
    options.validate().map_err(|error| error.message)?;
    Ok(Arguments {
        action,
        input,
        output,
        options,
        json,
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
    print_with_severity("error", error, input, source, json);
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
        Action::Check => Ok(Vec::new()),
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
    let project = match Project::load(&arguments.input) {
        Ok(project) => project,
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", arguments.json);
            return ExitCode::FAILURE;
        }
    };
    let result = project.analyze().and_then(|module| {
        for warning in &module.warnings {
            let source = project.source_for(warning);
            print_with_severity(
                "warning",
                warning,
                &source.path,
                &source.text,
                arguments.json,
            );
        }
        run_action(&arguments, &project, &module)
    });
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
    fn selects_target_defaults_and_honors_path_separator() {
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
            vec!["check"],
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
