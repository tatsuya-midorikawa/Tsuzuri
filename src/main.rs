use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tsuzuri::diagnostic::{Diagnostic, DiagnosticKind, Span, json_string};
use tsuzuri::driver::{
    self, Backend, BuildOptions, Cpu, Emit, OptLevel, Project, Target, Triple, WasmHost,
};
use tsuzuri::llvm::Allocator;
use tsuzuri::server;

#[path = "main_kg.rs"]
mod kg_command;

/// `eprintln!` to `err`. A failed write panics with the message that `eprintln!` has.
macro_rules! errln {
    ($err:expr) => {
        if let Err(error) = writeln!($err) {
            panic!("failed printing to stderr: {error}");
        }
    };
    ($err:expr, $($arg:tt)*) => {
        if let Err(error) = writeln!($err, $($arg)*) {
            panic!("failed printing to stderr: {error}");
        }
    };
}

/// The exit codes of the commands that `cli` runs. `ExitCode` cannot be read back, and the build
/// server hands the number to its client.
const EXIT_SUCCESS: u8 = 0;
const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

const HELP: &str = "\
Tsuzuri - a statically typed language with ownership, powered by LLVM

Usage:
    tsuzuri lsp
  tsuzuri check source.tz|source.tt|source.tc|directory [--json]
    tsuzuri doc source.tz|source.tt|source.tc|directory -o outdir [--json]
    tsuzuri fmt [--check] source.tz|source.tt|source.tc|directory [--json]
    tsuzuri test source.tz|directory [--list] [--filter TEXT] [--index N] [--json] [-O0|-O1|-O2|-O3]
                             [--target native|wasm32|wasm64|TRIPLE] [--wasm-max-memory SIZE] [--wasm-stack-size SIZE]
                             [--coverage PATH] [--seed N] [--affected BASE]
    tsuzuri test source.tz|directory --index N -g -o PATH [-O0|-O1|-O2|-O3] [--seed N] [--json]
                             Build test N alone with debug information as PATH without running it;
                             a debugger starts PATH with the printed argument 0
    tsuzuri bench source.tz|directory [--list] [--filter TEXT] [--index N] [--json] [--samples N] [--allocs]
                              [-O0|-O1|-O2|-O3] [--target native|TRIPLE]
    tsuzuri watch check|build|test source.tz|directory [options of that command]
                             Run it again each time a source or manifest that it reads changes
    tsuzuri serve directory [--idle-timeout SECONDS]
    tsuzuri serve --status directory | --stop directory
                             A build server (Unix) that runs check and build for that project
  tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
  tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3|-Os|-Oz] [--target native|TRIPLE] [--cpu generic|native]
                    [--profile-generate DIR|--profile-use PATH] [--json]
  tsuzuri repl [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--timeout SECONDS]
  tsuzuri script [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--json] FILE [arguments...]
  tsuzuri new directory [--namespace NAME] [--no-agent-files]
  tsuzuri fetch directory [--json]
  tsuzuri publish directory --git URL --rev COMMIT [--json]
  tsuzuri api-diff old new [--effective] [--json]
  tsuzuri kg build [PATH] [--kg-dir DIR] [--force] [--json] | kg build --std [--force] [--json]
  tsuzuri kg verify [PATH] [--kg-dir DIR] [--json]
  tsuzuri kg clean [PATH] [--kg-dir DIR] | kg clean --std
  tsuzuri kg export [PATH] [--kg-dir DIR] (--nodes | --edges | --facts) [--no-refresh]
  tsuzuri kg brief [ID | --project | --module M] [query options]
  tsuzuri kg show|tests-for ID [query options] | kg uses TYPE [query options]
  tsuzuri kg callers|callees ID [--depth N] [query options]
  tsuzuri kg search TEXT [--std] [query options]
  tsuzuri kg find TYPE | --result TYPE [--takes TYPE]... [--module M] [--limit N] [query options]
  tsuzuri kg path FROM TO [--rel R[,R...]] [--depth N] [query options]
                             query options: [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]
  tsuzuri kg impact (--git REV | --from PATH [--to PATH]) [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]
  tsuzuri kg diagram module|types|calls|ownership|concurrency|layout|transitions [ID] [--format mermaid|dot]
                             [--max-nodes N] [--depth N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]
  tsuzuri guide [--level 0|1] [--topic NAME] [--list] [--json] [--edition YEAR]
  tsuzuri guide std [MODULE] [--json]
  tsuzuri guide --agents [DIR] | --llms-txt [DIR]
  tsuzuri explain TARGET [--json]
  tsuzuri explain --catalog [--json]
  tsuzuri fix [--applicability certain|likely|maybe] [--dry-run] [--json] source.tz|source.tt|source.tc|directory
  tsuzuri fix --edition YEAR [--from YEAR] [--dry-run] [--json] input
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
`tsuzuri watch check|build|test PATH [options]` runs that command, then again each time a source
or manifest that it reads changes: it polls every 100 ms, and a save that leaves the text as it
was does not count. After each run it writes `watch: run N finished with exit code C` (with --json,
the object {\"type\":\"watch\",\"run\":N,\"exit\":C}) to standard error. It runs in this process and
ends only when it is stopped, for example with Ctrl-C. A directory named watch is built with
`tsuzuri build watch`.
`tsuzuri serve DIR` (macOS and Linux) is a build server for the project at DIR, in the foreground:
`tsuzuri check` and `tsuzuri build` of that project, started with the same compiler, working
directory, and environment variables that the compiler and its tools read (PATH, TSUZURI_*, the
SDK and locale variables, ...), ask it to run the command and print what it printed, and run the
command themselves, without a word, when no server is there or it refuses; --no-server skips the
asking. It keeps what it read between commands, and stops after --idle-timeout seconds (1 to
86400, default 1800) without a request, with `tsuzuri serve --stop DIR`, or when the compiler file
is replaced. `tsuzuri serve --status DIR` prints its counters. The sockets are in
TSUZURI_SERVER_DIR (an absolute path; empty turns servers off), by default the user's cache
(macOS) or runtime (Linux) directory, which only its owner can enter. A directory named serve is
built with `tsuzuri build serve`.
`tsuzuri new` creates Tsuzuri.toml, Main.tz, .gitignore, and for AI agents AGENTS.md and llms.txt
in an empty folder; --no-agent-files leaves out the last two.
`tsuzuri check`, `build`, `run`, and `test` of a project (a root with Tsuzuri.toml or Main.tz) write
its knowledge graph to target/tz/kg: the declarations as nodes (nodes.jsonl), their relations as
edges (edges.jsonl), facts and layouts per module, diagnostics, and short English brief cards. Sources
that did not change are not written again. --kg writes it for a loose file too, --no-kg or
TSUZURI_KG=0 does not write it (TSUZURI_KG=1 writes it), and --kg-dir DIR or TSUZURI_KG_DIR moves it;
a failure to write is one warning and changes neither the diagnostics nor the exit code.
`tsuzuri kg build|verify|clean|export` builds, checks, removes, and prints the graph; --std builds
the standard library's graph in the compiler's cache. `kg brief`, `show`, `callers`, `callees`,
`uses`, `tests-for`, `search`, `find` (by type), and `path` (a shortest chain of edges) answer from
the graph within --budget characters, rebuilding it first unless --no-refresh. `kg impact` lists
the changes between two graphs (--git REV: that commit of this project) and the tests that reach
them; `tsuzuri test --affected BASE` (a path, a graph directory, or a git revision) runs only those.
`kg diagram` draws the modules, types, calls, ownership, concurrency, layout, or transitions as
Mermaid, DOT, or Markdown; the graph keeps the modules, types, and concurrency diagrams in views/.
`tsuzuri guide` prints a short English guide to the language for AI agents (level 0, about 6 KB);
--level 1 prints the full guide, --topic NAME one chapter (--list names them), `guide std [MODULE]`
the std index. --agents and --llms-txt print the AGENTS.md and llms.txt of the project in DIR.
`tsuzuri fetch` downloads the git dependencies of Tsuzuri.toml
({ git = \"https://...\", rev = \"<40-hex commit>\" }) and the registry dependencies
({ version = \"1.2.3\" }, the minimal versions that satisfy every requirement in the
[registry] index) with git 2.32 or later into the package store (packages/ in the
build cache) and records them in Tsuzuri.lock. Other commands never run git or
use the network; they read Tsuzuri.lock and the store. `tsuzuri publish` checks a
package and prints the registry index entry of its commit; it changes no registry.
`tsuzuri api-diff old new` lists, one line per change, how the public API of the package or
project in `new` differs from `old` (each a directory, or a file in the root directory): a
removed or changed declaration, a record field or union case that was added, and a class method
added without a default are `breaking`; an added declaration, a documentation change and a new
`@deprecated` tag are `non-breaking`. It reads the declarations as `tsuzuri doc` shows them,
runs no type check, and loads no git or registry dependency by default. --effective loads the
locked dependencies, fully checks both versions, and compares resolved aliases, argument modes,
callable purity and region contracts; an analysis error never falls back to syntax comparison.
Exit status 1 means a breaking change or an analysis error; with --json each change and the
final summary is one JSON object per line.
`tsuzuri fix PATH` applies the fixes that diagnostics carry: `certain` ones by default, and `likely`
ones too with --applicability likely. It checks the project again after each round and writes nothing
when the fixes would add a diagnostic; --dry-run prints the unified diff instead of writing.
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
  --target native|wasm32|wasm64|TRIPLE
                         Target (default: native; wasm64 uses 64-bit memory). A TRIPLE, listed at the
                         end of this help, builds native code for that machine; its C headers, C
                         library, and linker come from the bundled toolchain or TSUZURI_SYSROOT
    --wasm-feature <name>   Opt in to simd128 (WASM build), threads (wasm32 build), jspi
                            (WASM build; Async.block_on waits through JavaScript Promise
                            Integration), webgpu (wasm32 build; Gpu.request Gpu.WebGpu and
                            the kernels on that device call the JSPI imports tsuzuri_gpu.open
                            and tsuzuri_gpu.run, which src/runtime/webgpu.mjs implements),
                            net (wasm32 build with jspi; the Net sockets become imports that
                            the generated JavaScript bindings implement on Node.js), or
                            tail-call (wasm32 or wasm64 object, LLVM IR, or WASM output; the
                            guaranteed tail calls become return_call, which Node.js 20 and
                            current browsers run, so a mutual recursion keeps a flat stack)
    --wasm-host wasi        Lower the standard IO and the File, Dir, Env, Time, Random, and
                            Process APIs to WASI preview1 (wasm32 object or WASM output;
                            the default wasm32 output rejects those APIs)
    --wasm-max-memory SIZE  WASM linear memory limit (WASM build/test; default 16MiB,
                            at most 4GiB-64KiB on wasm32 and 16GiB on wasm64)
    --wasm-stack-size SIZE  WASM main stack size (WASM output/test; default 1MiB)
                            Tsuzuri.toml [wasm] max-memory/stack-size set project defaults
    --emit KIND            exe, object, llvm, header, wasm, wgsl, wgsl-relaxed, spirv,
                         spirv-relaxed, shared, bitcode, bindings-js, bindings-cs, bindings-py,
                         or bindings-cpp
                         Default: exe for native, wasm for wasm32 and wasm64
                         wgsl writes the strict WebGPU compute shader of the one exported kernel
                         (i32 or i32u lanes); wgsl-relaxed also allows f32 lanes and values,
                         whose results follow WGSL's floating-point rules (Gpu.map_relaxed)
                         spirv writes the strict Vulkan compute module (SPIR-V binary) of the one
                         exported kernel (32- and 64-bit integer lanes, and f32 lanes whose
                         device must report the strict float controls); spirv-relaxed also
                         allows f32 division and follows the device's floating-point rules
                         shared links a native .dylib or .so that exports the C ABI
                         bitcode writes one ThinLTO LLVM bitcode file of the program and the
                         runtime it needs, for a host that links with -flto=thin and the same
                         Clang (native, where llvm-link is needed when the program uses the C
                         runtime, or --target wasm32, which links with wasm-ld)
                         bindings-js (with --target wasm32) writes a JavaScript module
                         NAME.mjs and its TypeScript declarations NAME.d.mts
                         bindings-cs, bindings-py, and bindings-cpp write C#, Python
                         ctypes, and C++20 bindings of the shared library NAME
  -O0, -O1, -O2, -O3    LLVM optimization level (default: -O3; no fast-math)
  -Os, -Oz               Optimize for size (LLVM -Os or -Oz) with build and run; test and bench
                         take -O0 to -O3. Language semantics, traps, and debug information do
                         not change; the code is usually slower than at -O3
  --backend llvm|fast    Code generator of build, run, and test (default: llvm). fast translates
                         the -O0 IR in the compiler itself instead of running Clang on it, for a
                         quicker edit-build-run loop; the code runs like -O0 code. It takes -O0
                         (its default, and test's) and two targets: native with --emit exe|object
                         (--trap-info, --strip, -g for function and line information, not
                         variables, --allocator, --freestanding, --trap-mode return), and wasm32
                         with --emit wasm|object (--trap-info, --allocator, --wasm-max-memory,
                         --wasm-stack-size). Stay with llvm: --cpu native, --profile-generate,
                         --profile-use, wasm64, other targets and --emit kinds, and for wasm32
                         --wasm-feature, --wasm-host, --wasm-opt, and -g; test runs the native
                         target only. It needs a tsuzuri built with --features fast-backend on a
                         macOS or Linux host with an AArch64 or x86-64 CPU
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
    --no-server            Run check or build in this process even where tsuzuri serve runs
                           for the project
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
    --allocator system|host|counting|small
                         Heap of native or WASM output (default: system; counting also with run and test).
                         host calls tsuzuri_host_alloc/free/realloc, which the host defines
                         (WASM imports them from tsuzuri_heap); counting adds the counts that
                         tsuzuri_alloc_stats reports; small serves blocks of up to 512 bytes
                         from per-thread size classes (native 64-bit targets, also executables
                         and shared libraries; not WASM, --freestanding, or --trap-mode return)
    --freestanding         Native object, llvm, or header output that needs no C library;
                         requires --allocator host and rejects IO, OS APIs, Net sockets, tasks, and Debug
    --strip                Link a native executable without its local symbols (build only; not with -g;
                         a Windows link is left as it is)
    --wasm-opt             Run Binaryen's wasm-opt -Oz over the linked .wasm (build with --target wasm32
                         only; not with -g or threads). It is never run unless asked: install Binaryen
                         or set TSUZURI_WASM_OPT, or the build fails with E2002
    --profile-generate DIR Native executable or object output, run, and a wasm32 module: instrument the
                         program so that it writes LLVM raw profiles (.profraw) under DIR when it exits
                         normally (a run that traps or is killed writes none). Run it on a typical
                         workload, then build again with --profile-use. An object needs
                         -fprofile-generate at its final link. A wasm32 module has no runtime: its host
                         calls writeProfile(instance) of the app.wasm.profile.mjs that the build writes
                         beside it (Clang 21; no indirect-call profile). The counters are not atomic:
                         Tasks that run in parallel are counted too low
    --profile-use PATH     Native executable or object output, run, and a wasm32 module: optimize with the
                         profile PATH, an indexed .profdata file or a directory of .profraw files that
                         llvm-profdata merges. Build both times with the same Clang and options; a profile
                         that no longer matches is one W2001 warning. Both profile options need an LLVM
                         Clang in TSUZURI_CLANG, not the bundled launcher; they are not available on
                         Windows, for another machine's --target, with wasm64, or with another --emit
                         kind such as bitcode, llvm, or shared
  --                     Treat remaining arguments as paths
  -h, --help             Show this help
  --version              Show the compiler version

Threads:
  TSUZURI_THREADS        Threads that the compiler itself uses (1-32; default: the available
                         cores, at most 8). 1 runs the analysis on one thread. The output does
                         not depend on it, and it does not affect the programs that are built.

Toolchain:
  TSUZURI_CLANG          Clang executable (default: clang; LLVM 17+)
  TSUZURI_WASM_LD        WebAssembly linker (default: wasm-ld)
  TSUZURI_LLVM_LINK      LLVM IR linker for macOS debug task objects and for --emit bitcode with the
                         C runtime (default: llvm-link; --emit bitcode also tries the directory of Clang)
  TSUZURI_DSYMUTIL       macOS debug symbol linker (default: dsymutil)
  TSUZURI_WASM_OPT       Binaryen optimizer that --wasm-opt runs (default: wasm-opt). Setting it alone
                         does not run anything
  Each tool comes from its variable, then a distribution's bin/, then PATH.
  TSUZURI_LLVM_PROFDATA  llvm-profdata that --profile-use runs to merge a directory of .profraw files
                         (default: the one beside Clang or in Clang's toolchain; PATH is not searched)
  TSUZURI_LINKER         Linker of a native executable or shared library: 'lld' makes Clang link with
                         LLD (-fuse-ld=lld; needs ld.lld or ld64.lld; the bundled Clang always does);
                         'default' or unset leaves it to Clang. Windows targets reject 'lld'.
  TSUZURI_SYSROOT        Sysroot of a TRIPLE target (its C headers and libraries): Clang gets
                         --sysroot, or -isysroot for Apple targets. A build for the machine
                         itself or for WebAssembly does not read it.
  TSUZURI_RUNNER         Command that runs the executables of a TRIPLE target this machine
                         cannot run, for run and test, such as 'qemu-aarch64 -L /usr/aarch64-linux-gnu'.
                         It is split at white space; nothing is downloaded or installed.

Code generation:
  TSUZURI_RANGE_FLAGS    off, proven, or audit (default: off). proven writes nsw/nuw on integer
                         + - * and on loop counters that a range proof shows cannot overflow;
                         audit keeps the wrapping instruction and traps where a proof is wrong,
                         for tests. Any other value is an error.
  TSUZURI_STRINGS        utf16 or compact (default: utf16). compact stores a string whose code
                         units are all at most 0xFF with one byte per unit (Latin-1) and any other
                         string as UTF-16. The values, the order, the hash, the output, and the
                         host ABI (always UTF-16) do not change. Any other value is an error.

Exports use the tz_ prefix in both C and WebAssembly. Native executables print
the numeric, bool, or UTF-8 string result of Main.tz's top-level code; unit
results do not print anything. 'def main :: unit -> i32' and
'def main :: Array<string> -> i32' print nothing and return the exit code; the
Array<string> form receives the command-line arguments.
Standard input and output and the File, Dir, Env, Time, Random, and Process APIs are
built in on native, and so are the Net sockets on macOS, Linux, and Windows; a GUI belongs to the
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
    /// `--profile-generate` or `--profile-use` (PR07).
    profile: driver::Profile,
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
    /// Report allocation counts from a separate counting runner.
    bench_allocs: bool,
    /// `tsuzuri test --seed N`: the property-test seed.
    test_seed: Option<u64>,
    /// `--no-server`: `check` and `build` run in this process even where a build server runs.
    no_server: bool,
    /// `--kg`, `--no-kg` and `--kg-dir` (G27): whether `check`, `build`, `run` and `test` write the
    /// knowledge graph, and where.
    kg: tsuzuri::kg::settings::KgFlags,
    /// `tsuzuri test --affected VALUE` (G28): run only the tests that the change since VALUE (a
    /// knowledge graph, a project, or a git revision) reaches.
    affected: Option<String>,
}

/// A diagnostic of the command line: it has no position, and the caller prints it at
/// `<command line>`.
fn command_line_error(kind: DiagnosticKind, message: impl Into<String>) -> Diagnostic {
    Diagnostic::of(kind, message, Span::default())
}

fn parse_arguments(arguments: &[OsString]) -> Result<Arguments, Diagnostic> {
    if arguments.first().is_some_and(|argument| argument == "lsp") {
        if arguments.len() != 1 {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "lsp takes no paths or build options",
            ));
        }
        return Ok(Arguments {
            action: Action::Lsp,
            input: PathBuf::new(),
            output: None,
            options: BuildOptions::default(),
            links: driver::LinkInputs::default(),
            profile: driver::Profile::None,
            json: false,
            deny_warnings: false,
            warn_implicit_copy: false,
            format_check: false,
            test_filter: None,
            test_list: false,
            test_indices: Vec::new(),
            coverage: None,
            bench_samples: None,
            bench_allocs: false,
            test_seed: None,
            no_server: false,
            kg: Default::default(),
            affected: None,
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
    let mut triple = None;
    let mut emit = None;
    let mut optimization = None;
    let mut backend = None;
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
    let mut bench_allocs = false;
    let mut test_seed = None;
    let mut debug_output = false;
    let mut trap_info = false;
    let mut trap_return = false;
    let mut allocator = None;
    let mut freestanding = false;
    let mut strip = false;
    let mut wasm_opt = false;
    let mut profile_generate = None;
    let mut profile_use = None;
    let mut debug_info = false;
    let mut wasm_simd = false;
    let mut wasm_threads = false;
    let mut wasm_jspi = false;
    let mut wasm_webgpu = false;
    let mut wasm_net = false;
    let mut wasm_tail_call = false;
    let mut wasm_host = None;
    let mut wasm_max_memory = None;
    let mut wasm_stack_size = None;
    let mut no_cache = false;
    let mut no_server = false;
    let mut kg = tsuzuri::kg::settings::KgFlags::default();
    let mut affected: Option<String> = None;
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
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "no-cache specified more than once",
                        ));
                    }
                    no_cache = true;
                    continue;
                }
                Some("--affected") => {
                    if affected.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "affected specified more than once",
                        ));
                    }
                    affected = Some(
                        next_value(arguments, &mut position, "--affected")?
                            .to_string_lossy()
                            .into_owned(),
                    );
                    continue;
                }
                Some("--kg") => {
                    if kg.kg {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "kg specified more than once",
                        ));
                    }
                    kg.kg = true;
                    continue;
                }
                Some("--no-kg") => {
                    if kg.no_kg {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "no-kg specified more than once",
                        ));
                    }
                    kg.no_kg = true;
                    continue;
                }
                Some("--kg-dir") => {
                    if kg.dir.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "kg-dir specified more than once",
                        ));
                    }
                    let value = next_value(arguments, &mut position, "--kg-dir")?;
                    if value.is_empty() {
                        return Err(command_line_error(
                            DiagnosticKind::OptionInvalidValue,
                            "--kg-dir must not be empty",
                        ));
                    }
                    kg.dir = Some(PathBuf::from(value));
                    continue;
                }
                Some("--no-server") => {
                    if no_server {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "no-server specified more than once",
                        ));
                    }
                    no_server = true;
                    continue;
                }
                Some("--deny-warnings") => {
                    if deny_warnings {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "deny-warnings specified more than once",
                        ));
                    }
                    deny_warnings = true;
                    continue;
                }
                Some("--warn") => {
                    let Some(name) = arguments.get(position) else {
                        return Err(command_line_error(
                            DiagnosticKind::OptionMissingValue,
                            "--warn requires a warning name; supported: implicit-copy",
                        ));
                    };
                    position += 1;
                    if name != "implicit-copy" {
                        return Err(command_line_error(
                            DiagnosticKind::OptionInvalidValue,
                            format!(
                                "unknown warning '{}' for --warn; supported: implicit-copy",
                                name.to_string_lossy()
                            ),
                        ));
                    }
                    if warn_implicit_copy {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "warn implicit-copy specified more than once",
                        ));
                    }
                    warn_implicit_copy = true;
                    continue;
                }
                Some("--check") => {
                    if format_check {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "check specified more than once",
                        ));
                    }
                    format_check = true;
                    continue;
                }
                Some("--list") => {
                    if test_list {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "list specified more than once",
                        ));
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
                        .ok_or_else(|| {
                            command_line_error(
                                DiagnosticKind::OptionInvalidValue,
                                "test index must be a nonnegative integer",
                            )
                        })?
                        .parse::<usize>()
                        .map_err(|_| {
                            command_line_error(
                                DiagnosticKind::OptionInvalidValue,
                                "test index is too large",
                            )
                        })?;
                    test_indices.push(value);
                    continue;
                }
                Some("--filter") => {
                    if test_filter.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "filter specified more than once",
                        ));
                    }
                    test_filter = Some(
                        next_value(arguments, &mut position, "--filter")?
                            .to_str()
                            .ok_or_else(|| {
                                command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "test filter must be UTF-8",
                                )
                            })?
                            .to_owned(),
                    );
                    continue;
                }
                Some("--allocs") => {
                    if bench_allocs {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "allocs specified more than once",
                        ));
                    }
                    bench_allocs = true;
                    continue;
                }
                Some("--samples") => {
                    if bench_samples.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "samples specified more than once",
                        ));
                    }
                    bench_samples = Some(
                        next_value(arguments, &mut position, "--samples")?
                            .to_str()
                            .filter(|value| {
                                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                            })
                            .and_then(|value| value.parse::<usize>().ok())
                            .filter(|samples| (1..=1000).contains(samples))
                            .ok_or_else(|| {
                                command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "bench samples must be an integer between 1 and 1000",
                                )
                            })?,
                    );
                    continue;
                }
                Some("--seed") => {
                    if test_seed.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "seed specified more than once",
                        ));
                    }
                    test_seed = Some(
                        next_value(arguments, &mut position, "--seed")?
                            .to_str()
                            .filter(|value| {
                                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                            })
                            .and_then(|value| value.parse::<u64>().ok())
                            .ok_or_else(|| {
                                command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "property seed must be an integer between 0 and 18446744073709551615",
                                )
                            })?,
                    );
                    continue;
                }
                Some("--coverage") => {
                    if coverage.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "coverage specified more than once",
                        ));
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
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "debug-output specified more than once",
                        ));
                    }
                    debug_output = true;
                    continue;
                }
                Some("--trap-info") => {
                    if trap_info {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "trap-info specified more than once",
                        ));
                    }
                    trap_info = true;
                    continue;
                }
                Some("--trap-mode") => {
                    if trap_return {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "trap mode specified more than once",
                        ));
                    }
                    match next_value(arguments, &mut position, "--trap-mode")?.to_str() {
                        Some("return") => trap_return = true,
                        _ => {
                            return Err(command_line_error(
                                DiagnosticKind::OptionInvalidValue,
                                "trap mode must be 'return'",
                            ));
                        }
                    }
                    continue;
                }
                Some("--allocator") => {
                    if allocator.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "allocator specified more than once",
                        ));
                    }
                    allocator = Some(
                        match next_value(arguments, &mut position, "--allocator")?.to_str() {
                            Some("system") => Allocator::System,
                            Some("host") => Allocator::Host,
                            Some("counting") => Allocator::Counting,
                            Some("small") => Allocator::Small,
                            _ => {
                                return Err(command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "allocator must be 'system', 'host', 'counting', or 'small'",
                                ));
                            }
                        },
                    );
                    continue;
                }
                Some("--freestanding") => {
                    if freestanding {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "freestanding specified more than once",
                        ));
                    }
                    freestanding = true;
                    continue;
                }
                Some("--strip") => {
                    if strip {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "strip specified more than once",
                        ));
                    }
                    strip = true;
                    continue;
                }
                Some("--wasm-opt") => {
                    if wasm_opt {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "wasm-opt specified more than once",
                        ));
                    }
                    wasm_opt = true;
                    continue;
                }
                Some("--profile-generate") => {
                    if profile_generate.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "profile generation specified more than once",
                        ));
                    }
                    profile_generate = Some(profile_path(
                        next_value(arguments, &mut position, "--profile-generate")?,
                        "--profile-generate",
                    )?);
                    continue;
                }
                Some("--profile-use") => {
                    if profile_use.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "profile use specified more than once",
                        ));
                    }
                    profile_use = Some(profile_path(
                        next_value(arguments, &mut position, "--profile-use")?,
                        "--profile-use",
                    )?);
                    continue;
                }
                Some("-g" | "--debug-info") => {
                    if debug_info {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "debug information specified more than once",
                        ));
                    }
                    debug_info = true;
                    continue;
                }
                Some("-o" | "--output") => {
                    if output.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "output specified more than once",
                        ));
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
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "target specified more than once",
                        ));
                    }
                    let value = next_value(arguments, &mut position, "--target")?;
                    let name = value.to_str().ok_or_else(|| {
                        command_line_error(
                            DiagnosticKind::OptionInvalidValue,
                            "target names are UTF-8; use native, wasm32, wasm64, or a TRIPLE",
                        )
                    })?;
                    let (named, named_triple) =
                        tsuzuri::target::parse_target(name).map_err(|message| {
                            command_line_error(DiagnosticKind::UnknownTarget, message)
                        })?;
                    target = Some(named);
                    triple = named_triple;
                    continue;
                }
                Some("--wasm-feature") => {
                    let feature = match next_value(arguments, &mut position, "--wasm-feature")?
                        .to_str()
                    {
                        Some("simd128") => &mut wasm_simd,
                        Some("threads") => &mut wasm_threads,
                        Some("jspi") => &mut wasm_jspi,
                        Some("webgpu") => &mut wasm_webgpu,
                        Some("net") => &mut wasm_net,
                        Some("tail-call") => &mut wasm_tail_call,
                        _ => {
                            return Err(command_line_error(
                                DiagnosticKind::OptionInvalidValue,
                                "supported WASM features are 'simd128', 'threads', 'jspi', 'webgpu', 'net', and 'tail-call'; relaxed SIMD is not supported",
                            ));
                        }
                    };
                    if *feature {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "WASM feature specified more than once",
                        ));
                    }
                    *feature = true;
                    continue;
                }
                Some("--wasm-host") => {
                    if wasm_host.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "--wasm-host specified more than once",
                        ));
                    }
                    wasm_host = Some(
                        match next_value(arguments, &mut position, "--wasm-host")?.to_str() {
                            Some("wasi") => WasmHost::Wasi,
                            other => {
                                return Err(command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    format!(
                                        "unknown wasm host '{}'; the supported host is wasi",
                                        other.unwrap_or("?")
                                    ),
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
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            format!("WASM {name} specified more than once"),
                        ));
                    }
                    *slot = Some(parse_size(
                        next_value(arguments, &mut position, option)?,
                        option,
                    )?);
                    continue;
                }
                Some("--emit") => {
                    if emit.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "emit kind specified more than once",
                        ));
                    }
                    emit = Some(
                        match next_value(arguments, &mut position, "--emit")?.to_str() {
                            Some("exe") => Emit::Executable,
                            Some("object") => Emit::Object,
                            Some("llvm") => Emit::Llvm,
                            Some("header") => Emit::Header,
                            Some("wasm") => Emit::Wasm,
                            Some("wgsl") => Emit::Wgsl,
                            Some("wgsl-relaxed") => Emit::WgslRelaxed,
                            Some("spirv") => Emit::Spirv,
                            Some("spirv-relaxed") => Emit::SpirvRelaxed,
                            Some("shared") => Emit::Shared,
                            Some("bitcode") => Emit::Bitcode,
                            Some("bindings-js") => Emit::BindingsJs,
                            Some("bindings-cs") => Emit::BindingsCs,
                            Some("bindings-py") => Emit::BindingsPy,
                            Some("bindings-cpp") => Emit::BindingsCpp,
                            _ => {
                                return Err(command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "emit kind must be exe, object, llvm, header, wasm, wgsl, wgsl-relaxed, spirv, spirv-relaxed, shared, bitcode, bindings-js, bindings-cs, bindings-py, or bindings-cpp",
                                ));
                            }
                        },
                    );
                    continue;
                }
                Some(level @ ("-O0" | "-O1" | "-O2" | "-O3" | "-Os" | "-Oz")) => {
                    if optimization.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "optimization specified more than once",
                        ));
                    }
                    optimization = OptLevel::from_flag(level);
                    continue;
                }
                Some("--backend") => {
                    if backend.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "backend specified more than once",
                        ));
                    }
                    backend = Some(
                        match next_value(arguments, &mut position, "--backend")?.to_str() {
                            Some("llvm") => Backend::Llvm,
                            Some("fast") => Backend::Fast,
                            _ => {
                                return Err(command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "backend must be 'llvm' or 'fast'",
                                ));
                            }
                        },
                    );
                    continue;
                }
                Some("--cpu") => {
                    if cpu.is_some() {
                        return Err(command_line_error(
                            DiagnosticKind::DuplicateOption,
                            "CPU tuning specified more than once",
                        ));
                    }
                    cpu = Some(
                        match next_value(arguments, &mut position, "--cpu")?.to_str() {
                            Some("generic") => Cpu::Generic,
                            Some("native") => Cpu::Native,
                            _ => {
                                return Err(command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "CPU tuning must be 'generic' or 'native'",
                                ));
                            }
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
                            .ok_or_else(|| {
                                command_line_error(
                                    DiagnosticKind::OptionInvalidValue,
                                    "library names must be UTF-8",
                                )
                            })?
                            .to_owned(),
                    );
                    continue;
                }
                Some(value) if value.starts_with('-') => {
                    return Err(command_line_error(
                        DiagnosticKind::UnknownOption,
                        format!("unknown option '{value}'; use --help"),
                    ));
                }
                _ => {}
            }
        }
        if input.replace(PathBuf::from(argument)).is_some() {
            return Err(command_line_error(
                DiagnosticKind::InputCountInvalid,
                "pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively",
            ));
        }
    }
    let input = input.ok_or_else(|| {
        command_line_error(
            DiagnosticKind::MissingInput,
            "missing .tz, .tt, or .tc input or project directory; use --help",
        )
    })?;
    if no_cache && !matches!(action, Action::Build | Action::Run) {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--no-cache is only valid with build, run, script, or repl",
        ));
    }
    if backend.is_some() && !matches!(action, Action::Build | Action::Run | Action::Test) {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--backend applies only to build, run, and test",
        ));
    }
    if no_server && !matches!(action, Action::Check | Action::Build) {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--no-server is only valid with check or build",
        ));
    }
    if emit.is_some_and(Emit::is_kernel)
        && (target.is_some() || optimization.is_some() || cpu.is_some())
    {
        return Err(command_line_error(
            DiagnosticKind::OptionRequiresBuild,
            if emit.is_some_and(Emit::is_spirv) {
                "SPIR-V output does not use target, optimization, or CPU options"
            } else {
                "WGSL output does not use target, optimization, or CPU options"
            },
        ));
    }
    if (wasm_simd || wasm_threads || wasm_jspi || wasm_webgpu || wasm_net || wasm_tail_call)
        && action != Action::Build
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--wasm-feature is only valid with build",
        ));
    }
    if wasm_host.is_some() && action != Action::Build {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--wasm-host is only valid with build",
        ));
    }
    if wasm_max_memory.is_some() || wasm_stack_size.is_some() {
        if !matches!(action, Action::Build | Action::Test) {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "--wasm-max-memory and --wasm-stack-size are only valid with build or test",
            ));
        }
        if action == Action::Test && !target.is_some_and(Target::is_wasm) {
            return Err(command_line_error(
                DiagnosticKind::OptionRequiresBuild,
                "--wasm-max-memory and --wasm-stack-size require --target wasm32 or wasm64",
            ));
        }
    }
    if debug_info && !matches!(action, Action::Build | Action::Run | Action::Test) {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--debug-info is only valid with build, run, or test",
        ));
    }
    // `test --index N -g -o PATH` builds the runner of one test for a debugger (G16 Phase 2).
    if action == Action::Test && (debug_info || output.is_some()) {
        if !debug_info || output.is_none() {
            return Err(command_line_error(
                DiagnosticKind::OptionRequiresOption,
                "debugging a test needs both -g and -o with the runner's path",
            ));
        }
        if test_indices.len() != 1 || test_list || test_filter.is_some() {
            return Err(command_line_error(
                DiagnosticKind::OptionConflict,
                "debugging a test needs exactly one --index, without --list or --filter",
            ));
        }
        if target.is_some_and(Target::is_wasm) {
            return Err(command_line_error(
                DiagnosticKind::OptionRequiresBuild,
                "debugging a test requires the native target",
            ));
        }
    }
    if debug_output && !matches!(action, Action::Build | Action::Run) {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--debug-output is only valid with build or run",
        ));
    }
    if trap_info && !matches!(action, Action::Build | Action::Run) {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--trap-info is only valid with build or run",
        ));
    }
    if trap_return && action != Action::Build {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--trap-mode is only valid with build",
        ));
    }
    if allocator.is_some()
        && action != Action::Build
        && !(matches!(action, Action::Run | Action::Test)
            && allocator == Some(tsuzuri::llvm::Allocator::Counting))
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--allocator is only valid with build",
        ));
    }
    if freestanding && action != Action::Build {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--freestanding is only valid with build",
        ));
    }
    if strip && action != Action::Build {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--strip is only valid with build",
        ));
    }
    if wasm_opt && action != Action::Build {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--wasm-opt is only valid with build",
        ));
    }
    if (profile_generate.is_some() || profile_use.is_some())
        && !matches!(action, Action::Build | Action::Run)
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--profile-generate and --profile-use are only valid with build or run",
        ));
    }
    if format_check && action != Action::Fmt {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--check is only valid with fmt",
        ));
    }
    if action == Action::Fmt
        && (optimization.is_some() || cpu.is_some() || deny_warnings || warn_implicit_copy)
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "fmt does not use optimization, CPU tuning, or compiler warning options",
        ));
    }
    if matches!(action, Action::Test | Action::Bench)
        && optimization.is_some_and(|level| level.speed_level().is_none())
    {
        let name = if action == Action::Test {
            "test"
        } else {
            "bench"
        };
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidValue,
            format!("{name} supports -O0 to -O3; use -Os or -Oz with build or run"),
        ));
    }
    if !matches!(
        action,
        Action::Check | Action::Build | Action::Run | Action::Test
    ) && (kg.kg || kg.no_kg || kg.dir.is_some())
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--kg, --no-kg, and --kg-dir are only valid with check, build, run, or test",
        ));
    }
    if affected.is_some() {
        if action != Action::Test {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "--affected is only valid with test",
            ));
        }
        if test_filter.is_some() || !test_indices.is_empty() {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "--affected cannot be combined with --filter or --index",
            ));
        }
        if kg.no_kg {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "--affected needs the KG; remove --no-kg or TSUZURI_KG=0",
            ));
        }
    }
    if !matches!(action, Action::Test | Action::Bench)
        && (test_filter.is_some() || test_list || !test_indices.is_empty())
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--filter, --list, and --index are only valid with test or bench",
        ));
    }
    if action == Action::Test && cpu.is_some() {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "test does not use CPU tuning",
        ));
    }
    if bench_samples.is_some() && action != Action::Bench {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--samples is only valid with bench",
        ));
    }
    if bench_allocs && action != Action::Bench {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--allocs is only valid with bench",
        ));
    }
    if test_seed.is_some() && action != Action::Test {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--seed is only valid with test",
        ));
    }
    if action == Action::Bench {
        if cpu.is_some() {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "bench does not use CPU tuning",
            ));
        }
        if target.is_some_and(Target::is_wasm) {
            return Err(command_line_error(
                DiagnosticKind::OptionRequiresBuild,
                "tsuzuri bench supports only the native target",
            ));
        }
    }
    if coverage.is_some() {
        if action != Action::Test {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "--coverage is only valid with test",
            ));
        }
        if target.is_some_and(Target::is_wasm) {
            return Err(command_line_error(
                DiagnosticKind::OptionRequiresBuild,
                "test coverage supports only the native target",
            ));
        }
        if test_list {
            return Err(command_line_error(
                DiagnosticKind::OptionConflict,
                "coverage cannot be combined with --list",
            ));
        }
        if debug_info {
            return Err(command_line_error(
                DiagnosticKind::OptionConflict,
                "coverage cannot be combined with -g; a debug runner only builds",
            ));
        }
    }
    if action == Action::Run && target.is_some_and(Target::is_wasm) {
        return Err(command_line_error(
            DiagnosticKind::OptionRequiresBuild,
            "run supports only a native target; build WebAssembly with 'tsuzuri build --target wasm32' and load the module in a host",
        ));
    }
    if output.is_some() && !matches!(action, Action::Build | Action::Doc | Action::Test)
        || emit.is_some() && action != Action::Build
        || target.is_some()
            && !matches!(
                action,
                Action::Build | Action::Test | Action::Bench | Action::Run
            )
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--output requires build or doc; --target and --emit require a supported build/test action",
        ));
    }
    if action == Action::Doc {
        if output.is_none() {
            return Err(command_line_error(
                DiagnosticKind::OptionRequiresOption,
                "doc requires -o or --output with an output directory",
            ));
        }
        if optimization.is_some() || cpu.is_some() {
            return Err(command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "doc does not use optimization or CPU tuning",
            ));
        }
    }
    if action == Action::Check && optimization.is_some() {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "check does not use an optimization level",
        ));
    }
    if action == Action::Check && cpu.is_some() {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "check does not use CPU tuning",
        ));
    }
    let target = target.unwrap_or(Target::Native);
    let backend = backend.unwrap_or_default();
    let options = BuildOptions {
        target,
        emit: emit.unwrap_or(if target.is_wasm() {
            Emit::Wasm
        } else {
            Emit::Executable
        }),
        // The fast backend translates -O0 IR, so -O0 is its default.
        optimization: optimization.unwrap_or(
            if action == Action::Test || backend == Backend::Fast {
                OptLevel::O0
            } else {
                OptLevel::O3
            },
        ),
        backend,
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
        wasm_webgpu,
        wasm_net,
        wasm_tail_call,
        wasm_host,
        wasm_max_memory,
        wasm_stack_size,
        cache: !no_cache,
        trap_return,
        allocator: allocator.unwrap_or_default(),
        freestanding,
        strip,
        wasm_opt,
        triple,
    };
    // These checks report `E2000` diagnostics without a position, as the command line does.
    options.validate()?;
    let profile = match (profile_generate, profile_use) {
        (Some(_), Some(_)) => {
            return Err(command_line_error(
                DiagnosticKind::OptionConflict,
                "--profile-generate and --profile-use cannot be combined; build once with each",
            ));
        }
        // A directory that cannot be made absolute is a value of the command line (`E2000`).
        (Some(directory), None) => driver::Profile::generate(&directory)
            .map_err(|error| error.recode(DiagnosticKind::OptionInvalidValue))?,
        (None, Some(path)) => driver::Profile::Use(path),
        (None, None) => driver::Profile::None,
    };
    profile.validate(options)?;
    if let Some(error) = output
        .as_deref()
        .and_then(|output| driver::bindings_output_error(options.emit, output))
    {
        return Err(error);
    }
    links.check_shape()?;
    if !links.is_empty() && !links_apply(action, &options) {
        return Err(command_line_error(
            DiagnosticKind::OptionRequiresBuild,
            "link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe",
        ));
    }
    Ok(Arguments {
        action,
        input,
        output,
        options,
        links,
        profile,
        json,
        deny_warnings,
        warn_implicit_copy,
        format_check,
        test_filter,
        test_list,
        test_indices,
        coverage,
        bench_samples,
        bench_allocs,
        test_seed,
        no_server,
        kg,
        affected,
    })
}

