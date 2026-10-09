use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::check::{CheckedModule, ModuleOrigin};
use crate::diagnostic::{Diagnostic, Span};
use crate::llvm::{self, Entry};
use crate::syntax::{MAX_SOURCE_BYTES, SourceKind};

#[path = "test_runner.rs"]
mod test_runner;
pub use test_runner::{
    BENCH_SAMPLE_NS, BENCH_TIMEOUT, BenchOptions, BenchReport, BenchResult, CoverageRun,
    DEFAULT_BENCH_SAMPLES, DebugRunner, MAX_BENCH_ITERATIONS, TestOptions, TestReport, TestResult,
    build_debug_runner, check_coverage_output, run_benches, run_benches_linked, run_tests,
    run_tests_linked, write_coverage,
};

#[path = "bindgen_driver.rs"]
mod bindgen_driver;
pub use bindgen_driver::bindgen;

/// The most link inputs the command line and the manifest may give together.
pub const MAX_LINK_INPUTS: usize = 256;

/// Host objects, libraries and search directories linked into a native executable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkInputs {
    /// Object files and static libraries, linked in this order.
    pub paths: Vec<PathBuf>,
    /// System library names for `-l`, without the `lib` prefix or extension.
    pub libraries: Vec<String>,
    /// Directories the linker searches for `libraries`.
    pub search: Vec<PathBuf>,
}

/// Why `name` cannot follow `-l`, if it cannot.
pub fn library_name_error(name: &str) -> Option<String> {
    const EXTENSIONS: [&str; 8] = [".a", ".so", ".dylib", ".lib", ".dll", ".o", ".obj", ".tbd"];
    let valid = name
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'.' | b'-'))
        && !(name.starts_with("lib") && name.len() > 3)
        && !EXTENSIONS.iter().any(|extension| name.ends_with(extension));
    (!valid).then(|| {
        format!(
            "invalid library name '{name}'; pass the name without the 'lib' prefix or extension, for example -l sqlite3"
        )
    })
}

impl LinkInputs {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.libraries.is_empty() && self.search.is_empty()
    }

    /// These inputs followed by `later` ones: the manifest's, then the command line's.
    pub fn followed_by(mut self, later: &LinkInputs) -> Self {
        self.paths.extend(later.paths.iter().cloned());
        self.libraries.extend(later.libraries.iter().cloned());
        self.search.extend(later.search.iter().cloned());
        self
    }

    /// The inputs as a whole: their number, repeats and library names. Does not touch the file system.
    pub fn check_shape(&self) -> Result<(), Diagnostic> {
        if self.paths.len() + self.libraries.len() + self.search.len() > MAX_LINK_INPUTS {
            return Err(driver_error(
                "E2000",
                format!("too many link inputs; at most {MAX_LINK_INPUTS} are supported"),
            ));
        }
        for name in &self.libraries {
            if let Some(message) = library_name_error(name) {
                return Err(driver_error("E2000", message));
            }
        }
        let repeated = |seen: &mut BTreeSet<String>, input: String| {
            if seen.insert(input.clone()) {
                Ok(())
            } else {
                Err(driver_error(
                    "E2000",
                    format!("link input '{input}' specified more than once"),
                ))
            }
        };
        let mut seen = BTreeSet::new();
        for path in &self.paths {
            repeated(&mut seen, path.display().to_string())?;
        }
        let mut seen = BTreeSet::new();
        for path in &self.search {
            repeated(&mut seen, path.display().to_string())?;
        }
        let mut seen = BTreeSet::new();
        for name in &self.libraries {
            repeated(&mut seen, name.clone())?;
        }
        Ok(())
    }

    /// That every object or library path is a file and every search path a directory.
    pub fn check_readable(&self) -> Result<(), Diagnostic> {
        let check = |path: &Path, directory: bool| match fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() == directory => Ok(()),
            Ok(_) => Err(driver_error(
                "E2001",
                format!(
                    "cannot read link input '{}': not a {}",
                    path.display(),
                    if directory { "directory" } else { "file" }
                ),
            )),
            Err(error) => Err(io_error("read link input", path, error)),
        };
        for path in &self.paths {
            check(path, false)?;
        }
        for path in &self.search {
            check(path, true)?;
        }
        Ok(())
    }

    /// Adds the inputs to a clang link line: `-L` directories, then paths, then `-l` libraries.
    fn add_to(&self, clang: &mut Command) {
        for directory in &self.search {
            let mut option = OsString::from("-L");
            option.push(directory);
            clang.arg(option);
        }
        if !self.paths.is_empty() {
            // An earlier `-x ir` or `-x c` would otherwise apply to the host objects.
            clang.args(["-x", "none"]).args(&self.paths);
        }
        for name in &self.libraries {
            clang.arg(format!("-l{name}"));
        }
    }
}

/// Refuses an output that would replace a link input.
fn protect_links(links: &LinkInputs, output: &Path) -> Result<(), Diagnostic> {
    let Ok(metadata) = fs::symlink_metadata(output) else {
        return Ok(());
    };
    for input in &links.paths {
        let same = fs::canonicalize(input).ok() == fs::canonicalize(output).ok()
            || same_file(input, output, &metadata).unwrap_or(false);
        if same {
            return Err(driver_error(
                "E2003",
                format!(
                    "output '{}' would overwrite a link input; choose a different -o path",
                    output.display()
                ),
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Native,
    Wasm32,
    Wasm64,
}

impl Target {
    pub fn is_wasm(self) -> bool {
        matches!(self, Self::Wasm32 | Self::Wasm64)
    }
}

/// Why wasm output cannot reach the operating-system primitives of E08.
pub(crate) const OS_WASM_MESSAGE: &str = "wasm output cannot use the File, Dir, Env, Time, Random, or Process operating-system APIs because the default wasm target has no host imports; build for the native target, use --wasm-host wasi, or keep to Path and Random.Pcg, which need no host";
/// Why a Windows build cannot reach them yet.
pub(crate) const OS_WINDOWS_MESSAGE: &str = "the File, Dir, Env, Time, Random, and Process operating-system APIs are not supported on Windows yet (G10); build on macOS or Linux";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cpu {
    Generic,
    Native,
}

/// The host a wasm32 module expects (E08 stage C). Without one, wasm output has no imports beyond
/// `tsuzuri_io`, `tsuzuri_debug`, and the program's own `extern` imports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmHost {
    /// WASI preview1: the standard IO and the operating-system APIs use `wasi_snapshot_preview1`.
    Wasi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emit {
    Executable,
    Object,
    Llvm,
    Header,
    Wasm,
    Wgsl,
    /// `--emit bindings-js` (E13): a JavaScript module with TypeScript declarations for a wasm32
    /// module of the same sources.
    BindingsJs,
    /// `--emit shared` (E13 Phase 2): a native shared library that exports the public C ABI.
    Shared,
    /// `--emit bindings-cs` (E13 Phase 2): C# `LibraryImport` bindings of the shared library.
    BindingsCs,
    /// `--emit bindings-py` (E13 Phase 2): a Python `ctypes` module for the shared library.
    BindingsPy,
    /// `--emit bindings-cpp` (E13 Phase 2): a header-only C++20 wrapper over the C header.
    BindingsCpp,
}

impl Emit {
    /// The host bindings that come from the sources alone, without LLVM (E13).
    pub fn is_bindings(self) -> bool {
        matches!(
            self,
            Self::BindingsJs | Self::BindingsCs | Self::BindingsPy | Self::BindingsCpp
        )
    }

    /// The `--emit` spelling of the bindings, and the extension of their output.
    fn bindings(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::BindingsJs => Some(("bindings-js", "mjs")),
            Self::BindingsCs => Some(("bindings-cs", "cs")),
            Self::BindingsPy => Some(("bindings-py", "py")),
            Self::BindingsCpp => Some(("bindings-cpp", "hpp")),
            _ => None,
        }
    }
}

pub const DEFAULT_WASM_MAX_MEMORY: u64 = 16 * 1024 * 1024;
pub const DEFAULT_WASM_STACK_SIZE: u64 = 1024 * 1024;
/// The wasm32 heap's `%ceil = add i32 %end, 65535` must not wrap.
pub const MAX_WASM32_MEMORY: u64 = (1 << 32) - 65536;
/// wasm-ld's limit for 64-bit memories.
pub const MAX_WASM64_MEMORY: u64 = 1 << 34;

#[derive(Clone, Copy, Debug)]
pub struct BuildOptions {
    pub target: Target,
    pub emit: Emit,
    pub optimization: u8,
    pub cpu: Cpu,
    pub debug_output: bool,
    pub trap_info: bool,
    pub debug_info: bool,
    pub wasm_simd: bool,
    pub wasm_threads: bool,
    /// `--wasm-feature jspi`: `Async.block_on` waits through JavaScript Promise Integration (B08).
    pub wasm_jspi: bool,
    /// `--wasm-host`: lowers the standard IO and the operating-system APIs to the named host.
    pub wasm_host: Option<WasmHost>,
    /// `None` selects [`DEFAULT_WASM_MAX_MEMORY`].
    pub wasm_max_memory: Option<u64>,
    /// `None` selects [`DEFAULT_WASM_STACK_SIZE`].
    pub wasm_stack_size: Option<u64>,
    pub cache: bool,
    /// `--trap-mode return`: native exports also come as `tsuzuri_try_<name>` (E14 Phase 2).
    pub trap_return: bool,
    /// `--allocator`: the heap runtime (F13).
    pub allocator: llvm::Allocator,
    /// `--freestanding`: a native object that needs no C library (F13 Phase 3).
    pub freestanding: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            emit: Emit::Executable,
            optimization: 3,
            cpu: Cpu::Generic,
            debug_output: false,
            trap_info: false,
            debug_info: false,
            wasm_simd: false,
            wasm_threads: false,
            wasm_jspi: false,
            wasm_host: None,
            wasm_max_memory: None,
            wasm_stack_size: None,
            cache: true,
            trap_return: false,
            allocator: llvm::Allocator::System,
            freestanding: false,
        }
    }
}

impl BuildOptions {
    pub fn validate(self) -> Result<(), Diagnostic> {
        if self.emit == Emit::Wgsl
            && (self.target != Target::Native
                || self.cpu != Cpu::Generic
                || self.debug_info
                || self.debug_output
                || self.trap_info
                || self.wasm_simd
                || self.wasm_threads
                || self.wasm_jspi)
        {
            return Err(driver_error(
                "E2000",
                "WGSL output does not use target, CPU, debug, or WASM feature options",
            ));
        }
        if self.emit.is_bindings() {
            self.validate_bindings()?;
        }
        if self.emit == Emit::Shared {
            self.validate_shared()?;
        }
        if self.wasm_threads
            && (self.target != Target::Wasm32
                || !matches!(self.emit, Emit::Wasm | Emit::Object | Emit::BindingsJs))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-feature threads requires wasm32 object or WASM output",
            ));
        }
        if self.wasm_jspi
            && (!self.target.is_wasm()
                || !matches!(
                    self.emit,
                    Emit::Wasm | Emit::Object | Emit::Llvm | Emit::BindingsJs
                ))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-feature jspi requires wasm32 or wasm64 object, LLVM IR, WASM, or JavaScript bindings output",
            ));
        }
        if self.wasm_jspi && (self.wasm_threads || self.wasm_host.is_some()) {
            return Err(driver_error(
                "E2000",
                "--wasm-feature jspi cannot be combined with --wasm-feature threads or --wasm-host",
            ));
        }
        if self.wasm_simd && (!self.target.is_wasm() || self.emit == Emit::Header) {
            return Err(driver_error(
                "E2000",
                "--wasm-feature simd128 requires wasm32 or wasm64 object, LLVM IR, or WASM output",
            ));
        }
        if self.wasm_host.is_some()
            && (self.target != Target::Wasm32 || !matches!(self.emit, Emit::Object | Emit::Wasm))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-host wasi requires wasm32 object or WASM output",
            ));
        }
        if self.wasm_host.is_some() && self.wasm_threads {
            return Err(driver_error(
                "E2000",
                "--wasm-host wasi cannot be combined with --wasm-feature threads",
            ));
        }
        if self.debug_info && self.emit == Emit::Header {
            return Err(driver_error(
                "E2000",
                "--debug-info is not valid for header output",
            ));
        }
        if self.trap_info && self.emit == Emit::Header {
            return Err(driver_error(
                "E2000",
                "--trap-info is not valid for header output",
            ));
        }
        if self.debug_output && self.emit == Emit::Header {
            return Err(driver_error(
                "E2000",
                "--debug-output is not valid for header output",
            ));
        }
        if self.trap_return
            && (self.target != Target::Native
                || !matches!(
                    self.emit,
                    Emit::Object
                        | Emit::Llvm
                        | Emit::Header
                        | Emit::Shared
                        | Emit::BindingsCs
                        | Emit::BindingsPy
                        | Emit::BindingsCpp
                ))
        {
            return Err(driver_error(
                "E2000",
                "--trap-mode return is only valid for native object, llvm or header output",
            ));
        }
        if self.optimization > 3 {
            return Err(driver_error(
                "E2000",
                "optimization level must be 0, 1, 2, or 3",
            ));
        }
        self.validate_allocator()?;
        if (self.emit == Emit::Executable && self.target != Target::Native)
            || (self.emit == Emit::Wasm && !self.target.is_wasm())
        {
            return Err(driver_error(
                "E2000",
                "'--emit exe' requires '--target native'; '--emit wasm' requires '--target wasm32' or '--target wasm64'",
            ));
        }
        if self.cpu == Cpu::Native {
            if self.target != Target::Native
                || matches!(self.emit, Emit::Llvm | Emit::Header)
                || self.emit.is_bindings()
            {
                return Err(driver_error(
                    "E2000",
                    "'--cpu native' requires native executable or object output",
                ));
            }
            native_cpu_flag(env::consts::ARCH)?;
        }
        if self.wasm_max_memory.is_some()
            && (!self.target.is_wasm()
                || !matches!(self.emit, Emit::Object | Emit::Llvm | Emit::Wasm))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-max-memory requires wasm32 or wasm64 object, LLVM IR, or WASM output",
            ));
        }
        if self.wasm_stack_size.is_some() && (!self.target.is_wasm() || self.emit != Emit::Wasm) {
            return Err(driver_error(
                "E2000",
                "--wasm-stack-size requires WASM output; link object and LLVM IR output with wasm-ld -z stack-size",
            ));
        }
        wasm_memory_limits(self.target, self.wasm_max_memory, self.wasm_stack_size)?;
        Ok(())
    }

    /// The options of the bindings (E13). They come from the sources alone, so the options that
    /// shape the `.wasm` or the shared library belong to the build of that artifact.
    fn validate_bindings(self) -> Result<(), Diagnostic> {
        if self.emit != Emit::BindingsJs {
            if self.target != Target::Native {
                let (kind, _) = self.emit.bindings().expect("bindings output");
                return Err(driver_error(
                    "E2000",
                    format!("'--emit {kind}' requires '--target native'"),
                ));
            }
            for (set, option) in [
                (self.trap_info, "--trap-info"),
                (self.debug_info, "--debug-info"),
                (self.debug_output, "--debug-output"),
                (self.allocator != llvm::Allocator::System, "--allocator"),
                (self.freestanding, "--freestanding"),
            ] {
                if set {
                    return Err(driver_error(
                        "E2000",
                        format!(
                            "{option} is not valid for bindings output; pass it when building the shared library"
                        ),
                    ));
                }
            }
            return Ok(());
        }
        if self.target != Target::Wasm32 {
            return Err(driver_error(
                "E2000",
                "'--emit bindings-js' requires '--target wasm32'",
            ));
        }
        for (set, option) in [
            (self.trap_info, "--trap-info"),
            (self.debug_info, "--debug-info"),
            (self.debug_output, "--debug-output"),
            // `threads` selects the glue of a thread pool and `jspi` the Promise-returning glue;
            // SIMD does not change the glue.
            (self.wasm_simd, "--wasm-feature"),
            (self.allocator != llvm::Allocator::System, "--allocator"),
        ] {
            if set {
                return Err(driver_error(
                    "E2000",
                    format!(
                        "{option} is not valid for bindings output; pass it when building the .wasm"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// The options of `--emit shared` (E13 Phase 2): a native library that hosts load at run time.
    fn validate_shared(self) -> Result<(), Diagnostic> {
        if self.target != Target::Native {
            return Err(driver_error(
                "E2000",
                "'--emit shared' requires '--target native'",
            ));
        }
        if cfg!(windows) {
            return Err(driver_error(
                "E2000",
                "'--emit shared' is not supported on Windows yet (G10); build an object with --emit object and link it into a DLL",
            ));
        }
        if self.allocator == llvm::Allocator::Host {
            return Err(driver_error(
                "E2000",
                "--allocator host cannot be combined with --emit shared: a shared library resolves its symbols when it is linked; link the object into the host that defines tsuzuri_host_alloc, tsuzuri_host_free, and tsuzuri_host_realloc",
            ));
        }
        Ok(())
    }

    /// The combinations of `--allocator` and `--freestanding` (F13): the target first, then the output.
    fn validate_allocator(self) -> Result<(), Diagnostic> {
        if self.allocator == llvm::Allocator::Host && self.wasm_threads {
            return Err(driver_error(
                "E2000",
                "--allocator host cannot be combined with --wasm-feature threads; WASM threads keep their locked internal allocator",
            ));
        }
        if self.allocator == llvm::Allocator::Host
            && matches!(self.emit, Emit::Executable | Emit::Wgsl)
        {
            return Err(driver_error(
                "E2000",
                "--allocator host requires object, LLVM IR, header, or WebAssembly output; link the object into a host that defines tsuzuri_host_alloc, tsuzuri_host_free, and tsuzuri_host_realloc",
            ));
        }
        if self.allocator == llvm::Allocator::Counting
            && matches!(self.emit, Emit::Executable | Emit::Wgsl)
        {
            return Err(driver_error(
                "E2000",
                "--allocator counting requires object, LLVM IR, header, or WebAssembly output; the host reads the counts with tsuzuri_alloc_stats",
            ));
        }
        if self.allocator != llvm::Allocator::System && self.trap_return {
            return Err(driver_error(
                "E2000",
                "--trap-mode return keeps its own tracked allocator; remove --allocator",
            ));
        }
        if self.freestanding {
            if self.target != Target::Native {
                return Err(driver_error(
                    "E2000",
                    "--freestanding requires a native target",
                ));
            }
            if !matches!(self.emit, Emit::Object | Emit::Llvm | Emit::Header) {
                return Err(driver_error(
                    "E2000",
                    "--freestanding requires object, LLVM IR, or header output",
                ));
            }
            if self.allocator != llvm::Allocator::Host {
                return Err(driver_error(
                    "E2000",
                    "--freestanding requires --allocator host: without the C library the host provides the heap",
                ));
            }
            if self.trap_info || self.debug_output {
                return Err(driver_error(
                    "E2000",
                    "--freestanding cannot be combined with --trap-info or --debug-output: they write through the C library",
                ));
            }
        }
        Ok(())
    }

    /// Fills options the command line left unset from the root manifest's
    /// `[wasm]`, for the outputs where each option applies.
    pub fn with_manifest_wasm(mut self, wasm: crate::package::WasmSettings) -> Self {
        if self.target.is_wasm() && matches!(self.emit, Emit::Object | Emit::Llvm | Emit::Wasm) {
            self.wasm_max_memory = self.wasm_max_memory.or(wasm.max_memory);
        }
        if self.target.is_wasm() && self.emit == Emit::Wasm {
            self.wasm_stack_size = self.wasm_stack_size.or(wasm.stack_size);
        }
        self
    }

    pub fn output_path(self, input: &Path) -> PathBuf {
        input.with_extension(match self.emit {
            Emit::Executable if cfg!(windows) => "exe",
            Emit::Executable => "",
            Emit::Object if cfg!(windows) => "obj",
            Emit::Object => "o",
            Emit::Llvm => "ll",
            Emit::Header => "h",
            Emit::Wasm => "wasm",
            Emit::Wgsl => "wgsl",
            Emit::Shared if cfg!(windows) => "dll",
            Emit::Shared if cfg!(target_os = "macos") => "dylib",
            Emit::Shared => "so",
            Emit::BindingsJs => "mjs",
            Emit::BindingsCs => "cs",
            Emit::BindingsPy => "py",
            Emit::BindingsCpp => "hpp",
        })
    }
}

/// The error for an output path that bindings cannot use. The JavaScript module must be a `.mjs`
/// file, because TypeScript reads the declarations of `<name>.mjs` only from `<name>.d.mts`; the
/// others keep the extension of their language, and their file stem names the library.
pub fn bindings_output_error(emit: Emit, output: &Path) -> Option<Diagnostic> {
    let (_, extension) = emit.bindings()?;
    let stem = output.file_stem().and_then(OsStr::to_str);
    if output.extension() == Some(OsStr::new(extension))
        && stem.is_some_and(|stem| !stem.is_empty())
    {
        return None;
    }
    Some(driver_error(
        "E2000",
        if emit == Emit::BindingsJs {
            "bindings output must end with '.mjs'; declarations are written next to it as '<name>.d.mts'".to_owned()
        } else {
            format!(
                "bindings output must end with '.{extension}'; its file name, without the extension, names the shared library"
            )
        },
    ))
}

/// The TypeScript declarations next to the JavaScript bindings `output`: `<name>.d.mts`.
pub fn bindings_sidecar_path(output: &Path) -> PathBuf {
    output.with_extension("d.mts")
}

/// Stacks wrap below address 0, which is always out of bounds on wasm64 and on
/// single-threaded wasm32 up to 2 GiB; only other builds need entry checks.
pub(crate) fn wasm_stack_checks(target: Target, threads: bool, max_memory: u64) -> bool {
    target == Target::Wasm32 && (threads || max_memory > 1 << 31)
}

/// Returns the effective (maximum memory, stack size) in bytes.
pub fn wasm_memory_limits(
    target: Target,
    max: Option<u64>,
    stack: Option<u64>,
) -> Result<(u64, u64), Diagnostic> {
    let stack = stack.unwrap_or(DEFAULT_WASM_STACK_SIZE);
    let max = max.unwrap_or(DEFAULT_WASM_MAX_MEMORY);
    if stack % 16 != 0 || stack < 65536 {
        return Err(driver_error(
            "E2000",
            "--wasm-stack-size must be a multiple of 16 bytes and at least 64 KiB",
        ));
    }
    let (ceiling, message) = if target == Target::Wasm64 {
        (
            MAX_WASM64_MEMORY,
            "--wasm-max-memory must be a multiple of 64 KiB and at most 16 GiB on wasm64",
        )
    } else {
        (
            MAX_WASM32_MEMORY,
            "--wasm-max-memory must be a multiple of 64 KiB and at most 4 GiB - 64 KiB on wasm32",
        )
    };
    if max % 65536 != 0 || max > ceiling {
        return Err(driver_error("E2000", message));
    }
    if max < stack.saturating_add(65536) {
        return Err(driver_error(
            "E2000",
            "--wasm-max-memory must be at least the stack size plus 64 KiB; raise the memory limit or lower --wasm-stack-size",
        ));
    }
    Ok((max, stack))
}

/// Explicit memory options can make static data exceed the limit at link time.
fn wasm_link_hint(hint: &str, max: Option<u64>, stack: Option<u64>) -> String {
    if max.is_some() || stack.is_some() {
        format!(
            "{hint}; if the memory limit is too small, raise --wasm-max-memory or lower --wasm-stack-size"
        )
    } else {
        hint.to_owned()
    }
}

fn native_cpu_flag(architecture: &str) -> Result<&'static str, Diagnostic> {
    match architecture {
        "x86" | "x86_64" => Ok("-march=native"),
        "arm" | "aarch64" => Ok("-mcpu=native"),
        _ => Err(driver_error(
            "E2000",
            format!("'--cpu native' is not supported on {architecture}; use '--cpu generic'"),
        )),
    }
}

/// Inlines the Win32 adapter: Clang cannot resolve quoted includes next to
/// verbatim (`\\?\`) source paths, and canonical temporary paths are verbatim.
pub(crate) fn task_runtime_source() -> String {
    let source = include_str!("runtime/task.c");
    if cfg!(windows) {
        source.replacen(
            "#include \"task-windows.h\"",
            include_str!("runtime/task-windows.h"),
            1,
        )
    } else {
        source.to_owned()
    }
}

fn native_compile_args(windows: bool, architecture: &str) -> &'static [&'static str] {
    if windows {
        if architecture == "aarch64" {
            &["--target=aarch64-pc-windows-msvc"]
        } else {
            &["--target=x86_64-pc-windows-msvc"]
        }
    } else {
        &["-fPIC"]
    }
}

#[derive(Debug)]
pub struct SourceFile {
    /// A file path, or a virtual `std/Name.tz` path for std sources.
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub name: String,
    pub text: String,
    pub origin: ModuleOrigin,
    pub package: Option<crate::package::PackageId>,
    /// The namespace that `relative_path` is relative to: the root package's
    /// default namespace for its files, and empty for dependency files, whose
    /// relative paths start with their package namespace, and for std files.
    pub namespace: String,
}

#[derive(Debug)]
pub struct Project {
    pub sources: Vec<SourceFile>,
    pub manifests: Vec<SourceFile>,
    pub root: usize,
    /// The root package's `[wasm]` section.
    pub wasm: crate::package::WasmSettings,
    /// The `[native]` link inputs of the root package and of the dependencies it marks `native = true`,
    /// each resolved against its package root.
    pub native: LinkInputs,
}

#[derive(Debug)]
pub struct SourceError {
    pub path: PathBuf,
    pub diagnostic: Diagnostic,
}

impl SourceError {
    pub(crate) fn new(path: &Path, diagnostic: Diagnostic) -> Self {
        Self {
            path: path.to_owned(),
            diagnostic,
        }
    }
}

impl SourceFile {
    fn read(path: &Path) -> Result<Self, SourceError> {
        let text = read_source(path).map_err(|error| SourceError::new(path, error))?;
        let name = path.file_stem().and_then(OsStr::to_str).ok_or_else(|| {
            SourceError::new(
                path,
                driver_error("E1011", "the module filename must be an ASCII identifier"),
            )
        })?;
        Ok(Self {
            path: path.to_owned(),
            relative_path: path.file_name().map(PathBuf::from).unwrap_or_default(),
            name: name.to_owned(),
            text,
            origin: ModuleOrigin::User,
            package: None,
            namespace: String::new(),
        })
    }
}

pub(crate) fn collect_sources(
    root: &Path,
    package_roots: &[PathBuf],
) -> Result<Vec<PathBuf>, SourceError> {
    let mut pending = vec![(root.to_owned(), 0)];
    let mut directories = 0;
    let mut paths = Vec::new();
    while let Some((directory, depth)) = pending.pop() {
        directories += 1;
        if directories > 1024 || depth >= 16 {
            return Err(SourceError::new(
                &directory,
                driver_error(
                    "E1017",
                    "module discovery exceeds 1024 directories or 16 path segments",
                ),
            ));
        }
        for entry in fs::read_dir(&directory).map_err(|error| {
            SourceError::new(
                &directory,
                io_error("list module directory", &directory, error),
            )
        })? {
            let entry = entry.map_err(|error| {
                SourceError::new(
                    &directory,
                    io_error("list module directory", &directory, error),
                )
            })?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| {
                SourceError::new(&path, io_error("inspect module path", &path, error))
            })?;
            if kind.is_symlink() {
                let target = fs::metadata(&path);
                if source_kind(&path).is_some()
                    || target.as_ref().is_ok_and(|metadata| metadata.is_dir())
                    || target.is_err()
                {
                    return Err(SourceError::new(
                        &path,
                        driver_error(
                            "E1011",
                            "module discovery does not follow source or directory symbolic links",
                        ),
                    ));
                }
            } else if kind.is_dir() {
                if !package_roots.contains(&path) {
                    pending.push((path, depth + 1));
                }
            } else if source_kind(&path).is_some() {
                if !kind.is_file() {
                    return Err(SourceError::new(
                        &path,
                        driver_error("E1011", "module sources must be regular files"),
                    ));
                }
                paths.push(path);
                if paths.len() > 4096 {
                    return Err(SourceError::new(
                        root,
                        driver_error("E1017", "module discovery exceeds 4096 source files"),
                    ));
                }
            }
        }
    }
    paths.sort_by_cached_key(|path| path.to_string_lossy().replace('\\', "/"));
    Ok(paths)
}

pub(crate) struct LoadedPackage {
    pub(crate) id: crate::package::PackageId,
    pub(crate) manifest: crate::package::Manifest,
    pub(crate) text: String,
    /// The content hash that `Tsuzuri.lock` records for a git or registry package.
    pub(crate) sha256: Option<String>,
    /// The selected version of a registry package.
    pub(crate) version: Option<crate::package::Version>,
}

impl LoadedPackage {
    /// How diagnostics name a fetched package.
    fn kind(&self) -> &'static str {
        if self.version.is_some() {
            "registry dependency"
        } else {
            "git dependency"
        }
    }
}

/// A git or registry dependency of the package graph. `tsuzuri fetch` downloads
/// it; every other command finds it through `Tsuzuri.lock` in the package store.
pub(crate) struct PackageRequest<'a> {
    pub(crate) name: &'a str,
    /// A `Git` or `Registry` source.
    pub(crate) source: &'a crate::package::DependencySource,
    /// The name of the declaring package, and its url when it is a git package.
    pub(crate) parent: &'a str,
    pub(crate) parent_url: Option<&'a str>,
    /// The declaring manifest and the dependency's line in it.
    pub(crate) manifest: &'a Path,
    pub(crate) span: Span,
}

