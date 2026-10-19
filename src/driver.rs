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
use crate::syntax::{Edition, MAX_SOURCE_BYTES, SourceKind};
use crate::target::{ForeignRunner, PlatformScope};
pub use crate::target::{Platform, Triple};

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

#[path = "profile.rs"]
mod profile;
pub use profile::Profile;

/// The target attributes and the checks of `--emit bitcode` (PR08).
pub mod lto;

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
/// Why the native reactor of `Async.block_on` (`src/runtime/async.c`, which `Platform::reactor` says
/// has an implementation) is missing on the machine the compiler runs on.
pub(crate) const ASYNC_NATIVE_MESSAGE: &str =
    "Async.block_on is only available on macOS, Linux, and Windows; use Async.run on this platform";
/// Why wasm output cannot reach the sockets of the standard Net module (E09).
pub(crate) const NET_WASM_MESSAGE: &str = "wasm32 output cannot use the Net socket API because the default wasm32 target has no host imports; build for the native target, build with --wasm-feature jspi --wasm-feature net and load the module with the generated JavaScript bindings on Node.js, or keep to Net address parsing, which needs no host";
/// Why a build for another host cannot reach the Net sockets (`src/runtime/net.c`, which
/// `Platform::net` says has an implementation: POSIX sockets on macOS and Linux, Winsock on Windows).
pub(crate) const NET_NATIVE_MESSAGE: &str =
    "the Net socket API is only available on macOS, Linux, and Windows; build on one of them";
/// Why a program that starts a Net async operation needs the reactor of `Async.block_on` natively.
pub(crate) const NET_ASYNC_MESSAGE: &str = "the Net async operations (connect_async, accept_async, read_async, ...) complete through the reactor of Async.block_on; run the computation with Async.block_on";

/// Why the operating-system APIs are missing: the Windows message, or the one that names a target.
pub(crate) fn os_message(platform: Platform) -> String {
    match platform.triple() {
        Some(triple) if !platform.windows() => format!(
            "the File, Dir, Env, Time, Random, and Process operating-system APIs are not available for {}: src/runtime/os.c is written for the headers of macOS, Linux, and FreeBSD",
            triple.name()
        ),
        // A named Windows target may be built on any machine, so "build on macOS or Linux" would
        // send someone who already is on one back to where they started.
        Some(_) => OS_WINDOWS_MESSAGE.replace(
            "build on macOS or Linux",
            "build for macOS, Linux, or FreeBSD",
        ),
        None => OS_WINDOWS_MESSAGE.to_owned(),
    }
}

/// Why the Net sockets are missing: the host message, or the one that names a target.
pub(crate) fn net_message(platform: Platform) -> String {
    match platform.triple() {
        None => NET_NATIVE_MESSAGE.to_owned(),
        Some(triple) => format!(
            "the Net socket API is only available for macOS, Linux, and Windows targets; {} has no implementation of it",
            triple.name()
        ),
    }
}

/// Why `Async.block_on` is missing: the host message, or the one that names a target.
pub(crate) fn async_message(platform: Platform) -> String {
    match platform.triple() {
        None => ASYNC_NATIVE_MESSAGE.to_owned(),
        Some(triple) => format!(
            "Async.block_on is only available for macOS, Linux, and Windows targets; use Async.run for {}",
            triple.name()
        ),
    }
}

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
    /// `--emit wgsl-relaxed` (F09): the WGSL of a kernel whose floating-point operations follow
    /// WGSL's freedoms. Its first line names the contract; `Emit::Wgsl` stays strict.
    WgslRelaxed,
    /// `--emit spirv` (F09 Phase 3): the Vulkan compute module, a SPIR-V binary, of the one exported
    /// kernel with the CPU reference's strict semantics. `Gpu.map` on a Vulkan device runs this module.
    Spirv,
    /// `--emit spirv-relaxed` (F09 Phase 3): the module of a `Gpu.map_relaxed` kernel, whose
    /// floating-point operations follow the device's own rules.
    SpirvRelaxed,
    /// `--emit bindings-js` (E13): a JavaScript module with TypeScript declarations for a wasm32
    /// module of the same sources.
    BindingsJs,
    /// `--emit shared` (E13 Phase 2): a native shared library that exports the public C ABI.
    Shared,
    /// `--emit bitcode` (PR08): one ThinLTO bitcode file of the program and the runtime it needs, which a
    /// host links with `-flto=thin` so that its calls into the program can inline.
    Bitcode,
    /// `--emit bindings-cs` (E13 Phase 2): C# `LibraryImport` bindings of the shared library.
    BindingsCs,
    /// `--emit bindings-py` (E13 Phase 2): a Python `ctypes` module for the shared library.
    BindingsPy,
    /// `--emit bindings-cpp` (E13 Phase 2): a header-only C++20 wrapper over the C header.
    BindingsCpp,
}

impl Emit {
    /// WGSL output, strict or relaxed (F09).
    pub fn is_wgsl(self) -> bool {
        matches!(self, Self::Wgsl | Self::WgslRelaxed)
    }

    /// SPIR-V output, strict or relaxed (F09 Phase 3).
    pub fn is_spirv(self) -> bool {
        matches!(self, Self::Spirv | Self::SpirvRelaxed)
    }

    /// The kernel of the one exported function as WGSL or SPIR-V, which needs no target, CPU, or
    /// tool options.
    pub fn is_kernel(self) -> bool {
        self.is_wgsl() || self.is_spirv()
    }

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

/// The LLVM optimization level of a build: `-O0` to `-O3` and the size levels `-Os` and `-Oz`.
/// None of them enables fast-math or any other change of the language's meaning. The size levels
/// change what Clang does with the IR, and `-O0`, `-Os` and `-Oz` leave out the one attribute that
/// forces the inlining of known closures on native targets (`llvm::without_closure_inlining`);
/// `build` and `run` accept them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptLevel {
    O0,
    O1,
    O2,
    O3,
    Os,
    Oz,
}

impl OptLevel {
    /// The Clang option of this level.
    pub fn flag(self) -> &'static str {
        match self {
            Self::O0 => "-O0",
            Self::O1 => "-O1",
            Self::O2 => "-O2",
            Self::O3 => "-O3",
            Self::Os => "-Os",
            Self::Oz => "-Oz",
        }
    }

    /// The level of a `-O0` to `-O3`, `-Os` or `-Oz` argument.
    pub fn from_flag(flag: &str) -> Option<Self> {
        [Self::O0, Self::O1, Self::O2, Self::O3, Self::Os, Self::Oz]
            .into_iter()
            .find(|level| level.flag() == flag)
    }

    /// The level as the number `test` and `bench` take; the size levels have none.
    pub fn speed_level(self) -> Option<u8> {
        match self {
            Self::O0 => Some(0),
            Self::O1 => Some(1),
            Self::O2 => Some(2),
            Self::O3 => Some(3),
            Self::Os | Self::Oz => None,
        }
    }

    /// The level of `-O<digit>`, for a digit from 0 to 3.
    pub fn from_speed_level(level: u8) -> Option<Self> {
        match level {
            0 => Some(Self::O0),
            1 => Some(Self::O1),
            2 => Some(Self::O2),
            3 => Some(Self::O3),
            _ => None,
        }
    }
}

/// The code generator of a native build (`--backend`, PB05): Clang over the module's LLVM IR, or the
/// built-in Cranelift translator of the `-O0` IR for a fast edit-build-run loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    #[default]
    Llvm,
    Fast,
}

#[derive(Clone, Copy, Debug)]
pub struct BuildOptions {
    pub target: Target,
    pub emit: Emit,
    pub optimization: OptLevel,
    /// `--backend`: part of the build cache key through the `Debug` text of the options.
    pub backend: Backend,
    pub cpu: Cpu,
    pub debug_output: bool,
    pub trap_info: bool,
    pub debug_info: bool,
    pub wasm_simd: bool,
    pub wasm_threads: bool,
    /// `--wasm-feature jspi`: `Async.block_on` waits through JavaScript Promise Integration (B08).
    pub wasm_jspi: bool,
    /// `--wasm-feature webgpu`: `Gpu.request Gpu.WebGpu` and the kernels on that device call the
    /// JSPI imports `tsuzuri_gpu.open` and `tsuzuri_gpu.run` (F09 Phase 2).
    pub wasm_webgpu: bool,
    /// `--wasm-feature net`: the Net sockets are imports of `tsuzuri_net`, which the generated
    /// JavaScript bindings implement on Node.js (E09 Phase 3).
    pub wasm_net: bool,
    /// `--wasm-feature tail-call`: the calls that the language guarantees to be tail calls are
    /// WebAssembly `return_call` instructions, so a recursion of several functions runs in the
    /// stack of one call (PM09). Part of the build cache key through the `Debug` text.
    pub wasm_tail_call: bool,
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
    /// `--strip`: a native executable without local symbols (PM08).
    pub strip: bool,
    /// `--wasm-opt`: run Binaryen's `wasm-opt -Oz` over the linked `.wasm` (PM08). The tool is never
    /// looked for unless this is set.
    pub wasm_opt: bool,
    /// A named native target (`--target TRIPLE`); `None` is the machine the compiler runs on and
    /// every non-native target.
    pub triple: Option<Triple>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            emit: Emit::Executable,
            optimization: OptLevel::O3,
            backend: Backend::Llvm,
            cpu: Cpu::Generic,
            debug_output: false,
            trap_info: false,
            debug_info: false,
            wasm_simd: false,
            wasm_threads: false,
            wasm_jspi: false,
            wasm_webgpu: false,
            wasm_net: false,
            wasm_tail_call: false,
            wasm_host: None,
            wasm_max_memory: None,
            wasm_stack_size: None,
            cache: true,
            trap_return: false,
            allocator: llvm::Allocator::System,
            freestanding: false,
            strip: false,
            wasm_opt: false,
            triple: None,
        }
    }
}

impl BuildOptions {
    /// Where the output runs: the named target, or the machine the compiler runs on.
    pub fn platform(self) -> Platform {
        Platform::new(self.triple)
    }

    /// An `--emit` that its `--target` cannot give, with any backend and on any host: `exe`,
    /// `shared` and the bindings of a library need the native target, `wasm` and `bindings-js` a
    /// WebAssembly one. These are the first checks of the validation of each of them.
    fn emit_needs_another_target(self) -> bool {
        match self.emit {
            Emit::Executable
            | Emit::Shared
            | Emit::BindingsCs
            | Emit::BindingsPy
            | Emit::BindingsCpp => self.target != Target::Native,
            Emit::Wasm => !self.target.is_wasm(),
            Emit::BindingsJs => self.target != Target::Wasm32,
            _ => false,
        }
    }