/// The value of `--profile-generate` or `--profile-use`: a path that is not empty.
fn profile_path(value: &OsStr, option: &str) -> Result<PathBuf, Diagnostic> {
    if value.is_empty() {
        return Err(command_line_error(
            DiagnosticKind::OptionMissingValue,
            format!("{option} needs a value"),
        ));
    }
    Ok(PathBuf::from(value))
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
) -> Result<&'a OsString, Diagnostic> {
    let value = arguments.get(*position).ok_or_else(|| {
        command_line_error(
            DiagnosticKind::OptionMissingValue,
            format!("{option} needs a value"),
        )
    })?;
    *position += 1;
    Ok(value)
}

/// Parses a byte count with an optional binary `KiB`, `MiB`, or `GiB` suffix.
fn parse_size(value: &OsStr, option: &str) -> Result<u64, Diagnostic> {
    value
        .to_str()
        .and_then(tsuzuri::package::parse_size)
        .ok_or_else(|| {
            let example = if option == "--wasm-stack-size" {
                "4194304 or 4MiB"
            } else {
                "67108864 or 64MiB"
            };
            command_line_error(
                DiagnosticKind::OptionInvalidValue,
                format!(
                    "{option} must be a byte count or a number followed by KiB, MiB, or GiB, such as {example}"
                ),
            )
        })
}

