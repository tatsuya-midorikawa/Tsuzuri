use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tsuzuri::diagnostic::{Diagnostic, Span, json_string};
use tsuzuri::driver::{self, BuildOptions, Cpu, Emit, Project, Target, WasmHost};
use tsuzuri::llvm::Allocator;

const HELP: &str = "\
Tsuzuri - a statically typed language with ownership, powered by LLVM

Usage:
    tsuzuri lsp
  tsuzuri check source.tz|source.tt|source.tc|directory [--json]
    tsuzuri doc source.tz|source.tt|source.tc|directory -o outdir [--json]
    tsuzuri fmt [--check] source.tz|source.tt|source.tc|directory [--json]
    tsuzuri test source.tz|directory [--list] [--filter TEXT] [--index N] [--json] [-O0|-O1|-O2|-O3]
                             [--target native|wasm32|wasm64] [--wasm-max-memory SIZE] [--wasm-stack-size SIZE]
                             [--coverage PATH] [--seed N]
    tsuzuri test source.tz|directory --index N -g -o PATH [-O0|-O1|-O2|-O3] [--seed N] [--json]
                             Build test N alone with debug information as PATH without running it;
                             a debugger starts PATH with the printed argument 0
    tsuzuri bench source.tz|directory [--list] [--filter TEXT] [--index N] [--json] [--samples N]
                              [-O0|-O1|-O2|-O3] [--target native]
  tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
  tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]
  tsuzuri repl [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--timeout SECONDS]
  tsuzuri script [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--json] FILE [arguments...]
  tsuzuri new directory [--namespace NAME]
  tsuzuri fetch directory [--json]
  tsuzuri publish directory --git URL --rev COMMIT [--json]
  tsuzuri bindgen header.h -o Module.tz [--include-dir DIR]... [--buffer F:P:L]...
                    [--consume F:P]... [--json]
  tsuzuri toolchain info

Each source file is one module named after its filename:
  .tz  Code (records, unions, functions, and type class instances)
  .tt  Type class declarations (multiple classes per file)
  .tc  One computation expression builder (its operations and helpers)