    pub fn validate(self) -> Result<(), Diagnostic> {
        if self.backend == Backend::Fast {
            // The advice of the fast backend's errors is `--backend llvm`, which would give the error
            // of a combination that no backend builds, so that error is the one reported.
            if self.emit_needs_another_target() {
                Self {
                    backend: Backend::Llvm,
                    ..self
                }
                .validate()?;
            }
            self.validate_fast()?;
        }
        if self.emit.is_kernel()
            && (self.target != Target::Native
                || self.triple.is_some()
                || self.cpu != Cpu::Generic
                || self.debug_info
                || self.debug_output
                || self.trap_info
                || self.wasm_simd
                || self.wasm_threads
                || self.wasm_jspi
                || self.wasm_webgpu
                || self.wasm_net
                || self.wasm_tail_call)
        {
            return Err(driver_error(
                "E2000",
                if self.emit.is_spirv() {
                    "SPIR-V output does not use target, CPU, debug, or WASM feature options"
                } else {
                    "WGSL output does not use target, CPU, debug, or WASM feature options"
                },
            ));
        }
        if self.emit.is_bindings() {
            self.validate_bindings()?;
        }
        if let Some(triple) = self.triple {
            self.validate_triple(triple)?;
        }
        if self.emit == Emit::Shared {
            self.validate_shared()?;
        }
        if self.emit == Emit::Bitcode {
            self.validate_bitcode()?;
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
        if self.wasm_webgpu
            && (self.target != Target::Wasm32
                || !matches!(self.emit, Emit::Wasm | Emit::Object | Emit::Llvm))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-feature webgpu requires wasm32 object, LLVM IR, or WASM output; the JavaScript bindings do not provide the WebGPU imports, so instantiate the module with createGpuImports of src/runtime/webgpu.mjs",
            ));
        }
        if self.wasm_webgpu && (self.wasm_threads || self.wasm_host.is_some()) {
            return Err(driver_error(
                "E2000",
                "--wasm-feature webgpu cannot be combined with --wasm-feature threads or --wasm-host: its imports suspend the WebAssembly stack with JavaScript Promise Integration",
            ));
        }
        if self.wasm_net
            && (self.target != Target::Wasm32
                || !matches!(
                    self.emit,
                    Emit::Wasm | Emit::Object | Emit::Llvm | Emit::BindingsJs
                ))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-feature net requires wasm32 object, LLVM IR, WASM, or JavaScript bindings output",
            ));
        }
        if self.wasm_net && !self.wasm_jspi {
            return Err(driver_error(
                "E2000",
                "--wasm-feature net requires --wasm-feature jspi: a blocking socket call suspends the WebAssembly stack with JavaScript Promise Integration while the host works",
            ));
        }
        if self.wasm_net && (self.wasm_threads || self.wasm_host.is_some()) {
            return Err(driver_error(
                "E2000",
                "--wasm-feature net cannot be combined with --wasm-feature threads or --wasm-host",
            ));
        }
        if self.wasm_simd && (!self.target.is_wasm() || self.emit == Emit::Header) {
            return Err(driver_error(
                "E2000",
                "--wasm-feature simd128 requires wasm32 or wasm64 object, LLVM IR, or WASM output",
            ));
        }
        if self.wasm_tail_call
            && (!self.target.is_wasm()
                || !matches!(self.emit, Emit::Wasm | Emit::Object | Emit::Llvm))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-feature tail-call requires wasm32 or wasm64 object, LLVM IR, or WASM output",
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
                        | Emit::Bitcode
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
        self.validate_allocator()?;
        if self.strip {
            if self.target.is_wasm() {
                return Err(driver_error(
                    "E2000",
                    "--strip requires a native executable; WASM output is already stripped unless --debug-info is used",
                ));
            }
            if self.emit != Emit::Executable {
                return Err(driver_error(
                    "E2000",
                    "--strip requires native executable output (--emit exe)",
                ));
            }
            if self.debug_info {
                return Err(driver_error(
                    "E2000",
                    "--strip cannot be combined with --debug-info: a debugger needs the symbols",
                ));
            }
        }
        if self.wasm_opt {
            if self.target != Target::Wasm32 || self.emit != Emit::Wasm {
                return Err(driver_error(
                    "E2000",
                    "--wasm-opt requires WASM output (--target wasm32, --emit wasm)",
                ));
            }
            if self.wasm_threads {
                return Err(driver_error(
                    "E2000",
                    "--wasm-opt cannot be combined with --wasm-feature threads",
                ));
            }
            if self.debug_info {
                return Err(driver_error(
                    "E2000",
                    "--wasm-opt cannot be combined with --debug-info: the optimizer rewrites the code the names and debug information describe",
                ));
            }
        }
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
            if let Some(triple) = self.triple.filter(|_| self.platform().cross()) {
                return Err(driver_error(
                    "E2000",
                    format!(
                        "'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to {}",
                        triple.name()
                    ),
                ));
            }
            native_cpu_flag(self.platform().arch_name())?;
        }
        if self.wasm_max_memory.is_some()
            && (!self.target.is_wasm()
                || !matches!(
                    self.emit,
                    Emit::Object | Emit::Llvm | Emit::Wasm | Emit::Bitcode
                ))
        {
            return Err(driver_error(
                "E2000",
                "--wasm-max-memory requires wasm32 or wasm64 object, LLVM IR, bitcode, or WASM output",
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

    /// What `--backend fast` accepts (PB05): `-O0` executables and objects of the machine the compiler
    /// runs on, and `-O0` `wasm32` modules and objects, with the options that do not change the code
    /// the translator sees. It names every field and has no `..`, so that an option added later does
    /// not compile until someone decides whether the fast backend supports it. The checks that
    /// depend on the build of `tsuzuri` and on the host come last, so that the option errors do not
    /// depend on them.
    fn validate_fast(self) -> Result<(), Diagnostic> {
        let Self {
            target,
            emit,
            optimization,
            backend: _,
            cpu,
            debug_output: _,
            trap_info: _,
            debug_info,
            wasm_simd,
            wasm_threads,
            wasm_jspi,
            wasm_webgpu,
            wasm_net,
            wasm_tail_call,
            wasm_host,
            wasm_max_memory: _,
            wasm_stack_size: _,
            cache: _,
            trap_return: _,
            allocator: _,
            freestanding: _,
            strip: _,
            wasm_opt,
            triple,
        } = self;
        if optimization != OptLevel::O0 {
            return Err(driver_error(
                "E2000",
                format!(
                    "--backend fast supports only -O0; remove {} or use --backend llvm",
                    optimization.flag()
                ),
            ));
        }
        // The name of the output kind as the command line spells it.
        let output_kind = || {
            emit.bindings().map_or_else(
                || match emit {
                    Emit::Executable => "exe".to_owned(),
                    Emit::WgslRelaxed => "wgsl-relaxed".to_owned(),
                    Emit::SpirvRelaxed => "spirv-relaxed".to_owned(),
                    other => format!("{other:?}").to_ascii_lowercase(),
                },
                |(kind, _)| kind.to_owned(),
            )
        };
        if target == Target::Wasm64 || triple.is_some() {
            let other = match (target, triple) {
                (_, Some(triple)) => format!("--target {}", triple.name()),
                _ => "wasm64".to_owned(),
            };
            return Err(driver_error(
                "E2000",
                format!(
                    "--backend fast supports only the native target and wasm32; use --backend llvm for {other}"
                ),
            ));
        }
        if target == Target::Wasm32 {
            // Nothing else of a WebAssembly build changes the IR or the objects that the translator
            // sees, apart from what the line table of `-g` would need. The translator makes an
            // ordinary call of a `musttail` call, so it cannot give the tail-call feature its
            // guarantee, and the request is an error and not a silent downgrade (PM09).
            let unsupported = [
                (wasm_simd, "--wasm-feature simd128"),
                (wasm_threads, "--wasm-feature threads"),
                (wasm_jspi, "--wasm-feature jspi"),
                (wasm_webgpu, "--wasm-feature webgpu"),
                (wasm_net, "--wasm-feature net"),
                (wasm_tail_call, "--wasm-feature tail-call"),
                (wasm_host.is_some(), "--wasm-host"),
                (wasm_opt, "--wasm-opt"),
                (debug_info, "-g"),
            ]
            .into_iter()
            .find_map(|(set, option)| set.then_some(option));
            if let Some(option) = unsupported {
                return Err(driver_error(
                    "E2000",
                    format!(
                        "--backend fast does not support {option} for wasm32; remove it or use --backend llvm"
                    ),
                ));
            }
            if !matches!(emit, Emit::Wasm | Emit::Object) {
                return Err(driver_error(
                    "E2000",
                    format!(
                        "--backend fast produces only wasm and object outputs for wasm32; use --backend llvm for --emit {}",
                        output_kind()
                    ),
                ));
            }
        } else if !matches!(emit, Emit::Executable | Emit::Object) {
            return Err(driver_error(
                "E2000",
                format!(
                    "--backend fast produces only exe and object outputs; use --backend llvm for --emit {}",
                    output_kind()
                ),
            ));
        }
        if cpu != Cpu::Generic {
            return Err(driver_error(
                "E2000",
                "--backend fast does not support --cpu native yet; remove it or use --backend llvm",
            ));
        }
        if !cfg!(all(
            any(target_os = "macos", target_os = "linux"),
            any(target_arch = "aarch64", target_arch = "x86_64")
        )) {
            return Err(driver_error(
                "E2000",
                format!(
                    "--backend fast is not available on {}-{}; use --backend llvm",
                    env::consts::OS,
                    env::consts::ARCH
                ),
            ));
        }
        if !cfg!(feature = "fast-backend") {
            return Err(driver_error(
                "E2000",
                "this tsuzuri was built without the fast backend; rebuild with cargo build --release --features fast-backend or use --backend llvm",
            ));
        }
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
            // SIMD and tail calls do not change the glue.
            (self.wasm_simd || self.wasm_tail_call, "--wasm-feature"),
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
        if self.platform().windows() {
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

    /// The options of `--emit bitcode` (PR08): bitcode that a host's LTO link reads, from a native
    /// target or from wasm32. The other options of an object apply to it unchanged.
    fn validate_bitcode(self) -> Result<(), Diagnostic> {
        if !matches!(self.target, Target::Native | Target::Wasm32) {
            return Err(driver_error(
                "E2000",
                "'--emit bitcode' requires '--target native' or '--target wasm32'; wasm64 bitcode is not supported",
            ));
        }
        if self.target == Target::Native && self.platform().windows() {
            return Err(driver_error(
                "E2000",
                "'--emit bitcode' is not supported on Windows yet (G10); build an object with --emit object and link it normally",
            ));
        }
        Ok(())
    }

    /// The options of a named native target (`--target TRIPLE`): what its operating system, or the
    /// lack of one, and the tools of the machine the compiler runs on can produce.
    fn validate_triple(self, triple: Triple) -> Result<(), Diagnostic> {
        let name = triple.name();
        if self.target != Target::Native {
            return Err(driver_error(
                "E2000",
                format!("--target {name} names a native target, not WebAssembly"),
            ));
        }
        if triple.is_bare_metal()
            && !(self.freestanding && matches!(self.emit, Emit::Object | Emit::Llvm | Emit::Header))
        {
            return Err(driver_error(
                "E2000",
                format!(
                    "{name} has no operating system: build it with --freestanding --allocator host and --emit object, llvm, or header"
                ),
            ));
        }
        // A sysroot that the target cannot read is refused when the build starts, not only at a link.
        sdk_args(self.platform())?;
        // The debug information of these outputs comes from a tool that only its own kind of host has.
        let platform = self.platform();
        if self.debug_info {
            let (needed, tool) = if platform.mach_o()
                && !cfg!(target_os = "macos")
                && matches!(self.emit, Emit::Executable | Emit::Shared)
            {
                ("macOS", "its dSYM comes from dsymutil")
            } else if platform.msvc() && !cfg!(windows) && self.emit == Emit::Executable {
                ("Windows", "its PDB comes from the MSVC linker")
            } else {
                return Ok(());
            };
            return Err(driver_error(
                "E2000",
                format!("--debug-info for {name} needs a {needed} host: {tool}"),
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
            && (self.emit.is_kernel() || self.emit == Emit::Executable)
        {
            return Err(driver_error(
                "E2000",
                "--allocator host requires object, LLVM IR, header, or WebAssembly output; link the object into a host that defines tsuzuri_host_alloc, tsuzuri_host_free, and tsuzuri_host_realloc",
            ));
        }
        if self.allocator == llvm::Allocator::Counting
            && (self.emit.is_kernel() || self.emit == Emit::Executable)
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
        if self.allocator == llvm::Allocator::Small && self.target != Target::Native {
            return Err(driver_error(
                "E2000",
                "--allocator small requires a native target; WebAssembly modules keep their own heap",
            ));
        }
        if self.allocator == llvm::Allocator::Small && self.emit.is_kernel() {
            return Err(driver_error(
                "E2000",
                "--allocator small requires executable, object, shared library, LLVM IR, or header output",
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
        if self.target.is_wasm()
            && matches!(
                self.emit,
                Emit::Object | Emit::Llvm | Emit::Wasm | Emit::Bitcode
            )
        {
            self.wasm_max_memory = self.wasm_max_memory.or(wasm.max_memory);
        }
        if self.target.is_wasm() && self.emit == Emit::Wasm {
            self.wasm_stack_size = self.wasm_stack_size.or(wasm.stack_size);
        }
        self
    }

    pub fn output_path(self, input: &Path) -> PathBuf {
        let platform = self.platform();
        input.with_extension(match self.emit {
            Emit::Executable => platform.executable_extension(),
            Emit::Object => platform.object_extension(),
            Emit::Llvm => "ll",
            Emit::Header => "h",
            Emit::Wasm => "wasm",
            Emit::Wgsl | Emit::WgslRelaxed => "wgsl",
            Emit::Spirv | Emit::SpirvRelaxed => "spv",
            Emit::Shared => platform.shared_extension(),
            Emit::Bitcode => "bc",
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

/// The CPU features that the f16 lowering reads (D10). A `--cpu native` build runs on the machine
/// that runs the compiler, so only such a build may use [`CpuFeatures::detect`]; a build for
/// another machine passes `CpuFeatures::default()`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CpuFeatures {
    /// x86-64 F16C: `vcvtph2ps` and `vcvtps2ph` convert between binary16 and binary32.
    pub f16c: bool,
    /// x86-64 AVX512-FP16: binary16 arithmetic and conversions in single instructions.
    pub avx512fp16: bool,
    /// AArch64 FEAT_FP16: binary16 arithmetic in single instructions.
    pub fp16: bool,
}

impl CpuFeatures {
    /// What the CPU that runs the compiler reports.
    #[cfg(target_arch = "x86_64")]
    pub(crate) fn detect() -> Self {
        Self {
            f16c: std::arch::is_x86_feature_detected!("f16c"),
            avx512fp16: std::arch::is_x86_feature_detected!("avx512fp16"),
            fp16: false,
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub(crate) fn detect() -> Self {
        Self {
            fp16: std::arch::is_aarch64_feature_detected!("fp16"),
            ..Self::default()
        }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub(crate) fn detect() -> Self {
        Self::default()
    }
}

/// Whether an AArch64 target without a guaranteed FEAT_FP16 computes f16 as `half` too (D10).
/// There LLVM lowers each `half` operation to conversions to binary32, one binary32 operation,
/// and a conversion back, which are base Armv8-A instructions and round once. `false` keeps the
/// `half` path to macOS, whose CPUs all have FEAT_FP16, and to a `--cpu native` build on a host
/// that reports it.
const AARCH64_HALF_WITHOUT_FP16: bool = true;

/// How a native build computes f16 (D10), from the machine its code runs on: the operating system
/// and architecture in the spelling of `std::env::consts`, and the CPU features, which count only
/// for `--cpu native` (the machine running the compiler). Without a capability the build falls
/// back to `Soft`, the bundled integer runtime, which every target has:
///
/// | Target | `--cpu generic` | `--cpu native` |
/// | --- | --- | --- |
/// | WASM | `Soft` | n/a |
/// | AArch64 | `Hardware` (see [`AARCH64_HALF_WITHOUT_FP16`]) | `Hardware` |
/// | x86-64 | `Soft` (F16C is not in the baseline) | `Hardware` with AVX512-FP16, `Conversions` with F16C, else `Soft` |
/// | other | `Soft` | `Soft` |
pub(crate) fn f16_path(
    wasm: bool,
    os: &str,
    arch: &str,
    cpu: Cpu,
    host: CpuFeatures,
) -> llvm::F16Path {
    f16_path_with(AARCH64_HALF_WITHOUT_FP16, wasm, os, arch, cpu, host)
}

/// [`f16_path`] with the setting of [`AARCH64_HALF_WITHOUT_FP16`] as a parameter, so that a test
/// covers both.
fn f16_path_with(
    aarch64_without_fp16: bool,
    wasm: bool,
    os: &str,
    arch: &str,
    cpu: Cpu,
    host: CpuFeatures,
) -> llvm::F16Path {
    use llvm::F16Path::{Conversions, Hardware, Soft};
    if wasm {
        return Soft;
    }
    let native = cpu == Cpu::Native;
    match arch {
        "aarch64" if aarch64_without_fp16 || os == "macos" || (native && host.fp16) => Hardware,
        "x86_64" if native && host.avx512fp16 => Hardware,
        "x86_64" if native && host.f16c => Conversions,
        _ => Soft,
    }
}

/// [`f16_path`] for a build for `platform`. The CPU features are the ones of the machine that runs
/// the compiler, which a build for another machine cannot use.
pub(crate) fn platform_f16_path(platform: Platform, target: Target, cpu: Cpu) -> llvm::F16Path {
    f16_path(
        target.is_wasm(),
        platform.os_name(),
        platform.arch_name(),
        cpu,
        if platform.cross() {
            CpuFeatures::default()
        } else {
            CpuFeatures::detect()
        },
    )
}

/// [`platform_f16_path`] for a build by `backend`. The fast backend has no `half`: it computes f16
/// with the bit-exact integer routines of the runtime (`Soft`), whatever the machine could do. Every
/// emission of IR that the backend translates takes its path from here (the build, and the runners of
/// `test`), so that none of them hands it a `half` operation (PB05).
pub(crate) fn f16_path_for(
    backend: Backend,
    platform: Platform,
    target: Target,
    cpu: Cpu,
) -> llvm::F16Path {
    if backend == Backend::Fast {
        llvm::F16Path::Soft
    } else {
        platform_f16_path(platform, target, cpu)
    }
}

/// The Clang options that give `path` its instructions in a `--cpu native` build. `-march=native`
/// lets Clang detect the CPU itself, and these make it use the instructions that the compiler
/// detected, so a `half` operation never becomes a libcall (`__extendhfsf2`, `__truncsfhf2`,
/// `__truncdfhf2`) because the two detections differ. AArch64 needs none: `half` lowers to base
/// instructions there.
fn f16_cpu_flags(arch: &str, path: llvm::F16Path) -> &'static [&'static str] {
    match (arch, path) {
        ("x86_64", llvm::F16Path::Hardware) => &["-mavx512fp16", "-mf16c"],
        ("x86_64", llvm::F16Path::Conversions) => &["-mf16c"],
        _ => &[],
    }
}

/// The Clang options of the IR of a `--cpu native` build: the CPU, and what its f16 path needs.
fn native_ir_cpu_args(arch: &str, f16: llvm::F16Path) -> Result<Vec<&'static str>, Diagnostic> {
    let mut args = vec![native_cpu_flag(arch)?];
    args.extend_from_slice(f16_cpu_flags(arch, f16));
    Ok(args)
}

/// Inlines the Win32 adapter: Clang cannot resolve quoted includes next to
/// verbatim (`\\?\`) source paths, and canonical temporary paths are verbatim. A Windows target
/// needs it on any host: the temporary directory has no `task-windows.h` for a quoted include.
pub(crate) fn task_runtime_source_for(platform: Platform) -> String {
    let source = include_str!("runtime/task.c");
    if platform.windows() {
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

/// The Clang arguments of every run that is for `platform`, and of the IR compile in them.
/// Without a named target they are the arguments of the machine the compiler runs on; a named
/// target also passes its Clang target, even when it is the host's own, so that the output does not
/// depend on the defaults of the Clang that is found (the deployment target of an Apple SDK).
fn compile_args(platform: Platform) -> Vec<OsString> {
    let Some(triple) = platform.triple() else {
        return native_compile_args(platform.windows(), platform.arch_name())
            .iter()
            .map(OsString::from)
            .collect();
    };
    let mut arguments = target_args(platform);
    arguments.extend(triple.clang_arguments().iter().map(OsString::from));
    // Clang rejects position-independent code for the Windows targets.
    if !platform.windows() {
        arguments.push("-fPIC".into());
    }
    arguments
}

/// Only the Clang target of a named platform: a link that is not a compile names its target too.
fn target_args(platform: Platform) -> Vec<OsString> {
    platform
        .triple()
        .map(|triple| format!("--target={}", triple.clang_target()).into())
        .into_iter()
        .collect()
}

/// The environment variable that names the sysroot of a named target: its C headers and libraries.
pub const SYSROOT_VARIABLE: &str = "TSUZURI_SYSROOT";
/// The Binaryen `wasm-opt` that `--wasm-opt` runs (PM08): the variable, then a distribution, then `PATH`.
pub(crate) const WASM_OPT_VARIABLE: &str = "TSUZURI_WASM_OPT";

/// The Clang arguments of the runs that read the headers and libraries of a named target: the
/// runtime's C compile and the link. A build for the machine the compiler runs on does not use
/// `TSUZURI_SYSROOT`.
fn sdk_args(platform: Platform) -> Result<Vec<OsString>, Diagnostic> {
    sdk_args_for(
        platform,
        env::var_os(SYSROOT_VARIABLE).filter(|value| !value.is_empty()),
    )
}

/// [`sdk_args`] with the value of `TSUZURI_SYSROOT`, if it is set to something.
fn sdk_args_for(
    platform: Platform,
    sysroot: Option<OsString>,
) -> Result<Vec<OsString>, Diagnostic> {
    let Some(triple) = platform.triple() else {
        return Ok(Vec::new());
    };
    let mut arguments = Vec::new();
    if platform.mach_o() {
        // Without an SDK for the target, Apple's Clang falls back to the macOS one with a warning
        // and links the wrong system library.
        arguments.push("-Werror=incompatible-sysroot".into());
        if let Some(sysroot) = sysroot {
            arguments.extend(["-isysroot".into(), sysroot]);
        }
    } else if let Some(sysroot) = sysroot {
        if triple.is_msvc() || triple.is_bare_metal() {
            return Err(driver_error(
                "E2000",
                format!(
                    "{SYSROOT_VARIABLE} does not apply to {}{}",
                    triple.name(),
                    if triple.is_msvc() {
                        ": the MSVC linker reads the Windows SDK and CRT from the INCLUDE and LIB environment variables"
                    } else {
                        ": a freestanding object reads no C headers or libraries"
                    }
                ),
            ));
        }
        let mut argument = OsString::from("--sysroot=");
        argument.push(sysroot);
        arguments.push(argument);
    }
    Ok(arguments)
}

/// Makes Clang link with LLD where the linker of the machine that the compiler runs on cannot write
/// the target's format: ld64 links only Apple targets, and the others need an ELF or COFF linker.
fn link_args(platform: Platform) -> Vec<OsString> {
    if platform.cross() && !(platform.mach_o() && cfg!(target_os = "macos")) {
        vec!["-fuse-ld=lld".into()]
    } else {
        Vec::new()
    }
}

/// The variable that chooses the linker of a native executable or shared library (PB03 D6).
pub(crate) const LINKER_VARIABLE: &str = "TSUZURI_LINKER";

/// The arguments of the final link of a native executable or shared library: those of `link_args`, and LLD
/// when `TSUZURI_LINKER=lld` asks for it. A relocatable `-r` link keeps `link_args`: `ld64.lld` cannot do it.
fn final_link_args(platform: Platform) -> Result<Vec<OsString>, Diagnostic> {
    final_link_args_for(platform, env::var_os(LINKER_VARIABLE).as_deref())
}

/// [`final_link_args`] for the value of `TSUZURI_LINKER`: unset, empty or `default` leave the choice to Clang.
/// The bundled launcher links with LLD whatever it is told, so `lld` changes nothing there.
fn final_link_args_for(
    platform: Platform,
    linker: Option<&OsStr>,
) -> Result<Vec<OsString>, Diagnostic> {
    let mut arguments = link_args(platform);
    match linker.and_then(OsStr::to_str) {
        None if linker.is_some() => {
            return Err(driver_error(
                "E2000",
                format!("{LINKER_VARIABLE} must be 'default' or 'lld'"),
            ));
        }
        None | Some("" | "default") => {}
        Some("lld") if platform.windows() => {
            return Err(driver_error(
                "E2000",
                format!("{LINKER_VARIABLE}=lld is not supported for Windows targets; unset it"),
            ));
        }
        Some("lld") => {
            let lld = OsString::from("-fuse-ld=lld");
            if !arguments.contains(&lld) {
                arguments.push(lld);
            }
        }
        Some(other) => {
            return Err(driver_error(
                "E2000",
                format!("{LINKER_VARIABLE} must be 'default' or 'lld', not '{other}'"),
            ));
        }
    }
    Ok(arguments)
}

/// `TSUZURI_LINKER=lld` with `-Os` or `-Oz` on a Mach-O target: the final link shares the compile's Clang run, and
/// Apple's Clang hands the size level to `ld64.lld`, which rejects it, so the combination fails before any work.
fn check_linker_with_optimization(
    platform: Platform,
    linker: Option<&OsStr>,
    optimization: OptLevel,
) -> Result<(), Diagnostic> {
    if platform.mach_o()
        && matches!(optimization, OptLevel::Os | OptLevel::Oz)
        && linker.is_some_and(|value| value == "lld")
    {
        return Err(driver_error(
            "E2000",
            format!(
                "{LINKER_VARIABLE}=lld cannot be combined with {} for a Mach-O target (Clang passes the size level to ld64.lld, which rejects it); use -O2 or unset {LINKER_VARIABLE}",
                optimization.flag()
            ),
        ));
    }
    Ok(())
}

/// What a failing final link tells the user: `hint`, and that LLD is needed when `TSUZURI_LINKER=lld` asked for it.
fn linker_hint(hint: String) -> String {
    if env::var_os(LINKER_VARIABLE).is_some_and(|value| value == "lld") {
        format!(
            "{hint}; {LINKER_VARIABLE}=lld needs LLD (ld.lld on Linux, ld64.lld on macOS) next to Clang or on PATH, or unset it"
        )
    } else {
        hint
    }
}

/// The Clang arguments of the runs that read no SDK: the compile of IR to an object, and the
/// relocatable link of objects. Apple's Clang would still look for an SDK and warn that the one it
/// found, the macOS one, is not the target's; there is nothing for that SDK to get wrong.
fn no_sdk_args(platform: Platform) -> Vec<OsString> {
    if platform.triple().is_some() && platform.mach_o() {
        vec!["-Wno-incompatible-sysroot".into()]
    } else {
        Vec::new()
    }
}

/// Why a Clang run for a named target failed, where it could be missing the target's tools.
fn cross_hint(platform: Platform, hint: &str) -> String {
    let Some(triple) = platform.triple().filter(|_| platform.cross()) else {
        return hint.to_owned();
    };
    let name = triple.name();
    // A freestanding object reads no C library, header, or linker: only the compile of IR can fail.
    if triple.is_bare_metal() {
        return format!(
            "cross-compiling to {name} needs a Clang with the {} backend; {hint}",
            triple.arch_name()
        );
    }
    let needs = if platform.mach_o() {
        format!("the SDK for {name}: set {SYSROOT_VARIABLE} to it")
    } else if triple.is_msvc() {
        "the Windows SDK and CRT: set the INCLUDE and LIB environment variables to them, or build the MinGW target with the bundled toolchain".to_owned()
    } else if triple.has_bundled_c_library() {
        format!("a sysroot for {name}: use the bundled toolchain, or set {SYSROOT_VARIABLE} to one")
    } else {
        // The bundled toolchain has no C library for it, and refuses a sysroot.
        format!(
            "a sysroot for {name} (the bundled toolchain has none): set {SYSROOT_VARIABLE} to one, with a Clang other than the bundled one"
        )
    };
    // LLD, only where `link_args` asks for it: Apple's own linker writes Mach-O on a Mac.
    let linker = if link_args(platform).is_empty() {
        " and"
    } else {
        ", LLD, and"
    };
    format!(
        "cross-compiling to {name} needs a Clang with the {} backend{linker} {needs}; {hint}",
        triple.arch_name()
    )
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
    /// The edition that the text is read with: its package manifest's `edition`, and the latest
    /// for std and for sources without a manifest (G19). A manifest or lockfile entry carries
    /// the edition of the package it belongs to.
    pub edition: Edition,
}

#[derive(Debug)]
pub struct Project {
    pub sources: Vec<SourceFile>,
    pub manifests: Vec<SourceFile>,
    pub root: usize,
    /// The package whose manifest is in the root directory, `None` for a project without one.
    pub root_package: Option<crate::package::PackageId>,
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
    fn read(path: &Path, edition: Edition) -> Result<Self, SourceError> {
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
            edition,
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
/// as written, and otherwise the global namespace. A project without a manifest
/// is read with the latest edition.
fn folder_namespace(directory: &Path) -> String {
    let name = fs::canonicalize(directory)
        .ok()
        .and_then(|path| path.file_name().and_then(OsStr::to_str).map(str::to_owned))
        .unwrap_or_default();
    let valid = |namespace: &str| crate::package::valid_namespace(namespace, Edition::LATEST);
    crate::package::namespace(&name, Span::default())
        .ok()
        .filter(|namespace| valid(namespace))
        .or_else(|| valid(&name).then(|| name.replace("::", ".")))
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

/// Reads and parses the `Tsuzuri.toml` at `path`, which must be a regular file and not a
/// symbolic link. Returns its text and the manifest.
pub(crate) fn read_manifest(
    path: &Path,
) -> Result<(String, crate::package::Manifest), SourceError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        SourceError::new(path, io_error("inspect package manifest", path, error))
    })?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(SourceError::new(
            path,
            driver_error(
                "E1011",
                "package manifests must be regular files, not symbolic links",
            ),
        ));
    }
    let text = read_source_text(path).map_err(|error| SourceError::new(path, error))?;
    let manifest =
        crate::package::parse_manifest(&text, 0).map_err(|error| SourceError::new(path, error))?;
    Ok((text, manifest))
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
            let (text, manifest) = read_manifest(&path)?;
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
        Self::load_from_root(&directory, None, &normalized, true, true)
    }

    /// Loads the package graph at `directory`. With `all_std` false, it loads only the
    /// opt-in std modules that the user sources name (`stdlib::sources_for`). With
    /// `dependencies` false, git and registry dependencies and `Tsuzuri.lock` are not read, so
    /// the graph lacks those packages: `tsuzuri api-diff` needs only the package's own sources.
    fn load_from_root(
        directory: &Path,
        selected: Option<&Path>,
        overlays: &std::collections::BTreeMap<PathBuf, String>,
        all_std: bool,
        dependencies: bool,
    ) -> Result<Self, SourceError> {
        let lock = if dependencies {
            read_lockfile(directory)?
        } else {
            None
        };
        let packages = load_packages(directory, &mut |request| {
            if dependencies {
                offline_package(lock.as_ref(), request)
            } else {
                Ok(None)
            }
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
            // A source is read with its own package's edition; without a manifest, the latest.
            let edition = package.map_or(Edition::LATEST, |package| package.manifest.edition);
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
                        edition,
                    }
                } else {
                    SourceFile::read(&path, edition)?
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
                edition: Edition::LATEST,
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
        let root_package = packages.iter().find(|package| package.id.root == directory);
        let root_id = root_package.map(|package| package.id.clone());
        // A lockfile belongs to the root manifest, which is there whenever the lockfile is.
        let root_edition = root_package.map_or(Edition::MANIFEST_DEFAULT, |package| {
            package.manifest.edition
        });
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
                edition: package.manifest.edition,
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
                package: root_id.clone(),
                namespace: String::new(),
                edition: root_edition,
            });
        }
        Ok(Self {
            sources,
            manifests,
            root,
            root_package: root_id,
            wasm,
            native,
        })
    }

    pub fn load_for_tests(input: &Path) -> Result<Self, SourceError> {
        Self::load_without_entry(input, true)
    }

    pub fn load_for_docs(input: &Path) -> Result<Self, SourceError> {
        let mut project = Self::load_without_entry(input, true)?;
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

    fn load_without_entry(input: &Path, dependencies: bool) -> Result<Self, SourceError> {
        let metadata = fs::symlink_metadata(input)
            .map_err(|error| SourceError::new(input, io_error("inspect input", input, error)))?;
        if !metadata.is_dir() {
            return Self::load_with(input, dependencies);
        }
        let project = Self::load_from_root(
            input,
            None,
            &std::collections::BTreeMap::new(),
            false,
            dependencies,
        )?;
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

    /// Loads `input` for `tsuzuri api-diff`: its sources as for documentation, but without
    /// reading git and registry dependencies or `Tsuzuri.lock`, which a package's own API does
    /// not depend on (G19). The root package is `Project::root_package`.
    pub fn load_for_api(input: &Path) -> Result<Self, SourceError> {
        Self::load_without_entry(input, false)
    }

    pub fn load(input: &Path) -> Result<Self, SourceError> {
        Self::load_with(input, true)
    }

    fn load_with(input: &Path, dependencies: bool) -> Result<Self, SourceError> {
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
            dependencies,
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
            edition: Edition::LATEST,
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
                edition: Edition::LATEST,
            }
        }));
        Self {
            sources,
            manifests: Vec::new(),
            root: 0,
            root_package: None,
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
        let cache_time = crate::timings::Phase::start("cache");
        let frontend = cache
            .then(|| {
                let project =
                    crate::frontend_cache::ProjectKey::new(&self.root_directory()?, kind.name())?;
                crate::frontend_cache::FrontendCache::open(&crate::cache::default_root()?, project)
            })
            .flatten();
        drop(cache_time);
        // The commands of the command line use the available cores unless `TSUZURI_THREADS` says otherwise.
        let parallelism = crate::parallel::Parallelism::command_line();
        match frontend {
            Some(mut cache) => {
                crate::analyze_inputs_with(&inputs, None, Some(&mut cache), parallelism)
            }
            None => crate::analyze_inputs_with(&inputs, None, None, parallelism),
        }
    }

    pub(crate) fn inputs(&self) -> Vec<crate::SourceInput<'_>> {
        self.sources
            .iter()
            .map(|source| crate::SourceInput {
                path: source.relative_path.to_str().unwrap(),
                text: &source.text,
                origin: source.origin,
                namespace: &source.namespace,
                edition: source.edition,
                package: self.dependency_package(source),
            })
            .collect()
    }

    /// The dependency package that owns `source`: `None` for a source of the root package,
    /// which nothing depends on, and for one that no manifest governs.
    pub fn dependency_package<'a>(
        &self,
        source: &'a SourceFile,
    ) -> Option<&'a crate::package::PackageId> {
        source
            .package
            .as_ref()
            .filter(|package| self.root_package.as_ref() != Some(*package))
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
    // A resident process remembers a text while the stamp of its file is the same, and then does not open the file.
    let started = crate::resident::clock();
    let metadata = fs::metadata(path).map_err(|error| io_error("inspect source", path, error))?;
    if !metadata.is_file() {
        return Err(driver_error("E2001", "the source must be a regular file"));
    }
    if started.is_some()
        && let Some(text) = crate::resident::cached_source(path, &metadata)
    {
        return Ok(text);
    }
    let file = fs::File::open(path).map_err(|error| io_error("read source", path, error))?;
    let opened = file
        .metadata()
        .map_err(|error| io_error("inspect source", path, error))?;
    if !opened.is_file() {
        return Err(driver_error("E2001", "the source must be a regular file"));
    }
    if opened.len() > MAX_SOURCE_BYTES as u64 {
        return Err(driver_error(
            "E0003",
            format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit"),
        ));
    }
    let mut source = String::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_string(&mut source)
        .map_err(|error| io_error("read UTF-8 source", path, error))?;
    if let Some(started) = started {
        crate::resident::remember_source(path, &metadata, started, &source);
    }
    Ok(source)
}

/// The edition that `tsuzuri fmt` reads `input` with: the `edition` of the `Tsuzuri.toml` in
/// the input directory, or in the parent directory of an input file, and the latest edition
/// without one. It does not search upward, as the root of a file input is its parent.
fn format_edition(input: &Path, directory_input: bool) -> Result<Edition, SourceError> {
    let directory = if directory_input {
        input
    } else {
        input
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    };
    let manifest = directory.join("Tsuzuri.toml");
    match fs::symlink_metadata(&manifest) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Edition::LATEST),
        Err(error) => Err(SourceError::new(
            &manifest,
            io_error("inspect package manifest", &manifest, error),
        )),
        Ok(_) => read_manifest(&manifest).map(|(_, manifest)| manifest.edition),
    }
}

pub fn format_sources(input: &Path, check_only: bool) -> Result<Vec<PathBuf>, SourceError> {
    let metadata = fs::symlink_metadata(input).map_err(|error| {
        SourceError::new(input, io_error("inspect formatting input", input, error))
    })?;
    let edition = format_edition(input, metadata.is_dir())?;
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
            let result = crate::formatter::format_source_with_edition(
                &path.to_string_lossy(),
                &source,
                kind,
                edition,
            )?;
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
    build_profiled(module, project, output, options, links, &Profile::None)
}

/// Like [`build_linked`], instrumenting the program or optimizing it with a profile (PR07).
pub fn build_profiled(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
    links: &LinkInputs,
    profile: &Profile,
) -> Result<Vec<String>, Diagnostic> {
    build_complete(module, project, output, options, links, "build", profile)
        .map(|(messages, _)| messages)
}

fn build_complete(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
    links: &LinkInputs,
    action: &str,
    profile: &Profile,
) -> Result<(Vec<String>, Vec<crate::trap::TrapSite>), Diagnostic> {
    options.validate()?;
    profile.validate(options)?;
    profile.require_llvm_clang()?;
    let platform = options.platform();
    // The IR of a native build follows the target, not the machine the compiler runs on.
    let _platform_scope = PlatformScope::enter(platform);
    // A bad TSUZURI_LINKER fails before any work, not when the link starts.
    if options.target == Target::Native && matches!(options.emit, Emit::Executable | Emit::Shared) {
        final_link_args(platform)?;
        check_linker_with_optimization(
            platform,
            env::var_os(LINKER_VARIABLE).as_deref(),
            options.optimization,
        )?;
    }
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
    // How the native code computes f16 (D10); the IR and the Clang options of this build agree on it.
    let fast = options.backend == Backend::Fast;
    let f16 = f16_path_for(options.backend, platform, options.target, options.cpu);
    let runtime_objects = options.cache
        && !options.debug_info
        && !options.trap_info
        && !options.trap_return
        && !options.wasm_threads
        && !stack_checks
        && matches!(
            (options.target, options.emit),
            (Target::Native, Emit::Executable) | (Target::Wasm32, Emit::Wasm)
        );
    // PM08 D10: a native executable without -g links with the linker's garbage collection even when
    // the runtime is embedded in the module (--no-cache, --trap-info, ...), as it does with runtime
    // objects. Every function and datum gets a section of its own, so that an ELF or COFF linker can
    // drop the unused ones; Mach-O subsections already allow it.
    let gc_sections =
        options.emit == Emit::Executable && options.target == Target::Native && !options.debug_info;
    // The bytes of a SPIR-V module, which have no text form for `text` to carry (F09 Phase 3).
    let mut spirv = None;
    let emit_time = crate::timings::Phase::start("emit");
    let mut text = if options.emit == Emit::Header {
        if options.trap_return {
            llvm::header_with(module, true)
        } else {
            llvm::header_with_allocator(module, options.allocator)
        }
    } else if options.emit.is_kernel() {
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
                if options.emit.is_spirv() {
                    "SPIR-V output requires exactly one exported scalar kernel; use a dedicated source project"
                } else {
                    "WGSL output requires exactly one exported scalar kernel; use a dedicated source project"
                },
            ));
        }
        let kernel = crate::gpu::extract_kernel(module, exports[0])?;
        match options.emit {
            Emit::WgslRelaxed => kernel.wgsl_relaxed()?,
            Emit::Spirv => {
                spirv = Some(kernel.spirv()?.bytes());
                String::new()
            }
            Emit::SpirvRelaxed => {
                spirv = Some(kernel.spirv_relaxed()?.bytes());
                String::new()
            }
            _ => kernel.wgsl()?,
        }
    } else {
        let emission = llvm::EmitOptions {
            runtime_objects,
            wasm_simd128: options.wasm_simd,
            entry: if options.emit == Emit::Executable {
                Entry::Console
            } else {
                Entry::Library
            },
            wasm: options.target.is_wasm(),
            debug_output: options.debug_output,
            allocator: options.allocator,
            f16,
        };
        if options.trap_return {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_trap_return(
                    module,
                    emission,
                    sources,
                    options
                        .debug_info
                        .then_some(options.optimization != OptLevel::O0),
                    matches!(options.emit, Emit::Object | Emit::Shared | Emit::Bitcode),
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
            && matches!(
                options.emit,
                Emit::Executable | Emit::Object | Emit::Shared | Emit::Bitcode
            )
            // A freestanding object has no CPU dispatch: its runtime reads the CPU through the C library.
            && !options.freestanding
        {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_native_build(
                    module,
                    emission,
                    sources,
                    options
                        .debug_info
                        .then_some(options.optimization != OptLevel::O0),
                    options.trap_info,
                )
            })?;
            trap_sites = output.trap_sites;
            output.ir
        } else if options.wasm_threads
            || options.target == Target::Wasm64
            || stack_checks
            || options.wasm_tail_call
        {
            let output = project.with_trap_sources(|sources| {
                llvm::emit_wasm_build(
                    module,
                    emission,
                    sources,
                    options
                        .debug_info
                        .then_some(options.optimization != OptLevel::O0),
                    options.trap_info,
                    options.wasm_threads,
                    options.target == Target::Wasm64,
                    stack_checks,
                    options.wasm_tail_call,
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
                    options.optimization != OptLevel::O0,
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
    // The attribute that forces known closures to inline (PM02) is for `-O1` to `-O3`; the other levels
    // leave the inlining to the compiler.
    if options.target == Target::Native
        && !(options.emit == Emit::Header || options.emit.is_kernel())
        && !matches!(
            options.optimization,
            OptLevel::O1 | OptLevel::O2 | OptLevel::O3
        )
    {
        text = llvm::without_closure_inlining(text);
    }
    if platform.windows()
        && options.target == Target::Native
        && !(options.emit == Emit::Header || options.emit.is_kernel())
    {
        text = llvm::windows_abi(text, module);
        if options.debug_info && codeview_for(platform, &tool("TSUZURI_CLANG", "clang")) {
            text = llvm::with_codeview(text);
        }
    }
    if platform.no_red_zone()
        && options.target == Target::Native
        && !(options.emit == Emit::Header || options.emit.is_kernel())
    {
        text = llvm::with_no_red_zone(text);
    }
    if options.target.is_wasm() && options.emit != Emit::Header {
        text = llvm::with_wasm_heap_limit(text, max_memory);
        if options.wasm_host == Some(WasmHost::Wasi) {
            text = llvm::with_wasi_host(&text);
        }
        if options.wasm_net {
            text = llvm::with_net_imports(&text);
        }
        // The notes tell whoever compiles the IR by hand which flags its instructions need.
        if options.wasm_tail_call {
            text.insert_str(
                0,
                "; wasm-feature: tail-call; compile this IR with -mtail-call\n",
            );
        }
        if options.wasm_simd {
            text.insert_str(
                0,
                "; wasm-feature: simd128; compile this IR with -msimd128\n",
            );
        }
        // The GPU runtime functions are JSPI imports under `--wasm-feature webgpu` and report
        // "unavailable" without it, so the default module has no import (F09 Phase 2).
        text = llvm::with_wasm_gpu_host(text, options.wasm_webgpu);
        text.push_str(include_str!("runtime/wasm.ll"));
    }
    // `llvm::emit_*` keeps one `declare` per symbol, so a repeat here comes from a pass above (D12).
    debug_assert!(
        options.emit == Emit::Header
            || options.emit.is_kernel()
            || llvm::duplicate_declarations(&text).is_empty(),
        "the IR declares {:?} more than once",
        llvm::duplicate_declarations(&text)
    );
    let runtime_symbols = crate::runtime_cache::RuntimeSymbols::collect(&text);
    let task_runtime =
        options.target == Target::Native && runtime_symbols.contains("tsuzuri_task_parallel");
    // A kernel call or a multiversioned function's stub declares a `tsuzuri_cpu_` function (F08).
    let cpu_runtime = options.target == Target::Native && runtime_symbols.prefix("tsuzuri_cpu_");
    let io_runtime = runtime_symbols.prefix("tsuzuri_io_");
    let os_runtime = runtime_symbols.prefix("tsuzuri_os_");
    // The sockets of the standard Net module are src/runtime/net.c (E09).
    let net_runtime = runtime_symbols.prefix("tsuzuri_net_");
    // `Gpu.request Gpu.WebGpu` loads a WebGPU library at run time in src/runtime/gpu.c, and `Gpu.Vulkan` and
    // `Gpu.Auto` add the Vulkan backend of src/runtime/gpu-vulkan.c to it (F09 Phases 2 and 3).
    let gpu_runtime = options.target == Target::Native && runtime_symbols.prefix("tsuzuri_gpu_");
    let gpu_source = if gpu_runtime {
        llvm::gpu_runtime_source(&text)
    } else {
        String::new()
    };
    // `Async.block_on` waits in src/runtime/async.c natively and through JSPI imports on WASM (B08).
    let async_reactor = runtime_symbols.contains("tsuzuri_async_wait");
    // A `def main :: Array<string> -> i32` reads its arguments in src/runtime/arguments.c.
    let arguments_runtime = runtime_symbols.contains("tsuzuri_arguments");
    if options.freestanding {
        // A header holds no IR, so it is checked against the IR the object of the same build holds.
        let library;
        let ir = if options.emit == Emit::Header {
            library = llvm::emit_with_options(
                module,
                llvm::EmitOptions {
                    runtime_objects: false,
                    wasm_simd128: false,
                    entry: Entry::Library,
                    wasm: false,
                    debug_output: options.debug_output,
                    allocator: options.allocator,
                    f16,
                },
            )?;
            library.as_str()
        } else {
            text.as_str()
        };
        if let Some((_, feature)) = [
            ("declare i32 @tsuzuri_mutex_lock(", "Mutex"),
            ("declare i32 @tsuzuri_channel_send(", "Channel"),
            ("declare void @tsuzuri_task_parallel(", "parallel tasks"),
            ("declare i32 @tsuzuri_io_", "the standard IO"),
            ("declare i64 @tsuzuri_os_", "the operating-system APIs"),
            ("@tsuzuri_net_", "the Net socket API"),
            ("declare i32 @tsuzuri_gpu_", "GPU devices"),
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
    drop(emit_time);
    // Only a WASM module of a `def main` or an IO entry is a WASI command; objects leave `_start` to the embedder.
    let wasi_command =
        options.emit == Emit::Wasm && (llvm::io_entry(module) || llvm::main_entry(module));
    // With `--wasm-host wasi` the standard IO, the OS APIs, and a command's `_start` come from
    // src/runtime/os-wasi.c.
    let wasi_runtime = options.wasm_host == Some(WasmHost::Wasi)
        && (io_runtime || os_runtime || arguments_runtime || wasi_command);
    // Only objects, libraries and bitcode embed it: a host that links the LLVM output provides src/runtime/trap.c itself.
    let trap_runtime = options.trap_return
        && matches!(options.emit, Emit::Object | Emit::Shared | Emit::Bitcode)
        && [
            "tsuzuri_trap_raise",
            "tsuzuri_boundary_run",
            "tsuzuri_tracked_",
        ]
        .iter()
        .any(|symbol| runtime_symbols.prefix(symbol));
    let native_runtime = task_runtime
        || cpu_runtime
        || trap_runtime
        || gpu_runtime
        || (options.target == Target::Native
            && (io_runtime || os_runtime || net_runtime || arguments_runtime || async_reactor));
    // Native executables of programs that can recurse report a stack overflow themselves (E14 Phase 3);
    // objects leave the host's signals alone, and a program without recursion cannot exhaust its stack.
    let stack_runtime = options.target == Target::Native
        && options.emit == Emit::Executable
        && platform.unix()
        && llvm::has_recursion(&text);
    let debug_import = options.target.is_wasm() && runtime_symbols.contains("tsuzuri_debug_write");
    // A callback's address is a table index the host resolves through the exported table.
    let callback_table = options.target.is_wasm() && runtime_symbols.prefix("tz.callback.");
    if task_runtime && !platform.threads() && !matches!(options.emit, Emit::Llvm | Emit::Header) {
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
    if net_runtime && options.target.is_wasm() && !options.wasm_net {
        return Err(driver_error("E2000", NET_WASM_MESSAGE));
    }
    // The poller completes an async operation through the reactor's mailbox, which exists only in `Async.block_on`.
    // On WebAssembly the host completes it with `tsuzuri_async_complete`.
    if net_runtime
        && options.target == Target::Native
        && llvm::uses_net_async(&text)
        && !async_reactor
    {
        return Err(driver_error("E2000", NET_ASYNC_MESSAGE));
    }
    if net_runtime
        && !platform.net()
        && options.target == Target::Native
        && options.emit != Emit::Llvm
    {
        return Err(driver_error("E2002", net_message(platform)));
    }
    if async_reactor
        && !platform.reactor()
        && options.target == Target::Native
        && options.emit != Emit::Llvm
    {
        return Err(driver_error("E2002", async_message(platform)));
    }
    if os_runtime
        && !platform.os_apis()
        && options.target == Target::Native
        && options.emit != Emit::Llvm
    {
        return Err(driver_error("E2002", os_message(platform)));
    }
    if platform.windows() && native_runtime && options.emit == Emit::Object {
        return Err(driver_error(
            "E2002",
            "Windows COFF objects with embedded task, CPU, IO, or Async runtime are not supported; emit LLVM and link the runtime once, or build an executable",
        ));
    }
    protect_sources(project, output)?;
    protect_links(links, output)?;
    let sidecar = options.trap_info.then(|| trap_sidecar_path(output));
    // An instrumented WebAssembly module comes with the writer that its host runs to save the counters.
    let profile_writer = (options.target.is_wasm() && matches!(profile, Profile::Generate(_)))
        .then(|| profile::wasm_writer_path(output));
    let dwarf_sidecar = (platform.mach_o()
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
    if let Some(sidecar) = &profile_writer {
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
        && msvc_linker_for(platform))
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
        .join(if options.emit == Emit::Executable && platform.windows() {
            "artifact.exe"
        } else {
            "artifact"
        });
    let mut messages = Vec::new();
    // The merge of a profile comes first: the cache key records the bytes that Clang will read.
    let staged_profile = profile::prepare_profile(
        profile,
        options.target.is_wasm(),
        &temporary.path,
        &mut messages,
    )?;
    let staged_sidecar = temporary.path.join("sites.json");
    let staged_dwarf = temporary.path.join("symbols.dwarf");
    let staged_pdb = temporary.path.join("artifact.pdb");
    let staged_writer = temporary.path.join("profile.mjs");
    if let (Profile::Generate(directory), Some(_)) = (profile, &profile_writer) {
        fs::write(&staged_writer, profile::wasm_writer(directory))
            .map_err(|error| io_error("write profile writer", &staged_writer, error))?;
    }
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
    if profile_writer.is_some() {
        cache_paths.insert("profile".into(), staged_writer.clone());
    }
    if dwarf_sidecar.is_some() {
        cache_paths.insert("dwarf".into(), staged_dwarf.clone());
    }
    if pdb.is_some() {
        cache_paths.insert("pdb".into(), staged_pdb.clone());
    }
    // The cache key does not cover the contents of link inputs, so a build with them is never cached. A
    // SPIR-V module is never cached either: no tool runs for it, and its bytes are not in the key's text.
    // Bitcode is not cached (PR08 D7): it runs `llvm-link`, whose identity the key does not hold, and its
    // C runtime is merged as IR, which the runtime object cache does not hold.
    let cache_time = crate::timings::Phase::start("cache");
    let clang_identity = if options.cache
        && !matches!(options.emit, Emit::Llvm | Emit::Header | Emit::Bitcode)
        && !options.emit.is_kernel()
    {
        match crate::cache::tool_identity("TSUZURI_CLANG", "clang", options.cpu) {
            Ok(identity) => Some(identity),
            Err(error) => {
                messages.push(format!("runtime object cache disabled: {error}"));
                None
            }
        }
    } else {
        None
    };
    let cache = if options.cache
        && !matches!(options.emit, Emit::Header | Emit::Bitcode)
        && !options.emit.is_spirv()
        && links.is_empty()
    {
        let prepared = (|| -> io::Result<_> {
            let root = crate::cache::default_root()
                .ok_or_else(|| io::Error::other("no cache directory is configured"))?;
            let key = crate::cache::build_key(
                project,
                options,
                action,
                &text,
                output,
                clang_identity.as_ref(),
                staged_profile
                    .as_ref()
                    .map(profile::PreparedProfile::cache_field),
            )?;
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
    drop(cache_time);
    if let Some(cached) = cached {
        let _cache_time = crate::timings::Phase::start("cache");
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
    } else if matches!(options.emit, Emit::Llvm | Emit::Header) || options.emit.is_wgsl() {
        fs::write(&artifact, text).map_err(|error| io_error("write output", &artifact, error))?;
    } else if let Some(module) = spirv {
        fs::write(&artifact, module).map_err(|error| io_error("write output", &artifact, error))?;
    } else {
        // The C names that a shared library exports, read from the IR before it is written.
        let shared_symbols = (options.emit == Emit::Shared)
            .then(|| shared_exports(&text, module, options.trap_return));
        let mut ir = temporary.path.join("module.ll");
        let mut numeric_objects = Vec::new();
        if runtime_objects {
            let mut args: Vec<OsString> = ["-x", "ir", "-Wno-override-module"]
                .into_iter()
                .map(OsString::from)
                .collect();
            args.push(options.optimization.flag().into());
            if options.target.is_wasm() {
                args.extend(
                    [
                        "--target=wasm32-unknown-unknown",
                        "-mbulk-memory",
                        if options.wasm_simd {
                            "-msimd128"
                        } else {
                            "-mno-simd128"
                        },
                    ]
                    .into_iter()
                    .map(OsString::from),
                );
            } else {
                args.extend(compile_args(platform));
                args.extend(no_sdk_args(platform));
                if options.cpu == Cpu::Native {
                    args.extend(
                        native_ir_cpu_args(platform.arch_name(), f16)?
                            .into_iter()
                            .map(OsString::from),
                    );
                }
            }
            args.extend(
                ["-ffunction-sections", "-fdata-sections"]
                    .into_iter()
                    .map(OsString::from),
            );
            for (unit, source, prefix) in [
                ("numeric", include_str!("runtime/numeric.ll"), "tz_soft_"),
                ("math", crate::runtime_cache::math_source(), "tz_math_"),
            ] {
                if runtime_symbols.prefix(prefix) {
                    numeric_objects.push(compile_runtime_object(
                        unit,
                        source,
                        &format!("{unit}.ll"),
                        &args,
                        &temporary.path,
                        clang_identity.as_ref(),
                        RUNTIME_OBJECT_HINT,
                        &mut messages,
                    )?);
                }
            }
        }
        // The attributes make a host's inliner accept the functions, and for `+simd128` keep the SIMD of a function
        // through the link. A WebAssembly function without SIMD carries none: it inlines without them.
        if options.emit == Emit::Bitcode && (options.target == Target::Native || options.wasm_simd)
        {
            text = bitcode_ir(&text, options, platform, f16, &temporary.path)?;
        }
        // The merge of the C runtime hides every symbol that only the runtime defines, so the ones a host calls are
        // named to keep (PR08 D5).
        let kept_exports = if options.emit == Emit::Bitcode && native_runtime {
            let exports = shared_exports(&text, module, options.trap_return);
            lto::keep_runtime_exports(&mut text, &exports)?
        } else {
            Vec::new()
        };
        let text = match &staged_profile {
            Some(staged) if staged.instruments_wasm() => profile::wasm_instrumented_ir(text),
            _ => text,
        };
        let object = temporary.path.join("module.o");
        if fast && options.target == Target::Wasm32 {
            // The runtime objects that Clang built decide how a 128-bit result is returned, and the
            // routines of the runtime that this IR defines must agree with them.
            let multivalue = wasm_multivalue(&numeric_objects)?;
            translate_fast_wasm(&text, multivalue, &object)?;
        } else if fast {
            // The IR is translated in memory and never written.
            translate_fast(&text, platform, &object)?;
        } else {
            fs::write(&ir, text).map_err(|error| io_error("write LLVM IR", &ir, error))?;
        }
        let runtime_object = temporary.path.join("task.o");
        // The fast backend writes no IR to merge: its object joins the C runtime in the relocatable link below,
        // which on macOS leaves a debug map in place of the DWARF.
        if fast
            && platform.mach_o()
            && options.debug_info
            && native_runtime
            && options.emit == Emit::Object
        {
            return Err(driver_error(
                "E2002",
                "--backend fast cannot put the C runtime that this program needs into a macOS debug object, because the relocatable link drops the DWARF; build an executable, drop -g, or use --backend llvm",
            ));
        }
        let merge_debug_ir = platform.mach_o()
            && options.debug_info
            && native_runtime
            && options.emit == Emit::Object;
        // Bitcode is one file, so its C runtime is merged as IR too (PR08 D4).
        let merge_runtime_ir = merge_debug_ir || options.emit == Emit::Bitcode;
        if native_runtime {
            let runtime_source = temporary.path.join("task.c");
            let source = format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
                // The feature macros of os.c and net.c are the same and must precede every include, so
                // they come first.
                if os_runtime && options.target == Target::Native {
                    include_str!("runtime/os.c")
                } else {
                    ""
                },
                if net_runtime && options.target == Target::Native {
                    include_str!("runtime/net.c")
                } else {
                    ""
                },
                if async_reactor && options.target == Target::Native {
                    include_str!("runtime/async.c")
                } else {
                    ""
                },
                gpu_source,
                if trap_runtime {
                    include_str!("runtime/trap.c")
                } else {
                    ""
                },
                if task_runtime {
                    task_runtime_source_for(platform)
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
            fs::write(&runtime_source, &source)
                .map_err(|error| io_error("write task runtime", &runtime_source, error))?;
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            // `-S -emit-llvm` writes the IR; `-c` beside it is an unused argument that Clang warns about.
            runtime.arg("-std=c11");
            if !merge_runtime_ir {
                runtime.arg("-c");
            }
            runtime
                .args(compile_args(platform))
                .args(sdk_args(platform)?)
                .arg(options.optimization.flag());
            if (task_runtime || trap_runtime || async_reactor || net_runtime || gpu_runtime)
                && !platform.windows()
            {
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
            if merge_runtime_ir {
                // `zig cc` adds its own `-c` to a `-S` compile, and then reports it as unused.
                runtime.args(["-S", "-emit-llvm", "-Wno-unused-command-line-argument"]);
            }
            if options.cpu == Cpu::Native {
                runtime.arg(native_cpu_flag(platform.arch_name())?);
            }
            if gc_sections {
                runtime.args(["-ffunction-sections", "-fdata-sections"]);
            }
            if options.cache && !merge_runtime_ir {
                let args: Vec<_> = runtime.get_args().map(OsStr::to_owned).collect();
                let cached = compile_runtime_object(
                    "native-c",
                    &source,
                    "task.c",
                    &args,
                    &temporary.path,
                    clang_identity.as_ref(),
                    RUNTIME_OBJECT_HINT,
                    &mut messages,
                )?;
                stage_runtime_object(&cached, &runtime_object, "stage native runtime")?;
            } else {
                runtime.arg(&runtime_source).arg("-o").arg(&runtime_object);
                collect_message(
                    &mut messages,
                    run_tool(
                        &mut runtime,
                        &cross_hint(
                            platform,
                            "native runtime requires Clang and the platform C/OS SDK headers",
                        ),
                    )?,
                );
            }
            if merge_runtime_ir {
                let combined = temporary.path.join("combined.ll");
                let bitcode = options.emit == Emit::Bitcode;
                let mut linker = Command::new(if bitcode {
                    lto::llvm_link_tool()
                } else {
                    tool("TSUZURI_LLVM_LINK", "llvm-link")
                });
                // The program's own file comes first: `--internalize` leaves its symbols alone and hides
                // the runtime's, so only the program's public ABI stays visible to a host.
                linker.arg(&ir).arg(&runtime_object);
                if bitcode {
                    linker.arg("--internalize");
                }
                linker.arg("-S").arg("-o").arg(&combined);
                collect_message(
                    &mut messages,
                    run_tool(
                        &mut linker,
                        if bitcode {
                            "bitcode with the C runtime requires llvm-link matching Clang; set TSUZURI_LLVM_LINK, or put llvm-link next to Clang"
                        } else {
                            "macOS debug objects with tasks require llvm-link matching Clang; set TSUZURI_LLVM_LINK"
                        },
                    )?,
                );
                lto::check_kept_exports(&combined, &kept_exports)?;
                ir = combined;
            }
        }
        let stack_source = temporary.path.join("stack.c");
        let mut stack_object = temporary.path.join("stack.o");
        if stack_runtime {
            fs::write(&stack_source, include_str!("runtime/stack.c"))
                .map_err(|error| io_error("write stack runtime", &stack_source, error))?;
        }
        if stack_runtime && options.debug_info {
            // Its own object, without debug information: the guard is not source a debugger should show.
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            runtime
                .args(["-std=c11", "-c", "-O1"])
                .args(compile_args(platform))
                .args(sdk_args(platform)?);
            let hint = cross_hint(
                platform,
                "native runtime requires Clang and the platform C/OS SDK headers",
            );
            if options.cache {
                let args: Vec<_> = runtime.get_args().map(OsStr::to_owned).collect();
                stack_object = compile_runtime_object(
                    "stack-debug",
                    include_str!("runtime/stack.c"),
                    "stack.c",
                    &args,
                    &temporary.path,
                    clang_identity.as_ref(),
                    &hint,
                    &mut messages,
                )?;
            } else {
                runtime.arg(&stack_source).arg("-o").arg(&stack_object);
                collect_message(&mut messages, run_tool(&mut runtime, &hint)?);
            }
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
            fs::write(&source, &text)
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
            runtime.arg(options.optimization.flag());
            let hint = "the WASI host needs Clang with WebAssembly support";
            if options.cache {
                let args: Vec<_> = runtime.get_args().map(OsStr::to_owned).collect();
                let cached = compile_runtime_object(
                    "wasi",
                    &text,
                    "wasi.c",
                    &args,
                    &temporary.path,
                    clang_identity.as_ref(),
                    hint,
                    &mut messages,
                )?;
                stage_runtime_object(&cached, &wasi_object, "stage WASI runtime")?;
            } else {
                runtime.arg(&source).arg("-o").arg(&wasi_object);
                collect_message(&mut messages, run_tool(&mut runtime, hint)?);
            }
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
                .arg(options.optimization.flag());
            let hint = "WASM threads require Clang atomics and bulk-memory support";
            if options.cache {
                let args: Vec<_> = runtime.get_args().map(OsStr::to_owned).collect();
                let cached = compile_runtime_object(
                    "wasm-threads",
                    include_str!("runtime/task-wasm-threads.c"),
                    "threads.c",
                    &args,
                    &temporary.path,
                    clang_identity.as_ref(),
                    hint,
                    &mut messages,
                )?;
                stage_runtime_object(&cached, &threads_object, "stage WASM thread runtime")?;
            } else {
                runtime.arg(&source).arg("-o").arg(&threads_object);
                collect_message(&mut messages, run_tool(&mut runtime, hint)?);
            }
        }
        if !fast {
            clang
                .arg("-x")
                .arg("ir")
                .arg("-Wno-override-module")
                .arg(options.optimization.flag());
        }
        if let Some(staged) = &staged_profile {
            clang.args(staged.args());
        }
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
            // The default is explicit, so that a Clang whose generic WebAssembly CPU has the
            // feature still makes the same code (PM09). The runtime objects have no `musttail` and
            // link with either.
            clang.arg(if options.wasm_tail_call {
                "-mtail-call"
            } else {
                "-mno-tail-call"
            });
        } else {
            clang.args(compile_args(platform));
            if options.cpu == Cpu::Native {
                clang.args(native_ir_cpu_args(platform.arch_name(), f16)?);
            }
            if gc_sections && !platform.mach_o() {
                clang.args(["-ffunction-sections", "-fdata-sections"]);
            }
            // The run that links reads the target's libraries; a compile of IR alone reads none.
            if options.emit == Emit::Executable && dwarf_sidecar.is_none() {
                clang
                    .args(sdk_args(platform)?)
                    .args(final_link_args(platform)?);
            } else {
                clang.args(no_sdk_args(platform));
            }
        }
        if options.emit != Emit::Executable || dwarf_sidecar.is_some() {
            clang.arg("-c");
        }
        // The bitcode carries the summary that a host's ThinLTO link imports from (PR08 D2).
        if options.emit == Emit::Bitcode {
            clang.arg("-flto=thin");
        }
        clang.arg(if fast { &object } else { &ir }).arg("-o").arg(
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
        if options.emit == Emit::Executable && !platform.windows() && dwarf_sidecar.is_none() {
            clang.arg("-lm");
        }
        // `-x` drops the local symbols and keeps the global ones; Windows links are left as they are.
        if options.strip && options.emit == Emit::Executable && !platform.windows() {
            clang.arg("-Wl,-x");
        }
        if gc_sections {
            clang.args(dead_strip_args(platform));
            for function in module.functions.iter().filter(|function| function.exported) {
                if platform.mach_o() {
                    clang.arg(format!("-Wl,-u,_tz_{}", function.name));
                } else if !platform.windows() {
                    clang.arg(format!("-Wl,-u,tz_{}", function.name));
                }
            }
        }
        if options.emit == Emit::Executable && runtime_objects && !numeric_objects.is_empty() {
            clang.args(["-x", "none"]).args(&numeric_objects);
        }
        if native_runtime && options.emit == Emit::Executable && dwarf_sidecar.is_none() {
            clang.args(["-x", "none"]).arg(&runtime_object);
            if (task_runtime || net_runtime) && !platform.windows() {
                clang.arg("-pthread");
            }
            // gpu.c loads a WebGPU library with dlopen, which older glibc keeps in libdl.
            if gpu_runtime && platform.linux() {
                clang.args(["-pthread", "-ldl"]);
            }
            // The runtime also asks for it with a default-library request, which not every linker honors.
            if net_runtime && platform.windows() {
                clang.arg("-lws2_32");
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
        if fast
            && (options.emit == Emit::Object || options.target.is_wasm() || dwarf_sidecar.is_some())
        {
            // The translated object is the output, and with the C runtime it is merged below; or it is
            // what the debug link below or the link of the module reads, so that no compile of it remains.
            if options.emit == Emit::Object && !native_runtime {
                fs::rename(&object, &artifact)
                    .map_err(|error| io_error("write object", &artifact, error))?;
            }
        } else {
            let mut clang_hint = linker_hint(cross_hint(
                platform,
                "install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable",
            ));
            if native_runtime && options.emit == Emit::Bitcode {
                // A Clang that cannot read the IR that llvm-link wrote belongs to another LLVM than llvm-link does.
                clang_hint.push_str("; the IR was merged by llvm-link, which must come from the same LLVM as Clang: set TSUZURI_LLVM_LINK");
            }
            let clang_hint = match &staged_profile {
                Some(staged) => staged.hint(clang_hint),
                None => clang_hint,
            };
            let clang_output =
                run_tool(&mut clang, &clang_hint).map_err(|error| match &staged_profile {
                    Some(staged) => staged.clang_error(error),
                    None => error,
                })?;
            collect_message(
                &mut messages,
                match &staged_profile {
                    Some(staged) => staged.clang_output(clang_output),
                    None => clang_output,
                },
            );
            if options.emit == Emit::Bitcode {
                lto::check_bitcode(&artifact, &tool("TSUZURI_CLANG", "clang"))?;
            }
        }
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
            linker
                .args(target_args(platform))
                .args(sdk_args(platform)?)
                .args(final_link_args(platform)?)
                .arg(&object)
                .args(["-g", "-lm"]);
            // The link of an instrumented program reads Clang's profile runtime, which the compile did not.
            if let Some(staged) = staged_profile
                .as_ref()
                .filter(|staged| staged.instruments())
            {
                linker.arg(staged.flag());
            }
            if native_runtime {
                linker.arg(&runtime_object);
                if task_runtime || net_runtime {
                    linker.arg("-pthread");
                }
                if gpu_runtime && platform.linux() {
                    linker.args(["-pthread", "-ldl"]);
                }
            }
            if stack_runtime {
                linker.arg(&stack_object).arg("-pthread");
            }
            links.add_to(&mut linker);
            linker.arg("-o").arg(&artifact);
            let linked = run_tool(
                &mut linker,
                &linker_hint(cross_hint(
                    platform,
                    "native debug executables require the Clang linker",
                )),
            )?;
            // The link of an instrumented program is where Apple's linker notes the alignment of the profile sections.
            collect_message(
                &mut messages,
                match &staged_profile {
                    Some(staged) => staged.clang_output(linked),
                    None => linked,
                },
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
            linker
                .args(target_args(platform))
                .args(no_sdk_args(platform))
                .args(link_args(platform))
                .args(["-r", "-nostdlib"]);
            if platform.mach_o() {
                linker.arg("-Wl,-keep_private_externs");
            }
            if platform.linux_kernel() {
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
                    &cross_hint(
                        platform,
                        "the native linker must support relocatable object linking for parallel tasks",
                    ),
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
            linker.args(&numeric_objects);
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
                || (options.wasm_net && net_runtime)
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
            if staged_profile
                .as_ref()
                .is_some_and(profile::PreparedProfile::instruments_wasm)
            {
                linker.args(profile::WASM_LINK_ARGS);
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
            if options.wasm_opt {
                run_wasm_opt(
                    &artifact,
                    &temporary.path,
                    options.wasm_simd,
                    options.wasm_tail_call,
                    &mut messages,
                )?;
            }
        }
    }
    let sidecars: Vec<_> = sidecar
        .as_ref()
        .map(|path| (&staged_sidecar, path))
        .into_iter()
        .chain(dwarf_sidecar.as_ref().map(|path| (&staged_dwarf, path)))
        .chain(pdb.as_ref().map(|path| (&staged_pdb, path)))
        .chain(profile_writer.as_ref().map(|path| (&staged_writer, path)))
        .collect();
    if !cache_hit && let Some((cache, key)) = &cache {
        let _cache_time = crate::timings::Phase::start("cache");
        if let Err(error) = cache.store(key, &cache_paths, &messages) {
            messages.push(format!("build cache write failed: {error}"));
        }
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
    let platform = options.platform();
    let mut linker = Command::new(tool("TSUZURI_CLANG", "clang"));
    linker
        .args(target_args(platform))
        .args(sdk_args(platform)?)
        .args(final_link_args(platform)?);
    if platform.mach_o() {
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
        // The runtime may load a WebGPU library with dlopen, which older glibc keeps in libdl.
        if platform.linux() {
            linker.arg("-ldl");
        }
    }
    linker.arg("-lm");
    links.add_to(&mut linker);
    linker.arg("-o").arg(artifact);
    collect_message(
        messages,
        run_tool(
            &mut linker,
            &linker_hint(cross_hint(
                platform,
                "shared libraries require the Clang linker; unresolved extern symbols need --link, -l or -L",
            )),
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
        // A module that reaches no socket has no `tsuzuri_net` import and exports no allocator for one, so it gets the glue
        // of `--wasm-feature jspi` alone, which needs no Node.js modules.
        _ if options.wasm_net && crate::llvm::reaches_net(module) => {
            crate::bindings::javascript_with_net(module)
        }
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

/// Whether the native executables of this machine link with the MSVC linker (`link.exe`, or
/// `lld-link` with `-fuse-ld=lld`): Windows with any Clang but the bundled launcher, which links
/// with MinGW's `ld.lld` and keeps DWARF only (G14). Their `-g` builds also carry CodeView and get
/// a PDB. The file name is enough: every other Clang, a MinGW distribution's included, gets
/// `--target=<arch>-pc-windows-msvc` from `native_compile_args`, and Clang's MSVC driver links
/// only with `link.exe` or `lld-link`; the launcher alone drops that target for `windows-gnu`.
pub(crate) fn msvc_linker() -> bool {
    cfg!(windows) && !bundled_launcher(&tool("TSUZURI_CLANG", "clang"))
}

/// Whether `clang` is the bundled launcher, which links MinGW and so never an MSVC target.
fn bundled_launcher(clang: &OsStr) -> bool {
    Path::new(clang)
        .file_stem()
        .is_some_and(|stem| stem == "tsuzuri-clang")
}

/// Whether the `-g` output for `platform` also carries CodeView: for an MSVC target on whichever
/// machine builds it, unless the Clang is the launcher, whose targets keep DWARF only.
fn codeview_for(platform: Platform, clang: &OsStr) -> bool {
    platform.msvc() && !bundled_launcher(clang)
}

/// [`msvc_linker`] for a build for `platform`: a MinGW target never links with the MSVC linker.
pub(crate) fn msvc_linker_for(platform: Platform) -> bool {
    platform.msvc() && msvc_linker()
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
    run_profiled(
        module,
        project,
        options,
        links,
        json,
        arguments,
        &Profile::None,
    )
}

/// [`run_with_arguments`] with a profile (PR07): `--profile-generate` runs the instrumented
/// program, which writes its raw profile when it exits, and `--profile-use` runs the program
/// that the profile optimized. `run` builds with `--trap-info`, so a profile of a `run` matches
/// a `build --trap-info` best.
pub fn run_profiled(
    module: &CheckedModule,
    project: &Project,
    options: BuildOptions,
    links: &LinkInputs,
    json: bool,
    arguments: &[OsString],
    profile: &Profile,
) -> Result<Vec<String>, Diagnostic> {
    run_process(
        module,
        project,
        options,
        links,
        RunStdio::Inherit { json, arguments },
        profile,
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
        &Profile::None,
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
    profile: &Profile,
) -> Result<(Vec<String>, Vec<u8>), Diagnostic> {
    if options.target != Target::Native || options.emit != Emit::Executable {
        return Err(driver_error(
            "E2000",
            "run requires native executable output",
        ));
    }
    let platform = options.platform();
    // A target this machine cannot run needs the user's runner, and a missing one is no reason to build first.
    options.validate()?;
    let runner = foreign_runner(platform, "programs", "the program")?;
    let temporary = TemporaryDirectory::new(&env::temp_dir())?;
    let output = temporary.path.join(if platform.windows() {
        "program.exe"
    } else {
        "program"
    });
    let (mut messages, sites) = build_complete(
        module,
        project,
        &output,
        BuildOptions {
            trap_info: true,
            ..options
        },
        links,
        "run",
        profile,
    )?;
    let started = std::time::SystemTime::now();
    let json = matches!(stdio, RunStdio::Inherit { json: true, .. });
    let (status, stderr, stdout) = match stdio {
        RunStdio::Inherit { json, arguments } => {
            let mut child = program_command(runner.as_ref(), &output)
                .args(arguments)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| spawn_error(runner.as_ref(), &output, error))?;
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
        RunStdio::Capture { timeout, limit } => capture(runner.as_ref(), &output, timeout, limit)?,
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
    if let Profile::Generate(directory) = profile
        && let Some(message) = profile::unwritten_profile(directory, started)
    {
        messages.push(message);
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

/// Runs `output` for [`run_captured`], through the runner of a foreign target. The streams are
/// read on their own threads, so neither pipe can fill up while the other is read.
fn capture(
    runner: Option<&ForeignRunner>,
    output: &Path,
    timeout: Option<std::time::Duration>,
    limit: usize,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), Diagnostic> {
    use std::sync::mpsc::{RecvTimeoutError, channel};
    use std::time::{Duration, Instant};
    let mut child = program_command(runner, output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| spawn_error(runner, output, error))?;
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

/// The runner of the executables of `platform`: none for a target this machine can run, the
/// command of `TSUZURI_RUNNER` for another one, and an error when that is not set. `what` names
/// the executables in the error and `them` what the user could run elsewhere.
pub(crate) fn foreign_runner(
    platform: Platform,
    what: &str,
    them: &str,
) -> Result<Option<ForeignRunner>, Diagnostic> {
    let Some(triple) = platform.triple().filter(|_| platform.cross()) else {
        return Ok(None);
    };
    let name = triple.name();
    ForeignRunner::from_environment().map(Some).ok_or_else(|| {
        driver_error(
            "E2000",
            format!(
                "cannot run {name} {what} on this machine; run {them} on a {name} machine, set {} to a command that runs {name} executables (such as qemu-user, Wine, or an emulator wrapper), or use 'tsuzuri build --target {name}'",
                crate::target::RUNNER_VARIABLE
            ),
        )
    })
}

/// The command that starts `executable`: itself, or through the runner of a foreign target.
pub(crate) fn program_command(runner: Option<&ForeignRunner>, executable: &Path) -> Command {
    match runner {
        Some(runner) => {
            let mut command = Command::new(&runner.program);
            command.args(&runner.arguments).arg(executable);
            command
        }
        None => Command::new(executable),
    }
}

/// The error for a [`program_command`] that did not start: the runner, if there is one, is the
/// program that could not be found or run.
fn spawn_error(runner: Option<&ForeignRunner>, executable: &Path, error: io::Error) -> Diagnostic {
    match runner {
        Some(runner) => io_error(
            &format!("start the runner of {}", crate::target::RUNNER_VARIABLE),
            Path::new(&runner.program),
            error,
        ),
        None => io_error("run executable", executable, error),
    }
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

fn dead_strip_args(platform: Platform) -> Vec<OsString> {
    if platform.mach_o() {
        vec!["-Wl,-dead_strip".into()]
    } else if platform.windows()
        && !bundled_launcher(&tool("TSUZURI_CLANG", "clang"))
        && platform.msvc()
    {
        vec!["-Wl,/OPT:REF".into()]
    } else {
        vec!["-Wl,--gc-sections".into()]
    }
}

/// What a failing compile of a runtime object tells the user to check, unless its caller knows more.
const RUNTIME_OBJECT_HINT: &str =
    "runtime objects require Clang with support for the selected target";

/// Translates the module's IR with the fast backend and writes the object (PB05).
#[cfg(feature = "fast-backend")]
fn translate_fast(text: &str, platform: Platform, object: &Path) -> Result<(), Diagnostic> {
    let bytes = crate::backend::translate(
        text,
        platform,
        crate::parallel::Parallelism::command_line(),
        cfg!(debug_assertions),
    )?;
    fs::write(object, bytes).map_err(|error| io_error("write object", object, error))
}

#[cfg(not(feature = "fast-backend"))]
fn translate_fast(_: &str, _: Platform, _: &Path) -> Result<(), Diagnostic> {
    Err(driver_error(
        "E2000",
        "this tsuzuri was built without the fast backend; rebuild with cargo build --release --features fast-backend or use --backend llvm",
    ))
}

/// Translates the module's IR with the fast backend to a `wasm32` object (PB05, D13).
#[cfg(feature = "fast-backend")]
fn translate_fast_wasm(text: &str, multivalue: bool, object: &Path) -> Result<(), Diagnostic> {
    let bytes = crate::backend::translate_wasm(
        text,
        crate::parallel::Parallelism::command_line(),
        multivalue,
    )?;
    fs::write(object, bytes).map_err(|error| io_error("write object", object, error))
}

#[cfg(not(feature = "fast-backend"))]
fn translate_fast_wasm(_: &str, _: bool, _: &Path) -> Result<(), Diagnostic> {
    Err(driver_error(
        "E2000",
        "this tsuzuri was built without the fast backend; rebuild with cargo build --release --features fast-backend or use --backend llvm",
    ))
}

/// Whether a 128-bit result is returned as two WebAssembly results, as the runtime objects that are
/// already compiled say (see `backend::wasm_multivalue`).
#[cfg(feature = "fast-backend")]
fn wasm_multivalue(objects: &[PathBuf]) -> Result<bool, Diagnostic> {
    let mut contents = Vec::with_capacity(objects.len());
    for path in objects {
        contents
            .push(fs::read(path).map_err(|error| io_error("read runtime object", path, error))?);
    }
    Ok(crate::backend::wasm_multivalue(
        contents.iter().map(Vec::as_slice),
    ))
}

#[cfg(not(feature = "fast-backend"))]
fn wasm_multivalue(_: &[PathBuf]) -> Result<bool, Diagnostic> {
    Ok(false)
}

/// The IR of a `--emit bitcode` build with the target attributes that a C function compiled with the same flags has
/// (PR08 D3): the CPU and its tuning, and for `--cpu native` the features too. A WebAssembly function keeps its
/// features as well, because `+simd128` is what lets the link's code generator use SIMD for it.
fn bitcode_ir(
    ir: &str,
    options: BuildOptions,
    platform: Platform,
    f16: llvm::F16Path,
    directory: &Path,
) -> Result<String, Diagnostic> {
    let wasm = options.target.is_wasm();
    let mut flags = if wasm {
        [
            "--target=wasm32-unknown-unknown",
            "-mbulk-memory",
            "-msimd128",
        ]
        .map(OsString::from)
        .to_vec()
    } else {
        let mut flags = compile_args(platform);
        flags.extend(no_sdk_args(platform));
        flags
    };
    if options.cpu == Cpu::Native {
        flags.extend(
            native_ir_cpu_args(platform.arch_name(), f16)?
                .into_iter()
                .map(OsString::from),
        );
    }
    let attributes =
        lto::probe_target_attributes(&tool("TSUZURI_CLANG", "clang"), &flags, directory)?;
    let attributes = if wasm || options.cpu == Cpu::Native {
        attributes
    } else {
        attributes.without_features()
    };
    lto::stamp_target_attributes(ir, &attributes)
}

/// The object of `source`, compiled with `args` for `unit`: the bundled one, the one in the runtime cache, or a
/// fresh compile of `input` in `directory` (which a failure reports with `hint`) that is then stored.
#[allow(clippy::too_many_arguments)]
fn compile_runtime_object(
    unit: &str,
    source: &str,
    input: &str,
    args: &[OsString],
    directory: &Path,
    identity: Option<&crate::cache::ToolIdentity>,
    hint: &str,
    messages: &mut Vec<String>,
) -> Result<PathBuf, Diagnostic> {
    let output = directory.join(format!("{unit}.o"));
    match crate::runtime_cache::prebuilt(unit, source, args, identity) {
        Ok(Some(bytes)) => {
            fs::write(&output, bytes)
                .map_err(|error| io_error("stage bundled runtime", &output, error))?;
            return Ok(output);
        }
        Ok(None) => {}
        Err(error) => messages.push(format!(
            "bundled runtime object unavailable; rebuilding: {error}"
        )),
    }
    let cache = identity.and_then(|identity| {
        let root = crate::cache::default_root()?;
        let key = crate::cache::runtime_key(unit, source.as_bytes(), args, identity)?;
        match crate::cache::BuildCache::open(&root)
            .and_then(|_| crate::cache::BuildCache::open(&root.join("runtime")))
        {
            Ok(cache) => Some((cache, key)),
            Err(error) => {
                messages.push(format!("runtime object cache disabled: {error}"));
                None
            }
        }
    });
    if let Some((cache, key)) = &cache {
        match cache.load(key) {
            Ok(Some(mut artifact)) if artifact.files.len() == 1 => {
                let bytes = artifact
                    .files
                    .remove("artifact")
                    .expect("cache artifact")
                    .bytes;
                fs::write(&output, bytes)
                    .map_err(|error| io_error("restore runtime object", &output, error))?;
                messages.extend(artifact.messages);
                return Ok(output);
            }
            Ok(_) => {}
            Err(error) => messages.push(format!(
                "runtime object cache read failed; rebuilding: {error}"
            )),
        }
    }
    let directory = crate::cache::real_path(directory)
        .map_err(|error| io_error("resolve runtime directory", directory, error))?;
    fs::write(directory.join(input), source)
        .map_err(|error| io_error("write runtime source", &directory.join(input), error))?;
    let mut command = Command::new(tool("TSUZURI_CLANG", "clang"));
    command
        .current_dir(&directory)
        .args(args)
        .args(["-c", input, "-o"])
        .arg(format!("{unit}.o"));
    let message = run_tool(&mut command, hint)?;
    collect_message(messages, message.clone());
    if let Some((cache, key)) = cache {
        let paths = std::collections::BTreeMap::from([("artifact".to_owned(), output.clone())]);
        let diagnostics = if message.is_empty() {
            Vec::new()
        } else {
            vec![message]
        };
        if let Err(error) = cache.store(&key, &paths, &diagnostics) {
            messages.push(format!("runtime object cache write failed: {error}"));
        }
    }
    Ok(output)
}

/// Moves the object of a runtime unit to where the link looks for it. A unit that is named like its
/// staging file (`wasi` and `wasi.o`) is there already, and renaming a file onto itself is not
/// defined on every platform.
fn stage_runtime_object(object: &Path, staged: &Path, what: &str) -> Result<(), Diagnostic> {
    if object == staged {
        return Ok(());
    }
    fs::rename(object, staged).map_err(|error| io_error(what, staged, error))
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
    let started = crate::timings::start();
    let output = command.output();
    if started.is_some() {
        let tool = Path::new(&name)
            .file_stem()
            .unwrap_or_else(|| command.get_program())
            .to_string_lossy();
        crate::timings::finish(&format!("tool.{tool}"), started);
    }
    let output = output.map_err(|error| {
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

/// Runs Binaryen's `wasm-opt -Oz` over the linked module and replaces it with the result (PM08).
/// The tool comes from `TSUZURI_WASM_OPT`, a distribution, or `PATH`, and only because `--wasm-opt`
/// asked: a missing or failing tool is E2002 with an install hint, never a silent skip. The features
/// are the ones this compiler's modules use; `simd` only for `--wasm-feature simd128`.
fn run_wasm_opt(
    artifact: &Path,
    directory: &Path,
    simd: bool,
    tail_call: bool,
    messages: &mut Vec<String>,
) -> Result<(), Diagnostic> {
    let optimized = directory.join("optimized.wasm");
    let mut command = Command::new(tool(WASM_OPT_VARIABLE, "wasm-opt"));
    command.arg(artifact).arg("-o").arg(&optimized).arg("-Oz");
    for feature in [
        "bulk-memory",
        "sign-ext",
        "nontrapping-float-to-int",
        "mutable-globals",
        "multivalue",
        "reference-types",
    ] {
        command.arg(format!("--enable-{feature}"));
    }
    if simd {
        command.arg("--enable-simd");
    }
    // A module with `return_call` does not validate in Binaryen without the feature.
    if tail_call {
        command.arg("--enable-tail-call");
    }
    collect_message(
        messages,
        run_tool(
            &mut command,
            "--wasm-opt runs Binaryen's wasm-opt: install Binaryen or set TSUZURI_WASM_OPT to the wasm-opt executable",
        )?,
    );
    fs::rename(&optimized, artifact).map_err(|error| {
        io_error(
            "replace the WASM output with the optimized module",
            artifact,
            error,
        )
    })
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
                    crate::resident::warn(&format!(
                        "warning: cannot remove '{}': {error}",
                        self.path.display()
                    ));
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
        // A level above 3 has no `OptLevel`; `parse` rejects it before a `BuildOptions` exists.
        assert_eq!(OptLevel::from_speed_level(4), None);
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
                    wasm_tail_call: true,
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
    fn validates_the_tail_call_feature_for_the_outputs_with_webassembly_instructions() {
        let tail = |target, emit| BuildOptions {
            target,
            emit,
            wasm_tail_call: true,
            ..BuildOptions::default()
        };
        assert!(!BuildOptions::default().wasm_tail_call);
        for (target, emit) in [
            (Target::Wasm32, Emit::Wasm),
            (Target::Wasm32, Emit::Object),
            (Target::Wasm32, Emit::Llvm),
            (Target::Wasm64, Emit::Wasm),
            (Target::Wasm64, Emit::Object),
            (Target::Wasm64, Emit::Llvm),
        ] {
            let result = tail(target, emit).validate();
            assert!(result.is_ok(), "{target:?} {emit:?}: {result:?}");
        }
        for (target, emit) in [
            (Target::Native, Emit::Executable),
            (Target::Native, Emit::Llvm),
            (Target::Wasm32, Emit::Header),
        ] {
            assert_eq!(
                tail(target, emit).validate().unwrap_err().message,
                "--wasm-feature tail-call requires wasm32 or wasm64 object, LLVM IR, or WASM output",
                "{target:?} {emit:?}"
            );
        }
        // The option is part of the text that the build cache hashes.
        let text = |options: BuildOptions| format!("{options:?}");
        let off = BuildOptions {
            target: Target::Wasm32,
            emit: Emit::Wasm,
            ..BuildOptions::default()
        };
        assert_ne!(text(off), text(tail(Target::Wasm32, Emit::Wasm)));
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
    fn validates_bitcode_output() {
        let bitcode = BuildOptions {
            emit: Emit::Bitcode,
            ..BuildOptions::default()
        };
        assert_eq!(
            bitcode.output_path(Path::new("dir/Main.tz")),
            Path::new("dir/Main.bc")
        );
        // WebAssembly: wasm32 on every host, with the options that an object of it has.
        let wasm = BuildOptions {
            target: Target::Wasm32,
            ..bitcode
        };
        for options in [
            wasm,
            BuildOptions {
                optimization: OptLevel::Oz,
                debug_info: true,
                wasm_simd: true,
                wasm_max_memory: Some(64 << 20),
                ..wasm
            },
        ] {
            assert!(options.validate().is_ok(), "{options:?}");
        }
        for (options, message) in [
            (
                BuildOptions {
                    target: Target::Wasm64,
                    ..bitcode
                },
                "'--emit bitcode' requires '--target native' or '--target wasm32'; wasm64 bitcode is not supported",
            ),
            (
                BuildOptions {
                    wasm_threads: true,
                    ..wasm
                },
                "--wasm-feature threads requires wasm32 object or WASM output",
            ),
            (
                BuildOptions {
                    wasm_jspi: true,
                    ..wasm
                },
                "--wasm-feature jspi requires wasm32 or wasm64 object, LLVM IR, WASM, or JavaScript bindings output",
            ),
            (
                BuildOptions {
                    wasm_host: Some(WasmHost::Wasi),
                    ..wasm
                },
                "--wasm-host wasi requires wasm32 object or WASM output",
            ),
            (
                BuildOptions {
                    wasm_stack_size: Some(1 << 20),
                    ..wasm
                },
                "--wasm-stack-size requires WASM output; link object and LLVM IR output with wasm-ld -z stack-size",
            ),
            (
                BuildOptions {
                    trap_return: true,
                    ..wasm
                },
                "--trap-mode return is only valid for native object, llvm or header output",
            ),
        ] {
            assert_eq!(
                options.validate().unwrap_err().message,
                message,
                "{options:?}"
            );
        }
        if cfg!(windows) {
            assert_eq!(
                bitcode.validate().unwrap_err().message,
                "'--emit bitcode' is not supported on Windows yet (G10); build an object with --emit object and link it normally"
            );
            return;
        }
        assert!(bitcode.validate().is_ok());
        // The options of an object keep their meaning for bitcode.
        for options in [
            BuildOptions {
                optimization: OptLevel::Oz,
                debug_info: true,
                ..bitcode
            },
            BuildOptions {
                trap_return: true,
                trap_info: true,
                ..bitcode
            },
            BuildOptions {
                allocator: llvm::Allocator::Small,
                ..bitcode
            },
            BuildOptions {
                allocator: llvm::Allocator::Host,
                ..bitcode
            },
            BuildOptions {
                allocator: llvm::Allocator::Counting,
                ..bitcode
            },
            BuildOptions {
                triple: Some(Triple::X86_64LinuxGnu),
                ..bitcode
            },
        ] {
            assert!(options.validate().is_ok(), "{options:?}");
        }
        if native_cpu_flag(env::consts::ARCH).is_ok() {
            assert!(
                BuildOptions {
                    cpu: Cpu::Native,
                    ..bitcode
                }
                .validate()
                .is_ok()
            );
        }
        for (options, message) in [
            (
                BuildOptions {
                    triple: Some(Triple::X86_64WindowsMsvc),
                    ..bitcode
                },
                "'--emit bitcode' is not supported on Windows yet (G10); build an object with --emit object and link it normally",
            ),
            (
                BuildOptions {
                    strip: true,
                    ..bitcode
                },
                "--strip requires native executable output (--emit exe)",
            ),
            (
                BuildOptions {
                    freestanding: true,
                    allocator: llvm::Allocator::Host,
                    ..bitcode
                },
                "--freestanding requires object, LLVM IR, or header output",
            ),
        ] {
            assert_eq!(
                options.validate().unwrap_err().message,
                message,
                "{options:?}"
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
        let limit =
            "--wasm-max-memory requires wasm32 or wasm64 object, LLVM IR, bitcode, or WASM output";
        let size = "--wasm-stack-size requires WASM output; link object and LLVM IR output with wasm-ld -z stack-size";
        for (target, emit, max, stack, message) in [
            (Target::Native, Emit::Executable, memory, None, limit),
            (Target::Native, Emit::Object, memory, None, limit),
            (Target::Native, Emit::Llvm, memory, None, limit),
            (Target::Native, Emit::Wgsl, memory, None, limit),
            (Target::Native, Emit::WgslRelaxed, memory, None, limit),
            (Target::Native, Emit::Spirv, memory, None, limit),
            (Target::Native, Emit::SpirvRelaxed, memory, None, limit),
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
            (Target::Native, Emit::WgslRelaxed),
            (Target::Native, Emit::Spirv),
            (Target::Native, Emit::SpirvRelaxed),
            (Target::Wasm32, Emit::Object),
            (Target::Wasm32, Emit::Llvm),
            (Target::Wasm32, Emit::Wasm),
            (Target::Wasm64, Emit::Object),
        ] {
            let error = options(target, emit).validate().unwrap_err();
            assert_eq!(error.code, "E2000", "{target:?} {emit:?}");
            assert!(
                error.message == message || emit.is_kernel(),
                "{target:?} {emit:?}: {}",
                error.message
            );
        }
    }

    #[test]
    fn validate_fast_allows_only_native_o0_executables_and_objects() {
        let fast = BuildOptions {
            backend: Backend::Fast,
            optimization: OptLevel::O0,
            ..BuildOptions::default()
        };
        // The host and the build of `tsuzuri` decide last, after every option has passed.
        let available = cfg!(all(
            feature = "fast-backend",
            any(target_os = "macos", target_os = "linux"),
            any(target_arch = "aarch64", target_arch = "x86_64")
        ));
        for emit in [Emit::Executable, Emit::Object] {
            let result = BuildOptions { emit, ..fast }.validate();
            assert_eq!(result.is_ok(), available, "{emit:?}: {result:?}");
            if let Err(error) = result {
                assert_eq!(error.code, "E2000");
            }
            // `-g` adds the line table and the functions of the program (WP12).
            let result = BuildOptions {
                emit,
                debug_info: true,
                ..fast
            }
            .validate();
            assert_eq!(result.is_ok(), available, "{emit:?} -g: {result:?}");
        }
        // The programmatic default is -O3, which the fast backend does not translate.
        let error = BuildOptions {
            backend: Backend::Fast,
            ..BuildOptions::default()
        }
        .validate()
        .unwrap_err();
        assert_eq!(
            error.message,
            "--backend fast supports only -O0; remove -O3 or use --backend llvm"
        );
        for emit in [
            Emit::Llvm,
            Emit::Header,
            Emit::Wgsl,
            Emit::WgslRelaxed,
            Emit::Spirv,
            Emit::SpirvRelaxed,
            Emit::Shared,
            Emit::BindingsCs,
            Emit::BindingsPy,
            Emit::BindingsCpp,
        ] {
            let error = BuildOptions { emit, ..fast }.validate().unwrap_err();
            assert_eq!(error.code, "E2000", "{emit:?}");
            assert!(
                error.message.starts_with(
                    "--backend fast produces only exe and object outputs; use --backend llvm for --emit "
                ),
                "{emit:?}: {}",
                error.message
            );
        }
        // An `--emit` that its `--target` cannot give is an error for every backend, on every host: the
        // error is the one of the LLVM backend, because `--backend llvm` is no way out.
        let emit_target = "'--emit exe' requires '--target native'; '--emit wasm' requires '--target wasm32' or '--target wasm64'";
        for (emit, target, message) in [
            (Emit::Wasm, Target::Native, emit_target),
            (Emit::Executable, Target::Wasm32, emit_target),
            (
                Emit::BindingsJs,
                Target::Native,
                "'--emit bindings-js' requires '--target wasm32'",
            ),
            (
                Emit::Shared,
                Target::Wasm32,
                "'--emit shared' requires '--target native'",
            ),
        ] {
            let error = BuildOptions {
                emit,
                target,
                ..fast
            }
            .validate()
            .unwrap_err();
            assert_eq!(error.message, message, "{emit:?} for {target:?}");
        }
        // The cache key hashes the `Debug` text of the options, so the backend changes the key.
        let llvm = BuildOptions {
            backend: Backend::Llvm,
            ..fast
        };
        assert_ne!(format!("{fast:?}"), format!("{llvm:?}"));
    }

    #[test]
    fn validate_fast_allows_wasm32_modules_and_objects_at_o0() {
        let wasm = BuildOptions {
            backend: Backend::Fast,
            optimization: OptLevel::O0,
            target: Target::Wasm32,
            emit: Emit::Wasm,
            ..BuildOptions::default()
        };
        let available = cfg!(all(
            feature = "fast-backend",
            any(target_os = "macos", target_os = "linux"),
            any(target_arch = "aarch64", target_arch = "x86_64")
        ));
        for emit in [Emit::Wasm, Emit::Object] {
            let result = BuildOptions { emit, ..wasm }.validate();
            assert_eq!(result.is_ok(), available, "{emit:?}: {result:?}");
        }
        // What only changes the link, the IR or the checks stays allowed.
        for options in [
            BuildOptions {
                wasm_max_memory: Some(64 << 20),
                wasm_stack_size: Some(128 << 10),
                ..wasm
            },
            BuildOptions {
                trap_info: true,
                allocator: llvm::Allocator::Counting,
                ..wasm
            },
            BuildOptions {
                allocator: llvm::Allocator::Host,
                ..wasm
            },
        ] {
            let result = options.validate();
            assert_eq!(result.is_ok(), available, "{options:?}: {result:?}");
        }
        for (options, option) in [
            (
                BuildOptions {
                    wasm_simd: true,
                    ..wasm
                },
                "--wasm-feature simd128",
            ),
            (
                BuildOptions {
                    wasm_threads: true,
                    ..wasm
                },
                "--wasm-feature threads",
            ),
            (
                BuildOptions {
                    wasm_jspi: true,
                    ..wasm
                },
                "--wasm-feature jspi",
            ),
            (
                BuildOptions {
                    wasm_webgpu: true,
                    ..wasm
                },
                "--wasm-feature webgpu",
            ),
            (
                BuildOptions {
                    wasm_net: true,
                    ..wasm
                },
                "--wasm-feature net",
            ),
            (
                BuildOptions {
                    wasm_tail_call: true,
                    ..wasm
                },
                "--wasm-feature tail-call",
            ),
            (
                BuildOptions {
                    wasm_host: Some(WasmHost::Wasi),
                    ..wasm
                },
                "--wasm-host",
            ),
            (
                BuildOptions {
                    wasm_opt: true,
                    ..wasm
                },
                "--wasm-opt",
            ),
            (
                BuildOptions {
                    debug_info: true,
                    ..wasm
                },
                "-g",
            ),
        ] {
            let error = options.validate().unwrap_err();
            assert_eq!(error.code, "E2000", "{option}");
            assert_eq!(
                error.message,
                format!(
                    "--backend fast does not support {option} for wasm32; remove it or use --backend llvm"
                )
            );
        }
        let error = BuildOptions {
            target: Target::Wasm64,
            ..wasm
        }
        .validate()
        .unwrap_err();
        assert_eq!(
            error.message,
            "--backend fast supports only the native target and wasm32; use --backend llvm for wasm64"
        );
        let error = BuildOptions {
            emit: Emit::Llvm,
            ..wasm
        }
        .validate()
        .unwrap_err();
        assert_eq!(
            error.message,
            "--backend fast produces only wasm and object outputs for wasm32; use --backend llvm for --emit llvm"
        );
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
        let source = task_runtime_source_for(Platform::HOST);
        assert_eq!(source.contains(include), !cfg!(windows));
        assert_eq!(source.contains("TZ_TASK_WINDOWS_H"), cfg!(windows));
        // A Windows target inlines it on every host, and no other target does.
        for triple in Triple::ALL {
            let source = task_runtime_source_for(Platform::new(Some(triple)));
            assert_eq!(
                source.contains(include),
                !Platform::new(Some(triple)).windows()
            );
            assert_eq!(
                source.contains("TZ_TASK_WINDOWS_H"),
                Platform::new(Some(triple)).windows()
            );
        }
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

    /// A target that is not the machine the compiler runs on, which never depends on the host.
    fn foreign_triple() -> Triple {
        Triple::ALL
            .into_iter()
            .find(|triple| Some(*triple) != Triple::host() && !triple.is_bare_metal())
            .unwrap()
    }

    #[test]
    fn named_targets_choose_clang_arguments_and_output_names() {
        use Triple::*;
        let args = |triple| {
            compile_args(Platform::new(Some(triple)))
                .iter()
                .map(|argument| argument.to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            args(Aarch64AppleDarwin),
            ["--target=arm64-apple-macosx12.0.0", "-fPIC"]
        );
        assert_eq!(
            args(X86_64LinuxMusl),
            ["--target=x86_64-unknown-linux-musl", "-fPIC"]
        );
        assert_eq!(
            args(X86_64FreeBsd),
            ["--target=x86_64-unknown-freebsd14", "-fPIC"]
        );
        assert_eq!(
            args(X86_64WindowsMsvc),
            [
                "--target=x86_64-pc-windows-msvc",
                "-mno-incremental-linker-compatible"
            ]
        );
        assert_eq!(
            args(Aarch64WindowsGnu),
            ["--target=aarch64-w64-windows-gnu"]
        );
        assert_eq!(
            args(Riscv64BareMetal),
            [
                "--target=riscv64-unknown-none-elf",
                "-march=rv64gc",
                "-mabi=lp64d",
                "-fPIC"
            ]
        );
        assert_eq!(
            args(X86_64BareMetal),
            ["--target=x86_64-unknown-none-elf", "-fPIC"]
        );
        // Without a target the arguments are the machine's own, as they always were.
        assert_eq!(
            compile_args(Platform::HOST),
            native_compile_args(cfg!(windows), env::consts::ARCH)
                .iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        );
        assert!(target_args(Platform::HOST).is_empty());
        assert!(link_args(Platform::HOST).is_empty());
        for triple in Triple::ALL {
            let platform = Platform::new(Some(triple));
            // Every named run starts with its Clang target, and only a Windows target has no -fPIC.
            assert_eq!(
                target_args(platform),
                [OsString::from(format!(
                    "--target={}",
                    triple.clang_target()
                ))]
            );
            assert_eq!(args(triple)[0], target_args(platform)[0].to_str().unwrap());
            assert_eq!(
                args(triple).contains(&"-fPIC".to_owned()),
                !platform.windows()
            );
            // LLD links what the machine's own linker cannot: everything but Apple targets from macOS.
            let expected = platform.cross() && !(platform.mach_o() && cfg!(target_os = "macos"));
            assert_eq!(
                link_args(platform).is_empty(),
                !expected,
                "{}",
                triple.name()
            );
        }
        for (triple, executable, object, shared) in [
            (Aarch64AppleDarwin, "app", "app.o", "app.dylib"),
            (X86_64LinuxGnu, "app", "app.o", "app.so"),
            (Aarch64LinuxAndroid, "app", "app.o", "app.so"),
            (X86_64FreeBsd, "app", "app.o", "app.so"),
            (X86_64WindowsMsvc, "app.exe", "app.obj", "app.dll"),
            (Aarch64WindowsGnu, "app.exe", "app.obj", "app.dll"),
            (Aarch64AppleIosSim, "app", "app.o", "app.dylib"),
        ] {
            let path = |emit| {
                BuildOptions {
                    emit,
                    triple: Some(triple),
                    ..BuildOptions::default()
                }
                .output_path(Path::new("app.tz"))
                .to_str()
                .unwrap()
                .to_owned()
            };
            assert_eq!(path(Emit::Executable), executable, "{}", triple.name());
            assert_eq!(path(Emit::Object), object, "{}", triple.name());
            assert_eq!(path(Emit::Shared), shared, "{}", triple.name());
            assert_eq!(path(Emit::Llvm), "app.ll");
        }
    }

    #[test]
    fn the_sysroot_reaches_the_runs_that_read_the_targets_libraries() {
        use Triple::*;
        let sysroot = || Some(OsString::from("/sdk"));
        let args = |triple: Triple, sysroot: Option<OsString>| {
            sdk_args_for(Platform::new(Some(triple)), sysroot)
                .map(|arguments| {
                    arguments
                        .iter()
                        .map(|argument| argument.to_str().unwrap().to_owned())
                        .collect::<Vec<_>>()
                })
                .map_err(|error| error.message)
        };
        assert_eq!(args(X86_64LinuxGnu, None), Ok(vec![]));
        assert_eq!(
            args(X86_64LinuxGnu, sysroot()),
            Ok(vec!["--sysroot=/sdk".to_owned()])
        );
        assert_eq!(
            args(Aarch64WindowsGnu, sysroot()),
            Ok(vec!["--sysroot=/sdk".to_owned()])
        );
        // Apple's Clang takes the SDK as -isysroot, and goes on with the macOS one if it is missing.
        assert_eq!(
            args(Aarch64AppleIos, None),
            Ok(vec!["-Werror=incompatible-sysroot".to_owned()])
        );
        assert_eq!(
            args(Aarch64AppleIos, sysroot()),
            Ok(vec![
                "-Werror=incompatible-sysroot".to_owned(),
                "-isysroot".to_owned(),
                "/sdk".to_owned()
            ])
        );
        // The targets that read no sysroot say so instead of ignoring it.
        for triple in [X86_64WindowsMsvc, Riscv64BareMetal] {
            let error = args(triple, sysroot()).unwrap_err();
            assert!(
                error.starts_with(&format!(
                    "TSUZURI_SYSROOT does not apply to {}",
                    triple.name()
                )),
                "{error}"
            );
            assert_eq!(args(triple, None), Ok(vec![]));
        }
        // The machine's own target, and WebAssembly, never read it.
        assert_eq!(
            sdk_args_for(Platform::HOST, sysroot()).map_err(|error| error.message),
            Ok(vec![])
        );
    }

    #[test]
    fn lld_is_refused_with_size_levels_for_mach_o_targets_only() {
        let lld = Some(OsStr::new("lld"));
        for platform in Triple::ALL
            .into_iter()
            .map(|triple| Platform::new(Some(triple)))
            .chain([Platform::HOST])
        {
            for level in [OptLevel::Os, OptLevel::Oz] {
                let result = check_linker_with_optimization(platform, lld, level);
                if platform.mach_o() {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, "E2000");
                    assert!(error.message.contains(level.flag()));
                } else {
                    assert!(result.is_ok());
                }
                // Unset, `default` and the other levels are never refused.
                assert!(check_linker_with_optimization(platform, None, level).is_ok());
                assert!(
                    check_linker_with_optimization(platform, Some(OsStr::new("default")), level)
                        .is_ok()
                );
            }
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2, OptLevel::O3] {
                assert!(check_linker_with_optimization(platform, lld, level).is_ok());
            }
        }
    }

    #[test]
    fn the_linker_choice_adds_lld_to_the_final_link_and_rejects_other_values() {
        let lld = OsString::from("-fuse-ld=lld");
        let choose = |platform, value: &str| final_link_args_for(platform, Some(OsStr::new(value)));
        for platform in Triple::ALL
            .into_iter()
            .map(|triple| Platform::new(Some(triple)))
            .chain([Platform::HOST])
        {
            // Unset, empty and `default` are the arguments that a link had before the variable existed.
            let default = final_link_args_for(platform, None).unwrap();
            assert_eq!(default, link_args(platform));
            assert_eq!(choose(platform, "").unwrap(), default);
            assert_eq!(choose(platform, "default").unwrap(), default);
            if platform.windows() {
                let error = choose(platform, "lld").unwrap_err();
                assert_eq!(error.code, "E2000");
                assert!(error.message.contains("not supported for Windows"));
            } else {
                // One flag, even where the machine's own linker could not write the target anyway.
                let chosen = choose(platform, "lld").unwrap();
                assert_eq!(
                    chosen.iter().filter(|argument| **argument == lld).count(),
                    1
                );
                assert!(default.iter().all(|argument| chosen.contains(argument)));
            }
            for bad in ["gold", "LLD", "lld ", "mold"] {
                let error = choose(platform, bad).unwrap_err();
                assert_eq!(error.code, "E2000");
                assert!(
                    error.message.contains("must be 'default' or 'lld'"),
                    "{bad}"
                );
            }
        }
    }

    #[test]
    fn a_runtime_object_that_is_staged_already_is_not_renamed_onto_itself() {
        let directory = TemporaryDirectory::new(&env::temp_dir()).unwrap();
        // The unit `wasi` compiles to `wasi.o`, where the WASI runtime is staged. A rename of that path onto
        // itself would fail here (the file is not there), so passing means it is skipped.
        let wasi = directory.path.join("wasi.o");
        stage_runtime_object(&wasi, &wasi, "stage WASI runtime").unwrap();
        assert!(!wasi.exists());
        // A unit with a name of its own is moved to the staging name.
        let cached = directory.path.join("native-c.o");
        let staged = directory.path.join("task.o");
        fs::write(&cached, b"object").unwrap();
        stage_runtime_object(&cached, &staged, "stage native runtime").unwrap();
        assert!(!cached.exists());
        assert_eq!(fs::read(&staged).unwrap(), b"object");
        // What it could not move is reported against the staged file.
        let error = stage_runtime_object(&cached, &staged, "stage native runtime").unwrap_err();
        assert!(
            error.message.contains("stage native runtime"),
            "{}",
            error.message
        );
    }

    #[test]
    fn cross_hints_name_what_the_target_needs() {
        let host = cross_hint(Platform::HOST, "install Clang");
        assert_eq!(host, "install Clang");
        let foreign = Platform::new(Some(foreign_triple()));
        let hint = cross_hint(foreign, "install Clang");
        let linker = if link_args(foreign).is_empty() {
            " and"
        } else {
            ", LLD, and"
        };
        assert!(
            hint.starts_with(&format!(
                "cross-compiling to {} needs a Clang with the {} backend{linker} ",
                foreign.triple().unwrap().name(),
                foreign.triple().unwrap().arch_name()
            )),
            "{hint}"
        );
        assert!(hint.ends_with("; install Clang"));
        // An iOS target asks for its SDK, and an MSVC one for INCLUDE and LIB, unless it is the host's own.
        assert!(
            cross_hint(Platform::new(Some(Triple::Aarch64AppleIos)), "x")
                .contains("set TSUZURI_SYSROOT to it")
        );
        assert!(
            cross_hint(Platform::new(Some(Triple::Aarch64WindowsMsvc)), "x").contains("INCLUDE")
                || Triple::host() == Some(Triple::Aarch64WindowsMsvc)
        );
        // The bundled toolchain is advised only for the targets it has a C library for, and LLD only
        // where Clang is told to link with it (Apple's linker writes Mach-O on a Mac).
        for triple in Triple::ALL {
            let platform = Platform::new(Some(triple));
            if !platform.cross() || triple.is_msvc() || platform.mach_o() {
                continue;
            }
            let hint = cross_hint(platform, "x");
            if triple.is_bare_metal() {
                assert_eq!(
                    hint,
                    format!(
                        "cross-compiling to {} needs a Clang with the {} backend; x",
                        triple.name(),
                        triple.arch_name()
                    )
                );
                continue;
            }
            assert_eq!(
                hint.contains("use the bundled toolchain"),
                triple.has_bundled_c_library(),
                "{hint}"
            );
            assert_eq!(
                hint.contains("the bundled toolchain has none"),
                !triple.has_bundled_c_library(),
                "{hint}"
            );
            assert!(hint.contains(", LLD, and"), "{hint}");
        }
        let apple = Platform::new(Some(Triple::Aarch64AppleIos));
        assert_eq!(
            cross_hint(apple, "x").contains("LLD"),
            !cfg!(target_os = "macos")
        );
    }

    #[test]
    fn codeview_follows_the_target_not_the_machine() {
        let msvc = Platform::new(Some(Triple::X86_64WindowsMsvc));
        for clang in ["clang", "/usr/bin/clang-21", "C:\\llvm\\bin\\clang.exe"] {
            assert!(codeview_for(msvc, OsStr::new(clang)), "{clang}");
        }
        for launcher in ["/opt/tsuzuri/bin/tsuzuri-clang", "tsuzuri-clang.exe"] {
            assert!(!codeview_for(msvc, OsStr::new(launcher)), "{launcher}");
        }
        for triple in [Triple::X86_64WindowsGnu, Triple::X86_64LinuxGnu] {
            assert!(!codeview_for(
                Platform::new(Some(triple)),
                OsStr::new("clang")
            ));
        }
        // Without a named target the machine decides, as the MSVC linker check did.
        assert_eq!(
            codeview_for(Platform::HOST, OsStr::new("clang")),
            cfg!(windows)
        );
    }

    #[test]
    fn cross_rejects_cpu_native_wgsl_and_unreachable_combinations() {
        let foreign = foreign_triple();
        let options = |emit, triple| BuildOptions {
            emit,
            triple: Some(triple),
            ..BuildOptions::default()
        };
        let error = BuildOptions {
            cpu: Cpu::Native,
            ..options(Emit::Executable, foreign)
        }
        .validate()
        .unwrap_err();
        assert_eq!(
            (error.code, error.message),
            (
                "E2000",
                format!(
                    "'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to {}",
                    foreign.name()
                )
            )
        );
        // The named machine itself may tune for its own CPU.
        if let Some(host) = Triple::host() {
            BuildOptions {
                cpu: Cpu::Native,
                ..options(Emit::Object, host)
            }
            .validate()
            .unwrap_or_else(|error| {
                // Only an architecture without a tuning flag may refuse, and then for that reason.
                assert!(
                    error.message.contains("is not supported on"),
                    "{}",
                    error.message
                );
            });
        }
        for emit in [Emit::Wgsl, Emit::Spirv] {
            assert_eq!(options(emit, foreign).validate().unwrap_err().code, "E2000");
        }
        // WebAssembly output has no named native target.
        let error = BuildOptions {
            target: Target::Wasm32,
            emit: Emit::Wasm,
            triple: Some(foreign),
            ..BuildOptions::default()
        }
        .validate()
        .unwrap_err();
        assert!(
            error.message.contains("names a native target"),
            "{}",
            error.message
        );
        assert!(options(Emit::Wasm, foreign).validate().is_err());
        // Debug information needs the tool of its own kind of host: dsymutil, or the MSVC linker.
        let debug = |triple| BuildOptions {
            debug_info: true,
            ..options(Emit::Executable, triple)
        };
        assert_eq!(
            debug(Triple::Aarch64AppleIos).validate().is_err(),
            !cfg!(target_os = "macos")
        );
        assert_eq!(
            debug(Triple::Aarch64WindowsMsvc).validate().is_err(),
            !cfg!(windows)
        );
        for triple in [
            Triple::X86_64LinuxMusl,
            Triple::X86_64FreeBsd,
            Triple::X86_64WindowsGnu,
        ] {
            debug(triple).validate().unwrap();
        }
    }

    #[test]
    fn bare_metal_targets_build_only_freestanding_objects() {
        for triple in Triple::ALL
            .into_iter()
            .filter(|triple| triple.is_bare_metal())
        {
            let options = |emit, freestanding| BuildOptions {
                emit,
                freestanding,
                allocator: llvm::Allocator::Host,
                triple: Some(triple),
                ..BuildOptions::default()
            };
            for emit in [Emit::Object, Emit::Llvm, Emit::Header] {
                options(emit, true).validate().unwrap();
                let error = options(emit, false).validate().unwrap_err();
                assert_eq!(
                    error.message,
                    format!(
                        "{} has no operating system: build it with --freestanding --allocator host and --emit object, llvm, or header",
                        triple.name()
                    )
                );
            }
            // An executable or a shared library needs an operating system, with or without the option.
            for emit in [Emit::Executable, Emit::Shared] {
                for freestanding in [false, true] {
                    assert!(
                        options(emit, freestanding)
                            .validate()
                            .unwrap_err()
                            .message
                            .contains("has no operating system")
                    );
                }
            }
        }
    }

    #[test]
    fn f16_path_follows_target_cpu_and_host_features() {
        use llvm::F16Path::{Conversions, Hardware, Soft};
        let none = CpuFeatures::default();
        let f16c = CpuFeatures { f16c: true, ..none };
        let sapphire = CpuFeatures {
            f16c: true,
            avx512fp16: true,
            ..none
        };
        let fp16 = CpuFeatures { fp16: true, ..none };
        let every_host = [none, f16c, sapphire, fp16];
        let path = |wasm, os, arch, cpu, host| f16_path_with(true, wasm, os, arch, cpu, host);
        // WASM computes f16 in the runtime whatever the CPU features say: there are no `half`
        // instructions, and LLVM would call `__extendhfsf2` and `__truncsfhf2`.
        for host in every_host {
            for cpu in [Cpu::Generic, Cpu::Native] {
                for arch in ["aarch64", "x86_64", "riscv64"] {
                    assert_eq!(path(true, "linux", arch, cpu, host), Soft, "{arch}");
                }
            }
        }
        // AArch64 lowers `half` with base instructions where it has no FEAT_FP16 arithmetic.
        for os in ["macos", "linux", "windows", "freebsd"] {
            for host in every_host {
                for cpu in [Cpu::Generic, Cpu::Native] {
                    assert_eq!(path(false, os, "aarch64", cpu, host), Hardware, "{os}");
                }
            }
        }
        // Narrowed, an AArch64 build needs a guarantee: macOS, or a native build that reports it.
        let narrowed = |os, cpu, host| f16_path_with(false, false, os, "aarch64", cpu, host);
        assert_eq!(narrowed("macos", Cpu::Generic, none), Hardware);
        assert_eq!(narrowed("macos", Cpu::Native, none), Hardware);
        assert_eq!(narrowed("linux", Cpu::Generic, fp16), Soft);
        assert_eq!(narrowed("windows", Cpu::Generic, none), Soft);
        assert_eq!(narrowed("linux", Cpu::Native, none), Soft);
        assert_eq!(narrowed("linux", Cpu::Native, fp16), Hardware);
        assert_eq!(narrowed("windows", Cpu::Native, fp16), Hardware);
        // x86-64: F16C is not in the baseline, so only a native build, whose CPU the host
        // detected, can use it; and a CPU with AVX512-FP16 has the arithmetic too.
        for os in ["macos", "linux", "windows"] {
            for host in every_host {
                assert_eq!(path(false, os, "x86_64", Cpu::Generic, host), Soft, "{os}");
            }
            assert_eq!(path(false, os, "x86_64", Cpu::Native, none), Soft, "{os}");
            assert_eq!(path(false, os, "x86_64", Cpu::Native, fp16), Soft, "{os}");
            assert_eq!(
                path(false, os, "x86_64", Cpu::Native, f16c),
                Conversions,
                "{os}"
            );
            assert_eq!(
                path(false, os, "x86_64", Cpu::Native, sapphire),
                Hardware,
                "{os}"
            );
        }
        // Other targets have no lowering that was inspected: the runtime, always.
        for arch in ["x86", "arm", "riscv64", "powerpc64", "s390x", "loongarch64"] {
            for host in every_host {
                for cpu in [Cpu::Generic, Cpu::Native] {
                    assert_eq!(path(false, "linux", arch, cpu, host), Soft, "{arch}");
                }
            }
        }
        // The entry point applies the setting of `AARCH64_HALF_WITHOUT_FP16`, and the entry
        // point for the host reads the host.
        assert_eq!(
            f16_path(false, "linux", "aarch64", Cpu::Generic, none),
            f16_path_with(
                AARCH64_HALF_WITHOUT_FP16,
                false,
                "linux",
                "aarch64",
                Cpu::Generic,
                none
            )
        );
        for target in [Target::Wasm32, Target::Wasm64] {
            for cpu in [Cpu::Generic, Cpu::Native] {
                assert_eq!(platform_f16_path(Platform::HOST, target, cpu), Soft);
            }
        }
        if cfg!(target_arch = "x86_64") {
            assert_eq!(
                platform_f16_path(Platform::HOST, Target::Native, Cpu::Generic),
                Soft
            );
        }
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            for cpu in [Cpu::Generic, Cpu::Native] {
                assert_eq!(
                    platform_f16_path(Platform::HOST, Target::Native, cpu),
                    Hardware
                );
            }
        }
    }

    #[test]
    fn the_fast_backend_computes_f16_in_the_runtime_whatever_the_machine_can_do() {
        use llvm::F16Path::Soft;
        // The IR that the fast backend translates has no `half`, on a machine with hardware half too.
        for target in [Target::Native, Target::Wasm32, Target::Wasm64] {
            for cpu in [Cpu::Generic, Cpu::Native] {
                assert_eq!(
                    f16_path_for(Backend::Fast, Platform::HOST, target, cpu),
                    Soft,
                    "{target:?} {cpu:?}"
                );
                assert_eq!(
                    f16_path_for(Backend::Llvm, Platform::HOST, target, cpu),
                    platform_f16_path(Platform::HOST, target, cpu),
                    "{target:?} {cpu:?}"
                );
            }
        }
    }

    #[test]
    fn detected_cpu_features_are_consistent() {
        let features = CpuFeatures::detect();
        // AVX512-FP16 CPUs have F16C, and only the host's own architecture reports anything.
        assert!(!features.avx512fp16 || features.f16c);
        if cfg!(not(target_arch = "x86_64")) {
            assert!(!features.f16c && !features.avx512fp16);
        }
        if cfg!(not(target_arch = "aarch64")) {
            assert!(!features.fp16);
        }
        // Every Apple Silicon CPU has FEAT_FP16, and every AArch64 Windows and Linux CPU that
        // has it says so.
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            assert!(features.fp16);
        }
    }

    #[test]
    fn f16_cpu_flags_name_the_instructions_of_the_path() {
        use llvm::F16Path::{Conversions, Hardware, Soft};
        assert_eq!(f16_cpu_flags("x86_64", Soft), &[] as &[&str]);
        assert_eq!(f16_cpu_flags("x86_64", Conversions), ["-mf16c"]);
        assert_eq!(
            f16_cpu_flags("x86_64", Hardware),
            ["-mavx512fp16", "-mf16c"]
        );
        // AArch64 needs no flag: `half` lowers to base instructions without FEAT_FP16.
        for path in [Soft, Conversions, Hardware] {
            assert_eq!(f16_cpu_flags("aarch64", path), &[] as &[&str]);
        }
        // The Clang options of the IR of a native build: the CPU first, then the instructions.
        assert_eq!(
            native_ir_cpu_args("x86_64", Conversions).unwrap(),
            ["-march=native", "-mf16c"]
        );
        assert_eq!(
            native_ir_cpu_args("x86_64", Hardware).unwrap(),
            ["-march=native", "-mavx512fp16", "-mf16c"]
        );
        assert_eq!(
            native_ir_cpu_args("x86_64", Soft).unwrap(),
            ["-march=native"]
        );
        assert_eq!(
            native_ir_cpu_args("aarch64", Hardware).unwrap(),
            ["-mcpu=native"]
        );
        assert!(native_ir_cpu_args("riscv64", Soft).is_err());
        // Every path that a `--cpu native` build can pick on x86-64 has the flags it needs.
        for host in [
            CpuFeatures::default(),
            CpuFeatures {
                f16c: true,
                ..CpuFeatures::default()
            },
            CpuFeatures {
                f16c: true,
                avx512fp16: true,
                ..CpuFeatures::default()
            },
        ] {
            let path = f16_path(false, "linux", "x86_64", Cpu::Native, host);
            let flags = f16_cpu_flags("x86_64", path);
            assert_eq!(flags.contains(&"-mf16c"), path != Soft);
            assert_eq!(flags.contains(&"-mavx512fp16"), path == Hardware);
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
            edition: Edition::LATEST,
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