/// Where a git or registry dependency is: its package root, content hash, and
/// for a registry package the selected version.
pub(crate) struct Resolution {
    pub(crate) root: PathBuf,
    pub(crate) sha256: String,
    pub(crate) version: Option<crate::package::Version>,
}

/// Resolves a git or registry dependency. `None` leaves the dependency out of the
/// walk; `tsuzuri fetch` does that for registry dependencies until it has selected
/// their versions.
pub(crate) type PackageResolver<'a> =
    dyn FnMut(&PackageRequest<'_>) -> Result<Option<Resolution>, SourceError> + 'a;

/// The default namespace of a root folder without a manifest: a kebab-case
/// folder name in PascalCase as for package names, another valid namespace
/// as written, and otherwise the global namespace.
fn folder_namespace(directory: &Path) -> String {
    let name = fs::canonicalize(directory)
        .ok()
        .and_then(|path| path.file_name().and_then(OsStr::to_str).map(str::to_owned))
        .unwrap_or_default();
    crate::package::namespace(&name, Span::default())
        .ok()
        .filter(|namespace| crate::package::valid_namespace(namespace))
        .or_else(|| crate::package::valid_namespace(&name).then(|| name.replace("::", ".")))
        .unwrap_or_default()
}

/// How the graph walk reached a package.
#[derive(Clone)]
enum Origin {
    /// The root package or a path dependency.
    Local,
    Git {
        url: String,
        sha256: String,
    },
    Registry {
        version: crate::package::Version,
        sha256: String,
    },
}

/// Walks the package graph of the manifest in `directory`. `resolve` turns each
/// git or registry dependency into a package root: `tsuzuri fetch` downloads it,
/// and the other commands look it up offline, so both share these rules.
pub(crate) fn load_packages(
    directory: &Path,
    resolve: &mut PackageResolver<'_>,
) -> Result<Vec<LoadedPackage>, SourceError> {
    use crate::package::DependencySource;
    use std::collections::{BTreeMap, BTreeSet};
    let manifest_path = directory.join("Tsuzuri.toml");
    match fs::symlink_metadata(&manifest_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(SourceError::new(
                &manifest_path,
                io_error("inspect manifest", &manifest_path, error),
            ));
        }
        Ok(_) => {}
    }
    let root = fs::canonicalize(directory).map_err(|error| {
        SourceError::new(
            directory,
            io_error("resolve package root", directory, error),
        )
    })?;
    // Each entry: the package root, the dependency that names it, whether the walk
    // leaves it, and how the walk reached it.
    let mut pending = vec![(root, None::<(String, PathBuf, Span)>, false, Origin::Local)];
    let mut active = BTreeSet::new();
    let mut loaded = BTreeMap::<PathBuf, LoadedPackage>::new();
    let mut namespaces = BTreeMap::new();
    // A package name has one source: paths (several roots may share a name if their
    // namespaces differ), one git url and rev, or the registry (one selected version).
    #[derive(PartialEq)]
    enum Source {
        Path,
        Git(String, String),
        Registry,
    }
    let mut sources = BTreeMap::<String, Source>::new();
    while let Some((root, expected, leaving, origin)) = pending.pop() {
        if leaving {
            active.remove(&root);
            continue;
        }
        let path = root.join("Tsuzuri.toml");
        if active.contains(&root) {
            return Err(SourceError::new(
                &path,
                driver_error("E1011", "cyclic package dependency"),
            ));
        }
        if !loaded.contains_key(&root) {
            if loaded.len() >= 1024 || active.len() >= 128 {
                return Err(SourceError::new(
                    &path,
                    driver_error("E1017", "package graph exceeds 1024 packages or depth 128"),
                ));
            }
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                SourceError::new(&path, io_error("inspect package manifest", &path, error))
            })?;
            if !metadata.is_file() || metadata.is_symlink() {
                return Err(SourceError::new(
                    &path,
                    driver_error(
                        "E1011",
                        "package manifests must be regular files, not symbolic links",
                    ),
                ));
            }
            let text = read_source_text(&path).map_err(|error| SourceError::new(&path, error))?;
            let manifest = crate::package::parse_manifest(&text, 0)
                .map_err(|error| SourceError::new(&path, error))?;
            if manifest.namespace.split('.').next().is_some_and(|first| {
                first == crate::stdlib::NAMESPACE
                    || crate::stdlib::is_reserved_module(first)
                    || first == "Task"
            }) {
                return Err(SourceError::new(
                    &path,
                    driver_error(
                        "E1011",
                        "package namespace is reserved by the standard library",
                    ),
                ));
            }
            if namespaces
                .insert(manifest.namespace.clone(), root.clone())
                .is_some()
            {
                return Err(SourceError::new(
                    &path,
                    driver_error(
                        "E1011",
                        "package name or namespace resolves to multiple roots",
                    ),
                ));
            }
            if let Origin::Registry { version, .. } = &origin
                && manifest.version != version.to_string()
            {
                return Err(SourceError::new(
                    &path,
                    driver_error(
                        "E2007",
                        format!(
                            "registry package '{}' declares version {} but Tsuzuri.lock records {version}; run tsuzuri fetch",
                            manifest.name, manifest.version
                        ),
                    ),
                ));
            }
            // The root package's name is taken by a path, so no fetched package may reuse it.
            if loaded.is_empty() {
                sources.insert(manifest.name.clone(), Source::Path);
            }
            active.insert(root.clone());
            pending.push((root.clone(), None, true, Origin::Local));
            let mut dependencies = Vec::new();
            for (name, dependency) in &manifest.dependencies {
                let source = match &dependency.source {
                    DependencySource::Path(_) => Source::Path,
                    DependencySource::Git { url, rev } => Source::Git(url.clone(), rev.clone()),
                    DependencySource::Registry(_) => Source::Registry,
                };
                let error = |code: &'static str, message: String| {
                    SourceError::new(&path, Diagnostic::new(code, message, dependency.span))
                };
                if sources
                    .get(name)
                    .is_some_and(|existing| *existing != source)
                {
                    return Err(error(
                        "E1011",
                        format!(
                            "package '{name}' is required from different sources; a package name has one source: paths, one git url and rev, or registry versions"
                        ),
                    ));
                }
                sources.insert(name.clone(), source);
                let (dependency_root, dependency_origin) = match (&dependency.source, &origin) {
                    (
                        DependencySource::Path(_) | DependencySource::Git { .. },
                        Origin::Registry { .. },
                    ) => {
                        return Err(error(
                            "E1011",
                            "registry packages can depend only on registry packages; use a version requirement".to_owned(),
                        ));
                    }
                    (DependencySource::Path(_), Origin::Git { .. }) => {
                        return Err(error(
                            "E1011",
                            "git packages cannot have path dependencies; use a git dependency"
                                .to_owned(),
                        ));
                    }
                    (DependencySource::Path(relative), Origin::Local) => (
                        dependency_directory(&root, relative, &path, dependency.span)?,
                        Origin::Local,
                    ),
                    (DependencySource::Git { .. } | DependencySource::Registry(_), _) => {
                        let Some(resolution) = resolve(&PackageRequest {
                            name,
                            source: &dependency.source,
                            parent: &manifest.name,
                            parent_url: match &origin {
                                Origin::Git { url, .. } => Some(url.as_str()),
                                _ => None,
                            },
                            manifest: &path,
                            span: dependency.span,
                        })?
                        else {
                            continue;
                        };
                        let package_root = fs::canonicalize(&resolution.root).map_err(|error| {
                            SourceError::new(
                                &path,
                                io_error("resolve fetched package root", &resolution.root, error),
                            )
                        })?;
                        let dependency_origin = match (&dependency.source, resolution.version) {
                            (DependencySource::Git { url, .. }, _) => Origin::Git {
                                url: url.clone(),
                                sha256: resolution.sha256,
                            },
                            (_, version) => Origin::Registry {
                                version: version.expect("registry resolutions have a version"),
                                sha256: resolution.sha256,
                            },
                        };
                        (package_root, dependency_origin)
                    }
                };
                dependencies.push((
                    dependency_root,
                    Some((name.clone(), path.clone(), dependency.span)),
                    false,
                    dependency_origin,
                ));
            }
            pending.extend(dependencies.into_iter().rev());
            let (sha256, version) = match origin {
                Origin::Local => (None, None),
                Origin::Git { sha256, .. } => (Some(sha256), None),
                Origin::Registry { version, sha256 } => (Some(sha256), Some(version)),
            };
            loaded.insert(
                root.clone(),
                LoadedPackage {
                    id: crate::package::PackageId {
                        name: manifest.name.clone(),
                        root: root.clone(),
                    },
                    manifest,
                    text,
                    sha256,
                    version,
                },
            );
        }
        if let Some((name, path, span)) = expected
            && loaded[&root].manifest.name != name
        {
            return Err(SourceError::new(
                &path,
                Diagnostic::new(
                    "E1011",
                    "dependency key must match the dependency package name",
                    span,
                ),
            ));
        }
    }
    let mut packages: Vec<_> = loaded.into_values().collect();
    packages.sort_by(|left, right| left.manifest.namespace.cmp(&right.manifest.namespace));
    Ok(packages)
}

/// The directory that a path dependency names, checked component by component so
/// that it never passes through a symbolic link.
fn dependency_directory(
    root: &Path,
    relative: &Path,
    manifest: &Path,
    span: Span,
) -> Result<PathBuf, SourceError> {
    let mut directory = root.to_owned();
    for component in relative.components() {
        match component {
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => {
                directory.pop();
            }
            std::path::Component::Normal(part) => directory.push(part),
            _ => {
                return Err(SourceError::new(
                    manifest,
                    Diagnostic::new("E1011", "dependency paths must be relative", span),
                ));
            }
        }
        let metadata = fs::symlink_metadata(&directory).map_err(|error| {
            SourceError::new(
                manifest,
                io_error("inspect dependency directory", &directory, error),
            )
        })?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(SourceError::new(
                manifest,
                Diagnostic::new(
                    "E1011",
                    "dependency paths must traverse real directories, not symbolic links",
                    span,
                ),
            ));
        }
    }
    Ok(directory)
}

/// The root package's `Tsuzuri.lock`.
pub(crate) struct Lockfile {
    pub(crate) path: PathBuf,
    pub(crate) text: String,
    pub(crate) entries: std::collections::BTreeMap<String, crate::package::LockEntry>,
}