/// An invalid `TSUZURI_THREADS` is a command-line error, reported before the command reads a
/// source (exit code 2, like the other invalid settings).
fn invalid_thread_setting(err: &mut dyn Write, json: bool) -> Option<u8> {
    let error = tsuzuri::validate_thread_setting().err()?;
    write_diagnostic(err, &error, Path::new("<command line>"), "", json);
    Some(EXIT_USAGE)
}

fn print_diagnostic(error: &Diagnostic, input: &Path, source: &str, json: bool) {
    write_diagnostic(&mut io::stderr(), error, input, source, json);
}

fn print_diagnostics(errors: &tsuzuri::diagnostic::DiagnosticSet, project: &Project, json: bool) {
    write_diagnostics(&mut io::stderr(), errors, project, json);
}

fn write_diagnostic(
    err: &mut dyn Write,
    error: &Diagnostic,
    input: &Path,
    source: &str,
    json: bool,
) {
    write_with_severity(err, error.severity.as_str(), error, input, source, json);
}

fn write_diagnostics(
    err: &mut dyn Write,
    errors: &tsuzuri::diagnostic::DiagnosticSet,
    project: &Project,
    json: bool,
) {
    // The related locations and the edits of a diagnostic may be in other sources of the
    // project; the numbers are those of `Project::source_for`.
    let files: Vec<(String, &str)> = project
        .sources
        .iter()
        .chain(&project.manifests)
        .map(|file| (file.path.to_string_lossy().into_owned(), file.text.as_str()))
        .collect();
    let lookup = |index: usize| files.get(index).map(|(path, text)| (path.as_str(), *text));
    for (index, error) in errors.diagnostics.iter().enumerate() {
        if index != 0 && !json {
            errln!(err);
        }
        let source = project.source_for(error);
        let path = source.path.to_string_lossy();
        let severity = error.severity.as_str();
        errln!(
            err,
            "{}",
            if json {
                error.json_with(severity, &path, &source.text, &lookup)
            } else {
                error.render_with(severity, &path, &source.text, &lookup)
            }
        );
    }
    if let Some(note) = errors.omission_note() {
        if json {
            errln!(
                err,
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
            errln!(err, "\n{severity}: {note}");
        }
    }
}

fn write_with_severity(
    err: &mut dyn Write,
    severity: &str,
    diagnostic: &Diagnostic,
    input: &Path,
    source: &str,
    json: bool,
) {
    let path = input.to_string_lossy();
    errln!(
        err,
        "{}",
        if json {
            diagnostic.json_with_severity(severity, &path, source)
        } else {
            diagnostic.render_with_severity(severity, &path, source)
        }
    );
}

/// A tool message of a build, of a test or bench runner, or of a script run as a JSON line: a
/// `W2001` warning without a position.
fn tool_notice_json(message: &str) -> String {
    let kind = DiagnosticKind::ToolNotice;
    format!(
        "{{\"severity\":\"warning\",\"code\":\"{}\",\"message\":{},\"kind\":\"{}\"}}",
        kind.code(),
        json_string(message.trim()),
        kind.id()
    )
}

/// The tool messages of a build (W2001 as JSON) that follow a successful command.
fn write_messages(err: &mut dyn Write, messages: &[String], json: bool) {
    for message in messages {
        if json {
            errln!(err, "{}", tool_notice_json(message));
        } else {
            errln!(err, "{}", message.trim());
        }
    }
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
        Action::Build => driver::build_profiled(
            module,
            project,
            &arguments
                .output
                .clone()
                .unwrap_or_else(|| arguments.options.output_path(project.input())),
            arguments.options,
            links,
            &arguments.profile,
        ),
        Action::Run => driver::run_profiled(
            module,
            project,
            arguments.options,
            links,
            arguments.json,
            program_arguments,
            &arguments.profile,
        ),
    }
}

fn run_formatter(arguments: &Arguments) -> u8 {
    match driver::format_sources(&arguments.input, arguments.format_check) {
        Ok(changed) => {
            if arguments.format_check {
                for path in &changed {
                    if arguments.json {
                        let kind = DiagnosticKind::FileNotFormatted;
                        eprintln!(
                            "{{\"severity\":\"error\",\"code\":\"{}\",\"message\":\"file is not formatted\",\"path\":{},\"kind\":\"{}\"}}",
                            kind.code(),
                            json_string(&path.to_string_lossy()),
                            kind.id()
                        );
                    } else {
                        eprintln!("{}: not formatted", path.display());
                    }
                }
                if !changed.is_empty() {
                    return EXIT_FAILURE;
                }
            }
            EXIT_SUCCESS
        }
        Err(error) => {
            let source = driver::read_source(&error.path).unwrap_or_default();
            print_diagnostic(&error.diagnostic, &error.path, &source, arguments.json);
            EXIT_FAILURE
        }
    }
}

/// The `-O0` to `-O3` number of a `test` or `bench` build; `parse` rejects `-Os` and `-Oz` for both.
fn speed_level(optimization: OptLevel) -> u8 {
    optimization
        .speed_level()
        .unwrap_or_else(|| unreachable!("parse rejects -Os and -Oz for test and bench"))
}