All .tz, .tt, and .tc files below the project root are loaded recursively.
Module filenames start with an uppercase ASCII letter. A module's full name is
its namespace and filename joined by '::': 'namespace Sample::Shapes' as a
file's first declaration sets the namespace, and 'using Sample::Features'
lines after it let the file name that namespace's modules without the
namespace. Otherwise the namespace is the package namespace (Tsuzuri.toml
namespace, else the package or folder name) followed by subdirectories
(Geometry/Point.tz becomes App::Geometry::Point). Members follow a module with
'.', as in Sample::Shapes::Circle.area.
`tsuzuri test --coverage PATH` writes the line and function coverage of the passing
tests as an lcov file (native only) and prints a summary after the test summary.
`tsuzuri test --seed N` sets the seed of property tests (Gen.for_all); without it they
use the fixed seed 11400714819323198485, so every run tries the same values.
`tsuzuri bench` runs each `bench \"name\" = body` declaration (body: i64 -> i64, iterations to
nanoseconds; see Bench.with_input and Bench.of) in its own native process, one at a time: it
doubles the iterations until a sample takes 10 ms, discards one warm-up sample, and prints the
median, minimum, and maximum time per iteration of --samples samples (default 11, at most 1000)
at -O3 by default. It has no pass or fail threshold.
`tsuzuri new` creates Tsuzuri.toml, Main.tz, and .gitignore in an empty folder.
`tsuzuri fetch` downloads the git dependencies of Tsuzuri.toml
({ git = \"https://...\", rev = \"<40-hex commit>\" }) and the registry dependencies
({ version = \"1.2.3\" }, the minimal versions that satisfy every requirement in the
[registry] index) with git 2.32 or later into the package store (packages/ in the
build cache) and records them in Tsuzuri.lock. Other commands never run git or
use the network; they read Tsuzuri.lock and the store. `tsuzuri publish` checks a
package and prints the registry index entry of its commit; it changes no registry.
`tsuzuri bindgen` writes extern declarations, constants (also from integer #defines),
records, opaque extern types, and type aliases for the C header's own declarations
whose ABI matches exactly (64-bit Linux and macOS, with TSUZURI_CLANG); it reports each
other declaration as W2002 and a '// skipped' line. --buffer FUNC:PTR:LEN makes a
pointer and the length after it one 'ref [T]' parameter; --consume FUNC:PARAM moves an
opaque handle into the call instead of borrowing it.
`tsuzuri repl` reads declarations, top-level lets, and expressions from stdin one at a
time and prints each new let's type and each expression's type and value. Every input
is checked and run as a new program after the accepted ones (default -O0; --timeout 10
seconds per run, 0 for none). Its commands are :type, :load, :list, :reset, and :quit.
`tsuzuri script` runs one file of any name as the Main.tz of a project without other
files or dependencies, like `tsuzuri run`, and passes the arguments after the file to
'def main :: Array<string> -> i32'. A '#!' line at the start of any source file is a
comment, so a file that starts with '#!/usr/bin/env -S tsuzuri script' can run itself.
File inputs use their parent as the root; directory inputs use that directory.
Applications start in Main.tz; a directory selects it.
Other source inputs can be checked or built as libraries.

Build options:
  -o, --output PATH       Output path (defaults to the input with a new extension)
  --target native|wasm32|wasm64  Target (default: native; wasm64 uses 64-bit memory)
    --wasm-feature <name>   Opt in to simd128 (WASM build), threads (wasm32 build), or jspi
                            (WASM build; Async.block_on waits through JavaScript Promise
                            Integration)
    --wasm-host wasi        Lower the standard IO and the File, Dir, Env, Time, Random, and
                            Process APIs to WASI preview1 (wasm32 object or WASM output;
                            the default wasm32 output rejects those APIs)
    --wasm-max-memory SIZE  WASM linear memory limit (WASM build/test; default 16MiB,
                            at most 4GiB-64KiB on wasm32 and 16GiB on wasm64)
    --wasm-stack-size SIZE  WASM main stack size (WASM output/test; default 1MiB)
                            Tsuzuri.toml [wasm] max-memory/stack-size set project defaults
    --emit KIND            exe, object, llvm, header, wasm, wgsl, shared, bindings-js,
                         bindings-cs, bindings-py, or bindings-cpp
                         Default: exe for native, wasm for wasm32 and wasm64
                         shared links a native .dylib or .so that exports the C ABI
                         bindings-js (with --target wasm32) writes a JavaScript module
                         NAME.mjs and its TypeScript declarations NAME.d.mts
                         bindings-cs, bindings-py, and bindings-cpp write C#, Python
                         ctypes, and C++20 bindings of the shared library NAME
  -O0, -O1, -O2, -O3    LLVM optimization level (default: -O3; no fast-math)
  --cpu generic|native   CPU tuning for native build/run (default: generic)
                         native uses this machine's ISA; not portable to older CPUs
  --link PATH            Link a host object or static library into a native executable
  -l NAME                Link a system library by name, such as -l sqlite3
  -L DIR                 Search DIR for system libraries
                         These three repeat and apply to build, run, and test of native
                         executables; Tsuzuri.toml [native] link/libraries/search come first
  --json                 Emit machine-readable diagnostics on stderr
    --no-cache             Disable cache reads and writes: artifacts and parsed sources for
                           build/run/script, artifacts for repl
    -g, --debug-info        Emit source-level DWARF debug information (Windows links with the
                            MSVC linker also emit CodeView and write OUTPUT.pdb with natvis views)
    --deny-warnings        Fail check/build/run before code generation on warnings
    --warn implicit-copy   Report W1006 at implicit copies of arrays and lists
    --debug-output         Enable WASM Debug output imports (native always writes)
    --trap-info            Report trap locations and emit an output.trap.json table
                                                 Enabled by default for run; disabled by default for build
    --trap-mode return     Native object, llvm or header output: each export also comes as
                         tsuzuri_try_<name>, which returns a status and a trap record instead
                         of ending the process (implies --trap-info for object and llvm)
    --allocator system|host|counting
                         Heap of object, llvm, header, or WASM output (default: system).
                         host calls tsuzuri_host_alloc/free/realloc, which the host defines
                         (WASM imports them from tsuzuri_heap); counting adds the counts that
                         tsuzuri_alloc_stats reports
    --freestanding         Native object, llvm, or header output that needs no C library;
                         requires --allocator host and rejects IO, OS APIs, Net sockets, tasks, and Debug
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
the numeric, bool, or UTF-8 string result of Main.tz's top-level code; unit
results do not print anything. 'def main :: unit -> i32' and
'def main :: Array<string> -> i32' print nothing and return the exit code; the
Array<string> form receives the command-line arguments.
Standard input and output and the File, Dir, Env, Time, Random, and Process APIs are
built in on native, and so are the Net sockets on macOS and Linux; a GUI belongs to the
host, not the language.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Lsp,
    Check,
    Doc,
    Build,
    Run,
    Fmt,
    Test,
    Bench,
}

#[derive(Debug)]
struct Arguments {
    action: Action,
    input: PathBuf,
    output: Option<PathBuf>,
    options: BuildOptions,
    links: driver::LinkInputs,
    json: bool,
    deny_warnings: bool,
    warn_implicit_copy: bool,
    format_check: bool,
    test_filter: Option<String>,
    test_list: bool,
    test_indices: Vec<usize>,
    /// `tsuzuri test --coverage PATH`: where the lcov report goes.
    coverage: Option<PathBuf>,
    /// `tsuzuri bench --samples N`.
    bench_samples: Option<usize>,
    /// `tsuzuri test --seed N`: the property-test seed.
    test_seed: Option<u64>,
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
            links: driver::LinkInputs::default(),
            json: false,
            deny_warnings: false,
            warn_implicit_copy: false,
            format_check: false,
            test_filter: None,
            test_list: false,
            test_indices: Vec::new(),
            coverage: None,
            bench_samples: None,
            test_seed: None,
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
        Some("bench") => {
            position = 1;
            Action::Bench
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
    let mut warn_implicit_copy = false;
    let mut format_check = false;
    let mut test_filter = None;
    let mut test_list = false;
    let mut test_indices = Vec::new();
    let mut coverage = None;
    let mut bench_samples = None;
    let mut test_seed = None;
    let mut debug_output = false;
    let mut trap_info = false;
    let mut trap_return = false;
    let mut allocator = None;
    let mut freestanding = false;
    let mut debug_info = false;
    let mut wasm_simd = false;
    let mut wasm_threads = false;
    let mut wasm_jspi = false;
    let mut wasm_host = None;
    let mut wasm_max_memory = None;
    let mut wasm_stack_size = None;
    let mut no_cache = false;
    let mut links = driver::LinkInputs::default();
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
                Some("--warn") => {
                    let Some(name) = arguments.get(position) else {
                        return Err(
                            "--warn requires a warning name; supported: implicit-copy".into()
                        );
                    };
                    position += 1;
                    if name != "implicit-copy" {
                        return Err(format!(
                            "unknown warning '{}' for --warn; supported: implicit-copy",
                            name.to_string_lossy()
                        ));
                    }
                    if warn_implicit_copy {
                        return Err("warn implicit-copy specified more than once".into());
                    }
                    warn_implicit_copy = true;
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
                Some("--samples") => {
                    if bench_samples.is_some() {
                        return Err("samples specified more than once".into());
                    }
                    bench_samples = Some(
                        next_value(arguments, &mut position, "--samples")?
                            .to_str()
                            .filter(|value| {
                                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                            })
                            .and_then(|value| value.parse::<usize>().ok())
                            .filter(|samples| (1..=1000).contains(samples))
                            .ok_or("bench samples must be an integer between 1 and 1000")?,
                    );
                    continue;
                }
                Some("--seed") => {
                    if test_seed.is_some() {
                        return Err("seed specified more than once".into());
                    }
                    test_seed = Some(
                        next_value(arguments, &mut position, "--seed")?
                            .to_str()
                            .filter(|value| {
                                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                            })
                            .and_then(|value| value.parse::<u64>().ok())
                            .ok_or(
                                "property seed must be an integer between 0 and 18446744073709551615",
                            )?,
                    );
                    continue;
                }
                Some("--coverage") => {
                    if coverage.is_some() {
                        return Err("coverage specified more than once".into());
                    }
                    coverage = Some(PathBuf::from(next_value(
                        arguments,
                        &mut position,
                        "--coverage",
                    )?));
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
                Some("--trap-mode") => {
                    if trap_return {
                        return Err("trap mode specified more than once".into());
                    }
                    match next_value(arguments, &mut position, "--trap-mode")?.to_str() {
                        Some("return") => trap_return = true,
                        _ => return Err("trap mode must be 'return'".into()),
                    }
                    continue;
                }
                Some("--allocator") => {
                    if allocator.is_some() {
                        return Err("allocator specified more than once".into());
                    }
                    allocator = Some(
                        match next_value(arguments, &mut position, "--allocator")?.to_str() {
                            Some("system") => Allocator::System,
                            Some("host") => Allocator::Host,
                            Some("counting") => Allocator::Counting,
                            _ => {
                                return Err(
                                    "allocator must be 'system', 'host', or 'counting'".into()
                                );
                            }
                        },
                    );
                    continue;
                }
                Some("--freestanding") => {
                    if freestanding {
                        return Err("freestanding specified more than once".into());
                    }
                    freestanding = true;
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
                        Some("jspi") => &mut wasm_jspi,
                        _ => return Err("supported WASM features are 'simd128', 'threads', and 'jspi'; relaxed SIMD is not supported".into()),
                    };
                    if *feature {
                        return Err("WASM feature specified more than once".into());
                    }
                    *feature = true;
                    continue;
                }
                Some("--wasm-host") => {
                    if wasm_host.is_some() {
                        return Err("--wasm-host specified more than once".into());
                    }
                    wasm_host = Some(
                        match next_value(arguments, &mut position, "--wasm-host")?.to_str() {
                            Some("wasi") => WasmHost::Wasi,
                            other => {
                                return Err(format!(
                                    "unknown wasm host '{}'; the supported host is wasi",
                                    other.unwrap_or("?")
                                ));
                            }
                        },
                    );
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
                            Some("shared") => Emit::Shared,
                            Some("bindings-js") => Emit::BindingsJs,
                            Some("bindings-cs") => Emit::BindingsCs,
                            Some("bindings-py") => Emit::BindingsPy,
                            Some("bindings-cpp") => Emit::BindingsCpp,
                            _ => {
                                return Err(
                                    "emit kind must be exe, object, llvm, header, wasm, wgsl, shared, bindings-js, bindings-cs, bindings-py, or bindings-cpp"
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
                Some("--link") => {
                    links.paths.push(PathBuf::from(next_value(
                        arguments,
                        &mut position,
                        "--link",
                    )?));
                    continue;
                }
                Some("-L") => {
                    links
                        .search
                        .push(PathBuf::from(next_value(arguments, &mut position, "-L")?));
                    continue;
                }
                Some("-l") => {
                    links.libraries.push(
                        next_value(arguments, &mut position, "-l")?
                            .to_str()
                            .ok_or("library names must be UTF-8")?
                            .to_owned(),
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
        return Err("--no-cache is only valid with build, run, script, or repl".into());
    }
    if emit == Some(Emit::Wgsl) && (target.is_some() || optimization.is_some() || cpu.is_some()) {
        return Err("WGSL output does not use target, optimization, or CPU options".into());
    }
    if (wasm_simd || wasm_threads || wasm_jspi) && action != Action::Build {
        return Err("--wasm-feature is only valid with build".into());
    }
    if wasm_host.is_some() && action != Action::Build {
        return Err("--wasm-host is only valid with build".into());
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
    if debug_info && !matches!(action, Action::Build | Action::Run | Action::Test) {
        return Err("--debug-info is only valid with build, run, or test".into());
    }
    // `test --index N -g -o PATH` builds the runner of one test for a debugger (G16 Phase 2).
    if action == Action::Test && (debug_info || output.is_some()) {
        if !debug_info || output.is_none() {
            return Err("debugging a test needs both -g and -o with the runner's path".into());
        }
        if test_indices.len() != 1 || test_list || test_filter.is_some() {
            return Err(
                "debugging a test needs exactly one --index, without --list or --filter".into(),
            );
        }
        if target.is_some_and(Target::is_wasm) {
            return Err("debugging a test requires the native target".into());
        }
    }
    if debug_output && !matches!(action, Action::Build | Action::Run) {
        return Err("--debug-output is only valid with build or run".into());
    }
    if trap_info && !matches!(action, Action::Build | Action::Run) {
        return Err("--trap-info is only valid with build or run".into());
    }
    if trap_return && action != Action::Build {
        return Err("--trap-mode is only valid with build".into());
    }
    if allocator.is_some() && action != Action::Build {
        return Err("--allocator is only valid with build".into());
    }
    if freestanding && action != Action::Build {
        return Err("--freestanding is only valid with build".into());
    }
    if format_check && action != Action::Fmt {
        return Err("--check is only valid with fmt".into());
    }
    if action == Action::Fmt
        && (optimization.is_some() || cpu.is_some() || deny_warnings || warn_implicit_copy)
    {
        return Err(
            "fmt does not use optimization, CPU tuning, or compiler warning options".into(),
        );
    }
    if !matches!(action, Action::Test | Action::Bench)
        && (test_filter.is_some() || test_list || !test_indices.is_empty())
    {
        return Err("--filter, --list, and --index are only valid with test or bench".into());
    }
    if action == Action::Test && cpu.is_some() {
        return Err("test does not use CPU tuning".into());
    }
    if bench_samples.is_some() && action != Action::Bench {
        return Err("--samples is only valid with bench".into());
    }
    if test_seed.is_some() && action != Action::Test {
        return Err("--seed is only valid with test".into());
    }
    if action == Action::Bench {
        if cpu.is_some() {
            return Err("bench does not use CPU tuning".into());
        }
        if target.is_some_and(Target::is_wasm) {
            return Err("tsuzuri bench supports only the native target".into());
        }
    }
    if coverage.is_some() {
        if action != Action::Test {
            return Err("--coverage is only valid with test".into());
        }
        if target.is_some_and(Target::is_wasm) {
            return Err("test coverage supports only the native target".into());
        }
        if test_list {
            return Err("coverage cannot be combined with --list".into());
        }
        if debug_info {
            return Err("coverage cannot be combined with -g; a debug runner only builds".into());
        }
    }
    if output.is_some() && !matches!(action, Action::Build | Action::Doc | Action::Test)
        || emit.is_some() && action != Action::Build
        || target.is_some() && !matches!(action, Action::Build | Action::Test | Action::Bench)
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
        trap_info: trap_info
            || action == Action::Run
            || (trap_return
                && !emit.is_some_and(|emit| emit == Emit::Header || emit.is_bindings())),
        debug_info,
        wasm_simd,
        wasm_threads,
        wasm_jspi,
        wasm_host,
        wasm_max_memory,
        wasm_stack_size,
        cache: !no_cache,
        trap_return,
        allocator: allocator.unwrap_or_default(),
        freestanding,
    };
    options.validate().map_err(|error| error.message)?;
    if let Some(error) = output
        .as_deref()
        .and_then(|output| driver::bindings_output_error(options.emit, output))
    {
        return Err(error.message);
    }
    links.check_shape().map_err(|error| error.message)?;
    if !links.is_empty() && !links_apply(action, &options) {
        return Err("link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe".into());
    }
    Ok(Arguments {
        action,
        input,
        output,
        options,
        links,
        json,
        deny_warnings,
        warn_implicit_copy,
        format_check,
        test_filter,
        test_list,
        test_indices,
        coverage,
        bench_samples,
        test_seed,
    })
}

/// Whether the action links a native executable, the only output that takes host link inputs.
fn links_apply(action: Action, options: &BuildOptions) -> bool {
    match action {
        Action::Run => true,
        Action::Build => {
            options.target == Target::Native
                && matches!(options.emit, Emit::Executable | Emit::Shared)
        }
        Action::Test | Action::Bench => options.target == Target::Native,
        _ => false,
    }
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
    links: &driver::LinkInputs,
    program_arguments: &[OsString],
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
        Action::Bench => unreachable!("benchmarks use an isolated runner"),
        Action::Build => driver::build_linked(
            module,
            project,
            &arguments
                .output
                .clone()
                .unwrap_or_else(|| arguments.options.output_path(project.input())),
            arguments.options,
            links,
        ),
        Action::Run => driver::run_with_arguments(
            module,
            project,
            arguments.options,
            links,
            arguments.json,
            program_arguments,
        ),
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
    links: &driver::LinkInputs,
) -> ExitCode {
    let options = driver::TestOptions {
        target: arguments.options.target,
        optimization: arguments.options.optimization,
        filter: arguments.test_filter.clone(),
        indices: arguments.test_indices.clone(),
        wasm_max_memory: arguments.options.wasm_max_memory,
        wasm_stack_size: arguments.options.wasm_stack_size,
        coverage: arguments.coverage.is_some(),
        seed: arguments.test_seed,
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
    if arguments.options.debug_info {
        return debug_test(arguments, project, module, &options, links);
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
    if let Some(output) = &arguments.coverage
        && let Err(error) = driver::check_coverage_output(project, output)
    {
        print_diagnostic(&error, output, "", arguments.json);
        return ExitCode::FAILURE;
    }
    let report = match driver::run_tests_linked(module, &options, links) {
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
                format!(
                    ",\"failure\":{}{}",
                    json_string(failure),
                    output_field(&result.output)
                )
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
                print_output(&result.output);
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
    let mut coverage_failed = false;
    if let (Some(output), Some(coverage)) = (&arguments.coverage, &report.coverage) {
        match driver::write_coverage(project, coverage, output) {
            Ok(files) => print_coverage(&files, coverage.excluded_failed, arguments.json),
            Err(error) => {
                print_diagnostic(&error, output, "", arguments.json);
                coverage_failed = true;
            }
        }
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
    } else if coverage_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Builds the runner of one test for a debugger and prints how to start it (G16 Phase 2).
fn debug_test(
    arguments: &Arguments,
    project: &Project,
    module: &tsuzuri::check::CheckedModule,
    options: &driver::TestOptions,
    links: &driver::LinkInputs,
) -> ExitCode {
    let output = arguments
        .output
        .as_ref()
        .expect("debugging a test requires output");
    let runner = match driver::build_debug_runner(module, project, options, links, output) {
        Ok(runner) => runner,
        Err(error) => {
            let source = project.source_for(&error);
            print_diagnostic(&error, &source.path, &source.text, arguments.json);
            return ExitCode::FAILURE;
        }
    };
    for message in &runner.messages {
        if arguments.json {
            eprintln!(
                "{{\"severity\":\"warning\",\"code\":\"W2001\",\"message\":{}}}",
                json_string(message.trim())
            );
        } else {
            eprintln!("{}", message.trim());
        }
    }
    let case = &runner.case;
    if arguments.json {
        println!(
            "{}",
            serde_json::json!({
                "type": "debug", "index": case.index, "module": case.module, "name": case.name,
                "program": runner.program.to_string_lossy(), "arguments": runner.arguments,
            })
        );
    } else {
        println!(
            "{} {}.{}: {} {}",
            case.index,
            case.module,
            case.name.escape_debug(),
            runner.program.display(),
            runner.arguments.join(" ")
        );
    }
    ExitCode::SUCCESS
}

/// `tsuzuri bench`: measures each selected bench in its own process, one at a time, and prints
/// the median, minimum, and maximum time per iteration. No result fails on speed.
fn run_bench_action(
    arguments: &Arguments,
    project: &Project,
    module: &tsuzuri::check::CheckedModule,
    links: &driver::LinkInputs,
) -> ExitCode {
    let options = driver::BenchOptions {
        target: arguments.options.target,
        optimization: arguments.options.optimization,
        filter: arguments.test_filter.clone(),
        indices: arguments.test_indices.clone(),
        samples: arguments
            .bench_samples
            .unwrap_or(driver::DEFAULT_BENCH_SAMPLES),
    };
    if options
        .indices
        .iter()
        .any(|index| *index >= module.benches.len())
    {
        print_diagnostic(
            &Diagnostic::new(
                "E2000",
                "bench index is out of range; refresh the bench list",
                Span::default(),
            ),
            project.input(),
            "",
            arguments.json,
        );
        return ExitCode::FAILURE;
    }
    if arguments.test_list {
        for case in module.benches.iter().filter(|case| options.includes(case)) {
            if arguments.json {
                let source = project.source_for(&Diagnostic::new("E2000", "", case.span));
                let mapper = tsuzuri::lsp::PositionMapper::new(
                    &source.text,
                    tsuzuri::lsp::PositionEncoding::Utf16,
                );
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "bench", "index": case.index, "module": case.module,
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
    let report = match driver::run_benches_linked(module, &options, links) {
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
    let number = |value: f64| serde_json::to_string(&value).unwrap_or_else(|_| "null".into());
    for result in &report.results {
        let case = &result.case;
        let qualified = format!("{}.{}", case.module, case.name);
        match (result.statistics(), &result.failure) {
            (Some((median, min, max)), None) => {
                if arguments.json {
                    let samples = result
                        .samples_ms
                        .iter()
                        .map(|sample| number(*sample))
                        .collect::<Vec<_>>()
                        .join(",");
                    println!(
                        "{{\"type\":\"bench\",\"index\":{},\"module\":{},\"name\":{},\"status\":\"passed\",\"workload\":{},\"target\":\"native\",\"opt\":\"O{}\",\"cpu_mode\":\"generic\",\"metric\":\"wall_time\",\"unit\":\"ms\",\"iterations\":{},\"samples\":[{samples}],\"median\":{},\"min\":{},\"max\":{}}}",
                        case.index,
                        json_string(&case.module),
                        json_string(&case.name),
                        json_string(&qualified),
                        options.optimization,
                        result.iterations,
                        number(median),
                        number(min),
                        number(max)
                    );
                } else {
                    let (scale, unit) = bench_unit(median);
                    println!(
                        "bench {} {}: median {:.3} {unit} (min {:.3} {unit}, max {:.3} {unit}; {} samples of {} iterations)",
                        case.index,
                        qualified.escape_debug(),
                        median * scale,
                        min * scale,
                        max * scale,
                        result.samples_ms.len(),
                        result.iterations
                    );
                }
            }
            _ => {
                let failure = result.failure.as_deref().unwrap_or("produced no samples");
                if arguments.json {
                    println!(
                        "{{\"type\":\"bench\",\"index\":{},\"module\":{},\"name\":{},\"status\":\"failed\",\"failure\":{}{}}}",
                        case.index,
                        json_string(&case.module),
                        json_string(&case.name),
                        json_string(failure),
                        output_field(&result.output)
                    );
                } else {
                    println!(
                        "bench {} {}: failed ({failure})",
                        case.index,
                        qualified.escape_debug()
                    );
                    print_output(&result.output);
                }
            }
        }
    }
    let failed: Vec<_> = report
        .results
        .iter()
        .filter(|result| result.failure.is_some())
        .collect();
    if arguments.json {
        println!(
            "{{\"type\":\"summary\",\"benchmarks\":{},\"failed\":{},\"ignored\":{}}}",
            report.results.len(),
            failed.len(),
            report.ignored
        );
    } else {
        println!(
            "\n{} benchmark{}; {} failed; {} ignored",
            report.results.len(),
            if report.results.len() == 1 { "" } else { "s" },
            failed.len(),
            report.ignored
        );
    }
    for result in &failed {
        let diagnostic = Diagnostic::new(
            "E2005",
            format!(
                "benchmark '{}.{}' {}",
                result.case.module,
                result.case.name,
                result.failure.as_deref().unwrap_or_default()
            ),
            result.case.span,
        );
        let source = project.source_for(&diagnostic);
        print_diagnostic(&diagnostic, &source.path, &source.text, arguments.json);
    }
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The unit for a time per iteration of `median` milliseconds, and its factor from milliseconds.
fn bench_unit(median: f64) -> (f64, &'static str) {
    if median < 1e-3 {
        (1e6, "ns")
    } else if median < 1.0 {
        (1e3, "us")
    } else if median < 1e3 {
        (1.0, "ms")
    } else {
        (1e-3, "s")
    }
}

/// The `"output"` field of a failed test or bench in JSON, if it wrote to stderr.
fn output_field(output: &str) -> String {
    if output.is_empty() {
        String::new()
    } else {
        format!(",\"output\":{}", json_string(output))
    }
}

/// The standard error of a failed test or bench, indented under its failure line.
fn print_output(output: &str) {
    for line in output.lines() {
        println!("  {line}");
    }
}

/// Prints the `--coverage` summary after the test summary: one text line or one JSON line.
fn print_coverage(files: &[tsuzuri::coverage::FileCoverage], excluded_failed: usize, json: bool) {
    let totals = tsuzuri::coverage::totals(files);
    if !json {
        println!(
            "{}",
            tsuzuri::coverage::render_summary(totals, excluded_failed)
        );
        return;
    }
    let files = files
        .iter()
        .map(|file| {
            format!(
                "{{\"path\":{},\"lines\":{{\"hit\":{},\"total\":{}}},\"functions\":{{\"hit\":{},\"total\":{}}}}}",
                json_string(&file.path),
                file.lines_hit(),
                file.lines.len(),
                file.functions_hit(),
                file.functions.len()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"type\":\"coverage\",\"lines\":{{\"hit\":{},\"total\":{}}},\"functions\":{{\"hit\":{},\"total\":{}}},\"excluded_failed\":{excluded_failed},\"files\":[{files}]}}",
        totals.lines_hit, totals.lines, totals.functions_hit, totals.functions
    );
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

/// `tsuzuri new directory [--namespace NAME]`: creates a package whose
/// manifest and Main.tz declare its namespace.
fn new_project(arguments: &[OsString]) -> ExitCode {
    let mut directory = None;
    let mut namespace = None;
    let mut rest = arguments.iter();
    let mut valid = true;
    while let Some(argument) = rest.next() {
        if argument == "--namespace" && namespace.is_none() {
            namespace = rest
                .next()
                .and_then(|value| value.to_str())
                .map(str::to_owned);
            valid &= namespace.is_some();
        } else if directory.is_none() && !argument.to_string_lossy().starts_with('-') {
            directory = Some(PathBuf::from(argument));
        } else {
            valid = false;
        }
    }
    let Some(directory) = directory.filter(|_| valid) else {
        print_diagnostic(
            &Diagnostic::new(
                "E2000",
                "usage: tsuzuri new directory [--namespace NAME]",
                Span::default(),
            ),
            Path::new("<command line>"),
            "",
            false,
        );
        return ExitCode::from(2);
    };
    match tsuzuri::package::create_project(&directory, namespace.as_deref()) {
        Ok(files) => {
            for file in files {
                println!("Created {}", file.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            print_diagnostic(&error, &directory, "", false);
            ExitCode::FAILURE
        }
    }
}

/// `tsuzuri fetch directory [--json]`: downloads the git dependencies with git
/// and writes Tsuzuri.lock. It prints nothing on success.
fn fetch_dependencies(arguments: &[OsString]) -> ExitCode {
    let json = arguments.iter().any(|argument| argument == "--json");
    let paths: Vec<_> = arguments
        .iter()
        .filter(|argument| *argument != "--json")
        .collect();
    let usage = |message: &str| {
        print_diagnostic(
            &Diagnostic::new("E2000", message, Span::default()),
            Path::new("<command line>"),
            "",
            json,
        );
        ExitCode::from(2)
    };
    if paths.len() != 1
        || paths[0].to_string_lossy().starts_with('-')
        || arguments.len() - paths.len() > 1
    {
        return usage("fetch takes one project directory and optionally --json");
    }
    let directory = Path::new(paths[0]);
    if !directory.join("Tsuzuri.toml").is_file() {
        return usage("fetch requires a Tsuzuri.toml in the project directory");
    }
    match tsuzuri::fetch::fetch(directory) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", json);
            ExitCode::FAILURE
        }
    }
}

/// `tsuzuri publish directory --git URL --rev COMMIT [--json]`: checks the package
/// and prints the registry index entry of that commit. It changes no registry.
fn publish_package(arguments: &[OsString]) -> ExitCode {
    let json = arguments.iter().any(|argument| argument == "--json");
    let usage = |message: &str| {
        print_diagnostic(
            &Diagnostic::new("E2000", message, Span::default()),
            Path::new("<command line>"),
            "",
            json,
        );
        ExitCode::from(2)
    };
    let (mut directory, mut url, mut rev) = (None, None, None);
    let mut valid = true;
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.to_str() {
            Some("--json") => {}
            Some("--git") if url.is_none() => {
                url = rest
                    .next()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned);
                valid &= url.is_some();
            }
            Some("--rev") if rev.is_none() => {
                rev = rest
                    .next()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned);
                valid &= rev.is_some();
            }
            _ if directory.is_none() && !argument.to_string_lossy().starts_with('-') => {
                directory = Some(PathBuf::from(argument));
            }
            _ => valid = false,
        }
    }
    let (Some(directory), Some(url), Some(rev), true) = (directory, url, rev, valid) else {
        return usage(
            "publish takes one package directory, --git URL, --rev COMMIT, and optionally --json",
        );
    };
    if !tsuzuri::package::valid_git_url(&url) {
        return usage(tsuzuri::package::GIT_URL_RULE);
    }
    if !tsuzuri::package::valid_rev(&rev) {
        return usage(tsuzuri::package::GIT_REV_RULE);
    }
    if !directory.join("Tsuzuri.toml").is_file() {
        return usage("publish requires a Tsuzuri.toml in the package directory");
    }
    // A published package must check like any library first.
    let project = match Project::load_for_tests(&directory) {
        Ok(project) => project,
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", json);
            return ExitCode::FAILURE;
        }
    };
    if let Err(errors) = project.analyze_all() {
        print_diagnostics(&errors, &project, json);
        return ExitCode::FAILURE;
    }
    match tsuzuri::fetch::publish(&directory, &url, &rev) {
        Ok(entry) => {
            print!("{entry}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", json);
            ExitCode::FAILURE
        }
    }
}

/// `tsuzuri bindgen header.h -o Module.tz`: writes the module and reports each
/// skipped declaration as a W2002 warning at the header.
fn bindgen_command(arguments: &[OsString]) -> ExitCode {
    let json = arguments
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json");
    let arguments = match tsuzuri::bindgen::parse_arguments(arguments) {
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
    match driver::bindgen(&arguments) {
        Ok((source, warnings)) => {
            let warnings = tsuzuri::diagnostic::DiagnosticSet::from_diagnostics(warnings, 0);
            for (index, warning) in warnings.diagnostics.iter().enumerate() {
                if index != 0 && !json {
                    eprintln!();
                }
                print_diagnostic(warning, &arguments.header, &source, json);
            }
            if let Some(note) = warnings.omission_note() {
                if json {
                    eprintln!(
                        "{{\"severity\":\"note\",\"message\":{}}}",
                        json_string(&note)
                    );
                } else {
                    eprintln!("\nwarning: {note}");
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", json);
            ExitCode::FAILURE
        }
    }
}

const REPL_USAGE: &str =
    "repl takes no paths; supported options are -O0 to -O3, --cpu, --no-cache, and --timeout";
const REPL_TIMEOUT: &str = "--timeout requires whole seconds from 0 to 3600";

/// `tsuzuri repl [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--timeout SECONDS]`.
fn parse_repl_arguments(arguments: &[OsString]) -> Result<tsuzuri::repl::ReplOptions, String> {
    let mut options = tsuzuri::repl::ReplOptions::default();
    let mut seen = BTreeSet::new();
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let option = match argument.to_str() {
            Some("--target") => {
                return Err("repl supports only the native target; build WebAssembly with 'tsuzuri build --target wasm32'".into());
            }
            Some(level @ ("-O0" | "-O1" | "-O2" | "-O3")) => {
                options.build.optimization = level.as_bytes()[2] - b'0';
                "optimization"
            }
            Some("--cpu") => {
                options.build.cpu = match rest.next().and_then(|value| value.to_str()) {
                    Some("generic") => Cpu::Generic,
                    Some("native") => Cpu::Native,
                    _ => return Err("CPU tuning must be 'generic' or 'native'".into()),
                };
                "CPU tuning"
            }
            Some("--no-cache") => {
                options.build.cache = false;
                "no-cache"
            }
            Some("--timeout") => {
                let seconds = rest
                    .next()
                    .and_then(|value| value.to_str())
                    .filter(|value| {
                        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|seconds| *seconds <= 3600)
                    .ok_or(REPL_TIMEOUT)?;
                options.timeout = (seconds != 0).then(|| std::time::Duration::from_secs(seconds));
                "timeout"
            }
            _ => return Err(REPL_USAGE.into()),
        };
        if !seen.insert(option) {
            return Err(format!("{option} specified more than once"));
        }
    }
    Ok(options)
}

/// `tsuzuri repl`: evaluates the inputs on stdin until `:quit` or its end (G13).
fn repl_command(arguments: &[OsString]) -> ExitCode {
    let options = match parse_repl_arguments(arguments) {
        Ok(options) => options,
        Err(message) => {
            print_diagnostic(
                &Diagnostic::new("E2000", message, Span::default()),
                Path::new("<command line>"),
                "",
                false,
            );
            return ExitCode::from(2);
        }
    };
    let input = std::io::stdin();
    let interactive = std::io::IsTerminal::is_terminal(&input);
    match tsuzuri::repl::run(
        options,
        input.lock(),
        std::io::stdout().lock(),
        std::io::stderr(),
        interactive,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_diagnostic(
                &Diagnostic::new(
                    "E2001",
                    format!("cannot read or write the REPL's standard streams: {error}"),
                    Span::default(),
                ),
                Path::new("<repl>"),
                "",
                false,
            );
            ExitCode::FAILURE
        }
    }
}

const SCRIPT_FILE: &str =
    "script needs a source file: tsuzuri script [options] FILE [arguments...]";
const SCRIPT_OPTIONS: &str = "script accepts -O0 to -O3, --cpu, --no-cache, --json, --deny-warnings, --warn, --link, -l, and -L before the file";

/// Splits `tsuzuri script [options] FILE [arguments...]` into the arguments of
/// `tsuzuri run [options] -- FILE` and the program's arguments: everything after FILE,
/// unchanged, so that a script's own options never reach the compiler.
fn script_arguments(arguments: &[OsString]) -> Result<(Vec<OsString>, Vec<OsString>), String> {
    let mut run = vec![OsString::from("run")];
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let file = match argument.to_str() {
            Some("--") => match rest.next() {
                Some(file) => file,
                None => break,
            },
            Some("-h" | "--help" | "-O0" | "-O1" | "-O2" | "-O3" | "--no-cache" | "--json")
            | Some("--deny-warnings") => {
                run.push(argument.clone());
                continue;
            }
            Some(option @ ("--cpu" | "--warn" | "--link" | "-l" | "-L")) => {
                run.push(argument.clone());
                run.push(
                    rest.next()
                        .ok_or_else(|| format!("{option} needs a value"))?
                        .clone(),
                );
                continue;
            }
            Some(option) if option.starts_with('-') => return Err(SCRIPT_OPTIONS.into()),
            _ => argument,
        };
        run.extend([OsString::from("--"), file.clone()]);
        return Ok((run, rest.cloned().collect()));
    }
    if run
        .iter()
        .any(|argument| argument == "-h" || argument == "--help")
    {
        return Ok((run, Vec::new()));
    }
    Err(SCRIPT_FILE.into())
}

/// Whether the options before a script's file ask for JSON diagnostics. Option values and the
/// program's own arguments after the file do not count, so `--cpu native --json` does.
fn script_json(arguments: &[OsString]) -> bool {
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.to_str() {
            Some("--json") => return true,
            Some("--cpu" | "--warn" | "--link" | "-l" | "-L") => {
                rest.next();
            }
            Some(option) if option.starts_with('-') && option != "--" => {}
            _ => return false,
        }
    }
    false
}

fn main() -> ExitCode {
    let raw: Vec<_> = env::args_os().skip(1).collect();
    if raw.is_empty() {
        eprintln!("{HELP}");
        return ExitCode::from(2);
    }
    // `tsuzuri script` is `tsuzuri run` of one file; the program's arguments are set aside first.
    let (raw, program_arguments) = if raw.first().is_some_and(|command| *command == "script") {
        match script_arguments(&raw[1..]) {
            Ok((run, program)) => (run, Some(program)),
            Err(message) => {
                print_diagnostic(
                    &Diagnostic::new("E2000", message, Span::default()),
                    Path::new("<command line>"),
                    "",
                    script_json(&raw[1..]),
                );
                return ExitCode::from(2);
            }
        }
    } else {
        (raw, None)
    };
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
    if raw.first().is_some_and(|command| *command == "new") {
        return new_project(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "fetch") {
        return fetch_dependencies(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "publish") {
        return publish_package(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "bindgen") {
        return bindgen_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "repl") {
        return repl_command(&raw[1..]);
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
    let loaded = if matches!(arguments.action, Action::Test | Action::Bench) {
        Project::load_for_tests(&arguments.input)
    } else if arguments.action == Action::Doc {
        Project::load_for_docs(&arguments.input)
    } else if program_arguments.is_some() {
        Project::load_script(&arguments.input)
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
    let kind = match arguments.action {
        // bench loads the same sources as test (`Project::load_for_tests`).
        Action::Test | Action::Bench => driver::AnalysisKind::Tests,
        Action::Doc => driver::AnalysisKind::Docs,
        _ => driver::AnalysisKind::Program,
    };
    // check, test and doc do not take --no-cache; TSUZURI_CACHE_DIR= disables every cache.
    let module = match project.analyze_cached(arguments.options.cache, kind) {
        Ok(module) => module,
        Err(errors) => {
            print_diagnostics(&errors, &project, arguments.json);
            return ExitCode::FAILURE;
        }
    };
    // The manifest's [native] inputs come first and apply only where the command line's would.
    let links = if links_apply(arguments.action, &arguments.options) {
        project.native.clone().followed_by(&arguments.links)
    } else {
        driver::LinkInputs::default()
    };
    let copies = if arguments.warn_implicit_copy {
        tsuzuri::copies::warnings(&module)
    } else {
        Vec::new()
    };
    let warnings = tsuzuri::diagnostic::DiagnosticSet::from_diagnostics(
        module.warnings.iter().cloned().chain(copies),
        0,
    );
    print_diagnostics(&warnings, &project, arguments.json);
    if arguments.deny_warnings && !warnings.is_empty() {
        return ExitCode::FAILURE;
    }
    if arguments.action == Action::Test {
        return run_test_action(&arguments, &project, &module, &links);
    }
    if arguments.action == Action::Bench {
        return run_bench_action(&arguments, &project, &module, &links);
    }
    let result = run_action(
        &arguments,
        &project,
        &module,
        &links,
        program_arguments.as_deref().unwrap_or_default(),
    );
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

    fn parse_repl(values: &[&str]) -> Result<tsuzuri::repl::ReplOptions, String> {
        parse_repl_arguments(&values.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn script_errors_follow_json_in_the_options_only() {
        let json =
            |values: &[&str]| script_json(&values.iter().map(OsString::from).collect::<Vec<_>>());
        // An option value does not end the options: a missing file still reports in JSON.
        assert!(json(&["--cpu", "native", "--json"]));
        assert!(json(&["--link", "m", "-O3", "--json"]));
        assert!(json(&["--bogus", "--json"]));
        // `--json` after the file, or as a value, belongs to the program or the option.
        assert!(!json(&["greet.tz", "--json"]));
        assert!(!json(&["--", "--json"]));
        assert!(!json(&["--warn", "--json"]));
        assert!(!json(&["-O0"]));
    }

    #[test]
    fn splits_script_options_from_program_arguments() {
        let split = |values: &[&str]| {
            script_arguments(&values.iter().map(OsString::from).collect::<Vec<_>>())
        };
        let strings = |values: &[OsString]| {
            values
                .iter()
                .map(|value| value.to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        let (run, program) = split(&[
            "-O3", "--cpu", "native", "greet.tz", "--help", "-O0", "--", "x",
        ])
        .unwrap();
        assert_eq!(
            strings(&run),
            ["run", "-O3", "--cpu", "native", "--", "greet.tz"]
        );
        assert_eq!(strings(&program), ["--help", "-O0", "--", "x"]);
        // `--` lets the file's name start with '-'.
        let (run, program) = split(&["--", "-odd.tz", "a"]).unwrap();
        assert_eq!(strings(&run), ["run", "--", "-odd.tz"]);
        assert_eq!(strings(&program), ["a"]);
        // The options are then those of `tsuzuri run`.
        let parsed = parse_arguments(
            &split(&["--no-cache", "--json", "-l", "m", "a.tz"])
                .unwrap()
                .0,
        )
        .unwrap();
        assert_eq!(parsed.action, Action::Run);
        assert_eq!(parsed.input, PathBuf::from("a.tz"));
        assert!(!parsed.options.cache && parsed.json && parsed.options.trap_info);
        assert_eq!(parsed.options.optimization, 3);
        assert_eq!(parsed.links.libraries, ["m"]);
        assert_eq!(
            parse_arguments(&split(&["-O0", "-O3", "a.tz"]).unwrap().0).unwrap_err(),
            "optimization specified more than once"
        );
        for (values, message) in [
            (&[][..], SCRIPT_FILE),
            (&["-O0"], SCRIPT_FILE),
            (&["--"], SCRIPT_FILE),
            (&["--target", "wasm32", "a.tz"], SCRIPT_OPTIONS),
            (&["-o", "out", "a.tz"], SCRIPT_OPTIONS),
            (&["-g", "a.tz"], SCRIPT_OPTIONS),
            (&["--timeout", "1", "a.tz"], SCRIPT_OPTIONS),
            (&["--cpu"], "--cpu needs a value"),
        ] {
            assert_eq!(split(values).unwrap_err(), message, "{values:?}");
        }
        // Help before the file needs no file.
        assert_eq!(strings(&split(&["--help"]).unwrap().0), ["run", "--help"]);
    }

    #[test]
    fn parses_repl_options() {
        let defaults = parse_repl(&[]).unwrap();
        assert_eq!(defaults.build.optimization, 0);
        assert_eq!(defaults.build.cpu, Cpu::Generic);
        assert!(defaults.build.cache);
        assert!(defaults.build.trap_info);
        assert_eq!(
            (defaults.build.target, defaults.build.emit),
            (Target::Native, Emit::Executable)
        );
        assert_eq!(defaults.timeout, Some(std::time::Duration::from_secs(10)));
        let custom =
            parse_repl(&["-O3", "--cpu", "native", "--no-cache", "--timeout", "0"]).unwrap();
        assert_eq!(custom.build.optimization, 3);
        assert_eq!(custom.build.cpu, Cpu::Native);
        assert!(!custom.build.cache);
        assert_eq!(custom.timeout, None);
        assert_eq!(
            parse_repl(&["--timeout", "3600"]).unwrap().timeout,
            Some(std::time::Duration::from_secs(3600))
        );
        // The REPL's -O0 default leaves build's and run's -O3 default alone.
        assert_eq!(parse(&["run", "Main.tz"]).unwrap().options.optimization, 3);
    }

    #[test]
    fn rejects_repl_paths_and_build_options() {
        let target = "repl supports only the native target; build WebAssembly with 'tsuzuri build --target wasm32'";
        for (values, message) in [
            (&["Main.tz"][..], REPL_USAGE),
            (&["--target", "wasm32"], target),
            (&["--target", "native"], target),
            (&["--json"], REPL_USAGE),
            (&["-o", "out"], REPL_USAGE),
            (&["-g"], REPL_USAGE),
            (&["--emit", "llvm"], REPL_USAGE),
            (&["--timeout", "3601"], REPL_TIMEOUT),
            (&["--timeout", "x"], REPL_TIMEOUT),
            (&["--timeout", "-1"], REPL_TIMEOUT),
            (&["--timeout"], REPL_TIMEOUT),
            (
                &["--cpu", "fast"],
                "CPU tuning must be 'generic' or 'native'",
            ),
            (&["-O0", "-O3"], "optimization specified more than once"),
            (
                &["--no-cache", "--no-cache"],
                "no-cache specified more than once",
            ),
        ] {
            assert_eq!(parse_repl(values).unwrap_err(), message, "{values:?}");
        }
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
    fn wasm_host_selects_wasi_for_wasm32_output_only() {
        let host = |extra: &[&str]| {
            let mut values = vec!["build", "Main.tz"];
            values.extend_from_slice(extra);
            parse(&values)
        };
        for emit in ["wasm", "object"] {
            let hosted =
                host(&["--target", "wasm32", "--emit", emit, "--wasm-host", "wasi"]).unwrap();
            assert_eq!(hosted.options.wasm_host, Some(WasmHost::Wasi), "{emit}");
        }
        assert_eq!(
            host(&["--target", "wasm32"]).unwrap().options.wasm_host,
            None
        );
        // LLVM IR is refused: the WASI runtime (src/runtime/os-wasi.c) is compiled in only for objects and WASM.
        let requires = "--wasm-host wasi requires wasm32 object or WASM output";
        for values in [
            vec!["--wasm-host", "wasi"],
            vec!["--target", "wasm64", "--wasm-host", "wasi"],
            vec![
                "--target",
                "wasm32",
                "--emit",
                "llvm",
                "--wasm-host",
                "wasi",
            ],
            vec![
                "--target",
                "wasm32",
                "--emit",
                "header",
                "--wasm-host",
                "wasi",
            ],
        ] {
            assert_eq!(host(&values).unwrap_err(), requires, "{values:?}");
        }
        assert_eq!(
            host(&["--target", "wasm32", "--wasm-host", "nope"]).unwrap_err(),
            "unknown wasm host 'nope'; the supported host is wasi"
        );
        assert_eq!(
            host(&[
                "--target",
                "wasm32",
                "--wasm-host",
                "wasi",
                "--wasm-host",
                "wasi"
            ])
            .unwrap_err(),
            "--wasm-host specified more than once"
        );
        assert_eq!(
            host(&[
                "--target",
                "wasm32",
                "--emit",
                "object",
                "--wasm-feature",
                "threads",
                "--wasm-host",
                "wasi"
            ])
            .unwrap_err(),
            "--wasm-host wasi cannot be combined with --wasm-feature threads"
        );
        for action in ["check", "run", "test"] {
            assert_eq!(
                parse(&[action, "Main.tz", "--wasm-host", "wasi"]).unwrap_err(),
                "--wasm-host is only valid with build",
                "{action}"
            );
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
        for values in [
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "jspi",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm64",
                "--emit",
                "llvm",
                "--wasm-feature",
                "jspi",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--emit",
                "bindings-js",
                "--wasm-feature",
                "jspi",
            ],
        ] {
            assert!(parse(&values).unwrap().options.wasm_jspi, "{values:?}");
        }
        for (values, message) in [
            (
                vec!["run", "Main.tz", "--wasm-feature", "jspi"],
                "--wasm-feature is only valid with build",
            ),
            (
                vec!["build", "Main.tz", "--wasm-feature", "jspi"],
                "--wasm-feature jspi requires wasm32 or wasm64 object, LLVM IR, WASM, or JavaScript bindings output",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "header",
                    "--wasm-feature",
                    "jspi",
                ],
                "--wasm-feature jspi requires wasm32 or wasm64 object, LLVM IR, WASM, or JavaScript bindings output",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "jspi",
                    "--wasm-feature",
                    "threads",
                ],
                "--wasm-feature jspi cannot be combined with --wasm-feature threads or --wasm-host",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "jspi",
                    "--wasm-host",
                    "wasi",
                ],
                "--wasm-feature jspi cannot be combined with --wasm-feature threads or --wasm-host",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "jspi",
                    "--wasm-feature",
                    "jspi",
                ],
                "WASM feature specified more than once",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "asyncify",
                ],
                "supported WASM features are 'simd128', 'threads', and 'jspi'; relaxed SIMD is not supported",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
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
        // G16 Phase 2: `test --index N -g -o PATH` builds one test's runner for a debugger.
        let debug = parse(&["test", "Main.tz", "--index", "1", "-g", "-o", "runner"]).unwrap();
        assert!(debug.options.debug_info);
        assert_eq!(debug.output.as_deref(), Some(Path::new("runner")));
        assert_eq!(debug.test_indices, [1]);
        for values in [
            vec!["test", "Main.tz", "-g", "-o", "runner"],
            vec![
                "test", "Main.tz", "--index", "1", "--index", "2", "-g", "-o", "r",
            ],
            vec!["test", "Main.tz", "--index", "1", "-g"],
            vec!["test", "Main.tz", "--index", "1", "-o", "runner"],
            vec!["test", "Main.tz", "--index", "1", "--list", "-g", "-o", "r"],
            vec![
                "test", "Main.tz", "--index", "1", "--filter", "x", "-g", "-o", "r",
            ],
            vec![
                "test", "Main.tz", "--index", "1", "--target", "wasm32", "-g", "-o", "r",
            ],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
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
        for (kind, emit) in [
            ("shared", Emit::Shared),
            ("bindings-cs", Emit::BindingsCs),
            ("bindings-py", Emit::BindingsPy),
            ("bindings-cpp", Emit::BindingsCpp),
        ] {
            if cfg!(windows) && emit == Emit::Shared {
                continue;
            }
            let parsed = parse(&["build", "A.tz", "--emit", kind]).unwrap();
            assert_eq!(parsed.options.emit, emit);
            // The trap table belongs to the library, not to its bindings.
            let trapping =
                parse(&["build", "A.tz", "--emit", kind, "--trap-mode", "return"]).unwrap();
            assert_eq!(trapping.options.trap_info, emit == Emit::Shared);
        }
        if !cfg!(windows) {
            // A shared library links like an executable, so it takes link inputs.
            let linked = parse(&["build", "A.tz", "--emit", "shared", "-l", "m"]).unwrap();
            assert_eq!(linked.links.libraries, ["m"]);
            assert!(links_apply(Action::Build, &linked.options));
        }
        let bindings = parse(&[
            "build",
            "A.tz",
            "--target",
            "wasm32",
            "--emit",
            "bindings-js",
            "-O3",
            "-o",
            "out/api.mjs",
        ])
        .unwrap();
        assert_eq!(bindings.options.emit, Emit::BindingsJs);
        assert_eq!(
            bindings.options.output_path(Path::new("dir/A.tz")),
            Path::new("dir/A.mjs")
        );
        for (values, message) in [
            (
                vec!["build", "A.tz", "--emit", "bindings-ts"],
                "emit kind must be exe, object, llvm, header, wasm, wgsl, shared, bindings-js, bindings-cs, bindings-py, or bindings-cpp",
            ),
            (
                vec!["build", "A.tz", "--emit", "bindings-js"],
                "'--emit bindings-js' requires '--target wasm32'",
            ),
            (
                vec![
                    "build",
                    "A.tz",
                    "--target",
                    "wasm64",
                    "--emit",
                    "bindings-js",
                ],
                "'--emit bindings-js' requires '--target wasm32'",
            ),
            (
                vec![
                    "build",
                    "A.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "bindings-js",
                    "--trap-info",
                ],
                "--trap-info is not valid for bindings output; pass it when building the .wasm",
            ),
            (
                vec![
                    "build",
                    "A.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "bindings-js",
                    "--wasm-feature",
                    "simd128",
                ],
                "--wasm-feature is not valid for bindings output; pass it when building the .wasm",
            ),
            (
                vec![
                    "build",
                    "A.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "bindings-js",
                    "-o",
                    "api.js",
                ],
                "bindings output must end with '.mjs'; declarations are written next to it as '<name>.d.mts'",
            ),
            (
                vec![
                    "build",
                    "A.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "bindings-js",
                    "--cpu",
                    "native",
                ],
                "'--cpu native' requires native executable or object output",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
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
    fn parses_warn_implicit_copy() {
        for action in ["check", "build", "run", "test"] {
            let arguments = parse(&[action, "Main.tz", "--warn", "implicit-copy"]).unwrap();
            assert!(arguments.warn_implicit_copy, "{action}");
            assert!(!parse(&[action, "Main.tz"]).unwrap().warn_implicit_copy);
        }
        for (values, message) in [
            (
                vec!["check", "Main.tz", "--warn"],
                "--warn requires a warning name; supported: implicit-copy",
            ),
            (
                vec!["check", "Main.tz", "--warn", "shadowing"],
                "unknown warning 'shadowing' for --warn; supported: implicit-copy",
            ),
            (
                vec![
                    "check",
                    "Main.tz",
                    "--warn",
                    "implicit-copy",
                    "--warn",
                    "implicit-copy",
                ],
                "warn implicit-copy specified more than once",
            ),
            (
                vec!["fmt", "Main.tz", "--warn", "implicit-copy"],
                "fmt does not use optimization, CPU tuning, or compiler warning options",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
    }

    #[test]
    fn parses_bench_coverage_and_seed_options() {
        let bench = parse(&[
            "bench",
            "Speed.tz",
            "--samples",
            "5",
            "--list",
            "--index",
            "1",
            "--filter",
            "total",
        ])
        .unwrap();
        assert_eq!(bench.action, Action::Bench);
        assert_eq!(bench.bench_samples, Some(5));
        assert_eq!(bench.options.optimization, 3);
        assert!(bench.test_list && bench.test_filter.as_deref() == Some("total"));
        assert_eq!(bench.test_indices, vec![1]);
        assert!(links_apply(Action::Bench, &bench.options));
        assert_eq!(
            parse(&["bench", "Speed.tz", "-O0", "--target", "native"])
                .unwrap()
                .options
                .optimization,
            0
        );
        let test = parse(&[
            "test",
            "Specs.tz",
            "--coverage",
            "c.info",
            "--seed",
            "18446744073709551615",
        ])
        .unwrap();
        assert_eq!(test.coverage, Some(PathBuf::from("c.info")));
        assert_eq!(test.test_seed, Some(u64::MAX));
        assert_eq!(test.bench_samples, None);
        let samples = "bench samples must be an integer between 1 and 1000";
        let seed = "property seed must be an integer between 0 and 18446744073709551615";
        for (values, message) in [
            (
                vec!["check", "Main.tz", "--coverage", "c.info"],
                "--coverage is only valid with test",
            ),
            (
                vec!["bench", "Main.tz", "--coverage", "c.info"],
                "--coverage is only valid with test",
            ),
            (
                vec!["test", "Main.tz", "--coverage"],
                "--coverage needs a value",
            ),
            (
                vec!["test", "Main.tz", "--coverage", "a", "--coverage", "b"],
                "coverage specified more than once",
            ),
            (
                vec!["test", "Main.tz", "--coverage", "a", "--list"],
                "coverage cannot be combined with --list",
            ),
            (
                vec!["test", "Main.tz", "--coverage", "a", "--target", "wasm64"],
                "test coverage supports only the native target",
            ),
            (
                vec!["test", "Main.tz", "--samples", "3"],
                "--samples is only valid with bench",
            ),
            (vec!["bench", "Main.tz", "--samples", "0"], samples),
            (vec!["bench", "Main.tz", "--samples", "1001"], samples),
            (vec!["bench", "Main.tz", "--samples", " 3"], samples),
            (
                vec!["bench", "Main.tz", "--samples", "3", "--samples", "4"],
                "samples specified more than once",
            ),
            (
                vec!["bench", "Main.tz", "--target", "wasm32"],
                "tsuzuri bench supports only the native target",
            ),
            (
                vec!["bench", "Main.tz", "--cpu", "native"],
                "bench does not use CPU tuning",
            ),
            (
                vec!["bench", "Main.tz", "--wasm-max-memory", "64MiB"],
                "--wasm-max-memory and --wasm-stack-size are only valid with build or test",
            ),
            (
                vec!["run", "Main.tz", "--index", "0"],
                "--filter, --list, and --index are only valid with test or bench",
            ),
            (
                vec!["build", "Main.tz", "--seed", "1"],
                "--seed is only valid with test",
            ),
            (
                vec!["bench", "Main.tz", "--seed", "1"],
                "--seed is only valid with test",
            ),
            (vec!["test", "Main.tz", "--seed", "0x10"], seed),
            (
                vec!["test", "Main.tz", "--seed", "18446744073709551616"],
                seed,
            ),
            (
                vec!["test", "Main.tz", "--seed", "1", "--seed", "2"],
                "seed specified more than once",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
        for values in [
            vec!["bench", "Main.tz", "-o", "out"],
            vec!["bench", "Main.tz", "--emit", "llvm"],
            vec!["bench", "Main.tz", "--trap-info"],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
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
            vec!["fmt", "Main.tz", "--warn", "implicit-copy"],
            vec!["check", "Main.tz", "--warn"],
            vec!["check", "Main.tz", "--warn", "shadowing"],
            vec![
                "check",
                "Main.tz",
                "--warn",
                "implicit-copy",
                "--warn",
                "implicit-copy",
            ],
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

    #[test]
    fn parses_link_inputs() {
        let arguments = parse(&[
            "build", "Main.tz", "--link", "host.o", "-L", "lib", "-l", "sqlite3", "--link",
            "util.a", "-l", "m",
        ])
        .unwrap();
        assert_eq!(
            arguments.links.paths,
            [PathBuf::from("host.o"), PathBuf::from("util.a")]
        );
        assert_eq!(arguments.links.libraries, ["sqlite3", "m"]);
        assert_eq!(arguments.links.search, [PathBuf::from("lib")]);
        for action in ["run", "test"] {
            assert!(
                parse(&[action, "Main.tz", "--link", "host.o"]).is_ok(),
                "{action}"
            );
        }
        assert!(parse(&["build", "Main.tz", "--emit", "exe", "-l", "m"]).is_ok());
        assert!(parse(&["build", "Main.tz"]).unwrap().links.is_empty());
        for (values, message) in [
            (vec!["build", "Main.tz", "-l"], "-l needs a value"),
            (vec!["run", "Main.tz", "--link"], "--link needs a value"),
            (vec!["run", "Main.tz", "-L"], "-L needs a value"),
            (
                vec!["run", "Main.tz", "-l", "libm"],
                "invalid library name 'libm'; pass the name without the 'lib' prefix or extension, for example -l sqlite3",
            ),
            (
                vec!["run", "Main.tz", "-l", "m.a"],
                "invalid library name 'm.a'; pass the name without the 'lib' prefix or extension, for example -l sqlite3",
            ),
            (
                vec!["run", "Main.tz", "-l", "a/b"],
                "invalid library name 'a/b'; pass the name without the 'lib' prefix or extension, for example -l sqlite3",
            ),
            (
                vec!["run", "Main.tz", "--link", "a.o", "--link", "a.o"],
                "link input 'a.o' specified more than once",
            ),
            (
                vec!["run", "Main.tz", "-l", "m", "-l", "m"],
                "link input 'm' specified more than once",
            ),
            (
                vec!["run", "Main.tz", "-L", "d", "-L", "d"],
                "link input 'd' specified more than once",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
        let many: Vec<String> = (0..=256)
            .flat_map(|index| ["--link".to_owned(), format!("object{index}.o")])
            .collect();
        let mut values = vec!["run", "Main.tz"];
        values.extend(many.iter().map(String::as_str));
        assert_eq!(
            parse(&values).unwrap_err(),
            "too many link inputs; at most 256 are supported"
        );
        values.truncate(2 + 2 * 256);
        assert!(parse(&values).is_ok());
    }

    #[test]
    fn rejects_link_inputs_outside_native_executables() {
        let message = "link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe";
        for values in [
            vec!["build", "Main.tz", "--target", "wasm32", "--link", "host.o"],
            vec!["build", "Main.tz", "--target", "wasm64", "-l", "m"],
            vec!["build", "Main.tz", "--emit", "object", "-l", "m"],
            vec!["build", "Main.tz", "--emit", "llvm", "-L", "lib"],
            vec!["build", "Main.tz", "--emit", "header", "--link", "host.o"],
            vec!["check", "Main.tz", "--link", "host.o"],
            vec!["doc", "Main.tz", "-o", "docs", "-l", "m"],
            vec!["fmt", "Main.tz", "-L", "lib"],
            vec!["test", "Main.tz", "--target", "wasm32", "--link", "host.o"],
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
        // Without link inputs every one of these stays valid.
        assert!(parse(&["build", "Main.tz", "--target", "wasm32"]).is_ok());
        assert!(parse(&["build", "Main.tz", "--emit", "object"]).is_ok());
    }

    #[test]
    fn parses_trap_mode_return() {
        let object = parse(&[
            "build",
            "Main.tz",
            "--emit",
            "object",
            "--trap-mode",
            "return",
        ])
        .unwrap();
        assert!(object.options.trap_return && object.options.trap_info);
        let header = parse(&[
            "build",
            "Main.tz",
            "--emit",
            "header",
            "--trap-mode",
            "return",
        ])
        .unwrap();
        assert!(header.options.trap_return && !header.options.trap_info);
        let plain = parse(&["build", "Main.tz", "--emit", "object"]).unwrap();
        assert!(!plain.options.trap_return && !plain.options.trap_info);
        for (values, message) in [
            (
                vec!["build", "Main.tz", "--trap-mode"],
                "--trap-mode needs a value",
            ),
            (
                vec!["build", "Main.tz", "--trap-mode", "abort"],
                "trap mode must be 'return'",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--trap-mode",
                    "return",
                    "--trap-mode",
                    "return",
                ],
                "trap mode specified more than once",
            ),
            (
                vec!["run", "Main.tz", "--trap-mode", "return"],
                "--trap-mode is only valid with build",
            ),
            (
                vec!["check", "Main.tz", "--trap-mode", "return"],
                "--trap-mode is only valid with build",
            ),
            (
                vec!["test", "Main.tz", "--trap-mode", "return"],
                "--trap-mode is only valid with build",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
    }

    #[test]
    fn parses_allocator_selection() {
        use tsuzuri::llvm::Allocator;
        let allocator = |values: &[&str]| parse(values).unwrap().options.allocator;
        for emit in ["object", "llvm", "header"] {
            assert_eq!(
                allocator(&["build", "Main.tz", "--emit", emit, "--allocator", "host"]),
                Allocator::Host
            );
            assert_eq!(
                allocator(&[
                    "build",
                    "Main.tz",
                    "--emit",
                    emit,
                    "--allocator",
                    "counting"
                ]),
                Allocator::Counting
            );
        }
        assert_eq!(
            allocator(&["build", "Main.tz", "--emit", "object"]),
            Allocator::System
        );
        for value in ["system", "host", "counting"] {
            parse(&[
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--allocator",
                value,
            ])
            .unwrap();
        }
        let freestanding = parse(&[
            "build",
            "Main.tz",
            "--emit",
            "object",
            "--allocator",
            "host",
            "--freestanding",
        ])
        .unwrap();
        assert!(freestanding.options.freestanding);
        let host_output = "--allocator host requires object, LLVM IR, header, or WebAssembly output; link the object into a host that defines tsuzuri_host_alloc, tsuzuri_host_free, and tsuzuri_host_realloc";
        for (values, message) in [
            (vec!["build", "Main.tz", "--allocator", "host"], host_output),
            (
                vec!["build", "Main.tz", "--emit", "wgsl", "--allocator", "host"],
                host_output,
            ),
            (
                vec!["build", "Main.tz", "--allocator", "counting"],
                "--allocator counting requires object, LLVM IR, header, or WebAssembly output; the host reads the counts with tsuzuri_alloc_stats",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "threads",
                    "--allocator",
                    "host",
                ],
                "--allocator host cannot be combined with --wasm-feature threads; WASM threads keep their locked internal allocator",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "object",
                    "--trap-mode",
                    "return",
                    "--allocator",
                    "counting",
                ],
                "--trap-mode return keeps its own tracked allocator; remove --allocator",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "object",
                    "--allocator",
                    "pool",
                ],
                "allocator must be 'system', 'host', or 'counting'",
            ),
            (
                vec!["build", "Main.tz", "--emit", "object", "--allocator"],
                "--allocator needs a value",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "object",
                    "--allocator",
                    "host",
                    "--allocator",
                    "host",
                ],
                "allocator specified more than once",
            ),
            (
                vec!["run", "Main.tz", "--allocator", "system"],
                "--allocator is only valid with build",
            ),
            (
                vec!["check", "Main.tz", "--allocator", "system"],
                "--allocator is only valid with build",
            ),
            (
                vec!["test", "Main.tz", "--allocator", "system"],
                "--allocator is only valid with build",
            ),
            (
                vec!["build", "Main.tz", "--emit", "object", "--freestanding"],
                "--freestanding requires --allocator host: without the C library the host provides the heap",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--allocator",
                    "host",
                    "--freestanding",
                ],
                "--freestanding requires a native target",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "object",
                    "--allocator",
                    "host",
                    "--freestanding",
                    "--trap-info",
                ],
                "--freestanding cannot be combined with --trap-info or --debug-output: they write through the C library",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "object",
                    "--allocator",
                    "host",
                    "--freestanding",
                    "--freestanding",
                ],
                "freestanding specified more than once",
            ),
            (
                vec!["run", "Main.tz", "--freestanding"],
                "--freestanding is only valid with build",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
    }
}