/// Reads and validates `Tsuzuri.lock` next to the manifest in `directory`, if both exist.
pub(crate) fn read_lockfile(directory: &Path) -> Result<Option<Lockfile>, SourceError> {
    if fs::symlink_metadata(directory.join("Tsuzuri.toml")).is_err() {
        return Ok(None);
    }
    let path = directory.join("Tsuzuri.lock");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(SourceError::new(
                &path,
                io_error("inspect lockfile", &path, error),
            ));
        }
    };
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(SourceError::new(
            &path,
            driver_error(
                "E2007",
                "Tsuzuri.lock must be a regular file, not a symbolic link or directory",
            ),
        ));
    }
    if metadata.len() > crate::package::MAX_PACKAGE_FILE_BYTES as u64 {
        return Err(SourceError::new(
            &path,
            driver_error("E1017", "Tsuzuri.lock exceeds 1 MiB"),
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .and_then(|file| {
            file.take(crate::package::MAX_PACKAGE_FILE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| SourceError::new(&path, io_error("read lockfile", &path, error)))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        SourceError::new(
            &path,
            driver_error(
                "E2007",
                "Tsuzuri.lock is not a valid lockfile (it is not UTF-8); restore it from version control or delete it and run tsuzuri fetch",
            ),
        )
    })?;
    let entries =
        crate::package::parse_lock(&text, 0).map_err(|error| SourceError::new(&path, error))?;
    Ok(Some(Lockfile {
        path,
        text,
        entries,
    }))
}

/// Finds a git or registry dependency in `Tsuzuri.lock` and the package store
/// without running git or touching the network.
fn offline_package(
    lock: Option<&Lockfile>,
    request: &PackageRequest<'_>,
) -> Result<Option<Resolution>, SourceError> {
    use crate::package::DependencySource;
    let error = |message: String| {
        SourceError::new(
            request.manifest,
            Diagnostic::new("E2007", message, request.span),
        )
    };
    let lock = lock.ok_or_else(|| {
        error("Tsuzuri.lock is missing; run tsuzuri fetch to download git dependencies and record them".to_owned())
    })?;
    let (entry, kind) = match request.source {
        DependencySource::Git { url, rev } => (
            lock.entries
                .get(request.name)
                .filter(|entry| entry.git == *url && entry.rev == *rev && entry.version.is_none())
                .ok_or_else(|| {
                    error(format!(
                        "Tsuzuri.lock does not record git dependency '{}' at this url and rev; run tsuzuri fetch",
                        request.name
                    ))
                })?,
            "git dependency",
        ),
        DependencySource::Registry(requirement) => (
            lock.entries
                .get(request.name)
                .filter(|entry| {
                    entry
                        .version
                        .is_some_and(|version| version.satisfies(*requirement))
                })
                .ok_or_else(|| {
                    error(format!(
                        "Tsuzuri.lock does not record a version of '{}' that satisfies {requirement}; run tsuzuri fetch",
                        request.name
                    ))
                })?,
            "registry dependency",
        ),
        DependencySource::Path(_) => unreachable!("the walk resolves path dependencies itself"),
    };
    let store = crate::cache::package_store().ok_or_else(|| {
        error(
            "no package store is available; set TSUZURI_CACHE_DIR and run tsuzuri fetch".to_owned(),
        )
    })?;
    // Check for a symbolic link before the walk canonicalizes the root.
    let root = store.join("git").join(&entry.sha256);
    if !fs::symlink_metadata(&root)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
    {
        return Err(error(format!(
            "{kind} '{}' is not downloaded; run tsuzuri fetch",
            request.name
        )));
    }
    Ok(Some(Resolution {
        root,
        sha256: entry.sha256.clone(),
        version: entry.version,
    }))
}

impl Project {
    pub fn load_with_overlays(
        input: &Path,
        overlays: &std::collections::BTreeMap<PathBuf, String>,
    ) -> Result<Self, SourceError> {
        let directory = if input.is_dir() {
            input
        } else {
            input
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
        };
        let directory = fs::canonicalize(directory).map_err(|error| {
            SourceError::new(
                directory,
                io_error("resolve module directory", directory, error),
            )
        })?;
        let mut normalized = std::collections::BTreeMap::new();
        for (path, text) in overlays {
            if source_kind(path).is_some()
                && let Some(parent) = path
                    .parent()
                    .and_then(|parent| fs::canonicalize(parent).ok())
            {
                if text.len() > MAX_SOURCE_BYTES {
                    return Err(SourceError::new(
                        path,
                        driver_error(
                            "E0003",
                            format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit"),
                        ),
                    ));
                }
                let path = parent.join(path.file_name().ok_or_else(|| {
                    SourceError::new(
                        path,
                        driver_error("E1011", "a source file needs a filename"),
                    )
                })?);
                if path.strip_prefix(&directory).is_ok_and(|relative| {
                    relative
                        .components()
                        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
                }) {
                    continue;
                }
                normalized.insert(path, text.clone());
            }
        }
        // The editor offers every std module, so it loads the opt-in ones too (D-40).
        Self::load_from_root(&directory, None, &normalized, true)
    }

    /// Loads the package graph at `directory`. With `all_std` false, it loads only the
    /// opt-in std modules that the user sources name (`stdlib::sources_for`).
    fn load_from_root(
        directory: &Path,
        selected: Option<&Path>,
        overlays: &std::collections::BTreeMap<PathBuf, String>,
        all_std: bool,
    ) -> Result<Self, SourceError> {
        let lock = read_lockfile(directory)?;
        let packages = load_packages(directory, &mut |request| {
            offline_package(lock.as_ref(), request)
        })?;
        let canonical;
        let directory = if packages.is_empty() {
            directory
        } else {
            canonical = fs::canonicalize(directory).map_err(|error| {
                SourceError::new(
                    directory,
                    io_error("resolve project root", directory, error),
                )
            })?;
            &canonical
        };
        let mut roots: Vec<_> = packages
            .iter()
            .map(|package| package.id.root.clone())
            .collect();
        if roots.is_empty() {
            roots.push(directory.to_owned());
        }
        let root_namespace = packages
            .iter()
            .find(|package| package.id.root == directory)
            .map_or_else(
                || folder_namespace(directory),
                |package| package.manifest.namespace.clone(),
            );
        let mut sources = Vec::new();
        // The sources of each git package by `/`-separated path, for its content hash.
        let mut fetched =
            std::collections::BTreeMap::<&Path, std::collections::BTreeMap<String, usize>>::new();
        for root in &roots {
            let package = packages.iter().find(|package| package.id.root == *root);
            let mut paths: std::collections::BTreeMap<_, _> = collect_sources(root, &roots)?
                .into_iter()
                .map(|path| (path, None))
                .collect();
            for (path, text) in overlays {
                if path.starts_with(root)
                    && !roots.iter().any(|other| {
                        other != root && other.starts_with(root) && path.starts_with(other)
                    })
                {
                    paths.insert(path.clone(), Some(text));
                }
            }
            for (path, overlay) in paths {
                let relative = path.strip_prefix(root).map_err(|_| {
                    SourceError::new(
                        &path,
                        driver_error("E1011", "source is outside its package root"),
                    )
                })?;
                if relative
                    .components()
                    .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                let hash_key = package
                    .filter(|package| package.sha256.is_some())
                    .map(|package| {
                        let key = relative
                            .components()
                            .map(|part| part.as_os_str().to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("/");
                        (package, key)
                    });
                let relative_path = if root != directory {
                    package
                        .unwrap()
                        .manifest
                        .namespace
                        .split('.')
                        .collect::<PathBuf>()
                        .join(relative)
                } else {
                    let module = crate::module_name_from_relative(relative).unwrap_or_default();
                    if packages.iter().any(|package| {
                        let namespace = &package.manifest.namespace;
                        package.id.root != directory
                            && module
                                .strip_prefix(namespace.as_str())
                                .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
                    }) {
                        return Err(SourceError::new(
                            &path,
                            driver_error(
                                "E1011",
                                "root module collides with a dependency namespace",
                            ),
                        ));
                    }
                    relative.to_owned()
                };
                let name = crate::module_name_from_relative(&relative_path)
                    .map_err(|error| SourceError::new(&path, error))?;
                let mut source = if let Some(text) = overlay {
                    SourceFile {
                        path,
                        relative_path: relative_path.clone(),
                        name: name.clone(),
                        text: text.clone(),
                        origin: ModuleOrigin::User,
                        package: None,
                        namespace: String::new(),
                    }
                } else {
                    SourceFile::read(&path)?
                };
                source.relative_path = relative_path;
                source.name = name;
                source.package = package.map(|package| package.id.clone());
                if root == directory {
                    source.namespace = root_namespace.clone();
                }
                if let Some((package, key)) = hash_key {
                    fetched
                        .entry(&package.id.root)
                        .or_default()
                        .insert(key, sources.len());
                }
                sources.push(source);
                if sources.len() > 4096 {
                    return Err(SourceError::new(
                        directory,
                        driver_error("E1017", "package graph exceeds 4096 source files"),
                    ));
                }
            }
        }
        // A git package must still have the content that `Tsuzuri.lock` records.
        for package in &packages {
            let Some(sha256) = &package.sha256 else {
                continue;
            };
            let mut files: std::collections::BTreeMap<String, &[u8]> = fetched
                .get(package.id.root.as_path())
                .into_iter()
                .flatten()
                .map(|(path, index)| (path.clone(), sources[*index].text.as_bytes()))
                .collect();
            files.insert("Tsuzuri.toml".to_owned(), package.text.as_bytes());
            if crate::package::content_sha256(&files) != *sha256 {
                let lock = lock
                    .as_ref()
                    .map_or_else(|| directory.join("Tsuzuri.lock"), |lock| lock.path.clone());
                return Err(SourceError::new(
                    &lock,
                    driver_error(
                        "E2007",
                        format!(
                            "{} '{}' does not match its sha256 in Tsuzuri.lock; run tsuzuri fetch to restore it",
                            package.kind(),
                            package.manifest.name
                        ),
                    ),
                ));
            }
        }
        sources
            .sort_by_cached_key(|source| source.relative_path.to_string_lossy().replace('\\', "/"));
        let root = if let Some(selected) = selected {
            sources.iter().position(|source| source.relative_path == selected).ok_or_else(|| SourceError::new(&directory.join(selected), driver_error("E1011", "source filename must match its directory entry exactly, including case")))?
        } else {
            sources
                .iter()
                .position(|source| source.name == "Main")
                .unwrap_or(0)
        };
        let std_sources = if all_std {
            crate::stdlib::SOURCES.to_vec()
        } else {
            crate::stdlib::sources_for(sources.iter().map(|source| source.text.as_str()))
        };
        sources.extend(std_sources.iter().map(|(path, text)| {
            SourceFile {
                path: PathBuf::from(path),
                relative_path: PathBuf::from(path),
                name: crate::stdlib::module_name(path)
                    .expect("embedded std paths are flat")
                    .to_owned(),
                text: (*text).to_owned(),
                origin: ModuleOrigin::Std,
                package: None,
                namespace: String::new(),
            }
        }));
        let wasm = packages
            .iter()
            .find(|package| package.id.root == directory)
            .map_or_else(Default::default, |package| package.manifest.wasm);
        // A dependency links host code only where the root package names it with `native = true`.
        let trusted: BTreeSet<PathBuf> = packages
            .iter()
            .find(|package| package.id.root == directory)
            .into_iter()
            .flat_map(|root| root.manifest.dependencies.values())
            .filter(|dependency| dependency.native)
            .filter_map(|dependency| match &dependency.source {
                crate::package::DependencySource::Path(path) => {
                    fs::canonicalize(directory.join(path)).ok()
                }
                crate::package::DependencySource::Git { .. }
                | crate::package::DependencySource::Registry(_) => None,
            })
            .collect();
        let mut native = LinkInputs::default();
        // The root's inputs come first, then each trusted dependency's in package order.
        let mut ordered: Vec<_> = packages.iter().collect();
        ordered.sort_by_key(|package| package.id.root != directory);
        for package in ordered {
            let Some((inputs, span)) = &package.manifest.native else {
                continue;
            };
            if package.sha256.is_some() {
                return Err(SourceError::new(
                    &package.id.root.join("Tsuzuri.toml"),
                    Diagnostic::new(
                        "E2000",
                        "git and registry packages cannot declare [native] link settings; move them to the application manifest",
                        *span,
                    ),
                ));
            }
            if package.id.root != directory && !trusted.contains(&package.id.root) {
                return Err(SourceError::new(
                    &package.id.root.join("Tsuzuri.toml"),
                    Diagnostic::new(
                        "E2000",
                        "only the root package and dependencies it marks with native = true may declare [native] link settings; move them to the application manifest or mark the dependency",
                        *span,
                    ),
                ));
            }
            // The canonical root is verbatim (`\\?\`) on Windows; the linker should see `C:\...`.
            let base = crate::cache::real_path(&package.id.root)
                .unwrap_or_else(|_| package.id.root.clone());
            let resolve = |paths: &[PathBuf]| -> Vec<PathBuf> {
                paths.iter().map(|path| base.join(path)).collect()
            };
            native = native.followed_by(&LinkInputs {
                paths: resolve(&inputs.paths),
                libraries: inputs.libraries.clone(),
                search: resolve(&inputs.search),
            });
        }
        let root_id = packages
            .iter()
            .find(|package| package.id.root == directory)
            .map(|package| package.id.clone());
        let mut manifests: Vec<_> = packages
            .into_iter()
            .map(|package| SourceFile {
                path: package.id.root.join("Tsuzuri.toml"),
                relative_path: PathBuf::from("Tsuzuri.toml"),
                name: package.manifest.namespace,
                text: package.text,
                origin: ModuleOrigin::User,
                package: Some(package.id),
                namespace: String::new(),
            })
            .collect();
        // The lockfile, too, is protected from outputs and keys the build cache.
        if let Some(lock) = lock {
            manifests.push(SourceFile {
                path: directory.join("Tsuzuri.lock"),
                relative_path: PathBuf::from("Tsuzuri.lock"),
                name: "Tsuzuri.lock".to_owned(),
                text: lock.text,
                origin: ModuleOrigin::User,
                package: root_id,
                namespace: String::new(),
            });
        }
        Ok(Self {
            sources,
            manifests,
            root,
            wasm,
            native,
        })
    }

    pub fn load_for_tests(input: &Path) -> Result<Self, SourceError> {
        Self::load_without_entry(input)
    }

    pub fn load_for_docs(input: &Path) -> Result<Self, SourceError> {
        let mut project = Self::load_without_entry(input)?;
        let directory = if input.is_dir() {
            input
        } else {
            input.parent().unwrap_or(Path::new("."))
        };
        let user_sources: Vec<_> = project
            .sources
            .iter()
            .filter(|source| source.origin == ModuleOrigin::User)
            .collect();
        let standard_library = directory.file_name() == Some(OsStr::new("std"))
            && user_sources.len() == crate::stdlib::SOURCES.len()
            && user_sources.iter().all(|source| {
                crate::stdlib::SOURCES.iter().any(|(path, _)| {
                    Path::new(path)
                        .strip_prefix("std")
                        .is_ok_and(|path| path == source.relative_path)
                })
            });
        if standard_library {
            project
                .sources
                .retain(|source| source.origin == ModuleOrigin::User);
            for source in &mut project.sources {
                source.origin = ModuleOrigin::Std;
                source.relative_path = Path::new("std").join(&source.relative_path);
                source.namespace.clear();
            }
        }
        Ok(project)
    }

    fn load_without_entry(input: &Path) -> Result<Self, SourceError> {
        let metadata = fs::symlink_metadata(input)
            .map_err(|error| SourceError::new(input, io_error("inspect input", input, error)))?;
        if !metadata.is_dir() {
            return Self::load(input);
        }
        let project = Self::load_from_root(input, None, &std::collections::BTreeMap::new(), false)?;
        if !project
            .sources
            .iter()
            .any(|source| source.origin == ModuleOrigin::User)
        {
            return Err(SourceError::new(
                input,
                driver_error("E2000", "input directory has no .tz, .tt, or .tc sources"),
            ));
        }
        Ok(project)
    }

    pub fn load(input: &Path) -> Result<Self, SourceError> {
        let metadata = fs::symlink_metadata(input)
            .map_err(|error| SourceError::new(input, io_error("inspect source", input, error)))?;
        if metadata.is_symlink() {
            return Err(SourceError::new(
                input,
                driver_error("E1011", "source inputs must not be symbolic links"),
            ));
        }
        let input = if metadata.is_dir() {
            input.join("Main.tz")
        } else {
            input.to_owned()
        };
        read_source(&input).map_err(|error| SourceError::new(&input, error))?;
        let parent = input
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Self::load_from_root(
            parent,
            input.file_name().map(Path::new),
            &std::collections::BTreeMap::new(),
            false,
        )
    }

    /// The project of `tsuzuri script`: the file at `path`, whatever its name, as the
    /// application's `Main.tz` and nothing else (see [`Project::single_main`]). A symbolic
    /// link is followed, since a script on PATH often is one.
    pub fn load_script(path: &Path) -> Result<Self, SourceError> {
        if matches!(
            source_kind(path),
            Some(SourceKind::TypeClass | SourceKind::Computation)
        ) {
            return Err(SourceError::new(
                path,
                driver_error(
                    "E2000",
                    "a script is a .tz program; .tt and .tc files are type class and builder modules",
                ),
            ));
        }
        let text = read_source_text(path).map_err(|error| SourceError::new(path, error))?;
        Ok(Self::single_main(path.to_owned(), text))
    }

    /// A project whose only user source is `text`, the application's `Main.tz`, shown at
    /// `path`. It reads no file, manifest, or lockfile, so the program sees no other module
    /// and no dependency; like [`Project::load`], it adds only the std modules that `text`
    /// names (D-40). The REPL passes a generated program and `tsuzuri script` a file.
    pub fn single_main(path: PathBuf, text: String) -> Self {
        let std_sources = crate::stdlib::sources_for([text.as_str()]);
        let mut sources = vec![SourceFile {
            path,
            relative_path: PathBuf::from("Main.tz"),
            name: "Main".to_owned(),
            text,
            origin: ModuleOrigin::User,
            package: None,
            namespace: String::new(),
        }];
        sources.extend(std_sources.iter().map(|(path, text)| {
            SourceFile {
                path: PathBuf::from(path),
                relative_path: PathBuf::from(path),
                name: crate::stdlib::module_name(path)
                    .expect("embedded std paths are flat")
                    .to_owned(),
                text: (*text).to_owned(),
                origin: ModuleOrigin::Std,
                package: None,
                namespace: String::new(),
            }
        }));
        Self {
            sources,
            manifests: Vec::new(),
            root: 0,
            wasm: Default::default(),
            native: LinkInputs::default(),
        }
    }

    pub fn input(&self) -> &Path {
        &self.sources[self.root].path
    }

    fn with_trap_sources<Output>(
        &self,
        operation: impl FnOnce(&[crate::trap::TrapSource<'_>]) -> Output,
    ) -> Output {
        let paths: Vec<_> = self
            .sources
            .iter()
            .map(|source| source.path.to_string_lossy())
            .collect();
        let sources: Vec<_> = paths
            .iter()
            .zip(&self.sources)
            .map(|(path, source)| crate::trap::TrapSource {
                path,
                text: &source.text,
            })
            .collect();
        operation(&sources)
    }

    pub fn source_for(&self, error: &Diagnostic) -> &SourceFile {
        let index = error.span.source.unwrap_or(self.root);
        self.sources
            .get(index)
            .unwrap_or_else(|| &self.manifests[index - self.sources.len()])
    }

    pub fn analyze(&self) -> Result<CheckedModule, Diagnostic> {
        self.analyze_all().map_err(crate::first_error)
    }

    pub fn analyze_all(&self) -> Result<CheckedModule, crate::diagnostic::DiagnosticSet> {
        crate::analyze_inputs_all(&self.inputs())
    }

    /// Analyzes the project like `analyze_all`, reusing parsed sources from the frontend cache
    /// under `cache::default_root()` when `cache` is set and the cache opens (G17). The result
    /// and the diagnostics do not depend on the cache.
    pub fn analyze_cached(
        &self,
        cache: bool,
        kind: AnalysisKind,
    ) -> Result<CheckedModule, crate::diagnostic::DiagnosticSet> {
        let inputs = self.inputs();
        let frontend = cache
            .then(|| {
                let project =
                    crate::frontend_cache::ProjectKey::new(&self.root_directory()?, kind.name())?;
                crate::frontend_cache::FrontendCache::open(&crate::cache::default_root()?, project)
            })
            .flatten();
        match frontend {
            Some(mut cache) => crate::analyze_inputs_with(&inputs, None, Some(&mut cache)),
            None => crate::analyze_inputs_all(&inputs),
        }
    }

    fn inputs(&self) -> Vec<crate::SourceInput<'_>> {
        self.sources
            .iter()
            .map(|source| crate::SourceInput {
                path: source.relative_path.to_str().unwrap(),
                text: &source.text,
                origin: source.origin,
                namespace: &source.namespace,
            })
            .collect()
    }

    /// The directory that the root source's relative path starts from.
    fn root_directory(&self) -> Option<PathBuf> {
        let source = self.sources.get(self.root)?;
        let directory = source
            .path
            .ancestors()
            .nth(source.relative_path.components().count())?;
        Some(if directory.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            directory.to_owned()
        })
    }
}

/// What a command analyzes a project for; each has its own frontend cache manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisKind {
    /// `Project::load`: check, build and run.
    Program,
    /// `Project::load_for_tests`: test.
    Tests,
    /// `Project::load_for_docs`: doc.
    Docs,
}

impl AnalysisKind {
    fn name(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Tests => "tests",
            Self::Docs => "docs",
        }
    }
}

pub fn document(project: &Project, output: &Path) -> Result<Vec<String>, Diagnostic> {
    let pages = crate::docgen::render_project(project)?;
    let name = output
        .file_name()
        .ok_or_else(|| driver_error("E2003", "documentation output needs a directory name"))?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create documentation parent", parent, error))?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| io_error("resolve documentation parent", parent, error))?;
    let output = parent.join(name);
    let previous = protect_documentation(project, &output)?;
    let mut temporary = TemporaryDirectory::new(&parent)?;
    let staged = temporary.path.join("next");
    fs::create_dir(&staged)
        .map_err(|error| io_error("create documentation stage", &staged, error))?;
    for (name, text) in pages {
        let path = staged.join(name);
        fs::write(&path, text).map_err(|error| io_error("write documentation", &path, error))?;
    }
    let marker = staged.join(".tsuzuri-docs");
    fs::write(&marker, "generated by tsuzuri doc\n")
        .map_err(|error| io_error("write documentation marker", &marker, error))?;
    if protect_documentation(project, &output)? != previous {
        return Err(driver_error(
            "E2003",
            "documentation output changed during generation",
        ));
    }
    let backup = temporary.path.join("previous");
    if previous {
        fs::rename(&output, &backup)
            .map_err(|error| io_error("back up documentation", &output, error))?;
    }
    if let Err(error) = fs::rename(&staged, &output) {
        if previous && let Err(restore) = fs::rename(&backup, &output) {
            temporary.removed = true;
            return Err(driver_error(
                "E2003",
                format!(
                    "documentation publication failed ({error}) and restore failed ({restore}); recovery files remain in '{}'",
                    temporary.path.display()
                ),
            ));
        }
        return Err(io_error("publish documentation", &output, error));
    }
    temporary.close()?;
    Ok(Vec::new())
}

fn protect_documentation(project: &Project, output: &Path) -> Result<bool, Diagnostic> {
    let metadata = match fs::symlink_metadata(output) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io_error("inspect documentation output", output, error)),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(driver_error(
            "E2003",
            "documentation output must be a regular directory, not a symlink",
        ));
    }
    let marker = output.join(".tsuzuri-docs");
    let valid_marker = fs::symlink_metadata(&marker).is_ok_and(|metadata| {
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 4096
    });
    if !valid_marker {
        return Err(driver_error(
            "E2003",
            "existing documentation output needs a regular .tsuzuri-docs marker",
        ));
    }
    let mut text = String::new();
    fs::File::open(&marker)
        .and_then(|file| file.take(4097).read_to_string(&mut text))
        .map_err(|error| io_error("read documentation marker", &marker, error))?;
    if text.len() > 4096 || !text.contains("generated by tsuzuri doc") {
        return Err(driver_error(
            "E2003",
            "documentation marker does not identify tsuzuri doc output",
        ));
    }
    let destination = fs::canonicalize(output)
        .map_err(|error| io_error("resolve documentation output", output, error))?;
    if project
        .sources
        .iter()
        .chain(&project.manifests)
        .any(|source| {
            fs::canonicalize(&source.path).is_ok_and(|source| source.starts_with(&destination))
        })
    {
        return Err(driver_error(
            "E2003",
            "documentation output must not contain project sources",
        ));
    }
    Ok(true)
}