fn run_test_action(
    arguments: &Arguments,
    project: &Project,
    module: &tsuzuri::check::CheckedModule,
    links: &driver::LinkInputs,
) -> u8 {
    let mut options = driver::TestOptions {
        target: arguments.options.target,
        optimization: speed_level(arguments.options.optimization),
        filter: arguments.test_filter.clone(),
        indices: arguments.test_indices.clone(),
        wasm_max_memory: arguments.options.wasm_max_memory,
        wasm_stack_size: arguments.options.wasm_stack_size,
        coverage: arguments.coverage.is_some(),
        seed: arguments.test_seed,
        triple: arguments.options.triple,
        backend: arguments.options.backend,
        allocator: arguments.options.allocator,
    };
    if let Some(base) = &arguments.affected {
        match select_affected(arguments, project, base) {
            Ok(Some(indices)) if indices.is_empty() => return EXIT_SUCCESS,
            Ok(Some(indices)) => options.indices = indices,
            Ok(None) => {}
            Err(error) => {
                print_diagnostic(&error, Path::new("<command line>"), "", arguments.json);
                return kg_command::exit_of(&error);
            }
        }
    }
    if options
        .indices
        .iter()
        .any(|index| *index >= module.tests.len())
    {
        print_diagnostic(
            &Diagnostic::of(
                DiagnosticKind::IndexOutOfRange,
                "test index is out of range; refresh the test list",
                Span::default(),
            ),
            project.input(),
            "",
            arguments.json,
        );
        return EXIT_FAILURE;
    }
    if arguments.options.debug_info {
        return debug_test(arguments, project, module, &options, links);
    }
    if arguments.test_list {
        for case in module.tests.iter().filter(|case| options.includes(case)) {
            if arguments.json {
                let source = project.source_for_span(case.span);
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
        return EXIT_SUCCESS;
    }
    if let Some(output) = &arguments.coverage
        && let Err(error) = driver::check_coverage_output(project, output)
    {
        print_diagnostic(&error, output, "", arguments.json);
        return EXIT_FAILURE;
    }
    let report = match driver::run_tests_linked(module, &options, links) {
        Ok(report) => report,
        Err(error) => {
            let source = project.source_for(&error);
            print_diagnostic(&error, &source.path, &source.text, arguments.json);
            return EXIT_FAILURE;
        }
    };
    for message in &report.messages {
        if arguments.json {
            eprintln!("{}", tool_notice_json(message));
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
        let diagnostic = Diagnostic::of(
            DiagnosticKind::TestsFailed,
            format!("{failed} test{} failed", if failed == 1 { "" } else { "s" }),
            first.case.span,
        );
        let source = project.source_for(&diagnostic);
        print_diagnostic(&diagnostic, &source.path, &source.text, arguments.json);
        EXIT_FAILURE
    } else if coverage_failed {
        EXIT_FAILURE
    } else {
        EXIT_SUCCESS
    }
}

/// Builds the runner of one test for a debugger and prints how to start it (G16 Phase 2).
fn debug_test(
    arguments: &Arguments,
    project: &Project,
    module: &tsuzuri::check::CheckedModule,
    options: &driver::TestOptions,
    links: &driver::LinkInputs,
) -> u8 {
    let output = arguments
        .output
        .as_ref()
        .expect("debugging a test requires output");
    let runner = match driver::build_debug_runner(module, project, options, links, output) {
        Ok(runner) => runner,
        Err(error) => {
            let source = project.source_for(&error);
            print_diagnostic(&error, &source.path, &source.text, arguments.json);
            return EXIT_FAILURE;
        }
    };
    for message in &runner.messages {
        if arguments.json {
            eprintln!("{}", tool_notice_json(message));
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
    EXIT_SUCCESS
}

/// `tsuzuri bench`: measures each selected bench in its own process, one at a time, and prints
/// the median, minimum, and maximum time per iteration. No result fails on speed.
fn run_bench_action(
    arguments: &Arguments,
    project: &Project,
    module: &tsuzuri::check::CheckedModule,
    links: &driver::LinkInputs,
) -> u8 {
    let options = driver::BenchOptions {
        target: arguments.options.target,
        optimization: speed_level(arguments.options.optimization),
        filter: arguments.test_filter.clone(),
        indices: arguments.test_indices.clone(),
        samples: arguments
            .bench_samples
            .unwrap_or(driver::DEFAULT_BENCH_SAMPLES),
        allocs: arguments.bench_allocs,
        triple: arguments.options.triple,
    };
    if options
        .indices
        .iter()
        .any(|index| *index >= module.benches.len())
    {
        print_diagnostic(
            &Diagnostic::of(
                DiagnosticKind::IndexOutOfRange,
                "bench index is out of range; refresh the bench list",
                Span::default(),
            ),
            project.input(),
            "",
            arguments.json,
        );
        return EXIT_FAILURE;
    }
    if arguments.test_list {
        for case in module.benches.iter().filter(|case| options.includes(case)) {
            if arguments.json {
                let source = project.source_for_span(case.span);
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
        return EXIT_SUCCESS;
    }
    let report = match driver::run_benches_linked(module, &options, links) {
        Ok(report) => report,
        Err(error) => {
            let source = project.source_for(&error);
            print_diagnostic(&error, &source.path, &source.text, arguments.json);
            return EXIT_FAILURE;
        }
    };
    for message in &report.messages {
        if arguments.json {
            eprintln!("{}", tool_notice_json(message));
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
                let counts = result.alloc_calls.zip(result.alloc_bytes);
                if arguments.json {
                    let allocations = counts.map_or_else(String::new, |(calls, bytes)| {
                        format!(",\"alloc_calls\":{calls},\"alloc_bytes\":{bytes}")
                    });
                    let samples = result
                        .samples_ms
                        .iter()
                        .map(|sample| number(*sample))
                        .collect::<Vec<_>>()
                        .join(",");
                    println!(
                        "{{\"type\":\"bench\",\"index\":{},\"module\":{},\"name\":{},\"status\":\"passed\",\"workload\":{},\"target\":\"native\",\"opt\":\"O{}\",\"cpu_mode\":\"generic\",\"metric\":\"wall_time\",\"unit\":\"ms\",\"iterations\":{},\"samples\":[{samples}],\"median\":{},\"min\":{},\"max\":{}{allocations}}}",
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
                    let allocations = counts.map_or_else(String::new, |(calls, bytes)| {
                        format!(", alloc_calls {calls}, alloc_bytes {bytes} per iteration")
                    });
                    let (scale, unit) = bench_unit(median);
                    println!(
                        "bench {} {}: median {:.3} {unit} (min {:.3} {unit}, max {:.3} {unit}; {} samples of {} iterations){allocations}",
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
        let diagnostic = Diagnostic::of(
            DiagnosticKind::BenchmarkFailed,
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
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
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
        ("TSUZURI_WASM_OPT", "wasm-opt"),
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
    let mut agent_files = true;
    let mut rest = arguments.iter();
    let mut valid = true;
    while let Some(argument) = rest.next() {
        if argument == "--no-agent-files" && agent_files {
            agent_files = false;
        } else if argument == "--namespace" && namespace.is_none() {
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
            &Diagnostic::of(
                DiagnosticKind::UsageError,
                "usage: tsuzuri new directory [--namespace NAME] [--no-agent-files]",
                Span::default(),
            ),
            Path::new("<command line>"),
            "",
            false,
        );
        return ExitCode::from(2);
    };
    match tsuzuri::package::create_project(&directory, namespace.as_deref(), agent_files) {
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
    let usage = |kind: DiagnosticKind, message: &str| {
        print_diagnostic(
            &command_line_error(kind, message),
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
        return usage(
            DiagnosticKind::UsageError,
            "fetch takes one project directory and optionally --json",
        );
    }
    let directory = Path::new(paths[0]);
    if !directory.join("Tsuzuri.toml").is_file() {
        return usage(
            DiagnosticKind::ProjectManifestMissing,
            "fetch requires a Tsuzuri.toml in the project directory",
        );
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
    let usage = |kind: DiagnosticKind, message: &str| {
        print_diagnostic(
            &command_line_error(kind, message),
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
            DiagnosticKind::UsageError,
            "publish takes one package directory, --git URL, --rev COMMIT, and optionally --json",
        );
    };
    if !tsuzuri::package::valid_git_url(&url) {
        return usage(
            DiagnosticKind::InvalidPublishGitUrl,
            tsuzuri::package::GIT_URL_RULE,
        );
    }
    if !tsuzuri::package::valid_rev(&rev) {
        return usage(
            DiagnosticKind::InvalidPublishGitRev,
            tsuzuri::package::GIT_REV_RULE,
        );
    }
    if !directory.join("Tsuzuri.toml").is_file() {
        return usage(
            DiagnosticKind::ProjectManifestMissing,
            "publish requires a Tsuzuri.toml in the package directory",
        );
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

/// `tsuzuri api-diff old new [--effective] [--json]`: how the public API of the package or project `new`
/// differs from `old`, one line per change on standard output. The exit status is 1 when a
/// change is breaking, and 2 for a usage error.
fn api_diff_command(arguments: &[OsString]) -> ExitCode {
    let mut json = false;
    let mut effective = false;
    let mut paths = Vec::new();
    let mut paths_only = false;
    let mut valid = true;
    for argument in arguments {
        match argument.to_str() {
            Some("--") if !paths_only => paths_only = true,
            Some("--json") if !paths_only => {
                valid &= !json;
                json = true;
            }
            Some("--effective") if !paths_only => {
                valid &= !effective;
                effective = true;
            }
            _ if !paths_only && argument.to_string_lossy().starts_with('-') => valid = false,
            _ => paths.push(PathBuf::from(argument)),
        }
    }
    let [old, new] = paths.as_slice() else {
        return api_diff_usage(json);
    };
    if !valid {
        return api_diff_usage(json);
    }
    if effective && let Some(code) = invalid_thread_setting(&mut io::stderr(), json) {
        return ExitCode::from(code);
    }
    let mut apis = Vec::new();
    for input in [old, new] {
        let loaded = if effective {
            Project::load_for_docs(input)
        } else {
            Project::load_for_api(input)
        };
        let project = match loaded {
            Ok(project) => project,
            Err(error) => {
                print_diagnostic(&error.diagnostic, &error.path, "", json);
                return ExitCode::FAILURE;
            }
        };
        let api = if effective {
            tsuzuri::api_diff::effective_api(&project)
        } else {
            tsuzuri::api_diff::api(&project)
        };
        match api {
            Ok(api) => apis.push(api),
            Err(error) => {
                let source = project.source_for(&error);
                print_diagnostic(&error, &source.path, &source.text, json);
                return ExitCode::FAILURE;
            }
        }
    }
    let report = tsuzuri::api_diff::compare(&apis[0], &apis[1]);
    print!(
        "{}",
        if json {
            report.render_json()
        } else {
            report.render_text()
        }
    );
    if report.breaking() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn api_diff_usage(json: bool) -> ExitCode {
    print_diagnostic(
        &Diagnostic::of(
            DiagnosticKind::UsageError,
            "usage: tsuzuri api-diff old new [--effective] [--json]; old and new are two directories or files",
            Span::default(),
        ),
        Path::new("<command line>"),
        "",
        json,
    );
    ExitCode::from(2)
}

/// `tsuzuri guide`: the guide for AI agents (G21), on standard output. The exit status is 2 for a
/// usage error and 1 when `--agents` or `--llms-txt` finds no readable manifest.
fn guide_command(arguments: &[OsString]) -> ExitCode {
    use tsuzuri::guide::{self, Action, Values};
    let words: Vec<String> = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    let json = words.iter().any(|word| word == "--json");
    let error = |diagnostic: &Diagnostic, code: u8| {
        print_diagnostic(diagnostic, Path::new("<command line>"), "", json);
        ExitCode::from(code)
    };
    let invocation = match guide::parse_arguments(&words) {
        Ok(invocation) => invocation,
        Err(diagnostic) => return error(&diagnostic, 2),
    };
    // The edition of the project in the working directory, else the latest.
    let edition = invocation.edition.unwrap_or_else(|| {
        guide::project_values(Path::new("."))
            .map_or(tsuzuri::syntax::Edition::LATEST, |values| values.2)
    });
    let text = match &invocation.action {
        Action::List if json => guide::list_json(edition),
        Action::List => guide::list_text(edition),
        Action::Agents(directory) | Action::LlmsTxt(directory) => {
            let (name, namespace, edition) = match guide::project_values(Path::new(directory)) {
                Ok(values) => values,
                Err(diagnostic) => return error(&diagnostic, 1),
            };
            let values = Values {
                name: &name,
                namespace: &namespace,
                edition,
                version: env!("CARGO_PKG_VERSION"),
            };
            if matches!(invocation.action, Action::Agents(_)) {
                guide::agents_md(&values)
            } else {
                guide::llms_txt(&values)
            }
        }
        action => match guide::render(action, edition) {
            Ok(rendered) if json => guide::json_text(&rendered, edition),
            Ok(rendered) => rendered.text,
            Err(diagnostic) => return error(&diagnostic, 2),
        },
    };
    print!("{text}");
    ExitCode::SUCCESS
}

/// `tsuzuri explain TARGET [--json]` and `tsuzuri explain --catalog [--json]`: the explanation of a
/// diagnostic kind or code, or the list of every kind, on standard output. The exit status is 2
/// for a usage error and for a target that is neither a kind nor a code.
fn explain_command(arguments: &[OsString]) -> ExitCode {
    use tsuzuri::explain::{self, Target};
    let mut json = false;
    let mut catalog = false;
    let mut unknown = None;
    let mut targets = Vec::new();
    for argument in arguments {
        let text = argument.to_string_lossy();
        match text.as_ref() {
            "--json" => json = true,
            "--catalog" => catalog = true,
            option if option.starts_with('-') => {
                unknown.get_or_insert_with(|| option.to_string());
            }
            _ => targets.push(text.into_owned()),
        }
    }
    let error = |kind: DiagnosticKind, message: String| {
        print_diagnostic(
            &Diagnostic::of(kind, message, Span::default()),
            Path::new("<command line>"),
            "",
            json,
        );
        ExitCode::from(2)
    };
    if let Some(option) = unknown {
        return error(
            DiagnosticKind::UnknownOption,
            format!("unknown option '{option}'; use --help"),
        );
    }
    if catalog {
        if !targets.is_empty() {
            return error(
                DiagnosticKind::OptionConflict,
                "--catalog cannot be combined with a target; list the kinds, or explain one"
                    .to_string(),
            );
        }
        print!("{}", explain::render_catalog(json));
        return ExitCode::SUCCESS;
    }
    let [target] = targets.as_slice() else {
        return error(
            DiagnosticKind::UsageError,
            "usage: tsuzuri explain TARGET [--json] | tsuzuri explain --catalog [--json]"
                .to_string(),
        );
    };
    match explain::resolve(target) {
        Ok(Target::Kind(kind)) => print!("{}", explain::render_kind(kind, json)),
        Ok(Target::Code(code)) => print!("{}", explain::render_code(code, json)),
        Err(candidates) => {
            return error(
                DiagnosticKind::UnknownExplainTarget,
                explain::unknown_target_message(target, &candidates),
            );
        }
    }
    ExitCode::SUCCESS
}

/// `tsuzuri fix [--applicability certain|likely|maybe] [--dry-run] [--json] PATH` applies the fixes
/// of the diagnostics (G24); `tsuzuri fix --edition YEAR [--from YEAR] [--dry-run] [--json] PATH`
/// migrates the input to an edition (A18). The exit status is 2 for a usage error.
fn fix_command(arguments: &[OsString]) -> ExitCode {
    let json = arguments
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json");
    let arguments = match tsuzuri::fix::parse_arguments(arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            print_diagnostic(&error, Path::new("<command line>"), "", json);
            return ExitCode::from(2);
        }
    };
    if let Some(code) = invalid_thread_setting(&mut io::stderr(), json) {
        return ExitCode::from(code);
    }
    if let Some(target) = arguments.edition {
        return fix_edition_command(
            &arguments.input,
            target,
            arguments.from,
            arguments.dry_run,
            json,
        );
    }
    let mut project = match Project::load(&arguments.input) {
        Ok(project) => project,
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", json);
            return ExitCode::FAILURE;
        }
    };
    match tsuzuri::fix::execute(&mut project, &arguments) {
        Ok(done) => {
            print!("{}", done.stdout);
            if let Some(remaining) = done.remaining {
                print_diagnostics(&remaining.errors, &project, json);
                let warnings =
                    tsuzuri::diagnostic::DiagnosticSet::from_diagnostics(remaining.warnings, 0);
                print_diagnostics(&warnings, &project, json);
            }
            ExitCode::from(done.exit)
        }
        Err(error) => {
            print_diagnostic(&error.diagnostic, &error.path, "", json);
            ExitCode::from(if error.diagnostic.code == "E2000" {
                2
            } else {
                1
            })
        }
    }
}

/// `tsuzuri fix --edition YEAR`: rewrites the input so that it keeps its meaning in `target`,
/// then updates the package's edition.
fn fix_edition_command(
    path: &Path,
    target: tsuzuri::syntax::Edition,
    from: Option<tsuzuri::syntax::Edition>,
    dry_run: bool,
    json: bool,
) -> ExitCode {
    let result = tsuzuri::edition_migration::prepare(path, target, from).and_then(|migration| {
        if !dry_run {
            tsuzuri::edition_migration::apply(&migration)?;
        }
        Ok(migration)
    });
    match result {
        Err(error) => {
            let text = std::fs::read_to_string(&error.path).unwrap_or_default();
            print_diagnostic(&error.diagnostic, &error.path, &text, json);
            ExitCode::FAILURE
        }
        Ok(migration) => {
            for change in &migration.changes {
                if json {
                    println!(
                        "{{\"type\":\"edition_edit\",\"path\":{},\"dry_run\":{},\"before\":{},\"after\":{}}}",
                        json_string(&change.path.to_string_lossy()),
                        dry_run,
                        json_string(&change.before),
                        json_string(&change.after)
                    );
                } else if dry_run {
                    println!(
                        "--- {}\n+++ {}\n@@ -1,{} +1,{} @@",
                        change.path.display(),
                        change.path.display(),
                        change.before.lines().count(),
                        change.after.lines().count()
                    );
                    for line in change.before.lines() {
                        println!("-{line}");
                    }
                    for line in change.after.lines() {
                        println!("+{line}");
                    }
                } else {
                    println!("migrated {}", change.path.display());
                }
            }
            if json {
                println!(
                    "{{\"type\":\"edition_summary\",\"from\":{},\"target\":{},\"files\":{},\"manifest\":{},\"dry_run\":{}}}",
                    json_string(&migration.source_edition.to_string()),
                    json_string(&migration.target.to_string()),
                    migration.changes.len(),
                    migration.manifest,
                    dry_run
                );
            } else if migration.source_edition == migration.target {
                println!("edition is already {target}; nothing to migrate");
            } else if !migration.manifest {
                println!(
                    "edition: source prepared for {target}; no manifest changed; migrate the project directory to update its edition"
                );
            }
            ExitCode::SUCCESS
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
                &command_line_error(DiagnosticKind::BindgenArgumentError, message),
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
fn parse_repl_arguments(arguments: &[OsString]) -> Result<tsuzuri::repl::ReplOptions, Diagnostic> {
    let mut options = tsuzuri::repl::ReplOptions::default();
    let mut seen = BTreeSet::new();
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let option = match argument.to_str() {
            Some("--target") => {
                return Err(command_line_error(
                    DiagnosticKind::OptionRequiresBuild,
                    "repl supports only the native target; build WebAssembly with 'tsuzuri build --target wasm32'",
                ));
            }
            Some(level @ ("-O0" | "-O1" | "-O2" | "-O3")) => {
                if let Some(level) = OptLevel::from_flag(level) {
                    options.build.optimization = level;
                }
                "optimization"
            }
            Some("--cpu") => {
                options.build.cpu = match rest.next().and_then(|value| value.to_str()) {
                    Some("generic") => Cpu::Generic,
                    Some("native") => Cpu::Native,
                    _ => {
                        return Err(command_line_error(
                            DiagnosticKind::OptionInvalidValue,
                            "CPU tuning must be 'generic' or 'native'",
                        ));
                    }
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
                    .ok_or_else(|| {
                        command_line_error(DiagnosticKind::OptionInvalidValue, REPL_TIMEOUT)
                    })?;
                options.timeout = (seconds != 0).then(|| std::time::Duration::from_secs(seconds));
                "timeout"
            }
            _ => return Err(command_line_error(DiagnosticKind::UsageError, REPL_USAGE)),
        };
        if !seen.insert(option) {
            return Err(command_line_error(
                DiagnosticKind::DuplicateOption,
                format!("{option} specified more than once"),
            ));
        }
    }
    Ok(options)
}

/// `tsuzuri repl`: evaluates the inputs on stdin until `:quit` or its end (G13).
fn repl_command(arguments: &[OsString]) -> ExitCode {
    let options = match parse_repl_arguments(arguments) {
        Ok(options) => options,
        Err(error) => {
            print_diagnostic(&error, Path::new("<command line>"), "", false);
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
                &Diagnostic::of(
                    DiagnosticKind::ReplStreamFailure,
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
fn script_arguments(arguments: &[OsString]) -> Result<(Vec<OsString>, Vec<OsString>), Diagnostic> {
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
                        .ok_or_else(|| {
                            command_line_error(
                                DiagnosticKind::OptionMissingValue,
                                format!("{option} needs a value"),
                            )
                        })?
                        .clone(),
                );
                continue;
            }
            Some(option) if option.starts_with('-') => {
                return Err(command_line_error(
                    DiagnosticKind::UsageError,
                    SCRIPT_OPTIONS,
                ));
            }
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
    Err(command_line_error(DiagnosticKind::UsageError, SCRIPT_FILE))
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

/// The help text, ending with the targets of `--target TRIPLE` as the compiler's table has them.
fn help() -> String {
    format!(
        "{HELP}\n\nTRIPLE (64-bit native targets only; 32-bit ones are not supported):\n  {}",
        Triple::names().replace(", ", "\n  ")
    )
}

fn main() -> ExitCode {
    let mut time_passes = tsuzuri::timings::Report::start();
    let raw: Vec<_> = env::args_os().skip(1).collect();
    if raw.is_empty() {
        eprintln!("{}", help());
        return ExitCode::from(2);
    }
    // `tsuzuri script` is `tsuzuri run` of one file; the program's arguments are set aside first.
    let (raw, program_arguments) = if raw.first().is_some_and(|command| *command == "script") {
        match script_arguments(&raw[1..]) {
            Ok((run, program)) => (run, Some(program)),
            Err(error) => {
                print_diagnostic(
                    &error,
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
        println!("{}", help());
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
    if raw.first().is_some_and(|command| *command == "api-diff") {
        return api_diff_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "kg") {
        return kg_command::kg_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "explain") {
        return explain_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "guide") {
        return guide_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "fix") {
        return fix_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "bindgen") {
        return bindgen_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "repl") {
        if let Some(code) = invalid_thread_setting(&mut io::stderr(), false) {
            return ExitCode::from(code);
        }
        return repl_command(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "watch") {
        time_passes.disarm();
        return watch_main(&raw[1..]);
    }
    if raw.first().is_some_and(|command| *command == "serve") {
        time_passes.disarm();
        return serve_main(&raw[1..]);
    }
    ExitCode::from(cli(
        &raw,
        program_arguments.as_deref(),
        &mut io::stdout(),
        &mut io::stderr(),
    ))
}

/// Runs the command line `raw` that is a project command: everything after the commands that need
/// no project (`--help`, `new`, `repl`, ...), from `parse_arguments` on. Its diagnostics and
/// warnings go to `err`. `out` is the stream of the result that a command prints: `check` and
/// `build` print none, and `test`, `bench`, `fmt`, and `run` write to the process's own standard
/// output, so a caller that captures `out` serves `check` and `build` only.
fn cli(
    raw: &[OsString],
    program_arguments: Option<&[OsString]>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let json = raw
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json");
    let arguments = match parse_arguments(raw) {
        Ok(arguments) => arguments,
        Err(error) => {
            write_diagnostic(err, &error, Path::new("<command line>"), "", json);
            return EXIT_USAGE;
        }
    };
    if arguments.action != Action::Fmt
        && let Some(code) = invalid_thread_setting(err, json)
    {
        return code;
    }
    // G27: the knowledge graph of check, build, run and test (not of a script). The settings are
    // checked before any source is read.
    let kg_settings = if matches!(
        arguments.action,
        Action::Check | Action::Build | Action::Run | Action::Test
    ) && program_arguments.is_none()
    {
        match tsuzuri::kg::settings::resolve(
            &arguments.kg,
            env::var_os("TSUZURI_KG").as_deref(),
            env::var_os("TSUZURI_KG_DIR").as_deref(),
        ) {
            Ok(settings) => Some(settings),
            Err(error) => {
                write_diagnostic(err, &error, Path::new("<command line>"), "", json);
                return EXIT_USAGE;
            }
        }
    } else {
        None
    };
    if arguments.affected.is_some()
        && kg_settings
            .as_ref()
            .is_some_and(|settings| settings.mode == tsuzuri::kg::settings::KgMode::Off)
    {
        write_diagnostic(
            err,
            &command_line_error(
                DiagnosticKind::OptionInvalidForCommand,
                "--affected needs the KG; remove --no-kg or TSUZURI_KG=0",
            ),
            Path::new("<command line>"),
            "",
            json,
        );
        return EXIT_USAGE;
    }
    if arguments.action == Action::Lsp {
        return tsuzuri::lsp::serve(io::stdin(), out, err) as u8;
    }
    if arguments.action == Action::Fmt {
        return run_formatter(&arguments);
    }
    // A build server that runs this project takes the command, and gives back what it printed.
    // A process that is one itself (`watch`, `serve`) never asks.
    if !arguments.no_server
        && matches!(arguments.action, Action::Check | Action::Build)
        && !tsuzuri::resident::is_active()
        && let Some(response) = tsuzuri::server::request(&arguments.input, raw)
    {
        tsuzuri::timings::silence_report();
        let _ = out.write_all(response.stdout.as_bytes());
        let _ = err.write_all(response.stderr.as_bytes());
        return response.exit;
    }
    let load_time = tsuzuri::timings::Phase::start("load");
    let loaded = if matches!(arguments.action, Action::Test | Action::Bench) {
        Project::load_for_tests(&arguments.input)
    } else if arguments.action == Action::Doc {
        Project::load_for_docs(&arguments.input)
    } else if program_arguments.is_some() {
        Project::load_script(&arguments.input)
    } else {
        Project::load(&arguments.input)
    };
    drop(load_time);
    let project = match loaded {
        Ok(project) => project,
        Err(error) => {
            write_diagnostic(err, &error.diagnostic, &error.path, "", arguments.json);
            return EXIT_FAILURE;
        }
    };
    let mut arguments = arguments;
    arguments.options = arguments.options.with_manifest_wasm(project.wasm);
    // Command-line values were validated during parsing, so only manifest values can fail here.
    if let Err(mut error) = arguments.options.validate() {
        // The diagnostic keeps its code and kind; the message names where the setting came from.
        error.message = format!(
            "{} (after applying the root package's [wasm])",
            error.message
        );
        error.span = Span::default();
        write_diagnostic(err, &error, Path::new("Tsuzuri.toml"), "", arguments.json);
        return EXIT_FAILURE;
    }
    let kind = match arguments.action {
        // bench loads the same sources as test (`Project::load_for_tests`).
        Action::Test | Action::Bench => driver::AnalysisKind::Tests,
        Action::Doc => driver::AnalysisKind::Docs,
        _ => driver::AnalysisKind::Program,
    };
    // G27: a graph that is up to date is left alone; the analysis captures what a new one needs.
    let mut kg_notice = None;
    let plan =
        match kg_settings.map(|settings| tsuzuri::kg::build::plan(&settings, &project, false)) {
            None => tsuzuri::kg::build::Plan::Off,
            Some(Ok(plan)) => plan,
            Some(Err(error)) => {
                kg_notice = Some(tsuzuri::kg::build::notice(&error));
                tsuzuri::kg::build::Plan::Off
            }
        };
    // check, test and doc do not take --no-cache; TSUZURI_CACHE_DIR= disables every cache.
    let (analysis, pending) =
        tsuzuri::kg::build::analyze(&project, arguments.options.cache, kind, plan);
    let module = match analysis {
        Ok(module) => module,
        Err(errors) => {
            write_diagnostics(err, &errors, &project, arguments.json);
            if let Some(pending) = pending {
                kg_notice = tsuzuri::kg::build::complete(pending, &project, Err(&errors));
            }
            write_messages(err, kg_notice.as_slice(), arguments.json);
            return EXIT_FAILURE;
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
    write_diagnostics(err, &warnings, &project, arguments.json);
    if let Some(pending) = pending {
        kg_notice = tsuzuri::kg::build::complete(pending, &project, Ok(&module));
    }
    write_messages(err, kg_notice.as_slice(), arguments.json);
    if arguments.deny_warnings && !warnings.is_empty() {
        return EXIT_FAILURE;
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
        program_arguments.unwrap_or_default(),
    );
    match result {
        Ok(messages) => {
            write_messages(err, &messages, arguments.json);
            EXIT_SUCCESS
        }
        Err(error) => {
            let source = project.source_for(&error);
            write_diagnostic(err, &error, &source.path, &source.text, arguments.json);
            EXIT_FAILURE
        }
    }
}

/// G28 `test --affected VALUE`: the indices of the tests that the change from VALUE to the project
/// reaches, after printing the `affected:` line; `None` runs every test.
fn select_affected(
    arguments: &Arguments,
    project: &Project,
    value: &str,
) -> Result<Option<Vec<usize>>, Diagnostic> {
    use tsuzuri::kg::open::{self, Refresh};
    let given = Path::new(value);
    let report = |set: &tsuzuri::diagnostic::DiagnosticSet, project: &Project| {
        print_diagnostics(set, project, arguments.json)
    };
    // The checkout lives until the diff has read the base graph's files.
    let mut checkout = None;
    let base = if given.is_dir() && tsuzuri::kg::write::has_marker(given) {
        open::read(given)?
    } else if given.is_dir() {
        let base = Project::load_for_tests(given).map_err(|error| error.diagnostic)?;
        let dir = open::own_directory(&base);
        open::open_project(&base, &dir, Refresh::Auto, &mut |set, project| {
            report(set, project)
        })?
    } else {
        let root = project
            .project_directory()
            .unwrap_or_else(|| PathBuf::from("."));
        let made = tsuzuri::kg::git::Checkout::create(&root, value)?;
        let base =
            Project::load_for_tests(made.project_root()).map_err(|error| error.diagnostic)?;
        let dir = open::own_directory(&base);
        let data = open::open_project(&base, &dir, Refresh::Auto, &mut |set, project| {
            report(set, project)
        })?;
        checkout = Some(made);
        data
    };
    let dir = open::directory(project, arguments.kg.dir.as_deref());
    if base
        .directory()
        .is_some_and(|base| open::same_directory(base, &dir))
    {
        base.read_all()?;
    }
    let head = open::open_project(project, &dir, Refresh::Auto, &mut |set, project| {
        report(set, project)
    })?;
    let impact = tsuzuri::kg::impact::diff(&base, &head, value, "working tree");
    base.intact()?;
    head.intact()?;
    drop(checkout);
    let total = impact.total_tests;
    match &impact.tests {
        tsuzuri::kg::impact::Tests::All(reason) => {
            if arguments.json {
                println!(
                    "{{\"type\":\"affected\",\"mode\":\"all\",\"affected\":{total},\"total\":{total},\"base\":{},\"reason\":{}}}",
                    json_string(value),
                    json_string(reason)
                );
            } else {
                println!("affected: all {total} tests ({reason})");
            }
            Ok(None)
        }
        tsuzuri::kg::impact::Tests::Some(tests) => {
            let mut indices: Vec<usize> =
                tests.iter().filter_map(|id| head.test_index(id)).collect();
            indices.sort_unstable();
            indices.dedup();
            if arguments.json {
                println!(
                    "{{\"type\":\"affected\",\"mode\":\"some\",\"affected\":{},\"total\":{total},\"base\":{},\"reason\":null}}",
                    indices.len(),
                    json_string(value)
                );
            } else {
                println!("affected: {} of {total} tests ({value})", indices.len());
            }
            Ok(Some(indices))
        }
    }
}

/// The arguments of `tsuzuri watch ARGS`, `ARGS` being those of `check`, `build` or `test`. The
/// command is named, so that `watch app` is not read as a build of `app`.
fn parse_watch_arguments(arguments: &[OsString]) -> Result<Arguments, Diagnostic> {
    match arguments.first().and_then(|command| command.to_str()) {
        Some("check" | "build" | "test") => parse_arguments(arguments),
        _ => Err(command_line_error(
            DiagnosticKind::UsageError,
            "watch runs check, build, or test; use tsuzuri watch check <path>",
        )),
    }
}

/// `tsuzuri watch ARGS`: runs `ARGS` (a `check`, `build` or `test` command) and again each time
/// that a file it reads changes. After each run it writes a status line to standard error. It
/// ends only when it is stopped.
fn watch_main(arguments: &[OsString]) -> ExitCode {
    let json = arguments
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json");
    let parsed = match parse_watch_arguments(arguments) {
        Ok(parsed) => parsed,
        Err(error) => {
            print_diagnostic(&error, Path::new("<command line>"), "", json);
            return ExitCode::from(EXIT_USAGE);
        }
    };
    // Every run would report the same invalid setting.
    if let Some(code) = invalid_thread_setting(&mut io::stderr(), json) {
        return ExitCode::from(code);
    }
    // The snapshot comes before the run: a save during the run starts another run. The state
    // remembers the sources that the runs and the snapshots read, so that a run opens only the
    // files that changed, and pins the compiler that started the watch.
    let mut resident = tsuzuri::resident::ResidentState::new();
    let mut watcher = {
        let _scope = tsuzuri::resident::Scope::enter(&mut resident);
        tsuzuri::watch::Watcher::new(&parsed.input, parsed.action == Action::Test)
    };
    let mut run = 0_u64;
    loop {
        // A rebuilt compiler is not the one that this process runs. Nothing starts with the old one
        // once the file was replaced: its run would show the result of the old compiler to somebody
        // who waits for the new one's. The wait below ends at once for the same reason.
        if resident.compiler_changed() {
            if json {
                errln!(
                    io::stderr(),
                    "{{\"type\":\"watch\",\"stopped\":\"compiler-changed\"}}"
                );
            } else {
                errln!(
                    io::stderr(),
                    "watch: the compiler binary changed; stopping (start tsuzuri watch again)"
                );
            }
            return ExitCode::from(EXIT_FAILURE);
        }
        run += 1;
        let code = {
            let _scope = tsuzuri::resident::Scope::enter(&mut resident);
            let _report = tsuzuri::timings::Report::start();
            cli(arguments, None, &mut io::stdout(), &mut io::stderr())
        };
        if json {
            errln!(
                io::stderr(),
                "{{\"type\":\"watch\",\"run\":{run},\"exit\":{code}}}"
            );
        } else {
            errln!(
                io::stderr(),
                "watch: run {run} finished with exit code {code}"
            );
        }
        resident.trim();
        let _scope = tsuzuri::resident::Scope::enter(&mut resident);
        watcher.wait_for_change();
    }
}

const SERVE_USAGE: &str = "serve takes one project directory: tsuzuri serve DIR [--idle-timeout SECONDS], tsuzuri serve --status DIR, or tsuzuri serve --stop DIR";

/// What `tsuzuri serve` was asked to do.
#[derive(Debug, PartialEq, Eq)]
enum ServeMode {
    Run { input: PathBuf, idle: u64 },
    Status(PathBuf),
    Stop(PathBuf),
}

fn parse_serve_arguments(arguments: &[OsString]) -> Result<ServeMode, Diagnostic> {
    let mut input = None;
    let mut idle = None;
    let (mut status, mut stop) = (false, false);
    let mut position = 0;
    while position < arguments.len() {
        let argument = &arguments[position];
        position += 1;
        match argument.to_str() {
            Some("--idle-timeout") => {
                if idle.is_some() {
                    return Err(command_line_error(
                        DiagnosticKind::DuplicateOption,
                        "idle-timeout specified more than once",
                    ));
                }
                let value = arguments
                    .get(position)
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| {
                        command_line_error(
                            DiagnosticKind::OptionMissingValue,
                            "--idle-timeout requires a number of seconds",
                        )
                    })?;
                position += 1;
                let invalid = || {
                    command_line_error(
                        DiagnosticKind::OptionInvalidValue,
                        format!(
                            "--idle-timeout must be an integer from 1 to {}, not '{value}'",
                            server::MAX_IDLE_SECONDS
                        ),
                    )
                };
                let seconds: u64 = value.parse().map_err(|_| invalid())?;
                if !(1..=server::MAX_IDLE_SECONDS).contains(&seconds) {
                    return Err(invalid());
                }
                idle = Some(seconds);
            }
            Some("--status") => {
                if std::mem::replace(&mut status, true) {
                    return Err(command_line_error(
                        DiagnosticKind::DuplicateOption,
                        "status specified more than once",
                    ));
                }
            }
            Some("--stop") => {
                if std::mem::replace(&mut stop, true) {
                    return Err(command_line_error(
                        DiagnosticKind::DuplicateOption,
                        "stop specified more than once",
                    ));
                }
            }
            Some(value) if value.starts_with('-') => {
                return Err(command_line_error(
                    DiagnosticKind::UnknownOption,
                    format!("unknown option '{value}' for serve; {SERVE_USAGE}"),
                ));
            }
            _ => {
                if input.replace(PathBuf::from(argument)).is_some() {
                    return Err(command_line_error(DiagnosticKind::UsageError, SERVE_USAGE));
                }
            }
        }
    }
    let input = input.ok_or_else(|| command_line_error(DiagnosticKind::UsageError, SERVE_USAGE))?;
    match (status, stop, idle) {
        (false, false, idle) => Ok(ServeMode::Run {
            input,
            idle: idle.unwrap_or(server::DEFAULT_IDLE_SECONDS),
        }),
        (true, true, _) => Err(command_line_error(
            DiagnosticKind::OptionConflict,
            "--status and --stop cannot be used together",
        )),
        (_, _, Some(_)) => Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            "--idle-timeout applies to starting a server",
        )),
        (true, false, None) => Ok(ServeMode::Status(input)),
        (false, true, None) => Ok(ServeMode::Stop(input)),
    }
}

/// The server's commands: `check` and `build` of the project at `root`, through [`cli`] with the
/// state that a process that stays keeps between them.
struct CommandServer {
    state: tsuzuri::resident::ResidentState,
    root: PathBuf,
}

impl CommandServer {
    /// Runs `command` with the state of the process, what it writes to `out` and to the standard
    /// error that a server collects (with the report of `TSUZURI_TIME_PASSES`). `None` when it
    /// panicked: what it left in the state cannot be trusted, so the state forgets what it read
    /// and keeps the compiler that it pinned (a new state would pin whichever file is there now).
    fn guarded(
        &mut self,
        command: impl FnOnce(&mut Vec<u8>, &mut tsuzuri::resident::Captured) -> u8,
    ) -> Option<(u8, Vec<u8>, Vec<u8>)> {
        let state = &mut self.state;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut out = Vec::new();
            let started = tsuzuri::timings::begin_command();
            let (code, mut err) = {
                let _scope = tsuzuri::resident::Scope::enter(state);
                // What the command writes and what its drops warn are one text, in order.
                tsuzuri::resident::capture(|err| command(&mut out, err))
            };
            if let Some(report) = tsuzuri::timings::take_report(started) {
                errln!(&mut err, "{report}");
            }
            (code, out, err)
        }));
        if outcome.is_err() {
            self.state.reset();
        }
        outcome.ok()
    }
}

impl server::Handler for CommandServer {
    fn run(
        &mut self,
        request: &server::Request,
        log: &mut dyn Write,
    ) -> Result<server::Response, server::Refused> {
        use server::{Refusal, Refused};
        let raw: Vec<OsString> = request.args.iter().map(OsString::from).collect();
        let arguments = parse_arguments(&raw).map_err(|error| {
            Refused::new(
                Refusal::Unsupported,
                format!("the command line is not valid ({})", error.message),
            )
        })?;
        if !matches!(arguments.action, Action::Check | Action::Build) || arguments.no_server {
            return Err(Refused::new(
                Refusal::Unsupported,
                "a server runs check and build, without --no-server",
            ));
        }
        if server::project_root(&arguments.input).as_deref() != Some(self.root.as_path()) {
            return Err(Refused::new(
                Refusal::Root,
                format!(
                    "{} is not in the project that this server runs ({})",
                    arguments.input.display(),
                    self.root.display()
                ),
            ));
        }
        let Some((code, out, err)) = self.guarded(|out, err| cli(&raw, None, out, err)) else {
            let _ = writeln!(log, "serve: the command panicked; resident state dropped");
            return Err(Refused::new(
                Refusal::Internal,
                "the command panicked; see the server's standard error",
            ));
        };
        if self.state.trim() {
            let _ = writeln!(
                log,
                "serve: resident state dropped (over {} bytes)",
                tsuzuri::resident::MAX_RESIDENT_BYTES
            );
        }
        match (String::from_utf8(out), String::from_utf8(err)) {
            (Ok(stdout), Ok(stderr)) => Ok(server::Response {
                exit: code,
                stdout,
                stderr,
            }),
            _ => Err(Refused::new(
                Refusal::Internal,
                "the output of the command is not valid UTF-8",
            )),
        }
    }

    fn compiler_changed(&self) -> bool {
        self.state.compiler_changed()
    }

    fn status(&self) -> Vec<(&'static str, serde_json::Value)> {
        use serde_json::json;
        vec![
            ("resident_bytes", json!(self.state.bytes())),
            (
                "resident_limit_bytes",
                json!(tsuzuri::resident::MAX_RESIDENT_BYTES),
            ),
            ("source_hits", json!(self.state.source_hits)),
            ("source_reads", json!(self.state.source_reads)),
            ("digest_hits", json!(self.state.digest_hits)),
            ("digest_reads", json!(self.state.digest_reads)),
        ]
    }
}

fn serve_failure(kind: DiagnosticKind, message: impl Into<String>, exit: u8) -> ExitCode {
    print_diagnostic(
        &command_line_error(kind, message),
        Path::new("<command line>"),
        "",
        false,
    );
    ExitCode::from(exit)
}

/// `tsuzuri serve DIR [--idle-timeout SECONDS]`, `serve --status DIR`, and `serve --stop DIR`.
fn serve_main(arguments: &[OsString]) -> ExitCode {
    let mode = match parse_serve_arguments(arguments) {
        Ok(mode) => mode,
        Err(error) => {
            print_diagnostic(&error, Path::new("<command line>"), "", false);
            return ExitCode::from(EXIT_USAGE);
        }
    };
    if !cfg!(unix) {
        return serve_failure(
            DiagnosticKind::OptionUnsupportedPlatform,
            "the build server needs Unix domain sockets; use tsuzuri watch, or run check and build without a server",
            EXIT_USAGE,
        );
    }
    match mode {
        ServeMode::Status(input) => match server::control(&input, &server::Operation::Status) {
            Ok(reply) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&reply).expect("a status is serializable")
                );
                ExitCode::SUCCESS
            }
            Err(message) => serve_failure(
                DiagnosticKind::BuildServerControlFailed,
                message,
                EXIT_FAILURE,
            ),
        },
        ServeMode::Stop(input) => match server::control(&input, &server::Operation::Shutdown) {
            Ok(_) => {
                let root = server::project_root(&input).unwrap_or(input);
                println!("stopped the build server for {}", root.display());
                ExitCode::SUCCESS
            }
            Err(message) => serve_failure(
                DiagnosticKind::BuildServerControlFailed,
                message,
                EXIT_FAILURE,
            ),
        },
        ServeMode::Run { input, idle } => start_server(&input, idle),
    }
}

fn start_server(input: &Path, idle: u64) -> ExitCode {
    // Every request would find the same invalid setting.
    if let Some(code) = invalid_thread_setting(&mut io::stderr(), false) {
        return ExitCode::from(code);
    }
    let directory = match server::server_directory() {
        Ok(directory) => directory,
        Err(error) => {
            return serve_failure(
                DiagnosticKind::EnvironmentSettingInvalid,
                error.to_string(),
                EXIT_USAGE,
            );
        }
    };
    let Some(root) = server::project_root(input).filter(|root| root.to_str().is_some()) else {
        return serve_failure(
            DiagnosticKind::IoFailure,
            format!(
                "cannot find the project of {}: it must be an existing project directory or file with a UTF-8 path, not a symbolic link",
                input.display()
            ),
            EXIT_FAILURE,
        );
    };
    if let Err(message) = server::ensure_private_directory(&directory) {
        return serve_failure(
            DiagnosticKind::BuildServerStartFailed,
            message,
            EXIT_FAILURE,
        );
    }
    let compiler = env::current_exe().and_then(std::fs::canonicalize);
    let cwd = env::current_dir();
    let (Ok(compiler), Ok(cwd)) = (compiler, cwd) else {
        return serve_failure(
            DiagnosticKind::IoFailure,
            "cannot find the compiler file or the working directory",
            EXIT_FAILURE,
        );
    };
    let environment = match server::current_environment() {
        Ok(environment) => environment,
        Err(name) => {
            return serve_failure(
                DiagnosticKind::EnvironmentSettingInvalid,
                format!(
                    "the environment variable {} is not valid UTF-8; the build server compares it with its clients'",
                    name.to_string_lossy()
                ),
                EXIT_USAGE,
            );
        }
    };
    let config = server::Config::new(
        directory,
        root.clone(),
        std::time::Duration::from_secs(idle),
        compiler,
        cwd,
        environment,
    );
    let mut handler = CommandServer {
        state: tsuzuri::resident::ResidentState::new(),
        root,
    };
    match server::serve(&config, &mut handler, io::stderr()) {
        Ok(server::Stop::Idle | server::Stop::Shutdown) => ExitCode::SUCCESS,
        Ok(server::Stop::CompilerChanged) => ExitCode::from(EXIT_FAILURE),
        // `ServeError` names its code: `E2000` for a setting to change (the socket path that
        // `TSUZURI_SERVER_DIR` gives is too long), `E2001` for the file system and the threads.
        Err(error) => {
            let (kind, exit) = if error.code == "E2000" {
                (DiagnosticKind::EnvironmentSettingInvalid, EXIT_USAGE)
            } else {
                (DiagnosticKind::BuildServerStartFailed, EXIT_FAILURE)
            };
            debug_assert_eq!(kind.code(), error.code, "{}", error.message);
            serve_failure(kind, error.message, exit)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[&str]) -> Result<Arguments, String> {
        parse_arguments(&values.iter().map(OsString::from).collect::<Vec<_>>())
            .map_err(|error| error.message.clone())
    }

    fn parse_repl(values: &[&str]) -> Result<tsuzuri::repl::ReplOptions, String> {
        parse_repl_arguments(&values.iter().map(OsString::from).collect::<Vec<_>>())
            .map_err(|error| error.message.clone())
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
        assert_eq!(parsed.options.optimization, OptLevel::O3);
        assert_eq!(parsed.links.libraries, ["m"]);
        assert_eq!(
            parse_arguments(&split(&["-O0", "-O3", "a.tz"]).unwrap().0)
                .unwrap_err()
                .message,
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
            assert_eq!(split(values).unwrap_err().message, message, "{values:?}");
        }
        // Help before the file needs no file.
        assert_eq!(strings(&split(&["--help"]).unwrap().0), ["run", "--help"]);
    }

    #[test]
    fn parses_repl_options() {
        let defaults = parse_repl(&[]).unwrap();
        assert_eq!(defaults.build.optimization, OptLevel::O0);
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
        assert_eq!(custom.build.optimization, OptLevel::O3);
        assert_eq!(custom.build.cpu, Cpu::Native);
        assert!(!custom.build.cache);
        assert_eq!(custom.timeout, None);
        assert_eq!(
            parse_repl(&["--timeout", "3600"]).unwrap().timeout,
            Some(std::time::Duration::from_secs(3600))
        );
        // The REPL's -O0 default leaves build's and run's -O3 default alone.
        assert_eq!(
            parse(&["run", "Main.tz"]).unwrap().options.optimization,
            OptLevel::O3
        );
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
            (&["-Os"], REPL_USAGE),
            (&["-Oz"], REPL_USAGE),
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
    fn size_levels_apply_to_build_and_run_only() {
        for (flag, level) in [("-Os", OptLevel::Os), ("-Oz", OptLevel::Oz)] {
            for action in ["build", "run"] {
                let arguments = parse(&[action, "Main.tz", flag]).unwrap();
                assert_eq!(arguments.options.optimization, level, "{action} {flag}");
            }
            let wasm = parse(&["build", "Main.tz", "--target", "wasm32", flag]).unwrap();
            assert_eq!(wasm.options.optimization, level);
            // `-Os` and `-Oz` are accepted without an action name, like the other levels.
            assert_eq!(
                parse(&["Main.tz", flag]).unwrap().options.optimization,
                level
            );
            for action in ["test", "bench"] {
                assert_eq!(
                    parse(&[action, "Main.tz", flag]).unwrap_err(),
                    format!("{action} supports -O0 to -O3; use -Os or -Oz with build or run")
                );
            }
            assert_eq!(
                parse(&["check", "Main.tz", flag]).unwrap_err(),
                "check does not use an optimization level"
            );
            assert_eq!(
                parse(&["fmt", "Main.tz", flag]).unwrap_err(),
                "fmt does not use optimization, CPU tuning, or compiler warning options"
            );
            assert_eq!(
                parse(&["build", "Main.tz", "-O3", flag]).unwrap_err(),
                "optimization specified more than once"
            );
            assert_eq!(
                parse(&["build", "Main.tz", flag, "-O3"]).unwrap_err(),
                "optimization specified more than once"
            );
            assert!(script_arguments(&[OsString::from(flag), OsString::from("a.tz")]).is_err());
        }
        // The default level and the size levels do not leak into each other.
        for action in ["build", "run"] {
            let arguments = parse(&[action, "Main.tz"]).unwrap();
            assert_eq!(arguments.options.optimization, OptLevel::O3);
        }
        assert_eq!(
            parse(&["test", "Main.tz"]).unwrap().options.optimization,
            OptLevel::O0
        );
        assert!(parse(&["build", "Main.tz", "-Ot"]).is_err());
        assert!(parse(&["build", "Main.tz", "-OS"]).is_err());
        assert!(HELP.contains("-Os, -Oz"));
    }

    #[test]
    fn strip_is_a_build_option_for_native_executables() {
        assert!(
            parse(&["build", "Main.tz", "--strip"])
                .unwrap()
                .options
                .strip
        );
        assert!(!parse(&["build", "Main.tz"]).unwrap().options.strip);
        assert!(parse(&["Main.tz", "--strip", "-Oz"]).unwrap().options.strip);
        let wasm = "--strip requires a native executable; WASM output is already stripped unless --debug-info is used";
        for (values, message) in [
            (
                vec!["build", "Main.tz", "--strip", "--strip"],
                "strip specified more than once",
            ),
            (
                vec!["run", "Main.tz", "--strip"],
                "--strip is only valid with build",
            ),
            (
                vec!["test", "Main.tz", "--strip"],
                "--strip is only valid with build",
            ),
            (
                vec!["check", "Main.tz", "--strip"],
                "--strip is only valid with build",
            ),
            (
                vec!["build", "Main.tz", "--strip", "-g"],
                "--strip cannot be combined with --debug-info: a debugger needs the symbols",
            ),
            (
                vec!["build", "Main.tz", "--strip", "--emit", "object"],
                "--strip requires native executable output (--emit exe)",
            ),
            (
                vec!["build", "Main.tz", "--strip", "--emit", "llvm"],
                "--strip requires native executable output (--emit exe)",
            ),
            (
                vec!["build", "Main.tz", "--strip", "--target", "wasm32"],
                wasm,
            ),
            (
                vec![
                    "build", "Main.tz", "--strip", "--target", "wasm32", "--emit", "object",
                ],
                wasm,
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
        assert!(HELP.contains("--strip"));
    }

    /// What `validate_fast` says last, to options that pass all of its own checks: a host that has no
    /// translator (checked first), then a `tsuzuri` built without it. None where the backend is there.
    fn fast_unavailable() -> Option<String> {
        if !cfg!(all(
            any(target_os = "macos", target_os = "linux"),
            any(target_arch = "aarch64", target_arch = "x86_64")
        )) {
            Some(format!(
                "--backend fast is not available on {}-{}; use --backend llvm",
                env::consts::OS,
                env::consts::ARCH
            ))
        } else if !cfg!(feature = "fast-backend") {
            Some("this tsuzuri was built without the fast backend; rebuild with cargo build --release --features fast-backend or use --backend llvm".to_owned())
        } else {
            None
        }
    }

    #[test]
    fn rejects_fast_backend_combinations() {
        let level = |flag: &str| {
            format!("--backend fast supports only -O0; remove {flag} or use --backend llvm")
        };
        let native = |other: &str| {
            format!(
                "--backend fast supports only the native target and wasm32; use --backend llvm for {other}"
            )
        };
        let wasm_option = |name: &str| {
            format!(
                "--backend fast does not support {name} for wasm32; remove it or use --backend llvm"
            )
        };
        let wasm_output = |kind: &str| {
            format!(
                "--backend fast produces only wasm and object outputs for wasm32; use --backend llvm for --emit {kind}"
            )
        };
        let output = |kind: &str| {
            format!(
                "--backend fast produces only exe and object outputs; use --backend llvm for --emit {kind}"
            )
        };
        let option = |name: &str| {
            format!("--backend fast does not support {name} yet; remove it or use --backend llvm")
        };
        let emit_target = "'--emit exe' requires '--target native'; '--emit wasm' requires '--target wasm32' or '--target wasm64'".to_owned();
        // The message does not name which profile option was given. The profile options are checked
        // after the others, so a host without the fast backend and a build without it say that first.
        let profile = fast_unavailable().unwrap_or_else(|| {
            "--backend fast cannot instrument or optimize with a profile; use --backend llvm with --profile-generate and --profile-use".to_owned()
        });
        let fast = |extra: &[&'static str]| {
            let mut values = vec!["build", "Main.tz", "--backend", "fast"];
            values.extend_from_slice(extra);
            values
        };
        let mut cases = vec![
            (
                vec!["build", "Main.tz", "--backend", "gcc"],
                "backend must be 'llvm' or 'fast'".to_owned(),
            ),
            (
                vec!["build", "Main.tz", "--backend"],
                "--backend needs a value".to_owned(),
            ),
            (
                vec!["build", "Main.tz", "--backend", "fast", "--backend", "fast"],
                "backend specified more than once".to_owned(),
            ),
            (
                vec!["build", "Main.tz", "--backend", "fast", "--backend", "llvm"],
                "backend specified more than once".to_owned(),
            ),
        ];
        for action in ["check", "fmt", "doc", "bench"] {
            for backend in ["fast", "llvm"] {
                cases.push((
                    vec![action, "Main.tz", "--backend", backend],
                    "--backend applies only to build, run, and test".to_owned(),
                ));
            }
        }
        for flag in ["-O1", "-O2", "-O3", "-Os", "-Oz"] {
            cases.push((fast(&[flag]), level(flag)));
        }
        cases.extend([
            (fast(&["--target", "wasm64"]), native("wasm64")),
            (fast(&["--target", "wasm32", "-O3"]), level("-O3")),
            (
                fast(&["--target", "x86_64-unknown-linux-gnu"]),
                native("--target x86_64-unknown-linux-gnu"),
            ),
            (
                fast(&["--target", "wasm32", "--wasm-feature", "simd128"]),
                wasm_option("--wasm-feature simd128"),
            ),
            (
                fast(&["--target", "wasm32", "--wasm-host", "wasi"]),
                wasm_option("--wasm-host"),
            ),
            (
                fast(&["--target", "wasm32", "--wasm-feature", "threads"]),
                wasm_option("--wasm-feature threads"),
            ),
            (
                fast(&["--target", "wasm32", "--wasm-feature", "jspi"]),
                wasm_option("--wasm-feature jspi"),
            ),
            (
                fast(&["--target", "wasm32", "--wasm-feature", "webgpu"]),
                wasm_option("--wasm-feature webgpu"),
            ),
            (
                fast(&["--target", "wasm32", "--wasm-feature", "net"]),
                wasm_option("--wasm-feature net"),
            ),
            (
                fast(&["--target", "wasm32", "--wasm-feature", "tail-call"]),
                wasm_option("--wasm-feature tail-call"),
            ),
            (fast(&["--target", "wasm32", "-g"]), wasm_option("-g")),
            (
                fast(&["--target", "wasm32", "--wasm-opt"]),
                wasm_option("--wasm-opt"),
            ),
            (
                fast(&["--target", "wasm32", "--emit", "llvm"]),
                wasm_output("llvm"),
            ),
            (
                fast(&["--target", "wasm32", "--emit", "bindings-js"]),
                wasm_output("bindings-js"),
            ),
            (fast(&["--emit", "llvm"]), output("llvm")),
            (fast(&["--emit", "header"]), output("header")),
            (fast(&["--emit", "wgsl"]), output("wgsl")),
            (fast(&["--emit", "wgsl-relaxed"]), output("wgsl-relaxed")),
            (fast(&["--emit", "spirv"]), output("spirv")),
            (fast(&["--emit", "spirv-relaxed"]), output("spirv-relaxed")),
            (fast(&["--emit", "shared"]), output("shared")),
            (fast(&["--emit", "bitcode"]), output("bitcode")),
            (fast(&["--profile-generate", "prof"]), profile.clone()),
            (fast(&["--profile-use", "p.profdata"]), profile.clone()),
            (fast(&["--emit", "bindings-cs"]), output("bindings-cs")),
            (fast(&["--emit", "bindings-py"]), output("bindings-py")),
            (fast(&["--emit", "bindings-cpp"]), output("bindings-cpp")),
            (fast(&["--cpu", "native"]), option("--cpu native")),
            // An `--emit` that its `--target` cannot give is an error for every backend, on every
            // host, and it is reported as the LLVM backend does: `--backend llvm` is no way out.
            (fast(&["--emit", "wasm"]), emit_target.clone()),
            (
                fast(&["--target", "wasm32", "--emit", "exe"]),
                emit_target.clone(),
            ),
            (
                fast(&["--emit", "bindings-js"]),
                "'--emit bindings-js' requires '--target wasm32'".to_owned(),
            ),
            (
                fast(&["--target", "wasm32", "--emit", "shared"]),
                "'--emit shared' requires '--target native'".to_owned(),
            ),
        ]);
        for (values, message) in cases {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
        // The default code generator keeps every option it had.
        for values in [
            vec!["build", "Main.tz", "--backend", "llvm", "-O2", "-g"],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "llvm",
                "--target",
                "wasm32",
            ],
            vec!["build", "Main.tz", "--backend", "llvm", "--cpu", "native"],
            vec!["run", "Main.tz", "--backend", "llvm", "-Oz"],
        ] {
            let options = parse(&values).unwrap().options;
            assert_eq!(options.backend, Backend::Llvm, "{values:?}");
        }
        assert!(HELP.contains("--backend llvm|fast"));
        // fast translates wasm32 too: the help says what it takes and what stays with llvm.
        assert!(HELP.contains("--emit wasm|object"));
        assert!(HELP.contains("--wasm-feature, --wasm-host, --wasm-opt, and -g"));
        assert!(!HELP.contains("WebAssembly stay with llvm"));
    }

    #[test]
    fn accepts_fast_backend_at_o0() {
        let unavailable = fast_unavailable();
        for values in [
            vec!["build", "Main.tz", "--backend", "fast"],
            vec!["run", "Main.tz", "--backend", "fast"],
            vec!["Main.tz", "--backend", "fast", "-O0"],
            vec!["build", "Main.tz", "--backend", "fast", "--emit", "object"],
            vec!["build", "Main.tz", "--backend", "fast", "--emit", "exe"],
            vec!["build", "Main.tz", "--backend", "fast", "-g"],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--target",
                "wasm32",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--target",
                "wasm32",
                "--emit",
                "object",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--target",
                "wasm32",
                "--wasm-max-memory",
                "64MiB",
                "--wasm-stack-size",
                "128KiB",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--target",
                "wasm32",
                "--trap-info",
                "--allocator",
                "counting",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--allocator",
                "small",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--allocator",
                "counting",
                "--emit",
                "object",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--trap-mode",
                "return",
                "--emit",
                "object",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--freestanding",
                "--allocator",
                "host",
                "--emit",
                "object",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--debug-info",
                "--emit",
                "object",
            ],
            vec![
                "test",
                "Main.tz",
                "--index",
                "0",
                "-g",
                "-o",
                "tests",
                "--backend",
                "fast",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--trap-info",
                "--strip",
                "--no-cache",
                "--debug-output",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--allocator",
                "host",
                "--emit",
                "object",
            ],
            vec![
                "build",
                "Main.tz",
                "--backend",
                "fast",
                "--allocator",
                "system",
                "-l",
                "m",
                "-L",
                ".",
            ],
        ] {
            match (parse(&values), &unavailable) {
                (Ok(arguments), None) => {
                    assert_eq!(arguments.options.backend, Backend::Fast, "{values:?}");
                    assert_eq!(arguments.options.optimization, OptLevel::O0, "{values:?}");
                }
                (Err(message), Some(expected)) => assert_eq!(&message, expected, "{values:?}"),
                (result, _) => panic!("{values:?}: {:?}", result.map(|_| ())),
            }
        }
        // Without the option, the options are the ones of the default code generator.
        for action in ["build", "run"] {
            let options = parse(&[action, "Main.tz"]).unwrap().options;
            assert_eq!(options.backend, Backend::Llvm);
            assert_eq!(options.optimization, OptLevel::O3);
        }
        let llvm = parse(&["build", "Main.tz", "--backend", "llvm"])
            .unwrap()
            .options;
        assert_eq!(llvm.backend, Backend::Llvm);
        assert_eq!(llvm.optimization, OptLevel::O3);
    }

    #[test]
    fn wasm_opt_is_a_build_option_for_wasm32_output() {
        let wasm_opt = |values: &[&str]| parse(values).map(|arguments| arguments.options.wasm_opt);
        assert_eq!(
            wasm_opt(&[
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-opt",
                "-Oz"
            ]),
            Ok(true)
        );
        assert_eq!(
            wasm_opt(&[
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "simd128",
                "--wasm-opt"
            ]),
            Ok(true)
        );
        // Nothing asks for the tool unless the flag is there.
        assert_eq!(
            wasm_opt(&["build", "Main.tz", "--target", "wasm32"]),
            Ok(false)
        );
        let output = "--wasm-opt requires WASM output (--target wasm32, --emit wasm)";
        for (values, message) in [
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-opt",
                    "--wasm-opt",
                ],
                "wasm-opt specified more than once",
            ),
            (
                vec!["run", "Main.tz", "--wasm-opt"],
                "--wasm-opt is only valid with build",
            ),
            (
                vec!["test", "Main.tz", "--wasm-opt"],
                "--wasm-opt is only valid with build",
            ),
            (vec!["build", "Main.tz", "--wasm-opt"], output),
            (
                vec!["build", "Main.tz", "--target", "wasm64", "--wasm-opt"],
                output,
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "object",
                    "--wasm-opt",
                ],
                output,
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "threads",
                    "--wasm-opt",
                ],
                "--wasm-opt cannot be combined with --wasm-feature threads",
            ),
            (
                vec!["build", "Main.tz", "--target", "wasm32", "-g", "--wasm-opt"],
                "--wasm-opt cannot be combined with --debug-info: the optimizer rewrites the code the names and debug information describe",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
        assert!(HELP.contains("--wasm-opt") && HELP.contains("TSUZURI_WASM_OPT"));
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
            vec!["--wasm-feature", "tail-call"],
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
        assert_eq!(
            parse(&["build", "Kernel.tz", "--emit", "wgsl-relaxed"])
                .unwrap()
                .options
                .emit,
            Emit::WgslRelaxed
        );
        for extra in [
            vec!["-O3"],
            vec!["--target", "wasm32"],
            vec!["--cpu", "native"],
        ] {
            let mut values = vec!["build", "Kernel.tz", "--emit", "wgsl-relaxed"];
            values.extend(extra);
            assert!(parse(&values).is_err(), "{values:?}");
        }
        for (name, emit) in [
            ("spirv", Emit::Spirv),
            ("spirv-relaxed", Emit::SpirvRelaxed),
        ] {
            assert_eq!(
                parse(&["build", "Kernel.tz", "--emit", name])
                    .unwrap()
                    .options
                    .emit,
                emit
            );
            for extra in [
                vec!["-O3"],
                vec!["--target", "wasm32"],
                vec!["--cpu", "native"],
            ] {
                let mut values = vec!["build", "Kernel.tz", "--emit", name];
                values.extend(extra);
                let error = parse(&values).unwrap_err();
                assert_eq!(
                    error, "SPIR-V output does not use target, optimization, or CPU options",
                    "{values:?}"
                );
            }
        }
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
        let net = parse(&[
            "build",
            "Main.tz",
            "--target",
            "wasm32",
            "--wasm-feature",
            "jspi",
            "--wasm-feature",
            "net",
            "--emit",
            "bindings-js",
        ])
        .unwrap()
        .options;
        assert!(net.wasm_net && net.wasm_jspi);
        assert!(!parse(&["build", "Main.tz"]).unwrap().options.wasm_net);
        // `tail-call` (PM09) goes with the other features and with the three outputs that carry
        // WebAssembly instructions, for wasm32 and wasm64.
        assert!(!parse(&["build", "Main.tz"]).unwrap().options.wasm_tail_call);
        for values in [
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "tail-call",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--emit",
                "object",
                "--wasm-feature",
                "tail-call",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--emit",
                "llvm",
                "--wasm-feature",
                "tail-call",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm64",
                "--emit",
                "llvm",
                "--wasm-feature",
                "tail-call",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "threads",
                "--wasm-feature",
                "simd128",
                "--wasm-feature",
                "tail-call",
            ],
            vec![
                "build",
                "Main.tz",
                "--target",
                "wasm32",
                "--wasm-feature",
                "tail-call",
                "--wasm-host",
                "wasi",
            ],
        ] {
            assert!(parse(&values).unwrap().options.wasm_tail_call, "{values:?}");
        }
        for (values, message) in [
            (
                vec!["build", "Main.tz", "--wasm-feature", "tail-call"],
                "--wasm-feature tail-call requires wasm32 or wasm64 object, LLVM IR, or WASM output",
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
                    "tail-call",
                ],
                "--wasm-feature tail-call requires wasm32 or wasm64 object, LLVM IR, or WASM output",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "bindings-js",
                    "--wasm-feature",
                    "tail-call",
                ],
                "--wasm-feature is not valid for bindings output; pass it when building the .wasm",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "tail-call",
                    "--wasm-feature",
                    "tail-call",
                ],
                "WASM feature specified more than once",
            ),
            (
                vec!["run", "Main.tz", "--wasm-feature", "tail-call"],
                "--wasm-feature is only valid with build",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "wgsl",
                    "--wasm-feature",
                    "tail-call",
                ],
                "WGSL output does not use target, CPU, debug, or WASM feature options",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
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
                "supported WASM features are 'simd128', 'threads', 'jspi', 'webgpu', 'net', and 'tail-call'; relaxed SIMD is not supported",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm64",
                    "--wasm-feature",
                    "webgpu",
                ],
                "--wasm-feature webgpu requires wasm32 object, LLVM IR, or WASM output; the JavaScript bindings do not provide the WebGPU imports, so instantiate the module with createGpuImports of src/runtime/webgpu.mjs",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "webgpu",
                    "--wasm-feature",
                    "threads",
                ],
                "--wasm-feature webgpu cannot be combined with --wasm-feature threads or --wasm-host: its imports suspend the WebAssembly stack with JavaScript Promise Integration",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "net",
                ],
                "--wasm-feature net requires --wasm-feature jspi: a blocking socket call suspends the WebAssembly stack with JavaScript Promise Integration while the host works",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "wgsl",
                    "--wasm-feature",
                    "webgpu",
                ],
                "WGSL output does not use target, CPU, debug, or WASM feature options",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm64",
                    "--emit",
                    "llvm",
                    "--wasm-feature",
                    "jspi",
                    "--wasm-feature",
                    "net",
                ],
                "--wasm-feature net requires wasm32 object, LLVM IR, WASM, or JavaScript bindings output",
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
                    "net",
                    "--wasm-feature",
                    "threads",
                ],
                "--wasm-feature jspi cannot be combined with --wasm-feature threads or --wasm-host",
            ),
            (
                vec!["run", "Main.tz", "--wasm-feature", "net"],
                "--wasm-feature is only valid with build",
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
        assert_eq!(tests.options.optimization, OptLevel::O0);
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
        assert_eq!(arguments.options.optimization, OptLevel::O0);
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
                "emit kind must be exe, object, llvm, header, wasm, wgsl, wgsl-relaxed, spirv, spirv-relaxed, shared, bitcode, bindings-js, bindings-cs, bindings-py, or bindings-cpp",
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
            "--allocs",
            "--list",
            "--index",
            "1",
            "--filter",
            "total",
        ])
        .unwrap();
        assert_eq!(bench.action, Action::Bench);
        assert_eq!(bench.bench_samples, Some(5));
        assert!(bench.bench_allocs);
        assert_eq!(bench.options.optimization, OptLevel::O3);
        assert!(bench.test_list && bench.test_filter.as_deref() == Some("total"));
        assert_eq!(bench.test_indices, vec![1]);
        assert!(links_apply(Action::Bench, &bench.options));
        assert_eq!(
            parse(&["bench", "Speed.tz", "-O0", "--target", "native"])
                .unwrap()
                .options
                .optimization,
            OptLevel::O0
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
        assert!(!test.bench_allocs);
        let samples = "bench samples must be an integer between 1 and 1000";
        let seed = "property seed must be an integer between 0 and 18446744073709551615";
        for (values, message) in [
            (
                vec!["test", "Main.tz", "--allocs"],
                "--allocs is only valid with bench",
            ),
            (
                vec!["bench", "Main.tz", "--allocs", "--allocs"],
                "allocs specified more than once",
            ),
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
    fn accepts_named_targets_and_rejects_unknown_ones() {
        // The names are spelled out here, not read from the table that parses them.
        let names = [
            "aarch64-apple-darwin",
            "aarch64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu",
            "aarch64-pc-windows-msvc",
            "x86_64-pc-windows-msvc",
            "aarch64-unknown-linux-musl",
            "x86_64-unknown-linux-musl",
            "aarch64-pc-windows-gnu",
            "x86_64-pc-windows-gnu",
        ];
        assert_eq!(Triple::names().split(", ").count(), names.len());
        for name in names {
            let build = parse(&["build", "app", "--target", name]).unwrap();
            assert_eq!(build.options.target, Target::Native, "{name}");
            assert_eq!(build.options.triple.map(Triple::name), Some(name));
            assert_eq!(build.options.emit, Emit::Executable);
            let test = parse(&["test", "app", "--target", name]).unwrap();
            assert_eq!(test.options.triple.map(Triple::name), Some(name));
            let run = parse(&["run", "app", "--target", name]).unwrap();
            assert_eq!(run.options.triple.map(Triple::name), Some(name));
            assert!(run.options.trap_info);
            let bench = parse(&["bench", "app", "--target", name]).unwrap();
            assert_eq!(bench.options.triple.map(Triple::name), Some(name));
        }
        // The aliases of WebAssembly select the same code generators, and no triple.
        for (text, target) in [
            ("wasm32-unknown-unknown", Target::Wasm32),
            ("wasm64-unknown-unknown", Target::Wasm64),
        ] {
            let build = parse(&["build", "app", "--target", text]).unwrap();
            assert_eq!((build.options.target, build.options.triple), (target, None));
            assert_eq!(build.options.emit, Emit::Wasm);
        }
        let native = parse(&["build", "app", "--target", "native"]).unwrap();
        assert_eq!(native.options.triple, None);
        // Unknown names, aliases, and 32-bit targets get one message that lists the closed set.
        let list = Triple::names();
        for value in [
            "x86_64-unknown-haiku",
            "arm64-apple-darwin",
            "x86_64-linux-gnu",
            "AARCH64-apple-darwin",
            "",
            "x86_64-apple-darwin",
            "riscv64gc-unknown-linux-gnu",
            "riscv64gc-unknown-linux-musl",
            "aarch64-unknown-freebsd",
            "x86_64-unknown-freebsd",
            "aarch64-linux-android",
            "x86_64-linux-android",
            "aarch64-apple-ios",
            "aarch64-apple-ios-sim",
            "x86_64-apple-ios",
            "aarch64-unknown-none",
            "x86_64-unknown-none",
            "riscv64gc-unknown-none-elf",
        ] {
            assert_eq!(
                parse(&["build", "app", "--target", value]).unwrap_err(),
                format!("unknown target '{value}'; use native, wasm32, wasm64, or one of {list}")
            );
        }
        for value in [
            "i686-unknown-linux-gnu",
            "armv7-unknown-linux-gnueabihf",
            "riscv32imac-unknown-none-elf",
        ] {
            assert_eq!(
                parse(&["test", "app", "--target", value]).unwrap_err(),
                format!(
                    "unknown target '{value}'; 32-bit native targets are not supported because native code generation assumes 64-bit pointers; use native, wasm32, wasm64, or one of {list}"
                )
            );
        }
        // `run` takes a native target; `script` and `repl` take none, and a repeated one is an error.
        assert!(parse(&["run", "app", "--target", "native"]).is_ok());
        assert!(parse(&["run", "app", "--target", "wasm32"]).is_err());
        assert!(parse(&["check", "app", "--target", "x86_64-unknown-linux-gnu"]).is_err());
        assert!(
            parse(&[
                "build",
                "app",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--target",
                "native"
            ])
            .is_err()
        );
        assert!(
            script_arguments(&[
                OsString::from("--target"),
                OsString::from("x86_64-unknown-linux-gnu"),
                OsString::from("a.tz")
            ])
            .is_err()
        );
        assert!(parse_repl(&["--target", "x86_64-unknown-linux-gnu"]).is_err());
        // WGSL, SPIR-V, and WebAssembly output name no native target.
        for emit in ["wgsl", "spirv"] {
            assert!(
                parse(&[
                    "build",
                    "app",
                    "--emit",
                    emit,
                    "--target",
                    "x86_64-unknown-linux-gnu"
                ])
                .is_err()
            );
        }
        assert!(
            parse(&[
                "build",
                "app",
                "--emit",
                "wasm",
                "--target",
                "x86_64-unknown-linux-gnu"
            ])
            .is_err()
        );
        // A named target is rejected with `--cpu native` only when it is not this machine's own.
        let host = Triple::host().map(Triple::name);
        for name in names {
            let parsed = parse(&["build", "app", "--target", name, "--cpu", "native"]);
            if Some(name) == host {
                assert!(parsed.is_ok() || parsed.unwrap_err().contains("is not supported on"));
            } else {
                assert_eq!(
                    parsed.unwrap_err(),
                    format!(
                        "'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to {name}"
                    )
                );
            }
        }
    }

    #[test]
    fn the_help_lists_the_fix_command() {
        let help = help();
        assert!(help.contains(
            "tsuzuri fix [--applicability certain|likely|maybe] [--dry-run] [--json] source.tz|source.tt|source.tc|directory"
        ));
        assert!(
            help.contains("tsuzuri fix --edition YEAR [--from YEAR] [--dry-run] [--json] input")
        );
        assert!(help.contains("`tsuzuri fix PATH` applies the fixes that diagnostics carry"));
    }

    #[test]
    fn the_help_lists_every_target_of_the_table() {
        let text = help();
        for triple in Triple::ALL {
            assert!(
                text.contains(&format!("\n  {}", triple.name())),
                "{}",
                triple.name()
            );
        }
        assert!(text.contains("TSUZURI_SYSROOT"));
        assert!(text.contains("TSUZURI_RUNNER"));
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
        let unknown_target = format!(
            "unknown target 'wasm128'; use native, wasm32, wasm64, or one of {}",
            Triple::names()
        );
        let untargeted =
            "--wasm-max-memory and --wasm-stack-size require --target wasm32 or wasm64";
        let output =
            "--wasm-max-memory requires wasm32 or wasm64 object, LLVM IR, bitcode, or WASM output";
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
                unknown_target.as_str(),
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
                parse_size(OsStr::new(text), "--wasm-stack-size")
                    .unwrap_err()
                    .message,
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
    fn parses_bitcode_output() {
        let wasm = parse(&[
            "build",
            "Kernel.tz",
            "--emit",
            "bitcode",
            "--target",
            "wasm32",
        ]);
        assert_eq!(wasm.unwrap().options.emit, Emit::Bitcode);
        if cfg!(windows) {
            assert_eq!(
                parse(&["build", "Kernel.tz", "--emit", "bitcode"]).unwrap_err(),
                "'--emit bitcode' is not supported on Windows yet (G10); build an object with --emit object and link it normally"
            );
            return;
        }
        let bitcode = parse(&["build", "Kernel.tz", "--emit", "bitcode"]).unwrap();
        assert_eq!(bitcode.options.emit, Emit::Bitcode);
        assert_eq!(
            bitcode.options.output_path(Path::new("dir/Kernel.tz")),
            Path::new("dir/Kernel.bc")
        );
        // The options of an object keep their meaning for bitcode.
        for values in [
            vec!["build", "Kernel.tz", "--emit", "bitcode", "-O0"],
            vec!["build", "Kernel.tz", "--emit", "bitcode", "-Oz", "-g"],
            vec!["build", "Kernel.tz", "--emit", "bitcode", "--trap-info"],
            vec![
                "build",
                "Kernel.tz",
                "--emit",
                "bitcode",
                "--allocator",
                "small",
            ],
            vec![
                "build",
                "Kernel.tz",
                "--emit",
                "bitcode",
                "--allocator",
                "host",
            ],
            vec!["build", "Kernel.tz", "--emit", "bitcode", "--no-cache"],
            vec![
                "build",
                "Kernel.tz",
                "--emit",
                "bitcode",
                "--target",
                "wasm32",
            ],
            vec![
                "build",
                "Kernel.tz",
                "--emit",
                "bitcode",
                "--target",
                "wasm32",
                "--wasm-feature",
                "simd128",
                "--wasm-max-memory",
                "64MiB",
            ],
        ] {
            assert_eq!(
                parse(&values).unwrap().options.emit,
                Emit::Bitcode,
                "{values:?}"
            );
        }
        let returning = parse(&[
            "build",
            "Kernel.tz",
            "--emit",
            "bitcode",
            "--trap-mode",
            "return",
        ])
        .unwrap();
        assert!(returning.options.trap_return && returning.options.trap_info);
        for (values, message) in [
            (
                vec![
                    "build",
                    "Kernel.tz",
                    "--emit",
                    "bitcode",
                    "--target",
                    "wasm64",
                ],
                "'--emit bitcode' requires '--target native' or '--target wasm32'; wasm64 bitcode is not supported",
            ),
            (
                vec![
                    "build",
                    "Kernel.tz",
                    "--emit",
                    "bitcode",
                    "--link",
                    "host.o",
                ],
                "link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe",
            ),
            (
                vec!["build", "Kernel.tz", "--emit", "bitcode", "--strip"],
                "--strip requires native executable output (--emit exe)",
            ),
            (
                vec![
                    "build",
                    "Kernel.tz",
                    "--emit",
                    "bitcode",
                    "--freestanding",
                    "--allocator",
                    "host",
                ],
                "--freestanding requires object, LLVM IR, or header output",
            ),
            (
                vec!["run", "Kernel.tz", "--emit", "bitcode"],
                "--output requires build or doc; --target and --emit require a supported build/test action",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
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

    #[cfg(not(windows))]
    #[test]
    fn parses_profile_options() {
        use tsuzuri::driver::Profile;
        assert_eq!(parse(&["build", "Main.tz"]).unwrap().profile, Profile::None);
        // The directory that an instrumented program writes to is absolute: the program decides where it runs.
        let generate = parse(&["build", "Main.tz", "--profile-generate", "prof"]).unwrap();
        assert_eq!(
            generate.profile,
            Profile::Generate(std::path::absolute("prof").unwrap())
        );
        let used = parse(&[
            "build",
            "Main.tz",
            "--profile-use",
            "p.profdata",
            "--emit",
            "object",
            "-O2",
        ])
        .unwrap();
        assert_eq!(used.profile, Profile::Use(PathBuf::from("p.profdata")));
        // `run` takes both options too: it builds the program with them and runs it.
        let run = parse(&["run", "Main.tz", "--profile-generate", "prof", "-O2"]).unwrap();
        assert_eq!(run.action, Action::Run);
        assert_eq!(
            run.profile,
            Profile::Generate(std::path::absolute("prof").unwrap())
        );
        let run = parse(&["run", "Main.tz", "--profile-use", "p.profdata"]).unwrap();
        assert_eq!(run.profile, Profile::Use(PathBuf::from("p.profdata")));
        assert_eq!(
            parse(&[
                "run",
                "Main.tz",
                "--profile-use",
                "p",
                "--profile-generate",
                "q"
            ])
            .unwrap_err(),
            "--profile-generate and --profile-use cannot be combined; build once with each"
        );
        // A `wasm32` module is instrumented, and trained by its host.
        if !cfg!(windows) {
            for option in ["--profile-generate", "--profile-use"] {
                let wasm = parse(&["build", "Main.tz", "--target", "wasm32", option, "prof"]);
                assert!(wasm.is_ok(), "{option}: {:?}", wasm.err());
            }
        }
        for text in [
            "--profile-generate DIR",
            "--profile-use PATH",
            "TSUZURI_LLVM_PROFDATA",
            "writeProfile(instance)",
            "The counters are not atomic",
            "W2001",
        ] {
            assert!(HELP.contains(text), "{text}");
        }
        // Nothing else of the build is a reason to refuse a profile (D16).
        for extra in [
            &["-O0"][..],
            &["-Os"],
            &["-Oz"],
            &["--no-cache"],
            &["--strip"],
            &["-g"],
            &["--trap-info"],
            &["--cpu", "native"],
            &["--allocator", "small"],
        ] {
            for option in ["--profile-generate", "--profile-use"] {
                let mut values = vec!["build", "Main.tz", option, "prof"];
                values.extend_from_slice(extra);
                parse(&values).unwrap_or_else(|error| panic!("{values:?}: {error}"));
            }
        }
    }

    #[test]
    fn rejects_invalid_profile_combinations() {
        let wasm64 = "profile-guided optimization supports --target wasm32 only; remove --target wasm64 or the profile option";
        let wasm_emit = "profile-guided optimization needs --emit wasm for WebAssembly; remove the profile option for other output kinds";
        let threads = "--profile-generate and --profile-use cannot be combined with --wasm-feature threads: the counters are not atomic";
        let emit = "profile-guided optimization needs --emit exe or object; remove the profile option for other output kinds";
        let only_build_or_run =
            "--profile-generate and --profile-use are only valid with build or run";
        for (values, expected) in [
            (
                &[
                    "build",
                    "Main.tz",
                    "--profile-generate",
                    "a",
                    "--profile-generate",
                    "b",
                ][..],
                "profile generation specified more than once",
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--profile-use",
                    "a",
                    "--profile-use",
                    "b",
                ],
                "profile use specified more than once",
            ),
            (
                &["build", "Main.tz", "--profile-generate"],
                "--profile-generate needs a value",
            ),
            (
                &["build", "Main.tz", "--profile-use"],
                "--profile-use needs a value",
            ),
            (
                &["build", "Main.tz", "--profile-generate", ""],
                "--profile-generate needs a value",
            ),
            (
                &["build", "Main.tz", "--profile-use", ""],
                "--profile-use needs a value",
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--profile-generate",
                    "a",
                    "--profile-use",
                    "b",
                ],
                "--profile-generate and --profile-use cannot be combined; build once with each",
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "object",
                    "--profile-use",
                    "p",
                ],
                wasm_emit,
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--emit",
                    "bitcode",
                    "--profile-generate",
                    "p",
                ],
                wasm_emit,
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "threads",
                    "--profile-generate",
                    "p",
                ],
                threads,
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-opt",
                    "--profile-generate",
                    "p",
                ],
                "--profile-generate cannot be combined with --wasm-opt: Binaryen may drop what the profile writer reads",
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm64",
                    "--profile-use",
                    "p",
                ],
                wasm64,
            ),
            (
                &["build", "Main.tz", "--emit", "llvm", "--profile-use", "p"],
                emit,
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--emit",
                    "header",
                    "--profile-generate",
                    "p",
                ],
                emit,
            ),
            (
                &["build", "Main.tz", "--emit", "wgsl", "--profile-use", "p"],
                emit,
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--emit",
                    "bindings-py",
                    "--profile-use",
                    "p",
                ],
                emit,
            ),
            (
                &["check", "Main.tz", "--profile-use", "p"],
                only_build_or_run,
            ),
            (
                &["test", "Main.tz", "--profile-use", "p"],
                only_build_or_run,
            ),
            (
                &["bench", "Main.tz", "--profile-generate", "p"],
                only_build_or_run,
            ),
            (&["fmt", "Main.tz", "--profile-use", "p"], only_build_or_run),
            (&["doc", "Main.tz", "--profile-use", "p"], only_build_or_run),
        ] {
            assert_eq!(parse(values).unwrap_err(), expected, "{values:?}");
        }
        // The directory and --freestanding are looked at after the host: a Windows host refuses the options before that.
        let windows = "--profile-generate and --profile-use are not supported on Windows yet; build on macOS or Linux";
        for (values, expected) in [
            (
                &["build", "Main.tz", "--profile-generate", "/tmp/100%/prof"][..],
                "--profile-generate directory must not contain '%'; choose another directory",
            ),
            (
                &[
                    "build",
                    "Main.tz",
                    "--emit",
                    "object",
                    "--allocator",
                    "host",
                    "--freestanding",
                    "--profile-generate",
                    "p",
                ],
                "--profile-generate cannot be combined with --freestanding: the profile runtime needs the C library",
            ),
        ] {
            let expected = if cfg!(windows) { windows } else { expected };
            assert_eq!(parse(values).unwrap_err(), expected, "{values:?}");
        }
        // The bitcode of --emit bitcode (PR08) is not instrumented or trained: a host's own link would do that.
        // A Windows host refuses --emit bitcode for itself before this check.
        if !cfg!(windows) {
            for option in ["--profile-generate", "--profile-use"] {
                assert_eq!(
                    parse(&["build", "Main.tz", "--emit", "bitcode", option, "p"]).unwrap_err(),
                    emit,
                    "{option}"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn rejects_profile_options_on_windows() {
        for option in ["--profile-generate", "--profile-use"] {
            assert_eq!(
                parse(&["build", "Main.tz", option, "p"]).unwrap_err(),
                "--profile-generate and --profile-use are not supported on Windows yet; build on macOS or Linux"
            );
        }
    }

    #[test]
    fn repl_and_script_do_not_take_profile_options() {
        for option in ["--profile-generate", "--profile-use"] {
            assert_eq!(parse_repl(&[option, "p"]).unwrap_err(), REPL_USAGE);
            let split = script_arguments(
                &[option, "p", "a.tz"]
                    .iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            );
            assert_eq!(split.unwrap_err().message, SCRIPT_OPTIONS);
        }
    }

    #[test]
    fn parses_allocator_selection() {
        use tsuzuri::llvm::Allocator;
        let allocator = |values: &[&str]| parse(values).unwrap().options.allocator;
        for action in ["build", "run", "test"] {
            assert_eq!(
                allocator(&[action, "Main.tz", "--allocator", "counting"]),
                Allocator::Counting
            );
        }
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
        // `small` is for native executables, objects, shared libraries, LLVM IR, and headers (PM05).
        for emit in ["object", "llvm", "header"] {
            assert_eq!(
                allocator(&["build", "Main.tz", "--emit", emit, "--allocator", "small"]),
                Allocator::Small
            );
        }
        assert_eq!(
            allocator(&["build", "Main.tz", "--allocator", "small"]),
            Allocator::Small
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
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "wgsl",
                    "--allocator",
                    "counting",
                ],
                "--allocator counting requires executable, object, shared library, LLVM IR, header, or WebAssembly output",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--allocator",
                    "small",
                ],
                "--allocator small requires a native target; WebAssembly modules keep their own heap",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm64",
                    "--allocator",
                    "small",
                ],
                "--allocator small requires a native target; WebAssembly modules keep their own heap",
            ),
            (
                vec!["build", "Main.tz", "--emit", "wgsl", "--allocator", "small"],
                "--allocator small requires executable, object, shared library, LLVM IR, or header output",
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
                    "small",
                ],
                "--trap-mode return keeps its own tracked allocator; remove --allocator",
            ),
            (
                vec![
                    "build",
                    "Main.tz",
                    "--emit",
                    "object",
                    "--freestanding",
                    "--allocator",
                    "small",
                ],
                "--freestanding requires --allocator host: without the C library the host provides the heap",
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
                "allocator must be 'system', 'host', 'counting', or 'small'",
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

    #[test]
    fn watch_arguments_accept_check_build_test_only() {
        let watch = |values: &[&str]| {
            parse_watch_arguments(&values.iter().map(OsString::from).collect::<Vec<_>>())
                .map_err(|error| error.message.clone())
        };
        for (values, action, input) in [
            (vec!["check", "Main.tz"], Action::Check, "Main.tz"),
            (
                vec!["build", "Main.tz", "-O0", "-o", "out"],
                Action::Build,
                "Main.tz",
            ),
            (
                vec!["test", "Specs.tz", "--filter", "x"],
                Action::Test,
                "Specs.tz",
            ),
            (vec!["check", "app", "--json"], Action::Check, "app"),
            // A directory named `watch` or `check` is a path after the command.
            (vec!["check", "watch"], Action::Check, "watch"),
            (vec!["build", "--", "check"], Action::Build, "check"),
        ] {
            let parsed = watch(&values).unwrap_or_else(|error| panic!("{values:?}: {error}"));
            assert_eq!(parsed.action, action, "{values:?}");
            assert_eq!(parsed.input, PathBuf::from(input), "{values:?}");
        }
        let usage = "watch runs check, build, or test; use tsuzuri watch check <path>";
        for values in [
            vec![],
            vec!["run", "Main.tz"],
            vec!["fmt", "Main.tz"],
            vec!["lsp"],
            vec!["doc", "app", "-o", "out"],
            vec!["bench", "Specs.tz"],
            vec!["watch", "check", "Main.tz"],
            // An input without a command is not a build: the command is always named.
            vec!["Main.tz"],
            vec!["--json", "check", "Main.tz"],
        ] {
            assert_eq!(watch(&values).unwrap_err(), usage, "{values:?}");
        }
        // The command's own rules apply.
        assert_eq!(
            watch(&["check", "Main.tz", "--no-cache"]).unwrap_err(),
            "--no-cache is only valid with build, run, script, or repl"
        );
        assert!(watch(&["check"]).is_err());
        assert!(watch(&["check", "a", "b"]).is_err());
    }

    #[test]
    fn serve_arguments_need_one_directory_and_valid_timeout() {
        let serve = |values: &[&str]| {
            parse_serve_arguments(&values.iter().map(OsString::from).collect::<Vec<_>>())
                .map_err(|error| error.message.clone())
        };
        let run = |input: &str, idle| ServeMode::Run {
            input: PathBuf::from(input),
            idle,
        };
        assert_eq!(serve(&["app"]), Ok(run("app", 1800)));
        assert_eq!(serve(&["app", "--idle-timeout", "1"]), Ok(run("app", 1)));
        assert_eq!(
            serve(&["--idle-timeout", "86400", "app"]),
            Ok(run("app", 86400))
        );
        assert_eq!(serve(&["app/Main.tz"]), Ok(run("app/Main.tz", 1800)));
        assert_eq!(
            serve(&["--status", "app"]),
            Ok(ServeMode::Status(PathBuf::from("app")))
        );
        assert_eq!(
            serve(&["app", "--stop"]),
            Ok(ServeMode::Stop(PathBuf::from("app")))
        );
        for values in [
            vec![],
            vec!["a", "b"],
            vec!["--status"],
            vec!["--idle-timeout", "5"],
            vec!["app", "--idle-timeout"],
            vec!["app", "--idle-timeout", "0"],
            vec!["app", "--idle-timeout", "86401"],
            vec!["app", "--idle-timeout", "x"],
            vec!["app", "--idle-timeout", "-1"],
            vec!["app", "--idle-timeout", "1", "--idle-timeout", "2"],
            vec!["app", "-O0"],
            vec!["app", "--json"],
            vec!["--status", "--stop", "app"],
            vec!["--status", "--status", "app"],
            vec!["--stop", "--stop", "app"],
            vec!["--status", "app", "--idle-timeout", "5"],
            vec!["--stop", "app", "--idle-timeout", "5"],
        ] {
            assert!(serve(&values).is_err(), "{values:?}");
        }
        assert_eq!(
            serve(&["app", "--idle-timeout", "0"]).unwrap_err(),
            "--idle-timeout must be an integer from 1 to 86400, not '0'"
        );
        assert_eq!(serve(&[]).unwrap_err(), SERVE_USAGE);
    }

    #[test]
    fn command_line_errors_carry_their_kinds() {
        let strings = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
        for (values, expected) in [
            (
                &["build", "Main.tz", "--nope"][..],
                DiagnosticKind::UnknownOption,
            ),
            (
                &["build", "-O0", "-O3", "Main.tz"],
                DiagnosticKind::DuplicateOption,
            ),
            (
                &["build", "Main.tz", "--emit"],
                DiagnosticKind::OptionMissingValue,
            ),
            (
                &["build", "Main.tz", "--cpu", "fast"],
                DiagnosticKind::OptionInvalidValue,
            ),
            (
                &["build", "Main.tz", "--target", "nope"],
                DiagnosticKind::UnknownTarget,
            ),
            (&["build"], DiagnosticKind::MissingInput),
            (
                &["build", "a.tz", "b.tz"],
                DiagnosticKind::InputCountInvalid,
            ),
            (
                &["check", "Main.tz", "--strip"],
                DiagnosticKind::OptionInvalidForCommand,
            ),
            (
                &["test", "Main.tz", "--coverage", "c", "--list"],
                DiagnosticKind::OptionConflict,
            ),
            // `BuildOptions::validate` gives the kind, which the command line keeps.
            (
                &[
                    "build",
                    "Main.tz",
                    "--target",
                    "wasm32",
                    "--wasm-feature",
                    "net",
                ],
                DiagnosticKind::OptionRequiresOption,
            ),
        ] {
            let error = parse_arguments(&strings(values)).unwrap_err();
            assert_eq!(
                error.kind_of(),
                Some(expected),
                "{values:?}: {}",
                error.message
            );
            assert_eq!(
                (error.code, error.span),
                ("E2000", Span::default()),
                "{values:?}"
            );
        }
        let kind = |result: Result<(), Diagnostic>| result.unwrap_err().kind_of();
        let usage = Some(DiagnosticKind::UsageError);
        assert_eq!(
            kind(parse_repl_arguments(&strings(&["--bogus"])).map(drop)),
            usage
        );
        assert_eq!(kind(script_arguments(&strings(&[])).map(drop)), usage);
        assert_eq!(
            kind(parse_watch_arguments(&strings(&["run", "a.tz"])).map(drop)),
            usage
        );
        assert_eq!(
            kind(parse_serve_arguments(&strings(&["--status", "--stop", "app"])).map(drop)),
            Some(DiagnosticKind::OptionConflict)
        );
    }

    #[test]
    fn no_server_is_only_valid_with_check_or_build() {
        for values in [
            vec!["check", "Main.tz", "--no-server"],
            vec!["build", "Main.tz", "--no-server", "-O0"],
            vec!["--no-server", "Main.tz"],
        ] {
            assert!(parse(&values).unwrap().no_server, "{values:?}");
        }
        assert!(!parse(&["check", "Main.tz"]).unwrap().no_server);
        for values in [
            vec!["run", "Main.tz", "--no-server"],
            vec!["test", "Main.tz", "--no-server"],
            vec!["bench", "Main.tz", "--no-server"],
            vec!["doc", "Main.tz", "-o", "out", "--no-server"],
            vec!["fmt", "Main.tz", "--no-server"],
        ] {
            assert_eq!(
                parse(&values).unwrap_err(),
                "--no-server is only valid with check or build",
                "{values:?}"
            );
        }
        assert_eq!(
            parse(&["check", "Main.tz", "--no-server", "--no-server"]).unwrap_err(),
            "no-server specified more than once"
        );
        assert!(parse(&["lsp", "--no-server"]).is_err());
    }

    /// G27: the knowledge graph flags belong to check, build, run and test.
    #[test]
    fn accepts_the_kg_flags_only_where_they_apply() {
        for command in ["check", "build", "run", "test"] {
            let parsed = parse(&[command, "Main.tz", "--kg", "--kg-dir", "out"]).unwrap();
            assert!(parsed.kg.kg && !parsed.kg.no_kg, "{command}");
            assert_eq!(parsed.kg.dir, Some(PathBuf::from("out")), "{command}");
            assert!(parse(&[command, "Main.tz", "--no-kg"]).unwrap().kg.no_kg);
        }
        for values in [
            vec!["doc", "Main.tz", "-o", "out", "--kg"],
            vec!["fmt", "Main.tz", "--no-kg"],
            vec!["bench", "Main.tz", "--kg-dir", "x"],
        ] {
            assert_eq!(
                parse(&values).unwrap_err(),
                "--kg, --no-kg, and --kg-dir are only valid with check, build, run, or test",
                "{values:?}"
            );
        }
        for (values, message) in [
            (
                vec!["check", "Main.tz", "--kg", "--kg"],
                "kg specified more than once",
            ),
            (
                vec!["check", "Main.tz", "--no-kg", "--no-kg"],
                "no-kg specified more than once",
            ),
            (
                vec!["check", "Main.tz", "--kg-dir", "a", "--kg-dir", "b"],
                "kg-dir specified more than once",
            ),
            (
                vec!["check", "Main.tz", "--kg-dir"],
                "--kg-dir needs a value",
            ),
            (
                vec!["check", "Main.tz", "--kg-dir", ""],
                "--kg-dir must not be empty",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
        // Both flags parse; the settings reject them (E2000) before any source is read.
        assert!(parse(&["check", "Main.tz", "--kg", "--no-kg"]).is_ok());
    }

    /// G28: `--affected BASE` selects the tests of `test` and nothing else.
    #[test]
    fn accepts_affected_only_with_test() {
        let parsed = parse(&["test", "Main.tz", "--affected", "HEAD"]).unwrap();
        assert_eq!(parsed.affected.as_deref(), Some("HEAD"));
        for (values, message) in [
            (
                vec!["check", "Main.tz", "--affected", "HEAD"],
                "--affected is only valid with test",
            ),
            (
                vec!["test", "Main.tz", "--affected", "a", "--affected", "b"],
                "affected specified more than once",
            ),
            (
                vec!["test", "Main.tz", "--affected"],
                "--affected needs a value",
            ),
            (
                vec!["test", "Main.tz", "--affected", "HEAD", "--filter", "x"],
                "--affected cannot be combined with --filter or --index",
            ),
            (
                vec!["test", "Main.tz", "--affected", "HEAD", "--index", "0"],
                "--affected cannot be combined with --filter or --index",
            ),
            (
                vec!["test", "Main.tz", "--affected", "HEAD", "--no-kg"],
                "--affected needs the KG; remove --no-kg or TSUZURI_KG=0",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
    }

    #[test]
    fn the_help_lists_the_kg_options() {
        let text = help();
        for needle in [
            "--kg-dir",
            "--no-kg",
            "TSUZURI_KG=0",
            "TSUZURI_KG_DIR",
            "tsuzuri kg build",
            "tsuzuri kg verify",
            "tsuzuri kg export",
            "tsuzuri kg brief",
            "tsuzuri kg show",
            "tsuzuri kg callers|callees",
            "tsuzuri kg search",
            "tsuzuri kg find",
            "tsuzuri kg path",
            "tsuzuri kg impact",
            "[--affected BASE]",
            "tsuzuri kg diagram",
        ] {
            assert!(text.contains(needle), "{needle}");
        }
    }

    /// A server hands a command's `out` and `err` to the client. A print that bypasses them in a
    /// function that `check` or `build` reach would appear on the server's terminal instead.
    #[test]
    fn only_functions_that_a_server_never_runs_print_directly() {
        let source = include_str!("main.rs");
        let source = &source[..source.find("#[cfg(test)]").unwrap()];
        let mut function = "";
        let mut found = std::collections::BTreeSet::new();
        for line in source.lines() {
            let line = line.trim_start();
            let declared = ["pub fn ", "async fn ", "fn "]
                .iter()
                .find_map(|prefix| line.strip_prefix(prefix));
            if let Some(rest) = declared {
                function = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap();
            }
            let prints = ["println!(", "print!(", "io::stdout()", "io::stderr()"]
                .iter()
                .any(|site| line.contains(site));
            if prints && !line.starts_with("//") {
                found.insert(function);
            }
        }
        let expected = std::collections::BTreeSet::from([
            "api_diff_command",
            "bindgen_command",
            "debug_test",
            "explain_command",
            "fix_command",
            "fix_edition_command",
            "guide_command",
            "main",
            "new_project",
            "print_coverage",
            "print_diagnostic",
            "print_diagnostics",
            "print_output",
            "publish_package",
            "repl_command",
            "run_bench_action",
            "run_formatter",
            "run_test_action",
            "select_affected",
            "serve_main",
            "start_server",
            "watch_main",
        ]);
        assert_eq!(
            found, expected,
            "write through the `out` and `err` of `cli` in whatever `check` and `build` reach; \
             list a function here only if a server refuses its command"
        );
    }

    #[test]
    fn a_command_that_panics_is_not_trusted_and_the_state_keeps_the_compiler_it_pinned() {
        let directory = env::temp_dir().join(format!("tz-panic-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("Main.tz"), "42\n").unwrap();
        let mut server = CommandServer {
            state: tsuzuri::resident::ResidentState::new(),
            root: directory.clone(),
        };
        let pinned = server.state.compiler_identity();
        assert!(pinned.is_some());
        // A command that read the project leaves its sources in the state.
        let (code, out, _) = server
            .guarded(|out, _| {
                out.extend_from_slice(b"read\n");
                u8::from(tsuzuri::driver::Project::load(&directory).is_err())
            })
            .unwrap();
        assert_eq!((code, out.as_slice()), (0, b"read\n".as_slice()));
        if cfg!(unix) {
            assert!(server.state.bytes() > 0);
        }
        // A panic gives no result; the state forgets what it held, and it is still the state of
        // the compiler that the process started from.
        let outcome = server.guarded(|_, _| panic!("a command that panics"));
        assert!(outcome.is_none());
        assert_eq!(server.state.bytes(), 0);
        assert_eq!(server.state.compiler_identity(), pinned);
        // The state works after it.
        assert!(server.guarded(|_, _| 0).is_some());
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