pub fn read_source(path: &Path) -> Result<String, Diagnostic> {
    if source_kind(path).is_none() {
        return Err(driver_error(
            "E2000",
            "input files must use '.tz' (code), '.tt' (type classes), or '.tc' (computation builder); rename old '.tzr' code files to '.tz'",
        ));
    }
    read_source_text(path)
}

pub(crate) fn read_source_text(path: &Path) -> Result<String, Diagnostic> {
    let metadata = fs::metadata(path).map_err(|error| io_error("inspect source", path, error))?;
    if !metadata.is_file() {
        return Err(driver_error("E2001", "the source must be a regular file"));
    }
    let file = fs::File::open(path).map_err(|error| io_error("read source", path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| io_error("inspect source", path, error))?;
    if !metadata.is_file() {
        return Err(driver_error("E2001", "the source must be a regular file"));
    }
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err(driver_error(
            "E0003",
            format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit"),
        ));
    }
    let mut source = String::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_string(&mut source)
        .map_err(|error| io_error("read UTF-8 source", path, error))?;
    Ok(source)
}

pub fn format_sources(input: &Path, check_only: bool) -> Result<Vec<PathBuf>, SourceError> {
    let metadata = fs::symlink_metadata(input).map_err(|error| {
        SourceError::new(input, io_error("inspect formatting input", input, error))
    })?;
    let mut paths = if metadata.is_dir() {
        fs::read_dir(input)
            .map_err(|error| {
                SourceError::new(input, io_error("list formatting input", input, error))
            })?
            .map(|entry| {
                entry.map(|entry| entry.path()).map_err(|error| {
                    SourceError::new(input, io_error("list formatting input", input, error))
                })
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|path| source_kind(path).is_some())
            .collect::<Vec<_>>()
    } else {
        vec![input.to_owned()]
    };
    paths.sort();
    let mut updates = Vec::new();
    for path in paths {
        let update = (|| {
            require_regular_source(&path)?;
            let kind = source_kind(&path)
                .ok_or_else(|| driver_error("E2000", "fmt accepts .tz, .tt, or .tc files"))?;
            let source = read_source(&path)?;
            let result = crate::formatter::format_source(&path.to_string_lossy(), &source, kind)?;
            Ok::<_, Diagnostic>((source, result))
        })()
        .map_err(|error| SourceError::new(&path, error))?;
        if update.1.changed {
            updates.push((path, update.0, update.1.formatted));
        }
    }
    if !check_only {
        for (path, original, formatted) in &updates {
            replace_source_atomically(path, original, formatted)
                .map_err(|error| SourceError::new(path, error))?;
        }
    }
    Ok(updates.into_iter().map(|(path, _, _)| path).collect())
}

fn require_regular_source(path: &Path) -> Result<fs::Metadata, Diagnostic> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("inspect source", path, error))?;
    if !metadata.file_type().is_file() {
        return Err(driver_error(
            "E2003",
            "fmt refuses symlinks, directories, and special files",
        ));
    }
    Ok(metadata)
}

fn replace_source_atomically(
    path: &Path,
    original: &str,
    formatted: &str,
) -> Result<(), Diagnostic> {
    let metadata = require_regular_source(path)?;
    if read_source(path)? != original {
        return Err(driver_error(
            "E2003",
            "source changed during formatting; no replacement made",
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = TemporaryDirectory::new(parent)?;
    let staged = temporary.path.join("formatted");
    let mut file = fs::File::create_new(&staged)
        .map_err(|error| io_error("create formatted source", &staged, error))?;
    file.write_all(formatted.as_bytes())
        .map_err(|error| io_error("write formatted source", &staged, error))?;
    file.set_permissions(metadata.permissions())
        .map_err(|error| io_error("preserve source permissions", &staged, error))?;
    file.sync_all()
        .map_err(|error| io_error("flush formatted source", &staged, error))?;
    drop(file);
    require_regular_source(path)?;
    if read_source(path)? != original {
        return Err(driver_error(
            "E2003",
            "source changed during formatting; no replacement made",
        ));
    }
    fs::rename(&staged, path).map_err(|error| io_error("replace source", path, error))?;
    Ok(())
}

pub(crate) fn source_kind(path: &Path) -> Option<SourceKind> {
    path.extension()
        .and_then(OsStr::to_str)
        .and_then(SourceKind::from_extension)
}

pub fn build(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
) -> Result<Vec<String>, Diagnostic> {
    build_linked(module, project, output, options, &LinkInputs::default())
}

/// Like [`build`], linking the host `links` into a native executable.
pub fn build_linked(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
    links: &LinkInputs,
) -> Result<Vec<String>, Diagnostic> {
    build_complete(module, project, output, options, links, "build").map(|(messages, _)| messages)
}

fn build_complete(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
    links: &LinkInputs,
    action: &str,
) -> Result<(Vec<String>, Vec<crate::trap::TrapSite>), Diagnostic> {
    options.validate()?;
    if !links.is_empty() {
        if options.target != Target::Native
            || !matches!(options.emit, Emit::Executable | Emit::Shared)
        {
            return Err(driver_error(
                "E2000",
                "link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe",
            ));
        }
        links.check_shape()?;
        links.check_readable()?;
    }
    // The executor of `Async.start` and `Async.block_on` keeps its state across calls (B08), so the
    // blocks that a trapped call allocated and the trap boundary frees could still be reachable from it.
    if options.trap_return && !llvm::host_entries(module).is_empty() {
        return Err(driver_error(
            "E2000",
            "--trap-mode return cannot be combined with Async.start or Async.block_on: a trap inside the executor would leave its state half updated",
        ));
    }
    // Each thread has its own executor in thread-local storage, which the workers of WebAssembly
    // threads do not set up. Their glue would also call exports from several workers (B08).
    if options.wasm_threads && !llvm::host_entries(module).is_empty() {
        return Err(driver_error(
            "E2000",
            "--wasm-feature threads cannot be combined with Async.start or Async.block_on: their executor keeps per-thread state that WebAssembly threads do not set up",
        ));
    }
    if options.target.is_wasm() && !options.wasm_jspi && llvm::reaches_reactor(module) {
        return Err(driver_error(
            "E2000",
            "Async.block_on on WebAssembly needs --wasm-feature jspi: it suspends the WebAssembly stack with JavaScript Promise Integration while it waits",
        ));
    }
    if options.emit.is_bindings() {
        return build_bindings(module, project, output, options).map(|()| (Vec::new(), Vec::new()));
    }
    let (max_memory, stack_size) = wasm_memory_limits(
        options.target,
        options.wasm_max_memory,
        options.wasm_stack_size,
    )?;
    // The entry module, which `tsuzuri script` reads from a file of any name.
    if options.emit == Emit::Executable
        && project.sources[project.root].relative_path != Path::new("Main.tz")
    {
        return Err(driver_error(
            "E2004",
            "an application must start from Main.tz; pass Main.tz or its directory, or use '--emit object' for a library",
        ));
    }
    if options.emit == Emit::Wasm
        && !llvm::io_entry(module)
        && !llvm::main_entry(module)
        && !module.functions.iter().any(|function| function.exported)
    {
        return Err(driver_error(
            "E2004",
            "a WebAssembly module needs 'def main', top-level IO<T> entry-point code, or at least one 'export def' entry point",
        ));
    }
    if options.emit == Emit::Shared && !module.functions.iter().any(|function| function.exported) {
        return Err(driver_error(
            "E2004",
            "a shared library needs at least one 'export def' entry point",
        ));
    }
    let mut trap_sites = Vec::new();
    let stack_checks = options.emit != Emit::Header
        && wasm_stack_checks(options.target, options.wasm_threads, max_memory);
    let mut text = if options.emit == Emit::Header {
        if options.trap_return {
            llvm::header_with(module, true)
        } else {
            llvm::header_with_allocator(module, options.allocator)
        }
    } else if options.emit == Emit::Wgsl {
        let exports: Vec<_> = module
            .functions
            .iter()
            .enumerate()
            .filter(|(_, function)| function.exported)
            .map(|(id, _)| id)
            .collect();
        if exports.len() != 1 {
            return Err(driver_error(
                "E2004",
                "WGSL output requires exactly one exported scalar kernel; use a dedicated source project",
            ));
        }
        crate::gpu::extract_kernel(module, exports[0])?.wgsl()?
    } else {
        let emission = llvm::EmitOptions {
            entry: if options.emit == Emit::Executable {
                Entry::Console
            } else {
                Entry::Library
            },
            wasm: options.target.is_wasm(),
            debug_output: options.debug_output,
            allocator: options.allocator,
        };
        if options.trap_return {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_trap_return(
                    module,
                    emission,
                    sources,
                    options.debug_info.then_some(options.optimization != 0),
                    matches!(options.emit, Emit::Object | Emit::Shared),
                )
            })?;
            if output.ir.contains("@tz.callback.") {
                return Err(driver_error(
                    "E2000",
                    "--trap-mode return cannot be combined with extern callbacks: a trap would unwind through the host's frames",
                ));
            }
            trap_sites = output.trap_sites;
            output.ir
        } else if options.target == Target::Native
            && matches!(options.emit, Emit::Executable | Emit::Object | Emit::Shared)
            // A freestanding object has no CPU dispatch: its runtime reads the CPU through the C library.
            && !options.freestanding
        {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_native_build(
                    module,
                    emission,
                    sources,
                    options.debug_info.then_some(options.optimization != 0),
                    options.trap_info,
                )
            })?;
            trap_sites = output.trap_sites;
            output.ir
        } else if options.wasm_threads || options.target == Target::Wasm64 || stack_checks {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_wasm_build(
                    module,
                    emission,
                    sources,
                    options.debug_info.then_some(options.optimization != 0),
                    options.trap_info,
                    options.wasm_threads,
                    options.target == Target::Wasm64,
                    stack_checks,
                )
            })?;
            trap_sites = output.trap_sites;
            output.ir
        } else if options.debug_info {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_with_debug_info(
                    module,
                    emission,
                    sources,
                    options.optimization != 0,
                    options.trap_info,
                )
            })?;
            trap_sites = output.trap_sites;
            output.ir
        } else if options.trap_info {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_with_trap_info(module, emission, sources)
            })?;
            trap_sites = output.trap_sites;
            output.ir
        } else {
            llvm::emit_with_options(module, emission)?
        }
    };
    if cfg!(windows)
        && options.target == Target::Native
        && !matches!(options.emit, Emit::Header | Emit::Wgsl)
    {
        text = llvm::windows_abi(text, module);
        if options.debug_info && msvc_linker() {
            text = llvm::with_codeview(text);
        }
    }
    if options.target.is_wasm() && options.emit != Emit::Header {
        text = llvm::with_wasm_heap_limit(text, max_memory);
        if options.wasm_host == Some(WasmHost::Wasi) {
            text = llvm::with_wasi_host(&text);
        }
        if options.wasm_simd {
            text.insert_str(
                0,
                "; wasm-feature: simd128; compile this IR with -msimd128\n",
            );
        }
        text.push_str(include_str!("runtime/wasm.ll"));
    }
    let task_runtime =
        options.target == Target::Native && text.contains("declare void @tsuzuri_task_parallel(");
    // A kernel call or a multiversioned function's stub declares a `tsuzuri_cpu_` function (F08).
    let cpu_runtime = options.target == Target::Native
        && text
            .lines()
            .any(|line| line.starts_with("declare ") && line.contains(" @tsuzuri_cpu_"));
    let io_runtime = text.contains("declare i32 @tsuzuri_io_");
    let os_runtime = text.contains("declare i64 @tsuzuri_os_");
    // `Async.block_on` waits in src/runtime/async.c natively and through JSPI imports on WASM (B08).
    let async_reactor = llvm::uses_reactor(&text);
    // A `def main :: Array<string> -> i32` reads its arguments in src/runtime/arguments.c.
    let arguments_runtime = text.contains("@tsuzuri_arguments(");
    if options.freestanding {
        // A header holds no IR, so it is checked against the IR the object of the same build holds.
        let library;
        let ir = if options.emit == Emit::Header {
            library = llvm::emit_with_options(
                module,
                llvm::EmitOptions {
                    entry: Entry::Library,
                    wasm: false,
                    debug_output: options.debug_output,
                    allocator: options.allocator,
                },
            )?;
            library.as_str()
        } else {
            text.as_str()
        };
        if let Some((_, feature)) = [
            ("declare void @tsuzuri_task_parallel(", "parallel tasks"),
            ("declare i32 @tsuzuri_io_", "the standard IO"),
            ("declare i64 @tsuzuri_os_", "the operating-system APIs"),
            ("declare void @tsuzuri_async_wait(", "Async.block_on"),
            ("@tsuzuri_arguments(", "program arguments"),
            ("declare i64 @write(", "Debug output"),
            ("declare i32 @putchar(", "Debug output"),
        ]
        .into_iter()
        .find(|(marker, _)| ir.contains(marker))
        {
            return Err(driver_error(
                "E2000",
                format!(
                    "--freestanding cannot use {feature}: its runtime needs the C library; keep it in the host and pass the results to the exports"
                ),
            ));
        }
    }
    // Only a WASM module of a `def main` or an IO entry is a WASI command; objects leave `_start` to the embedder.
    let wasi_command =
        options.emit == Emit::Wasm && (llvm::io_entry(module) || llvm::main_entry(module));
    // With `--wasm-host wasi` the standard IO, the OS APIs, and a command's `_start` come from
    // src/runtime/os-wasi.c.
    let wasi_runtime = options.wasm_host == Some(WasmHost::Wasi)
        && (io_runtime || os_runtime || arguments_runtime || wasi_command);
    // Only objects embed it: a host that links the LLVM output provides src/runtime/trap.c itself.
    let trap_runtime = options.trap_return
        && matches!(options.emit, Emit::Object | Emit::Shared)
        && [
            "@tsuzuri_trap_raise(",
            "@tsuzuri_boundary_run(",
            "@tsuzuri_tracked_",
        ]
        .iter()
        .any(|symbol| text.contains(symbol));
    let native_runtime = task_runtime
        || cpu_runtime
        || trap_runtime
        || (options.target == Target::Native
            && (io_runtime || os_runtime || arguments_runtime || async_reactor));
    // Native executables of programs that can recurse report a stack overflow themselves (E14 Phase 3);
    // objects leave the host's signals alone, and a program without recursion cannot exhaust its stack.
    let stack_runtime = options.target == Target::Native
        && options.emit == Emit::Executable
        && cfg!(unix)
        && llvm::has_recursion(&text);
    let debug_import = options.target.is_wasm() && text.contains("@tsuzuri_debug_write(");
    // A callback's address is a table index the host resolves through the exported table.
    let callback_table = options.target.is_wasm() && text.contains("@tz.callback.");
    if task_runtime
        && !cfg!(any(unix, windows))
        && !matches!(options.emit, Emit::Llvm | Emit::Header)
    {
        return Err(driver_error(
            "E2002",
            "native parallel tasks require a POSIX or Windows toolchain; wasm32 provides the portable sequential backend",
        ));
    }
    // The OS-API errors come first: `os_runtime` is part of `native_runtime`, and the generic message below
    // would send a Windows user to link a POSIX runtime that cannot be linked there.
    if os_runtime && options.target.is_wasm() && options.wasm_host.is_none() {
        return Err(driver_error("E2000", OS_WASM_MESSAGE));
    }
    if async_reactor
        && cfg!(windows)
        && options.target == Target::Native
        && options.emit != Emit::Llvm
    {
        return Err(driver_error(
            "E2002",
            "Async.block_on is not available on Windows yet; its reactor is POSIX only (G10)",
        ));
    }
    if os_runtime && cfg!(windows) && options.target == Target::Native && options.emit != Emit::Llvm
    {
        return Err(driver_error("E2002", OS_WINDOWS_MESSAGE));
    }
    if cfg!(windows) && native_runtime && options.emit == Emit::Object {
        return Err(driver_error(
            "E2002",
            "Windows COFF objects with embedded task, CPU, or IO runtime are not supported; emit LLVM and link the runtime once, or build an executable",
        ));
    }
    protect_sources(project, output)?;
    protect_links(links, output)?;
    let sidecar = options.trap_info.then(|| trap_sidecar_path(output));
    let dwarf_sidecar = (cfg!(target_os = "macos")
        && options.debug_info
        && matches!(options.emit, Emit::Executable | Emit::Shared))
    .then(|| {
        let mut path = output.as_os_str().to_owned();
        path.push(".dwarf");
        PathBuf::from(path)
    });
    if let Some(sidecar) = &sidecar {
        protect_sources(project, sidecar)?;
        protect_links(links, sidecar)?;
    }
    if let Some(sidecar) = &dwarf_sidecar {
        protect_sources(project, sidecar)?;
        protect_links(links, sidecar)?;
    }
    let pdb = (options.debug_info
        && options.target == Target::Native
        && options.emit == Emit::Executable
        && msvc_linker())
    .then(|| pdb_path(output));
    if let Some(pdb) = &pdb {
        protect_sources(project, pdb)?;
        protect_links(links, pdb)?;
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create output directory", parent, error))?;
    let mut temporary = TemporaryDirectory::new(parent)?;
    let artifact = temporary
        .path
        .join(if options.emit == Emit::Executable && cfg!(windows) {
            "artifact.exe"
        } else {
            "artifact"
        });
    let mut messages = Vec::new();
    let staged_sidecar = temporary.path.join("sites.json");
    let staged_dwarf = temporary.path.join("symbols.dwarf");
    let staged_pdb = temporary.path.join("artifact.pdb");
    if sidecar.is_some() {
        let table =
            project.with_trap_sources(|sources| crate::trap::side_table(&trap_sites, sources))?;
        fs::write(&staged_sidecar, table)
            .map_err(|error| io_error("write trap side table", &staged_sidecar, error))?;
    }
    let mut cache_paths =
        std::collections::BTreeMap::from([("artifact".to_owned(), artifact.clone())]);
    if sidecar.is_some() {
        cache_paths.insert("traps".into(), staged_sidecar.clone());
    }
    if dwarf_sidecar.is_some() {
        cache_paths.insert("dwarf".into(), staged_dwarf.clone());
    }
    if pdb.is_some() {
        cache_paths.insert("pdb".into(), staged_pdb.clone());
    }
    // The cache key does not cover the contents of link inputs, so a build with them is never cached.
    let cache = if options.cache && options.emit != Emit::Header && links.is_empty() {
        let prepared = (|| -> io::Result<_> {
            let root = crate::cache::default_root()
                .ok_or_else(|| io::Error::other("no cache directory is configured"))?;
            let key = crate::cache::build_key(project, options, action, &text, output)?;
            let cache = crate::cache::BuildCache::open(&root)?;
            Ok((cache, key))
        })();
        match prepared {
            Ok(value) => Some(value),
            Err(error) => {
                messages.push(format!("build cache disabled: {error}"));
                None
            }
        }
    } else {
        None
    };
    let mut cached = None;
    if let Some((cache, key)) = &cache {
        match cache.load(key) {
            Ok(Some(artifact)) if artifact.files.keys().eq(cache_paths.keys()) => {
                cached = Some(artifact)
            }
            Ok(_) => {}
            Err(error) => messages.push(format!("build cache read failed; rebuilding: {error}")),
        }
    }
    let cache_hit = cached.is_some();
    if let Some(cached) = cached {
        for (name, file) in cached.files {
            let path = &cache_paths[&name];
            fs::write(path, file.bytes)
                .map_err(|error| io_error("restore cached artifact", path, error))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(file.mode))
                    .map_err(|error| io_error("restore cached file mode", path, error))?;
            }
            #[cfg(not(unix))]
            debug_assert_eq!(file.mode, 0);
        }
        messages.extend(cached.messages);
        if let Some((cache, _)) = &cache
            && let Err(error) = cache.evict()
        {
            messages.push(format!("build cache cleanup failed: {error}"));
        }
    } else if matches!(options.emit, Emit::Llvm | Emit::Header | Emit::Wgsl) {
        fs::write(&artifact, text).map_err(|error| io_error("write output", &artifact, error))?;
    } else {
        // The C names that a shared library exports, read from the IR before it is written.
        let shared_symbols = (options.emit == Emit::Shared)
            .then(|| shared_exports(&text, module, options.trap_return));
        let mut ir = temporary.path.join("module.ll");
        fs::write(&ir, text).map_err(|error| io_error("write LLVM IR", &ir, error))?;
        let runtime_object = temporary.path.join("task.o");
        let merge_debug_ir = cfg!(target_os = "macos")
            && options.debug_info
            && native_runtime
            && options.emit == Emit::Object;
        if native_runtime {
            let runtime_source = temporary.path.join("task.c");
            let source = format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}",
                // The feature macros of os.c must precede every include, so it comes first.
                if os_runtime && options.target == Target::Native {
                    include_str!("runtime/os.c")
                } else {
                    ""
                },
                if async_reactor && options.target == Target::Native {
                    include_str!("runtime/async.c")
                } else {
                    ""
                },
                if trap_runtime {
                    include_str!("runtime/trap.c")
                } else {
                    ""
                },
                if task_runtime {
                    task_runtime_source()
                } else {
                    String::new()
                },
                if cpu_runtime {
                    include_str!("runtime/cpu.c")
                } else {
                    ""
                },
                if io_runtime {
                    include_str!("runtime/io.c")
                } else {
                    ""
                },
                if arguments_runtime {
                    include_str!("runtime/arguments.c")
                } else {
                    ""
                }
            );
            fs::write(&runtime_source, source)
                .map_err(|error| io_error("write task runtime", &runtime_source, error))?;
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            runtime
                .args(["-std=c11", "-c"])
                .args(native_compile_args(cfg!(windows), env::consts::ARCH))
                .arg(format!("-O{}", options.optimization));
            if (task_runtime || trap_runtime || async_reactor) && !cfg!(windows) {
                runtime.arg("-pthread");
            }
            if trap_runtime {
                runtime.arg("-DTZ_TRAP_BOUNDARY");
            }
            if stack_runtime {
                runtime.arg("-DTZ_STACK_GUARD");
            }
            // The runtime has no debug information, like the stack guard below: stepping never
            // stops in it, and the program's DWARF is the compiler's unit alone, whose version a
            // runtime unit at Clang's default (DWARF 5) would otherwise raise when merged (G16).
            if merge_debug_ir {
                runtime.args(["-S", "-emit-llvm"]);
            }
            if options.cpu == Cpu::Native {
                runtime.arg(native_cpu_flag(env::consts::ARCH)?);
            }
            runtime.arg(&runtime_source).arg("-o").arg(&runtime_object);
            collect_message(
                &mut messages,
                run_tool(
                    &mut runtime,
                    "native runtime requires Clang and the platform C/OS SDK headers",
                )?,
            );
            if merge_debug_ir {
                let combined = temporary.path.join("combined.ll");
                let mut linker = Command::new(tool("TSUZURI_LLVM_LINK", "llvm-link"));
                linker
                    .arg(&ir)
                    .arg(&runtime_object)
                    .arg("-S")
                    .arg("-o")
                    .arg(&combined);
                collect_message(
                    &mut messages,
                    run_tool(
                        &mut linker,
                        "macOS debug objects with tasks require llvm-link matching Clang; set TSUZURI_LLVM_LINK",
                    )?,
                );
                ir = combined;
            }
        }
        let stack_source = temporary.path.join("stack.c");
        let stack_object = temporary.path.join("stack.o");
        if stack_runtime {
            fs::write(&stack_source, include_str!("runtime/stack.c"))
                .map_err(|error| io_error("write stack runtime", &stack_source, error))?;
        }
        if stack_runtime && options.debug_info {
            // Its own object, without debug information: the guard is not source a debugger should show.
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            runtime
                .args(["-std=c11", "-c", "-O1"])
                .args(native_compile_args(cfg!(windows), env::consts::ARCH))
                .arg(&stack_source)
                .arg("-o")
                .arg(&stack_object);
            collect_message(
                &mut messages,
                run_tool(
                    &mut runtime,
                    "native runtime requires Clang and the platform C/OS SDK headers",
                )?,
            );
        }
        let mut clang = Command::new(tool("TSUZURI_CLANG", "clang"));
        let wasi_object = temporary.path.join("wasi.o");
        if wasi_runtime {
            let source = temporary.path.join("wasi.c");
            let text = if arguments_runtime {
                format!(
                    "{}\n{}",
                    include_str!("runtime/os-wasi.c"),
                    include_str!("runtime/arguments.c")
                )
            } else {
                include_str!("runtime/os-wasi.c").to_owned()
            };
            fs::write(&source, text)
                .map_err(|error| io_error("write WASI runtime", &source, error))?;
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            runtime.args([
                "--target=wasm32-unknown-unknown",
                "-std=c11",
                "-ffreestanding",
                "-fno-builtin",
                "-fno-stack-protector",
                "-mbulk-memory",
                "-c",
            ]);
            if wasi_command {
                runtime.arg("-DTZ_WASI_START");
                if llvm::exit_code_entry(module) {
                    runtime.arg("-DTZ_WASI_EXIT_CODE");
                }
            }
            runtime
                .arg(format!("-O{}", options.optimization))
                .arg(&source)
                .arg("-o")
                .arg(&wasi_object);
            collect_message(
                &mut messages,
                run_tool(
                    &mut runtime,
                    "the WASI host needs Clang with WebAssembly support",
                )?,
            );
        }
        let threads_object = temporary.path.join("threads.o");
        if options.wasm_threads {
            let source = temporary.path.join("threads.c");
            fs::write(&source, include_str!("runtime/task-wasm-threads.c"))
                .map_err(|error| io_error("write WASM thread runtime", &source, error))?;
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            runtime
                .args([
                    "--target=wasm32-unknown-unknown",
                    "-std=c11",
                    "-ffreestanding",
                    "-fno-stack-protector",
                    "-matomics",
                    "-mbulk-memory",
                    "-c",
                ])
                .arg(format!("-O{}", options.optimization))
                .arg(&source)
                .arg("-o")
                .arg(&threads_object);
            collect_message(
                &mut messages,
                run_tool(
                    &mut runtime,
                    "WASM threads require Clang atomics and bulk-memory support",
                )?,
            );
        }
        clang
            .arg("-x")
            .arg("ir")
            .arg("-Wno-override-module")
            .arg(format!("-O{}", options.optimization));
        if options.debug_info {
            clang.arg("-g");
        }
        if options.target.is_wasm() {
            clang
                .arg(if options.target == Target::Wasm64 {
                    "--target=wasm64-unknown-unknown"
                } else {
                    "--target=wasm32-unknown-unknown"
                })
                .arg("-mbulk-memory");
            if options.wasm_threads {
                clang.arg("-matomics");
            }
            clang.arg(if options.wasm_simd {
                "-msimd128"
            } else {
                "-mno-simd128"
            });
        } else {
            clang.args(native_compile_args(cfg!(windows), env::consts::ARCH));
            if options.cpu == Cpu::Native {
                clang.arg(native_cpu_flag(env::consts::ARCH)?);
            }
        }
        if options.emit != Emit::Executable || dwarf_sidecar.is_some() {
            clang.arg("-c");
        }
        let object = temporary.path.join("module.o");
        clang.arg(&ir).arg("-o").arg(
            if matches!(options.emit, Emit::Wasm | Emit::Shared)
                || dwarf_sidecar.is_some()
                || ((options.wasm_threads || wasi_runtime) && options.emit == Emit::Object)
                || (native_runtime && options.emit == Emit::Object && !merge_debug_ir)
            {
                &object
            } else {
                &artifact
            },
        );
        if options.emit == Emit::Executable && !cfg!(windows) && dwarf_sidecar.is_none() {
            clang.arg("-lm");
        }
        if native_runtime && options.emit == Emit::Executable && dwarf_sidecar.is_none() {
            clang.args(["-x", "none"]).arg(&runtime_object);
            if task_runtime && !cfg!(windows) {
                clang.arg("-pthread");
            }
        }
        if stack_runtime && dwarf_sidecar.is_none() {
            if options.debug_info {
                clang.args(["-x", "none"]).arg(&stack_object);
            } else {
                // The same Clang run compiles the guard, which costs far less than a process of its own.
                clang.args(["-x", "c", "-std=c11"]).arg(&stack_source);
            }
            clang.arg("-pthread");
        }
        if options.emit == Emit::Executable && dwarf_sidecar.is_none() {
            links.add_to(&mut clang);
        }
        if let Some(pdb) = &pdb {
            clang.args(pdb_link_args(&staged_pdb, pdb, &temporary.path)?);
        }
        collect_message(
            &mut messages,
            run_tool(
                &mut clang,
                "install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable",
            )?,
        );
        if let Some(symbols) = &shared_symbols {
            link_shared(
                symbols,
                options,
                links,
                (&object, native_runtime.then_some(runtime_object.as_path())),
                (&artifact, output),
                &temporary.path,
                &mut messages,
            )?;
        }
        if dwarf_sidecar.is_some() && options.emit == Emit::Shared {
            let mut symbols = Command::new(tool("TSUZURI_DSYMUTIL", "dsymutil"));
            symbols
                .arg("--flat")
                .arg(&artifact)
                .arg("-o")
                .arg(&staged_dwarf);
            collect_message(
                &mut messages,
                run_tool(
                    &mut symbols,
                    "macOS debug shared libraries require dsymutil; set TSUZURI_DSYMUTIL",
                )?,
            );
        } else if dwarf_sidecar.is_some() {
            let mut linker = Command::new(tool("TSUZURI_CLANG", "clang"));
            linker.arg(&object).args(["-g", "-lm"]);
            if native_runtime {
                linker.arg(&runtime_object);
                if task_runtime {
                    linker.arg("-pthread");
                }
            }
            if stack_runtime {
                linker.arg(&stack_object).arg("-pthread");
            }
            links.add_to(&mut linker);
            linker.arg("-o").arg(&artifact);
            collect_message(
                &mut messages,
                run_tool(
                    &mut linker,
                    "native debug executables require the Clang linker",
                )?,
            );
            let mut symbols = Command::new(tool("TSUZURI_DSYMUTIL", "dsymutil"));
            symbols
                .arg("--flat")
                .arg(&artifact)
                .arg("-o")
                .arg(&staged_dwarf);
            collect_message(
                &mut messages,
                run_tool(
                    &mut symbols,
                    "macOS debug executables require dsymutil; set TSUZURI_DSYMUTIL",
                )?,
            );
        }
        if native_runtime && options.emit == Emit::Object && !merge_debug_ir {
            let mut linker = Command::new(tool("TSUZURI_CLANG", "clang"));
            linker.args(["-r", "-nostdlib"]);
            if cfg!(target_os = "macos") {
                linker.arg("-Wl,-keep_private_externs");
            }
            if cfg!(target_os = "linux") {
                linker.arg("-no-pie");
            }
            linker
                .arg(&object)
                .arg(&runtime_object)
                .arg("-o")
                .arg(&artifact);
            collect_message(
                &mut messages,
                run_tool(
                    &mut linker,
                    "the native linker must support relocatable object linking for parallel tasks",
                )?,
            );
        }
        if (options.wasm_threads || wasi_runtime) && options.emit == Emit::Object {
            let mut linker = Command::new(tool("TSUZURI_WASM_LD", "wasm-ld"));
            linker.arg("-r").arg(&object);
            if options.wasm_threads {
                linker.arg(&threads_object);
            }
            if wasi_runtime {
                linker.arg(&wasi_object);
            }
            linker.arg("-o").arg(&artifact);
            collect_message(
                &mut messages,
                run_tool(
                    &mut linker,
                    "WASM threads and the WASI host require wasm-ld relocatable linking",
                )?,
            );
        }
        if options.emit == Emit::Wasm {
            let mut linker = Command::new(tool("TSUZURI_WASM_LD", "wasm-ld"));
            linker
                .arg("--no-entry")
                .arg("--stack-first")
                .arg("-z")
                .arg(format!("stack-size={stack_size}"))
                .arg(format!("--max-memory={max_memory}"));
            if options.target == Target::Wasm64 {
                linker.arg("-mwasm64");
            }
            if options.wasm_threads {
                linker
                    .args([
                        "--shared-memory",
                        "--import-memory",
                        "--export-memory",
                        "--export=__stack_pointer",
                        "--export=tsuzuri_thread_stack_alloc",
                        "--export=tsuzuri_thread_stack_size",
                        "--export=tsuzuri_thread_stack_probe",
                        "--export=tsuzuri_thread_heap_live_bytes",
                        "--export=tsuzuri_thread_entry",
                        "--export=tsuzuri_threads_init",
                        "--export=tsuzuri_threads_control",
                    ])
                    .arg(&threads_object);
            }
            if stack_checks && options.wasm_threads {
                linker.args(["--export=tsuzuri_stack_base", "--export=tsuzuri_stack_top"]);
            }
            if !options.debug_info {
                linker.arg("--strip-all");
            }
            if debug_import {
                linker.arg("--export-memory");
            }
            if callback_table {
                linker.arg("--export-table");
            }
            if llvm::uses_host_abi(module)
                || io_runtime
                || os_runtime
                || options.allocator == llvm::Allocator::Counting
            {
                linker.args([
                    "--export=tsuzuri_alloc",
                    "--export=tsuzuri_free",
                    "--export-memory",
                ]);
            }
            // F13: a counting module reports its counts; a host allocator manages the memory
            // above `__heap_base`.
            if options.allocator == llvm::Allocator::Counting {
                linker.arg("--export=tsuzuri_alloc_stats");
            }
            if options.allocator == llvm::Allocator::Host {
                linker.args(["--export=__heap_base", "--export-memory"]);
            }
            if wasi_runtime {
                linker.arg(&wasi_object);
            }
            if llvm::io_entry(module) || llvm::main_entry(module) {
                linker.arg("--export=tsuzuri_main");
            }
            if options.trap_info {
                linker.arg("--export=tsuzuri_trap_site");
            }
            for function in &module.functions {
                if function.exported {
                    linker.arg(format!("--export=tz_{}", function.name));
                }
            }
            for (symbol, _) in llvm::host_entries(module) {
                linker.arg(format!("--export={symbol}"));
            }
            if !llvm::host_entries(module).is_empty() {
                linker.arg("--export=tsuzuri_async_set_epoch");
            }
            linker.arg(&object).arg("-o").arg(&artifact);
            collect_message(
                &mut messages,
                run_tool(
                    &mut linker,
                    &wasm_link_hint(
                        "install LLVM LLD or set TSUZURI_WASM_LD to the wasm-ld executable",
                        options.wasm_max_memory,
                        options.wasm_stack_size,
                    ),
                )?,
            );
        }
    }
    let sidecars: Vec<_> = sidecar
        .as_ref()
        .map(|path| (&staged_sidecar, path))
        .into_iter()
        .chain(dwarf_sidecar.as_ref().map(|path| (&staged_dwarf, path)))
        .chain(pdb.as_ref().map(|path| (&staged_pdb, path)))
        .collect();
    if !cache_hit
        && let Some((cache, key)) = &cache
        && let Err(error) = cache.store(key, &cache_paths, &messages)
    {
        messages.push(format!("build cache write failed: {error}"));
    }
    publish_outputs(project, &artifact, output, &sidecars, &mut temporary)?;
    temporary.close()?;
    Ok((messages, trap_sites))
}

/// The C names a shared library exports: the header's entry points that the IR defines.
pub(crate) fn shared_exports(ir: &str, module: &CheckedModule, trap_return: bool) -> Vec<String> {
    let defined = |symbol: &str| {
        let call = format!("@{symbol}(");
        ir.lines().any(|line| {
            line.starts_with("define ")
                && line.contains(&call)
                && !line.contains(" internal ")
                && !line.contains(" hidden ")
        })
    };
    let mut symbols: Vec<String> = module
        .functions
        .iter()
        .filter(|function| function.exported)
        .flat_map(|function| {
            let mut names = vec![format!("tz_{}", function.name)];
            if trap_return {
                names.push(format!("tsuzuri_try_{}", function.name));
            }
            names
        })
        .collect();
    // The reactor's runtime defines `tsuzuri_async_post`, which other threads call (B08).
    if llvm::uses_reactor(ir) {
        symbols.push("tsuzuri_async_post".to_owned());
    }
    for symbol in [
        "tsuzuri_alloc",
        "tsuzuri_alloc_stats",
        "tsuzuri_async_complete",
        "tsuzuri_async_poll",
        "tsuzuri_free",
        "tsuzuri_main",
    ] {
        if defined(symbol) {
            symbols.push(symbol.to_owned());
        }
    }
    symbols.sort();
    symbols
}

/// Links the object of `--emit shared` (E13 Phase 2) into a shared library that exports only the
/// public C ABI and resolves every symbol at link time, like an executable.
fn link_shared(
    symbols: &[String],
    options: BuildOptions,
    links: &LinkInputs,
    (object, runtime): (&Path, Option<&Path>),
    (artifact, output): (&Path, &Path),
    temporary: &Path,
    messages: &mut Vec<String>,
) -> Result<(), Diagnostic> {
    let list = temporary.join("exports.txt");
    let file_name = output
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("library");
    let mut linker = Command::new(tool("TSUZURI_CLANG", "clang"));
    if cfg!(target_os = "macos") {
        let text: String = symbols
            .iter()
            .map(|symbol| format!("_{symbol}\n"))
            .collect();
        fs::write(&list, text).map_err(|error| io_error("write export list", &list, error))?;
        let mut exported = OsString::from("-Wl,-exported_symbols_list,");
        exported.push(&list);
        linker
            .arg("-dynamiclib")
            .arg(exported)
            .arg(format!("-Wl,-install_name,@rpath/{file_name}"));
    } else {
        let text = format!(
            "{{\n  global:\n{}  local: *;\n}};\n",
            symbols
                .iter()
                .map(|symbol| format!("    {symbol};\n"))
                .collect::<String>()
        );
        fs::write(&list, text).map_err(|error| io_error("write export list", &list, error))?;
        let mut script = OsString::from("-Wl,--version-script=");
        script.push(&list);
        linker
            .arg("-shared")
            .arg(script)
            .arg(format!("-Wl,-soname,{file_name}"))
            .arg("-Wl,--no-undefined");
    }
    if options.debug_info {
        linker.arg("-g");
    }
    linker.args(["-x", "none"]).arg(object);
    if let Some(runtime) = runtime {
        linker.arg(runtime).arg("-pthread");
    }
    linker.arg("-lm");
    links.add_to(&mut linker);
    linker.arg("-o").arg(artifact);
    collect_message(
        messages,
        run_tool(
            &mut linker,
            "shared libraries require the Clang linker; unresolved extern symbols need --link, -l or -L",
        )?,
    );
    Ok(())
}

/// Writes the bindings of `--emit bindings-js` (E13) and their declarations without LLVM.
fn build_bindings(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
) -> Result<(), Diagnostic> {
    if let Some(error) = bindings_output_error(options.emit, output) {
        return Err(error);
    }
    if !module.functions.iter().any(|function| function.exported) {
        return Err(driver_error(
            "E2004",
            "bindings need at least one 'export def' entry point",
        ));
    }
    let declarations = (options.emit == Emit::BindingsJs).then(|| bindings_sidecar_path(output));
    protect_sources(project, output)?;
    if let Some(declarations) = &declarations {
        protect_sources(project, declarations)?;
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create output directory", parent, error))?;
    let mut temporary = TemporaryDirectory::new(parent)?;
    let artifact = temporary.path.join("artifact");
    let staged = temporary.path.join("declarations");
    let flavor = if options.wasm_threads {
        crate::bindings::JsFlavor::Threads
    } else if options.wasm_jspi {
        crate::bindings::JsFlavor::Jspi
    } else {
        crate::bindings::JsFlavor::Single
    };
    // The file stem names the shared library of the native bindings, and the C++ header includes
    // the C header of the same stem.
    let stem = output
        .file_stem()
        .and_then(OsStr::to_str)
        .expect("bindings_output_error checked the file name");
    let text = match options.emit {
        Emit::BindingsCs => crate::bindings::csharp(module, stem, options.trap_return),
        Emit::BindingsPy => crate::bindings::python(module, stem, options.trap_return),
        Emit::BindingsCpp => crate::bindings::cpp(module, stem, options.trap_return),
        _ => crate::bindings::javascript_for(module, flavor),
    };
    fs::write(&artifact, text).map_err(|error| io_error("write output", &artifact, error))?;
    let mut sidecars = Vec::new();
    if let Some(declarations) = &declarations {
        fs::write(&staged, crate::bindings::declarations_for(module, flavor))
            .map_err(|error| io_error("write bindings declarations", &staged, error))?;
        sidecars.push((&staged, declarations));
    }
    publish_outputs(project, &artifact, output, &sidecars, &mut temporary)?;
    temporary.close()
}

fn publish_outputs(
    project: &Project,
    artifact: &Path,
    output: &Path,
    sidecars: &[(&PathBuf, &PathBuf)],
    temporary: &mut TemporaryDirectory,
) -> Result<(), Diagnostic> {
    protect_sources(project, output)?;
    let mut published = Vec::new();
    let result = (|| {
        for (index, (staged, sidecar)) in sidecars.iter().enumerate() {
            protect_sources(project, sidecar)?;
            let backup = temporary.path.join(format!("previous-sidecar-{index}"));
            let previous = match fs::symlink_metadata(sidecar) {
                Ok(_) => {
                    fs::hard_link(sidecar, &backup)
                        .map_err(|error| io_error("back up output sidecar", sidecar, error))?;
                    true
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => false,
                Err(error) => return Err(io_error("inspect output sidecar", sidecar, error)),
            };
            fs::rename(staged, sidecar)
                .map_err(|error| io_error("publish output sidecar", sidecar, error))?;
            published.push((*sidecar, backup, previous));
        }
        fs::rename(artifact, output).map_err(|error| io_error("publish output", output, error))
    })();
    if let Err(error) = result {
        for (sidecar, backup, previous) in published.into_iter().rev() {
            let restored = if previous {
                fs::rename(backup, sidecar)
            } else {
                fs::remove_file(sidecar)
            };
            if let Err(restore_error) = restored {
                temporary.removed = true;
                return Err(driver_error(
                    "E2003",
                    format!(
                        "output publication failed ({}) and a sidecar could not be restored ({restore_error}); recovery files remain in '{}'",
                        error.message,
                        temporary.path.display()
                    ),
                ));
            }
        }
        return Err(error);
    }
    Ok(())
}

/// Whether native executables on this host link with the MSVC linker (`link.exe`, or `lld-link`
/// with `-fuse-ld=lld`): Windows with any Clang but the bundled launcher, which links with MinGW's
/// `ld.lld` and keeps DWARF only (G14). Their `-g` builds also carry CodeView and get a PDB.
/// The file name is enough: every other Clang, a MinGW distribution's included, gets
/// `--target=<arch>-pc-windows-msvc` from `native_compile_args`, and Clang's MSVC driver links
/// only with `link.exe` or `lld-link`; the launcher alone drops that target for `windows-gnu`.
pub(crate) fn msvc_linker() -> bool {
    cfg!(windows)
        && Path::new(&tool("TSUZURI_CLANG", "clang"))
            .file_stem()
            .is_none_or(|stem| stem != "tsuzuri-clang")
}

/// The PDB of the `-g` executable `output` on Windows: `<output>` with the extension `.pdb`.
pub fn pdb_path(output: &Path) -> PathBuf {
    output.with_extension("pdb")
}

/// The Clang arguments with which the MSVC linker writes the PDB to `staged` in `directory` and
/// embeds the Tsuzuri natvis views (G16 Phase 3). The executable names the PDB by the file name of
/// `published`, where the build publishes it, so debuggers find it beside the executable.
fn pdb_link_args(
    staged: &Path,
    published: &Path,
    directory: &Path,
) -> Result<Vec<OsString>, Diagnostic> {
    let natvis = directory.join("tsuzuri.natvis");
    fs::write(&natvis, include_str!("runtime/tsuzuri.natvis"))
        .map_err(|error| io_error("write natvis", &natvis, error))?;
    let name = published
        .file_name()
        .map_or_else(|| OsString::from("artifact.pdb"), OsString::from);
    let mut arguments = Vec::new();
    for (option, value) in [
        ("/PDB:", staged.as_os_str()),
        ("/PDBALTPATH:", name.as_os_str()),
        ("/NATVIS:", natvis.as_os_str()),
    ] {
        let mut argument = OsString::from(option);
        argument.push(value);
        // `-Xlinker` passes the argument whole; `-Wl,` would split a path at its commas.
        arguments.extend([OsString::from("-Xlinker"), argument]);
    }
    Ok(arguments)
}

pub fn trap_sidecar_path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_owned();
    name.push(".trap.json");
    PathBuf::from(name)
}

/// What a native executable writes to stderr when its stack overflows (`src/runtime/stack.c`).
pub(crate) const STACK_OVERFLOW_REPORT: &str = "trap: stack overflow";

/// Whether a child died of an invalid memory access, which unbounded recursion causes without a trap report.
#[cfg(unix)]
pub(crate) fn probable_stack_exhaustion(status: &std::process::ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    const SIGSEGV: i32 = 11;
    #[cfg(target_os = "linux")]
    const SIGBUS: Option<i32> = Some(7);
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    const SIGBUS: Option<i32> = Some(10);
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    const SIGBUS: Option<i32> = None;
    status
        .signal()
        .is_some_and(|signal| signal == SIGSEGV || Some(signal) == SIGBUS)
}

#[cfg(not(unix))]
pub(crate) fn probable_stack_exhaustion(_status: &std::process::ExitStatus) -> bool {
    false
}

pub fn run(
    module: &CheckedModule,
    project: &Project,
    options: BuildOptions,
) -> Result<Vec<String>, Diagnostic> {
    run_with_diagnostics(module, project, options, &LinkInputs::default(), false)
}

pub fn run_with_diagnostics(
    module: &CheckedModule,
    project: &Project,
    options: BuildOptions,
    links: &LinkInputs,
    json: bool,
) -> Result<Vec<String>, Diagnostic> {
    run_with_arguments(module, project, options, links, json, &[])
}

/// [`run_with_diagnostics`] that passes `arguments` to the program, after its own path:
/// `tsuzuri script FILE [arguments...]`.
pub fn run_with_arguments(
    module: &CheckedModule,
    project: &Project,
    options: BuildOptions,
    links: &LinkInputs,
    json: bool,
    arguments: &[OsString],
) -> Result<Vec<String>, Diagnostic> {
    run_process(
        module,
        project,
        options,
        links,
        RunStdio::Inherit { json, arguments },
    )
    .map(|(messages, _)| messages)
}

/// The most standard output that [`run_captured`] collects before it stops the program.
pub const MAX_CAPTURED_OUTPUT: usize = 16 * 1024 * 1024;

/// Builds and runs the program like `tsuzuri run`, but with an empty standard input and its
/// standard output collected and returned; its standard error is relayed as `run` relays it.
/// A program that runs longer than `timeout` or writes more than [`MAX_CAPTURED_OUTPUT`]
/// bytes is killed and reported as `E2005` without a source position. The REPL uses it.
pub fn run_captured(
    module: &CheckedModule,
    project: &Project,
    options: BuildOptions,
    timeout: Option<std::time::Duration>,
) -> Result<(Vec<String>, Vec<u8>), Diagnostic> {
    run_process(
        module,
        project,
        options,
        &LinkInputs::default(),
        RunStdio::Capture {
            timeout,
            limit: MAX_CAPTURED_OUTPUT,
        },
    )
}

/// How [`run_process`] connects the program's standard streams.
enum RunStdio<'a> {
    /// `tsuzuri run` and `script`: the program gets `arguments` and shares the compiler's stdin
    /// and stdout, and its stderr is relayed, or with `json` collected and reported after it ends.
    Inherit {
        json: bool,
        arguments: &'a [OsString],
    },
    /// [`run_captured`]: stdin is empty, stdout is collected up to `limit` bytes, and stderr is
    /// relayed; the program is killed past `timeout` or `limit`.
    Capture {
        timeout: Option<std::time::Duration>,
        limit: usize,
    },
}

/// Builds the program into a temporary directory, runs it, and returns the build messages and
/// the collected stdout (empty unless captured).
fn run_process(
    module: &CheckedModule,
    project: &Project,
    options: BuildOptions,
    links: &LinkInputs,
    stdio: RunStdio<'_>,
) -> Result<(Vec<String>, Vec<u8>), Diagnostic> {
    if options.target != Target::Native || options.emit != Emit::Executable {
        return Err(driver_error(
            "E2000",
            "run requires native executable output",
        ));
    }
    let temporary = TemporaryDirectory::new(&env::temp_dir())?;
    let output = temporary.path.join(if cfg!(windows) {
        "program.exe"
    } else {
        "program"
    });
    let (messages, sites) = build_complete(
        module,
        project,
        &output,
        BuildOptions {
            trap_info: true,
            ..options
        },
        links,
        "run",
    )?;
    let json = matches!(stdio, RunStdio::Inherit { json: true, .. });
    let (status, stderr, stdout) = match stdio {
        RunStdio::Inherit { json, arguments } => {
            let mut child = Command::new(&output)
                .args(arguments)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| io_error("run executable", &output, error))?;
            let mut stderr = Vec::new();
            let stream = child.stderr.take().expect("stderr is piped");
            if let Err(error) = relay(stream, &mut stderr, !json) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io_error("relay program stderr", &output, error));
            }
            let status = child
                .wait()
                .map_err(|error| io_error("wait for executable", &output, error))?;
            (status, stderr, Vec::new())
        }
        RunStdio::Capture { timeout, limit } => capture(&output, timeout, limit)?,
    };
    temporary.close()?;
    // An `IO<i32>` entry returns its value as the exit code, so a non-zero code is not a trap.
    if let Some(code) = status.code()
        && code != 0
        && llvm::exit_code_entry(module)
    {
        let stderr = String::from_utf8_lossy(&stderr);
        let message = if json && !stderr.trim().is_empty() {
            format!("program exited with code {code}:\n{}", stderr.trim_end())
        } else {
            format!("program exited with code {code}")
        };
        return Err(driver_error("E2005", message));
    }
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        let site = project.with_trap_sources(|sources| {
            sites
                .iter()
                .find(|site| {
                    site.message(sources)
                        .is_ok_and(|message| stderr.contains(&message))
                })
                .map(|site| site.span)
        });
        let message = if !json || stderr.trim().is_empty() {
            if site.is_none() && stderr.contains(STACK_OVERFLOW_REPORT) {
                format!(
                    "program terminated with {status}; stack overflow: the stack was exhausted by deep recursion; reduce the recursion depth or use a loop"
                )
            } else if site.is_none() && probable_stack_exhaustion(&status) {
                format!(
                    "program terminated with {status}; the stack was probably exhausted by deep recursion; reduce the recursion depth or use a loop"
                )
            } else {
                format!(
                    "program terminated with {status}; integer division, indexing, assert, or allocation may have trapped"
                )
            }
        } else {
            format!("program terminated with {}:\n{}", status, stderr.trim_end())
        };
        return Err(Diagnostic::new("E2005", message, site.unwrap_or_default()));
    }
    if json {
        io::stderr()
            .write_all(&stderr)
            .map_err(|error| io_error("relay program stderr", &output, error))?;
    }
    Ok((messages, stdout))
}

/// Copies `stream` into `collected` until it ends, also writing it to stderr when `echo`.
fn relay(mut stream: impl Read, collected: &mut Vec<u8>, echo: bool) -> io::Result<()> {
    let mut buffer = [0; 8192];
    loop {
        let count = match stream.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(());
        }
        collected.extend_from_slice(&buffer[..count]);
        if echo {
            io::stderr().write_all(&buffer[..count])?;
        }
    }
}

/// How a captured stream of the program ended.
enum StreamEnd {
    Closed,
    /// The program wrote more stdout than the limit.
    Overflow,
    Failed(io::Error),
}

/// Runs `output` for [`run_captured`]. The streams are read on their own threads, so neither
/// pipe can fill up while the other is read.
fn capture(
    output: &Path,
    timeout: Option<std::time::Duration>,
    limit: usize,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), Diagnostic> {
    use std::sync::mpsc::{RecvTimeoutError, channel};
    use std::time::{Duration, Instant};
    let mut child = Command::new(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| io_error("run executable", output, error))?;
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let (sender, receiver) = channel();
    let stdout = child.stdout.take().expect("stdout is piped");
    let stdout_sender = sender.clone();
    let stdout_reader = std::thread::spawn(move || {
        let mut collected = Vec::new();
        let mut stream = stdout;
        let mut buffer = [0; 8192];
        let end = loop {
            let count = match stream.read(&mut buffer) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => break StreamEnd::Failed(error),
                Ok(count) => count,
            };
            if count == 0 {
                break StreamEnd::Closed;
            }
            if collected.len() + count > limit {
                break StreamEnd::Overflow;
            }
            collected.extend_from_slice(&buffer[..count]);
        };
        let _ = stdout_sender.send(end);
        collected
    });
    let stderr = child.stderr.take().expect("stderr is piped");
    let stderr_reader = std::thread::spawn(move || {
        let mut collected = Vec::new();
        let end = match relay(stderr, &mut collected, true) {
            Ok(()) => StreamEnd::Closed,
            Err(error) => StreamEnd::Failed(error),
        };
        let _ = sender.send(end);
        collected
    });
    let next = |deadline: Option<Instant>| match deadline {
        Some(deadline) => receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())),
        None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
    };
    let exceeded = || {
        format!(
            "evaluation exceeded the {}-second limit and was stopped; use --timeout to change it",
            timeout.unwrap_or_default().as_secs()
        )
    };
    let mut open = 2;
    let mut stopped = None;
    while open > 0 && stopped.is_none() {
        match next(deadline) {
            Ok(StreamEnd::Closed) => open -= 1,
            Ok(StreamEnd::Overflow) => {
                open -= 1;
                stopped = Some(format!(
                    "program output exceeded {} MiB and the program was stopped",
                    limit >> 20
                ));
            }
            Ok(StreamEnd::Failed(error)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io_error("read program output", output, error));
            }
            Err(RecvTimeoutError::Timeout) => stopped = Some(exceeded()),
            Err(RecvTimeoutError::Disconnected) => open = 0,
        }
    }
    let status = if stopped.is_none() {
        // Both streams are closed, so the program has exited unless it closed them itself.
        // ponytail: 1–10 ms try_wait polling for that rare case; use a blocking wait with a timeout if std gains one
        let mut pause = Duration::from_millis(1);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if deadline.is_some_and(|deadline| Instant::now() >= deadline) => {
                    stopped = Some(exceeded());
                    break None;
                }
                Ok(None) if deadline.is_none() => {
                    break Some(
                        child
                            .wait()
                            .map_err(|error| io_error("wait for executable", output, error))?,
                    );
                }
                Ok(None) => {
                    std::thread::sleep(pause);
                    pause = (pause * 2).min(Duration::from_millis(10));
                }
                Err(error) => return Err(io_error("wait for executable", output, error)),
            }
        }
    } else {
        None
    };
    if let Some(message) = stopped {
        let _ = child.kill();
        let _ = child.wait();
        // Let the relay finish the program's last stderr before the diagnostic follows it; a
        // process the program started may hold the pipes open, so this waits only briefly.
        let grace = Instant::now() + Duration::from_millis(500);
        while open > 0 && next(Some(grace)).is_ok() {
            open -= 1;
        }
        return Err(Diagnostic::new("E2005", message, Span::default()));
    }
    let stdout = stdout_reader
        .join()
        .expect("the stdout reader does not panic");
    let stderr = stderr_reader
        .join()
        .expect("the stderr reader does not panic");
    Ok((
        status.expect("a program that was not stopped has a status"),
        stderr,
        stdout,
    ))
}

/// Where a compiler tool was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSource {
    Env,
    Bundled,
    Path,
}

/// The distribution root two levels above the compiler, marked by `manifest.json`.
pub fn distribution_root() -> Option<PathBuf> {
    let executable = crate::cache::real_path(&env::current_exe().ok()?).ok()?;
    let root = executable.parent()?.parent()?;
    root.join("manifest.json")
        .is_file()
        .then(|| root.to_path_buf())
}

/// Resolves a tool from its variable (even when empty), the distribution, then `PATH`.
pub fn resolve_tool(variable: &str, fallback: &str) -> (ToolSource, OsString) {
    if let Some(value) = env::var_os(variable) {
        return (ToolSource::Env, value);
    }
    let name = if variable == "TSUZURI_CLANG" {
        "tsuzuri-clang"
    } else {
        fallback
    };
    if let Some(root) = distribution_root() {
        let path = root
            .join("bin")
            .join(format!("{name}{}", env::consts::EXE_SUFFIX));
        if path.is_file() {
            return (ToolSource::Bundled, path.into_os_string());
        }
    }
    (ToolSource::Path, fallback.into())
}

fn tool(variable: &str, fallback: &str) -> OsString {
    resolve_tool(variable, fallback).1
}

fn run_tool(command: &mut Command, hint: &str) -> Result<String, Diagnostic> {
    let name = command.get_program().to_string_lossy().into_owned();
    let output = command.output().map_err(|error| {
        driver_error("E2002", format!("cannot execute '{name}': {error}; {hint}"))
    })?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(driver_error(
            "E2002",
            format!(
                "'{name}' failed ({}):\n{}\n{hint}",
                output.status,
                text.trim()
            ),
        ));
    }
    Ok(text)
}

fn collect_message(messages: &mut Vec<String>, message: String) {
    if !message.trim().is_empty() {
        messages.push(message);
    }
}

fn protect_sources(project: &Project, output: &Path) -> Result<(), Diagnostic> {
    for source in project.sources.iter().chain(&project.manifests) {
        // Std sources are embedded, so no output can overwrite them.
        if source.origin == ModuleOrigin::User {
            protect_source(&source.path, output)?;
        }
    }
    Ok(())
}

fn protect_source(input: &Path, output: &Path) -> Result<(), Diagnostic> {
    if source_kind(output).is_some() {
        return Err(driver_error(
            "E2003",
            "an output must not have a '.tz', '.tt', or '.tc' source extension",
        ));
    }
    match fs::symlink_metadata(output) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(driver_error(
                    "E2003",
                    "the output must not be a symlink, directory, or special file",
                ));
            }
            let source = fs::canonicalize(input)
                .map_err(|error| io_error("resolve source", input, error))?;
            let destination = fs::canonicalize(output)
                .map_err(|error| io_error("resolve output", output, error))?;
            if source == destination || same_file(input, output, &metadata)? {
                return Err(driver_error(
                    "E2003",
                    "refusing to overwrite the input source",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("inspect output", output, error)),
    }
    Ok(())
}

#[cfg(unix)]
fn same_file(input: &Path, _output_path: &Path, output: &fs::Metadata) -> Result<bool, Diagnostic> {
    use std::os::unix::fs::MetadataExt;
    let input_metadata =
        fs::metadata(input).map_err(|error| io_error("inspect source", input, error))?;
    Ok(input_metadata.dev() == output.dev() && input_metadata.ino() == output.ino())
}

#[cfg(windows)]
fn same_file(input: &Path, output: &Path, _metadata: &fs::Metadata) -> Result<bool, Diagnostic> {
    same_file::is_same_file(input, output)
        .map_err(|error| io_error("compare file identity", output, error))
}

#[cfg(not(any(unix, windows)))]
fn same_file(
    _input: &Path,
    _output_path: &Path,
    _output: &fs::Metadata,
) -> Result<bool, Diagnostic> {
    // Atomic replacement never modifies the contents of other hard links.
    Ok(false)
}

fn driver_error(code: &'static str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(code, message, Span::default())
}

fn io_error(action: &str, path: &Path, error: io::Error) -> Diagnostic {
    driver_error(
        "E2001",
        format!("cannot {action} '{}': {error}", path.display()),
    )
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TemporaryDirectory {
    pub(crate) path: PathBuf,
    removed: bool,
}

impl TemporaryDirectory {
    pub(crate) fn new(parent: &Path) -> Result<Self, Diagnostic> {
        let parent = fs::canonicalize(parent)
            .map_err(|error| io_error("resolve temporary directory parent", parent, error))?;
        for _ in 0..128 {
            let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".tsuzuri-{}-{id}", std::process::id()));
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        removed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(io_error("create temporary directory", &path, error)),
            }
        }
        Err(driver_error(
            "E2001",
            "could not create a unique temporary directory",
        ))
    }

    pub(crate) fn close(mut self) -> Result<(), Diagnostic> {
        fs::remove_dir_all(&self.path)
            .map_err(|error| io_error("remove temporary directory", &self.path, error))?;
        self.removed = true;
        Ok(())
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if !self.removed {
            if let Err(error) = fs::remove_dir_all(&self.path) {
                if error.kind() != io::ErrorKind::NotFound {
                    eprintln!("warning: cannot remove '{}': {error}", self.path.display());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(sources: &[(&str, &str)], input: &str) -> (TemporaryDirectory, Project) {
        let directory = TemporaryDirectory::new(&env::temp_dir()).unwrap();
        for (name, text) in sources {
            fs::write(directory.path.join(name), text).unwrap();
        }
        let project = Project::load(&directory.path.join(input)).unwrap();
        (directory, project)
    }

    #[test]
    fn script_projects_read_one_file_of_any_name() {
        let directory = TemporaryDirectory::new(&env::temp_dir()).unwrap();
        let script = directory.path.join("greet");
        fs::write(
            &script,
            "#!/usr/bin/env tsuzuri script\nlet value = Json.Null\n0\n",
        )
        .unwrap();
        fs::write(directory.path.join("Broken.tz"), "def broken :: i64 = (\n").unwrap();
        let project = Project::load_script(&script).unwrap();
        assert_eq!(project.input(), script);
        assert_eq!(
            project.sources[project.root].relative_path,
            Path::new("Main.tz")
        );
        let names: Vec<_> = project
            .sources
            .iter()
            .filter(|source| source.origin == ModuleOrigin::User)
            .map(|source| source.name.as_str())
            .collect();
        assert_eq!(names, ["Main"]);
        assert!(project.sources.iter().any(|source| source.name == "Json"));
        assert!(project.analyze_all().is_ok());
        #[cfg(unix)]
        {
            let link = directory.path.join("linked");
            std::os::unix::fs::symlink(&script, &link).unwrap();
            assert_eq!(Project::load_script(&link).unwrap().input(), link);
        }
        let module = Project::load_script(&directory.path.join("Traits.tt")).unwrap_err();
        assert_eq!(module.diagnostic.code, "E2000");
        for missing in [directory.path.join("missing.tz"), directory.path.clone()] {
            assert_eq!(
                Project::load_script(&missing).unwrap_err().diagnostic.code,
                "E2001"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn probable_stack_exhaustion_matches_segv_and_bus() {
        use std::os::unix::process::ExitStatusExt;
        use std::process::ExitStatus;
        assert!(probable_stack_exhaustion(&ExitStatus::from_raw(11)));
        #[cfg(target_os = "linux")]
        assert!(probable_stack_exhaustion(&ExitStatus::from_raw(7)));
        // 7 is SIGEMT on macOS, and SIGBUS is 10 there.
        #[cfg(target_os = "macos")]
        {
            assert!(probable_stack_exhaustion(&ExitStatus::from_raw(10)));
            assert!(!probable_stack_exhaustion(&ExitStatus::from_raw(7)));
        }
        // SIGILL, SIGTRAP, SIGABRT, a normal exit and exit code 1.
        for raw in [4, 5, 6, 0, 1 << 8] {
            assert!(
                !probable_stack_exhaustion(&ExitStatus::from_raw(raw)),
                "{raw}"
            );
        }
    }

    #[test]
    fn docs_publish_protects_unowned_directories_and_sources() {
        let (directory, project) = project(
            &[(
                "Library.tz",
                "/// Answer.\ndef answer :: i64\nfn answer = 42",
            )],
            "Library.tz",
        );
        project.analyze().unwrap();
        let output = directory.path.join("docs");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep.txt"), "user data").unwrap();
        assert_eq!(document(&project, &output).unwrap_err().code, "E2003");
        assert_eq!(
            fs::read_to_string(output.join("keep.txt")).unwrap(),
            "user data"
        );
        fs::write(output.join(".tsuzuri-docs"), "not a generator marker").unwrap();
        assert_eq!(document(&project, &output).unwrap_err().code, "E2003");
        let generated = directory.path.join("generated");
        document(&project, &generated).unwrap();
        let before = fs::read(generated.join("Library.md")).unwrap();
        fs::write(generated.join("stale.md"), "old generated page").unwrap();
        document(&project, &generated).unwrap();
        assert_eq!(fs::read(generated.join("Library.md")).unwrap(), before);
        assert!(!generated.join("stale.md").exists());
        fs::write(
            directory.path.join(".tsuzuri-docs"),
            "generated by tsuzuri doc",
        )
        .unwrap();
        assert_eq!(
            document(&project, &directory.path).unwrap_err().code,
            "E2003"
        );
        assert!(project.input().exists());
        assert!(fs::read_dir(&directory.path).unwrap().all(|entry| {
            let entry = entry.unwrap();
            !entry.file_type().unwrap().is_dir()
                || !entry.file_name().to_string_lossy().starts_with(".tsuzuri-")
        }));
    }

    #[test]
    fn docs_loads_libraries_without_main_and_rejects_page_collisions() {
        let (directory, _) = project(
            &[("Index.tz", "def answer :: i64\nfn answer = 42")],
            "Index.tz",
        );
        let project = Project::load_for_docs(&directory.path).unwrap();
        project.analyze().unwrap();
        let output = directory.path.join("generated");
        assert_eq!(document(&project, &output).unwrap_err().code, "E2003");
        assert!(!output.exists());
    }

    #[test]
    #[cfg(unix)]
    fn docs_refuses_output_and_marker_symlinks() {
        use std::os::unix::fs::symlink;
        let (directory, project) = project(
            &[("Library.tz", "def answer :: i64\nfn answer = 42")],
            "Library.tz",
        );
        let owned = directory.path.join("owned");
        document(&project, &owned).unwrap();
        let linked = directory.path.join("linked");
        symlink(&owned, &linked).unwrap();
        assert_eq!(document(&project, &linked).unwrap_err().code, "E2003");
        let other = directory.path.join("other");
        fs::create_dir(&other).unwrap();
        symlink(owned.join(".tsuzuri-docs"), other.join(".tsuzuri-docs")).unwrap();
        assert_eq!(document(&project, &other).unwrap_err().code, "E2003");
        assert!(owned.join("Library.md").exists());
    }

    #[test]
    fn publishes_text_atomically_and_protects_source() {
        let source = "export fn f() -> i64 { 42 }";
        let (directory, project) = project(&[("Example.tz", source)], "Example.tz");
        let input = project.input();
        let module = project.analyze().unwrap();
        let options = BuildOptions {
            emit: Emit::Llvm,
            ..BuildOptions::default()
        };
        let output = directory.path.join("example.ll");
        fs::write(&output, "old output").unwrap();
        build(&module, &project, &output, options).unwrap();
        assert!(fs::read_to_string(&output).unwrap().contains("@tz_f"));
        assert!(build(&module, &project, input, options).is_err());
        assert_eq!(fs::read_to_string(input).unwrap(), source);
        let error = build(&module, &project, &output, BuildOptions::default()).unwrap_err();
        assert_eq!(error.code, "E2004");
        assert!(fs::read_to_string(&output).unwrap().contains("@tz_f"));
        directory.close().unwrap();
    }

    #[test]
    fn refuses_invalid_options_and_empty_wasm_modules() {
        let (directory, project) = project(&[("F.tz", "fn f() -> i64 { 1 }")], "F.tz");
        let module = project.analyze().unwrap();
        let options = BuildOptions {
            target: Target::Wasm32,
            emit: Emit::Wasm,
            ..BuildOptions::default()
        };
        assert_eq!(
            build(&module, &project, &directory.path.join("f.wasm"), options)
                .unwrap_err()
                .code,
            "E2004"
        );
        assert!(
            BuildOptions {
                optimization: 4,
                ..BuildOptions::default()
            }
            .validate()
            .is_err()
        );
        let bindings = BuildOptions {
            target: Target::Wasm32,
            emit: Emit::BindingsJs,
            ..BuildOptions::default()
        };
        let error = build(&module, &project, &directory.path.join("f.mjs"), bindings).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            (
                "E2004",
                "bindings need at least one 'export def' entry point"
            )
        );
        let error = build(&module, &project, &directory.path.join("f.js"), bindings).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            (
                "E2000",
                "bindings output must end with '.mjs'; declarations are written next to it as '<name>.d.mts'"
            )
        );
        for (options, message) in [
            (
                BuildOptions {
                    target: Target::Native,
                    ..bindings
                },
                "'--emit bindings-js' requires '--target wasm32'",
            ),
            (
                BuildOptions {
                    target: Target::Wasm64,
                    ..bindings
                },
                "'--emit bindings-js' requires '--target wasm32'",
            ),
            (
                BuildOptions {
                    trap_info: true,
                    ..bindings
                },
                "--trap-info is not valid for bindings output; pass it when building the .wasm",
            ),
            (
                BuildOptions {
                    debug_info: true,
                    ..bindings
                },
                "--debug-info is not valid for bindings output; pass it when building the .wasm",
            ),
            (
                BuildOptions {
                    debug_output: true,
                    ..bindings
                },
                "--debug-output is not valid for bindings output; pass it when building the .wasm",
            ),
            (
                BuildOptions {
                    wasm_simd: true,
                    ..bindings
                },
                "--wasm-feature is not valid for bindings output; pass it when building the .wasm",
            ),
            (
                BuildOptions {
                    allocator: llvm::Allocator::Counting,
                    ..bindings
                },
                "--allocator is not valid for bindings output; pass it when building the .wasm",
            ),
        ] {
            let error = options.validate().unwrap_err();
            assert_eq!((error.code, error.message.as_str()), ("E2000", message));
        }
        assert!(bindings.validate().is_ok());
        // `--wasm-feature threads` selects the glue of a thread pool.
        assert!(
            BuildOptions {
                wasm_threads: true,
                ..bindings
            }
            .validate()
            .is_ok()
        );
        assert_eq!(
            bindings_sidecar_path(Path::new("out/api.v1.mjs")),
            Path::new("out/api.v1.d.mts")
        );
        assert!(bindings_output_error(Emit::BindingsJs, Path::new(".mjs")).is_some());
        assert!(bindings_output_error(Emit::Wasm, Path::new("f.js")).is_none());
        directory.close().unwrap();
    }

    #[test]
    fn validates_shared_libraries_and_native_bindings() {
        let shared = BuildOptions {
            emit: Emit::Shared,
            ..BuildOptions::default()
        };
        if cfg!(windows) {
            assert_eq!(
                shared.validate().unwrap_err().message,
                "'--emit shared' is not supported on Windows yet (G10); build an object with --emit object and link it into a DLL"
            );
        } else {
            assert!(shared.validate().is_ok());
            for options in [
                BuildOptions {
                    trap_return: true,
                    trap_info: true,
                    ..shared
                },
                BuildOptions {
                    allocator: llvm::Allocator::Counting,
                    ..shared
                },
                BuildOptions {
                    cpu: Cpu::Native,
                    ..shared
                },
            ] {
                if options.cpu == Cpu::Native && native_cpu_flag(env::consts::ARCH).is_err() {
                    continue;
                }
                assert!(options.validate().is_ok(), "{options:?}");
            }
            assert_eq!(
                BuildOptions {
                    allocator: llvm::Allocator::Host,
                    ..shared
                }
                .validate()
                .unwrap_err()
                .message,
                "--allocator host cannot be combined with --emit shared: a shared library resolves its symbols when it is linked; link the object into the host that defines tsuzuri_host_alloc, tsuzuri_host_free, and tsuzuri_host_realloc"
            );
        }
        assert_eq!(
            BuildOptions {
                target: Target::Wasm32,
                ..shared
            }
            .validate()
            .unwrap_err()
            .message,
            "'--emit shared' requires '--target native'"
        );
        for (emit, kind, extension) in [
            (Emit::BindingsCs, "bindings-cs", "cs"),
            (Emit::BindingsPy, "bindings-py", "py"),
            (Emit::BindingsCpp, "bindings-cpp", "hpp"),
        ] {
            let bindings = BuildOptions {
                emit,
                ..BuildOptions::default()
            };
            assert!(bindings.validate().is_ok());
            assert!(
                BuildOptions {
                    trap_return: true,
                    ..bindings
                }
                .validate()
                .is_ok()
            );
            assert_eq!(
                BuildOptions {
                    target: Target::Wasm32,
                    ..bindings
                }
                .validate()
                .unwrap_err()
                .message,
                format!("'--emit {kind}' requires '--target native'")
            );
            for (options, option) in [
                (
                    BuildOptions {
                        trap_info: true,
                        ..bindings
                    },
                    "--trap-info",
                ),
                (
                    BuildOptions {
                        debug_info: true,
                        ..bindings
                    },
                    "--debug-info",
                ),
                (
                    BuildOptions {
                        allocator: llvm::Allocator::Counting,
                        ..bindings
                    },
                    "--allocator",
                ),
            ] {
                assert_eq!(
                    options.validate().unwrap_err().message,
                    format!(
                        "{option} is not valid for bindings output; pass it when building the shared library"
                    )
                );
            }
            assert_eq!(
                bindings.output_path(Path::new("dir/Main.tz")),
                Path::new(&format!("dir/Main.{extension}"))
            );
            assert!(bindings_output_error(emit, Path::new(&format!("lib.{extension}"))).is_none());
            assert_eq!(
                bindings_output_error(emit, Path::new("lib.txt"))
                    .unwrap()
                    .message,
                format!(
                    "bindings output must end with '.{extension}'; its file name, without the extension, names the shared library"
                )
            );
        }
    }

    #[test]
    fn shared_libraries_export_the_public_entry_points_the_ir_defines() {
        let (directory, project) = project(
            &[(
                "Main.tz",
                "export def add :: i64 -> i64 -> i64\nfn add a b = a + b\nexport def copy :: ref [i64] -> [i64]\nfn copy values = Array.map (value -> value) values\n",
            )],
            "Main.tz",
        );
        let module = project.analyze().unwrap();
        let ir = "define i64 @tz_add(i64 %arg0) {\ndefine weak ptr @tsuzuri_alloc(i64 %size) nounwind {\ndefine weak void @tsuzuri_free(ptr %value) nounwind {\ndefine weak hidden void @tz_soft_op(ptr %0) {\ndefine internal ptr @tz.alloc(i64 %size) {\n";
        assert_eq!(
            shared_exports(ir, &module, false),
            ["tsuzuri_alloc", "tsuzuri_free", "tz_add", "tz_copy"]
        );
        assert_eq!(
            shared_exports(ir, &module, true),
            [
                "tsuzuri_alloc",
                "tsuzuri_free",
                "tsuzuri_try_add",
                "tsuzuri_try_copy",
                "tz_add",
                "tz_copy"
            ]
        );
        directory.close().unwrap();
    }

    #[test]
    fn wasm_memory_limits_accept_boundaries_and_reject_invalid_values() {
        let wasm32 = |max, stack| wasm_memory_limits(Target::Wasm32, max, stack);
        let wasm64 = |max, stack| wasm_memory_limits(Target::Wasm64, max, stack);
        assert_eq!(wasm32(None, None).unwrap(), (16777216, 1048576));
        assert_eq!(wasm64(None, None).unwrap(), (16777216, 1048576));
        assert_eq!(
            wasm32(Some(4294901760), None).unwrap(),
            (4294901760, 1048576)
        );
        assert_eq!(
            wasm64(Some(17179869184), Some(4294967296)).unwrap(),
            (17179869184, 4294967296)
        );
        assert_eq!(wasm32(Some(131072), Some(65536)).unwrap(), (131072, 65536));
        let memory =
            "--wasm-max-memory must be a multiple of 64 KiB and at most 4 GiB - 64 KiB on wasm32";
        let memory64 =
            "--wasm-max-memory must be a multiple of 64 KiB and at most 16 GiB on wasm64";
        let stack = "--wasm-stack-size must be a multiple of 16 bytes and at least 64 KiB";
        let relation = "--wasm-max-memory must be at least the stack size plus 64 KiB; raise the memory limit or lower --wasm-stack-size";
        for (target, max, size, message) in [
            (Target::Wasm32, Some(4294967296), None, memory),
            (Target::Wasm32, Some(17179869184), None, memory),
            (Target::Wasm32, Some(100000), None, memory),
            (Target::Wasm64, Some(17179934720), None, memory64),
            (Target::Wasm64, Some(100000), None, memory64),
            (Target::Wasm32, Some(0), None, relation),
            (Target::Wasm32, Some(1048576), None, relation),
            (Target::Wasm32, Some(131072), Some(65552), relation),
            (Target::Wasm32, None, Some(16777216), relation),
            (Target::Wasm64, None, Some(u64::MAX - 15), relation),
            (Target::Wasm32, None, Some(1000), stack),
            (Target::Wasm32, None, Some(32768), stack),
            (Target::Wasm64, Some(100000), Some(1000), stack),
        ] {
            let error = wasm_memory_limits(target, max, size).unwrap_err();
            assert_eq!(
                (error.code, error.message.as_str()),
                ("E2000", message),
                "{target:?} {max:?} {size:?}"
            );
        }
    }

    #[test]
    fn stack_checks_cover_threads_and_wasm32_memory_above_2_gib() {
        for (target, threads, max, checked) in [
            (Target::Wasm32, false, DEFAULT_WASM_MAX_MEMORY, false),
            (Target::Wasm32, false, 1 << 31, false),
            (Target::Wasm32, false, (1 << 31) + 65536, true),
            (Target::Wasm32, true, DEFAULT_WASM_MAX_MEMORY, true),
            (Target::Wasm64, false, 1 << 34, false),
            (Target::Native, false, DEFAULT_WASM_MAX_MEMORY, false),
        ] {
            assert_eq!(
                wasm_stack_checks(target, threads, max),
                checked,
                "{target:?} {threads} {max}"
            );
        }
    }

    #[test]
    fn manifest_wasm_fills_only_unset_options_for_applicable_outputs() {
        let manifest = crate::package::WasmSettings {
            max_memory: Some(268435456),
            stack_size: Some(4194304),
        };
        let options = |target, emit, max| BuildOptions {
            target,
            emit,
            wasm_max_memory: max,
            ..BuildOptions::default()
        };
        for (target, emit, cli, max, stack) in [
            (
                Target::Wasm32,
                Emit::Wasm,
                None,
                Some(268435456),
                Some(4194304),
            ),
            (
                Target::Wasm64,
                Emit::Wasm,
                None,
                Some(268435456),
                Some(4194304),
            ),
            (
                Target::Wasm32,
                Emit::Wasm,
                Some(65536 * 1024),
                Some(65536 * 1024),
                Some(4194304),
            ),
            (Target::Wasm32, Emit::Object, None, Some(268435456), None),
            (Target::Wasm64, Emit::Llvm, None, Some(268435456), None),
            (Target::Wasm32, Emit::Header, None, None, None),
            (Target::Native, Emit::Executable, None, None, None),
            (Target::Native, Emit::Llvm, None, None, None),
        ] {
            let merged = options(target, emit, cli).with_manifest_wasm(manifest);
            assert_eq!(
                (merged.wasm_max_memory, merged.wasm_stack_size),
                (max, stack),
                "{target:?} {emit:?}"
            );
            merged.validate().unwrap();
        }
    }

    #[test]
    fn validate_rejects_wasm_memory_options_outside_wasm_outputs() {
        let memory = Some(67108864);
        let stack = Some(2097152);
        let options = |target, emit, wasm_max_memory, wasm_stack_size, wasm_threads| BuildOptions {
            target,
            emit,
            wasm_max_memory,
            wasm_stack_size,
            wasm_threads,
            ..BuildOptions::default()
        };
        let limit = "--wasm-max-memory requires wasm32 or wasm64 object, LLVM IR, or WASM output";
        let size = "--wasm-stack-size requires WASM output; link object and LLVM IR output with wasm-ld -z stack-size";
        for (target, emit, max, stack, message) in [
            (Target::Native, Emit::Executable, memory, None, limit),
            (Target::Native, Emit::Object, memory, None, limit),
            (Target::Native, Emit::Llvm, memory, None, limit),
            (Target::Native, Emit::Wgsl, memory, None, limit),
            (Target::Wasm32, Emit::Header, memory, None, limit),
            (Target::Wasm64, Emit::Header, memory, None, limit),
            (Target::Native, Emit::Executable, None, stack, size),
            (Target::Wasm32, Emit::Object, None, stack, size),
            (Target::Wasm32, Emit::Llvm, None, stack, size),
            (Target::Wasm64, Emit::Llvm, None, stack, size),
            (Target::Wasm32, Emit::Header, None, stack, size),
            (Target::Wasm32, Emit::Object, memory, stack, size),
        ] {
            let error = options(target, emit, max, stack, false)
                .validate()
                .unwrap_err();
            assert_eq!(
                (error.code, error.message.as_str()),
                ("E2000", message),
                "{target:?} {emit:?}"
            );
        }
        for (emit, max, stack, threads) in [
            (Emit::Object, memory, None, false),
            (Emit::Llvm, memory, None, false),
            (Emit::Wasm, memory, stack, false),
            (Emit::Wasm, None, stack, false),
            (Emit::Wasm, memory, stack, true),
            (Emit::Object, memory, None, true),
        ] {
            options(Target::Wasm32, emit, max, stack, threads)
                .validate()
                .unwrap();
            if !threads {
                options(Target::Wasm64, emit, max, stack, threads)
                    .validate()
                    .unwrap();
            }
        }
        assert_eq!(
            options(Target::Wasm64, Emit::Wasm, None, None, true)
                .validate()
                .unwrap_err()
                .message,
            "--wasm-feature threads requires wasm32 object or WASM output"
        );
        assert_eq!(
            options(Target::Wasm32, Emit::Wasm, Some(1048576), None, false)
                .validate()
                .unwrap_err()
                .code,
            "E2000"
        );
    }

    #[test]
    fn validate_limits_trap_mode_return_to_native_object_llvm_and_header_output() {
        let options = |target, emit| BuildOptions {
            target,
            emit,
            trap_return: true,
            trap_info: emit != Emit::Header,
            ..BuildOptions::default()
        };
        for emit in [Emit::Object, Emit::Llvm, Emit::Header] {
            options(Target::Native, emit).validate().unwrap();
        }
        let message = "--trap-mode return is only valid for native object, llvm or header output";
        for (target, emit) in [
            (Target::Native, Emit::Executable),
            (Target::Native, Emit::Wgsl),
            (Target::Wasm32, Emit::Object),
            (Target::Wasm32, Emit::Llvm),
            (Target::Wasm32, Emit::Wasm),
            (Target::Wasm64, Emit::Object),
        ] {
            let error = options(target, emit).validate().unwrap_err();
            assert_eq!(error.code, "E2000", "{target:?} {emit:?}");
            assert!(
                error.message == message || emit == Emit::Wgsl,
                "{target:?} {emit:?}: {}",
                error.message
            );
        }
    }

    #[test]
    fn link_inputs_reach_the_msvc_linker_as_libpath_and_lib_names() {
        let links = LinkInputs {
            paths: vec![PathBuf::from("host.obj")],
            libraries: vec!["sqlite3".into()],
            search: vec![PathBuf::from("libs")],
        };
        for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
            let mut clang = Command::new(tool("TSUZURI_CLANG", "clang"));
            clang.args(["-###", &format!("--target={target}"), "-o", "a.exe"]);
            links.add_to(&mut clang);
            // A dry run shows the linker line without needing the Windows SDK; without Clang there is nothing to check.
            let Ok(output) = clang.output() else { return };
            let log = String::from_utf8_lossy(&output.stderr);
            if !log.contains("link.exe") && !log.contains("lld-link") {
                return;
            }
            assert!(log.contains("-libpath:libs"), "{target}: {log}");
            assert!(log.contains("\"sqlite3.lib\""), "{target}: {log}");
            assert!(log.contains("host.obj"), "{target}: {log}");
        }
    }

    #[test]
    fn stack_runtime_writes_the_report_the_driver_looks_for() {
        let source = include_str!("runtime/stack.c");
        assert!(source.contains(&format!("\"{STACK_OVERFLOW_REPORT}\\n\"")));
        assert!(source.contains("tsuzuri_stack_thread"));
        assert!(include_str!("runtime/task.c").contains("tsuzuri_stack_thread();"));
    }

    #[test]
    fn pdb_links_write_a_named_pdb_and_embed_the_natvis_views() {
        assert_eq!(pdb_path(Path::new("out/app.exe")), Path::new("out/app.pdb"));
        assert_eq!(
            pdb_path(Path::new("out/runner")),
            Path::new("out/runner.pdb")
        );
        let temporary = TemporaryDirectory::new(&env::temp_dir()).unwrap();
        let staged = temporary.path.join("artifact.pdb");
        let arguments =
            pdb_link_args(&staged, Path::new("a, b/my app.pdb"), &temporary.path).unwrap();
        let natvis = temporary.path.join("tsuzuri.natvis");
        let expected: Vec<OsString> = [
            "-Xlinker".into(),
            format!("/PDB:{}", staged.display()),
            "-Xlinker".into(),
            "/PDBALTPATH:my app.pdb".into(),
            "-Xlinker".into(),
            format!("/NATVIS:{}", natvis.display()),
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert_eq!(arguments, expected);
        assert_eq!(
            fs::read_to_string(&natvis).unwrap(),
            include_str!("runtime/tsuzuri.natvis")
        );
        // Only Windows links with the MSVC linker.
        assert!(cfg!(windows) || !msvc_linker());
        temporary.close().unwrap();
    }

    #[test]
    fn task_runtime_source_inlines_the_win32_adapter() {
        let include = "#include \"task-windows.h\"";
        assert!(include_str!("runtime/task.c").contains(include));
        let source = task_runtime_source();
        assert_eq!(source.contains(include), !cfg!(windows));
        assert_eq!(source.contains("TZ_TASK_WINDOWS_H"), cfg!(windows));
    }

    #[test]
    fn validates_native_cpu_tuning_and_selects_architecture_flags() {
        assert_eq!(
            native_compile_args(true, "x86_64"),
            ["--target=x86_64-pc-windows-msvc"]
        );
        assert_eq!(
            native_compile_args(true, "aarch64"),
            ["--target=aarch64-pc-windows-msvc"]
        );
        assert_eq!(native_compile_args(false, "x86_64"), ["-fPIC"]);
        assert_eq!(native_compile_args(false, "aarch64"), ["-fPIC"]);
        assert_eq!(BuildOptions::default().cpu, Cpu::Generic);
        for architecture in ["x86", "x86_64"] {
            assert_eq!(native_cpu_flag(architecture).unwrap(), "-march=native");
        }
        for architecture in ["arm", "aarch64"] {
            assert_eq!(native_cpu_flag(architecture).unwrap(), "-mcpu=native");
        }
        assert!(native_cpu_flag("unknown").is_err());
        for (target, emit) in [
            (Target::Wasm32, Emit::Wasm),
            (Target::Wasm32, Emit::Object),
            (Target::Wasm64, Emit::Wasm),
            (Target::Native, Emit::Llvm),
            (Target::Native, Emit::Header),
        ] {
            assert!(
                BuildOptions {
                    target,
                    emit,
                    cpu: Cpu::Native,
                    ..BuildOptions::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn loads_sorted_sibling_modules_and_selects_main_for_directories() {
        let (directory, project) = project(
            &[
                ("Zebra.tz", "fn value() -> i64 { 2 }"),
                ("Main.tz", "Alpha.value() + Zebra.value()"),
                ("Alpha.tz", "fn value() -> i64 { 40 }"),
                ("ignored.txt", "not a Tsuzuri module"),
                ("ignored.tsz", "not a Tsuzuri module"),
            ],
            "",
        );
        fs::create_dir(directory.path.join(".hidden")).unwrap();
        fs::write(directory.path.join(".hidden/Hidden.tz"), "invalid").unwrap();
        assert_eq!(project.input(), directory.path.join("Main.tz"));
        assert_eq!(
            project
                .sources
                .iter()
                .filter(|source| source.origin == ModuleOrigin::User)
                .map(|source| source.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "Main", "Zebra"]
        );
        // The embedded standard library follows the user sources, so user
        // source indices and the root are unchanged.
        assert_eq!(project.root, 1);
        let std: Vec<_> = project
            .sources
            .iter()
            .skip_while(|source| source.origin == ModuleOrigin::User)
            .collect();
        // The sources name no opt-in std module, so only the others are loaded (D-40).
        assert_eq!(
            std.len(),
            crate::stdlib::SOURCES.len() - crate::stdlib::OPT_IN.len()
        );
        assert_eq!(std.len(), crate::stdlib::sources_for(["42"]).len());
        assert!(
            std.iter()
                .all(|source| source.origin == ModuleOrigin::Std && source.path.starts_with("std"))
        );
        // The editor loads every std module.
        let edited = Project::load_with_overlays(&directory.path, &Default::default()).unwrap();
        assert_eq!(
            edited
                .sources
                .iter()
                .filter(|source| source.origin == ModuleOrigin::Std)
                .count(),
            crate::stdlib::SOURCES.len()
        );
        let first = llvm::emit(&project.analyze().unwrap(), Entry::Console).unwrap();
        let reloaded = Project::load(&directory.path.join("Main.tz")).unwrap();
        assert_eq!(
            first,
            llvm::emit(&reloaded.analyze().unwrap(), Entry::Console).unwrap()
        );
        let wrong_case = Project::load(&directory.path.join("MAIN.tz")).unwrap_err();
        assert!(matches!(wrong_case.diagnostic.code, "E2001" | "E1011"));
        let library = Project::load(&directory.path.join("Alpha.tz")).unwrap();
        assert_eq!(
            build(
                &library.analyze().unwrap(),
                &library,
                &directory.path.join("not-main"),
                BuildOptions::default(),
            )
            .unwrap_err()
            .code,
            "E2004"
        );
        directory.close().unwrap();
    }

    #[test]
    fn loads_all_source_kinds_and_rejects_old_extensions_and_colliding_stems() {
        let (directory, project) = project(
            &[
                ("Main.tz", "Identity { return 42 }"),
                (
                    "Identity.tc",
                    "def Return :: 'a -> 'a\nfn Return value = value",
                ),
                (
                    "Classes.tt",
                    "class Score<'a> { def score :: 'a -> i64 }\nclass Size<'a> { def size :: 'a -> i64 }",
                ),
                ("Ignored.tzr", "not a supported source"),
            ],
            "",
        );
        assert_eq!(
            project
                .sources
                .iter()
                .filter(|source| source.origin == ModuleOrigin::User)
                .map(|source| source.name.as_str())
                .collect::<Vec<_>>(),
            ["Classes", "Identity", "Main"]
        );
        let ir = llvm::emit(&project.analyze().unwrap(), Entry::Console).unwrap();
        assert!(ir.contains("@tz.fn.Identity.Return"));
        for name in ["Classes.tt", "Identity.tc"] {
            let library = Project::load(&directory.path.join(name)).unwrap();
            let module = library.analyze().unwrap();
            assert_eq!(ir, llvm::emit(&module, Entry::Console).unwrap());
            for extension in ["tz", "tt", "tc"] {
                let output = directory.path.join(format!("Output.{extension}"));
                let options = BuildOptions {
                    emit: Emit::Llvm,
                    ..BuildOptions::default()
                };
                assert_eq!(
                    build(&module, &library, &output, options).unwrap_err().code,
                    "E2003"
                );
                assert!(!output.exists());
            }
            assert_eq!(
                build(
                    &module,
                    &library,
                    &directory.path.join("not-main"),
                    BuildOptions::default(),
                )
                .unwrap_err()
                .code,
                "E2004"
            );
        }
        let old = Project::load(&directory.path.join("Ignored.tzr")).unwrap_err();
        assert_eq!(old.diagnostic.code, "E2000");
        assert!(old.diagnostic.message.contains("rename"));
        fs::write(directory.path.join("Identity.tz"), "").unwrap();
        let collision = Project::load(&directory.path)
            .unwrap()
            .analyze()
            .unwrap_err();
        assert_eq!(collision.code, "E1011");
        directory.close().unwrap();
    }

    #[test]
    fn reports_errors_in_type_class_and_computation_files() {
        let (directory, project) = project(
            &[
                ("Main.tz", "Identity { return 42 }"),
                (
                    "Identity.tc",
                    "def Return :: i64 -> i64\nfn Return value = false",
                ),
            ],
            "",
        );
        let error = project.analyze().unwrap_err();
        assert_eq!(error.code, "E1003");
        assert_eq!(
            project.source_for(&error).path,
            directory.path.join("Identity.tc")
        );
        fs::write(
            directory.path.join("Identity.tc"),
            "def Return :: i64 -> i64\nfn Return value = value",
        )
        .unwrap();
        fs::write(
            directory.path.join("Classes.tt"),
            "class Wrong<'a> { def value :: 'b -> 'b }",
        )
        .unwrap();
        let project = Project::load(&directory.path).unwrap();
        let error = project.analyze().unwrap_err();
        assert_eq!(error.code, "E1016");
        assert_eq!(
            project.source_for(&error).path,
            directory.path.join("Classes.tt")
        );
        directory.close().unwrap();
    }

    #[test]
    fn reports_the_source_file_for_module_errors() {
        let (directory, project) = project(
            &[
                ("Main.tz", "Other.value()"),
                ("Other.tz", "// other module\nfn value() -> i64 { false }"),
            ],
            "Main.tz",
        );
        let error = project.analyze().unwrap_err();
        assert_eq!(error.code, "E1003");
        assert_eq!(
            project.source_for(&error).path,
            directory.path.join("Other.tz")
        );
        fs::write(directory.path.join("Other.tz"), [0xff]).unwrap();
        let error = Project::load(&directory.path).unwrap_err();
        assert_eq!(error.diagnostic.code, "E2001");
        assert_eq!(error.path, directory.path.join("Other.tz"));
        fs::remove_file(directory.path.join("Main.tz")).unwrap();
        let error = Project::load(&directory.path).unwrap_err();
        assert_eq!(error.path, directory.path.join("Main.tz"));
        directory.close().unwrap();
        assert!(
            BuildOptions {
                target: Target::Wasm32,
                ..BuildOptions::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn reports_std_errors_at_their_virtual_path() {
        let (directory, mut project) = project(&[("Main.tz", "0")], "Main.tz");
        project.sources.push(SourceFile {
            path: PathBuf::from("std/Broken.tz"),
            relative_path: PathBuf::from("std/Broken.tz"),
            name: "Broken".to_owned(),
            text: "def broken :: i64\nfn broken = false".to_owned(),
            origin: ModuleOrigin::Std,
            package: None,
            namespace: String::new(),
        });
        let error = project.analyze().unwrap_err();
        assert_eq!(error.code, "E1003");
        assert_eq!(project.source_for(&error).path, Path::new("std/Broken.tz"));
        assert_eq!(project.input(), directory.path.join("Main.tz"));
        directory.close().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_hardlinks_and_symlinks() {
        use std::os::unix::fs::symlink;
        let directory = TemporaryDirectory::new(&env::temp_dir()).unwrap();
        let input = directory.path.join("Source.tz");
        let output = directory.path.join("alias.ll");
        let link = directory.path.join("symlink.ll");
        fs::write(&input, "source").unwrap();
        fs::hard_link(&input, &output).unwrap();
        symlink(&input, &link).unwrap();
        assert!(protect_source(&input, &output).is_err());
        assert!(protect_source(&input, &link).is_err());
        directory.close().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn protects_every_module_from_output_aliases() {
        let (directory, project) = project(
            &[
                ("Main.tz", "Other.value()"),
                ("Other.tz", "fn value() -> i64 { 42 }"),
                ("Traits.tt", "class Score<'a> { def score :: 'a -> i64 }"),
                (
                    "Builder.tc",
                    "def Return :: 'a -> 'a\nfn Return value = value",
                ),
            ],
            "Main.tz",
        );
        let module = project.analyze().unwrap();
        let other = directory.path.join("Other.tz");
        let options = BuildOptions {
            emit: Emit::Llvm,
            ..BuildOptions::default()
        };
        for name in ["Other.tz", "Traits.tt", "Builder.tc"] {
            let input = directory.path.join(name);
            let alias = directory.path.join(format!("{name}.ll"));
            fs::hard_link(&input, &alias).unwrap();
            for output in [&input, &alias] {
                assert_eq!(
                    build(&module, &project, output, options).unwrap_err().code,
                    "E2003"
                );
            }
        }
        assert_eq!(
            fs::read_to_string(other).unwrap(),
            "fn value() -> i64 { 42 }"
        );
        directory.close().unwrap();
    }
}
