use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::check::{CheckedModule, ModuleOrigin};
use crate::diagnostic::{Diagnostic, Span};
use crate::llvm::{self, Entry};
use crate::syntax::{MAX_SOURCE_BYTES, SourceKind};

#[path = "test_runner.rs"]
mod test_runner;
pub use test_runner::{TestOptions, TestReport, TestResult, run_tests};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Native,
    Wasm32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cpu {
    Generic,
    Native,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emit {
    Executable,
    Object,
    Llvm,
    Header,
    Wasm,
}

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
        }
    }
}

impl BuildOptions {
    pub fn validate(self) -> Result<(), Diagnostic> {
        if self.wasm_simd && (self.target != Target::Wasm32 || self.emit == Emit::Header) {
            return Err(driver_error(
                "E2000",
                "--wasm-feature simd128 requires wasm32 object, LLVM IR, or WASM output",
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
        if self.optimization > 3 {
            return Err(driver_error(
                "E2000",
                "optimization level must be 0, 1, 2, or 3",
            ));
        }
        if (self.emit == Emit::Executable && self.target != Target::Native)
            || (self.emit == Emit::Wasm && self.target != Target::Wasm32)
        {
            return Err(driver_error(
                "E2000",
                "'--emit exe' requires '--target native'; '--emit wasm' requires '--target wasm32'",
            ));
        }
        if self.cpu == Cpu::Native {
            if self.target != Target::Native || matches!(self.emit, Emit::Llvm | Emit::Header) {
                return Err(driver_error(
                    "E2000",
                    "'--cpu native' requires native executable or object output",
                ));
            }
            native_cpu_flag(env::consts::ARCH)?;
        }
        Ok(())
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
        })
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

#[derive(Debug)]
pub struct SourceFile {
    /// A file path, or a virtual `std/Name.tz` path for std sources.
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub name: String,
    pub text: String,
    pub origin: ModuleOrigin,
}

#[derive(Debug)]
pub struct Project {
    pub sources: Vec<SourceFile>,
    pub root: usize,
}

#[derive(Debug)]
pub struct SourceError {
    pub path: PathBuf,
    pub diagnostic: Diagnostic,
}

impl SourceError {
    fn new(path: &Path, diagnostic: Diagnostic) -> Self {
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
        })
    }
}

fn collect_sources(root: &Path) -> Result<Vec<PathBuf>, SourceError> {
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
                pending.push((path, depth + 1));
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
                && parent.starts_with(&directory)
            {
                if text.len() > MAX_SOURCE_BYTES {
                    return Err(SourceError::new(
                        path,
                        driver_error("E0003", "source exceeds the 1 MiB limit"),
                    ));
                }
                let path = parent.join(path.file_name().ok_or_else(|| {
                    SourceError::new(
                        path,
                        driver_error("E1011", "a source file needs a filename"),
                    )
                })?);
                if path
                    .strip_prefix(&directory)
                    .unwrap()
                    .components()
                    .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                normalized.insert(path, text.clone());
            }
        }
        Self::load_from_root(&directory, None, &normalized)
    }

    fn load_from_root(
        directory: &Path,
        selected: Option<&Path>,
        overlays: &std::collections::BTreeMap<PathBuf, String>,
    ) -> Result<Self, SourceError> {
        let mut paths: std::collections::BTreeMap<_, _> = collect_sources(directory)?
            .into_iter()
            .map(|path| (path, None))
            .collect();
        for (path, text) in overlays {
            paths.insert(path.clone(), Some(text));
        }
        if paths.len() > 4096 {
            return Err(SourceError::new(
                directory,
                driver_error("E1017", "module discovery exceeds 4096 source files"),
            ));
        }
        let mut paths: Vec<_> = paths.into_iter().collect();
        paths.sort_by_cached_key(|(path, _)| path.to_string_lossy().replace('\\', "/"));
        let mut sources = Vec::new();
        for (path, overlay) in paths {
            let relative_path = path
                .strip_prefix(directory)
                .map_err(|_| {
                    SourceError::new(
                        &path,
                        driver_error("E1011", "source is outside the project root"),
                    )
                })?
                .to_owned();
            let name = crate::module_name_from_relative(&relative_path)
                .map_err(|error| SourceError::new(&path, error))?;
            if let Some(text) = overlay {
                sources.push(SourceFile {
                    path,
                    relative_path,
                    name,
                    text: text.clone(),
                    origin: ModuleOrigin::User,
                });
            } else {
                let mut source = SourceFile::read(&path)?;
                source.relative_path = relative_path;
                source.name = name;
                sources.push(source);
            }
        }
        let root = if let Some(selected) = selected {
            sources.iter().position(|source| source.relative_path == selected).ok_or_else(|| SourceError::new(&directory.join(selected), driver_error("E1011", "source filename must match its directory entry exactly, including case")))?
        } else {
            sources
                .iter()
                .position(|source| source.name == "Main")
                .unwrap_or(0)
        };
        sources.extend(crate::stdlib::SOURCES.iter().map(|(path, text)| {
            SourceFile {
                path: PathBuf::from(path),
                relative_path: PathBuf::from(path),
                name: crate::stdlib::module_name(path)
                    .expect("embedded std paths are flat")
                    .to_owned(),
                text: (*text).to_owned(),
                origin: ModuleOrigin::Std,
            }
        }));
        Ok(Self { sources, root })
    }

    pub fn load_for_tests(input: &Path) -> Result<Self, SourceError> {
        let metadata = fs::symlink_metadata(input).map_err(|error| {
            SourceError::new(input, io_error("inspect test input", input, error))
        })?;
        if !metadata.is_dir() {
            return Self::load(input);
        }
        let project = Self::load_from_root(input, None, &std::collections::BTreeMap::new())?;
        if !project
            .sources
            .iter()
            .any(|source| source.origin == ModuleOrigin::User)
        {
            return Err(SourceError::new(
                input,
                driver_error(
                    "E2000",
                    "test input directory has no .tz, .tt, or .tc sources",
                ),
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
        )
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
        &self.sources[error.span.source.unwrap_or(self.root)]
    }

    pub fn analyze(&self) -> Result<CheckedModule, Diagnostic> {
        self.analyze_all().map_err(crate::first_error)
    }

    pub fn analyze_all(&self) -> Result<CheckedModule, crate::diagnostic::DiagnosticSet> {
        let sources: Vec<_> = self
            .sources
            .iter()
            .map(|source| crate::SourceInput {
                path: match source.origin {
                    ModuleOrigin::User => source.relative_path.to_str().unwrap(),
                    ModuleOrigin::Std => source.path.to_str().unwrap(),
                },
                text: &source.text,
                origin: source.origin,
            })
            .collect();
        crate::analyze_inputs_all(&sources)
    }
}

pub fn read_source(path: &Path) -> Result<String, Diagnostic> {
    if source_kind(path).is_none() {
        return Err(driver_error(
            "E2000",
            "input files must use '.tz' (code), '.tt' (type classes), or '.tc' (computation builder); rename old '.tzr' code files to '.tz'",
        ));
    }
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

fn source_kind(path: &Path) -> Option<SourceKind> {
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
    build_complete(module, project, output, options).map(|(messages, _)| messages)
}

fn build_complete(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
) -> Result<(Vec<String>, Vec<crate::trap::TrapSite>), Diagnostic> {
    options.validate()?;
    if options.emit == Emit::Executable
        && project.input().file_name() != Some(OsStr::new("Main.tz"))
    {
        return Err(driver_error(
            "E2004",
            "an application must start from Main.tz; pass Main.tz or its directory, or use '--emit object' for a library",
        ));
    }
    if options.emit == Emit::Wasm && !module.functions.iter().any(|function| function.exported) {
        return Err(driver_error(
            "E2004",
            "a WebAssembly module needs at least one 'export def' entry point",
        ));
    }
    let mut trap_sites = Vec::new();
    let mut text = if options.emit == Emit::Header {
        llvm::header(module)
    } else {
        let emission = llvm::EmitOptions {
            entry: if options.emit == Emit::Executable {
                Entry::Console
            } else {
                Entry::Library
            },
            wasm: options.target == Target::Wasm32,
            debug_output: options.debug_output,
        };
        if options.debug_info {
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
    if options.target == Target::Wasm32 && options.emit != Emit::Header {
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
    let debug_import = options.target == Target::Wasm32 && text.contains("@tsuzuri_debug_write(");
    if task_runtime && !cfg!(unix) && !matches!(options.emit, Emit::Llvm | Emit::Header) {
        return Err(driver_error(
            "E2002",
            "native parallel tasks require a POSIX pthread toolchain; wasm32 provides the portable sequential backend",
        ));
    }
    protect_sources(project, output)?;
    let sidecar = options.trap_info.then(|| trap_sidecar_path(output));
    let dwarf_sidecar = (cfg!(target_os = "macos")
        && options.debug_info
        && options.emit == Emit::Executable)
        .then(|| {
            let mut path = output.as_os_str().to_owned();
            path.push(".dwarf");
            PathBuf::from(path)
        });
    if let Some(sidecar) = &sidecar {
        protect_sources(project, sidecar)?;
    }
    if let Some(sidecar) = &dwarf_sidecar {
        protect_sources(project, sidecar)?;
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
    if sidecar.is_some() {
        let table =
            project.with_trap_sources(|sources| crate::trap::side_table(&trap_sites, sources))?;
        fs::write(&staged_sidecar, table)
            .map_err(|error| io_error("write trap side table", &staged_sidecar, error))?;
    }
    if matches!(options.emit, Emit::Llvm | Emit::Header) {
        fs::write(&artifact, text).map_err(|error| io_error("write output", &artifact, error))?;
    } else {
        let mut ir = temporary.path.join("module.ll");
        fs::write(&ir, text).map_err(|error| io_error("write LLVM IR", &ir, error))?;
        let runtime_object = temporary.path.join("task.o");
        let merge_debug_ir = cfg!(target_os = "macos")
            && options.debug_info
            && task_runtime
            && options.emit == Emit::Object;
        if task_runtime {
            let runtime_source = temporary.path.join("task.c");
            fs::write(&runtime_source, include_str!("runtime/task.c"))
                .map_err(|error| io_error("write task runtime", &runtime_source, error))?;
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            runtime
                .args(["-std=c11", "-fPIC", "-pthread", "-c"])
                .arg(format!("-O{}", options.optimization));
            if options.debug_info {
                runtime.arg("-g");
            }
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
                    "parallel tasks require Clang and POSIX pthread headers",
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
        let mut clang = Command::new(tool("TSUZURI_CLANG", "clang"));
        clang
            .arg("-x")
            .arg("ir")
            .arg("-Wno-override-module")
            .arg(format!("-O{}", options.optimization));
        if options.debug_info {
            clang.arg("-g");
        }
        if options.target == Target::Wasm32 {
            clang
                .arg("--target=wasm32-unknown-unknown")
                .arg("-mbulk-memory");
            clang.arg(if options.wasm_simd {
                "-msimd128"
            } else {
                "-mno-simd128"
            });
        } else {
            clang.arg("-fPIC");
            if options.cpu == Cpu::Native {
                clang.arg(native_cpu_flag(env::consts::ARCH)?);
            }
        }
        if options.emit != Emit::Executable || dwarf_sidecar.is_some() {
            clang.arg("-c");
        }
        let object = temporary.path.join("module.o");
        clang.arg(&ir).arg("-o").arg(
            if options.emit == Emit::Wasm
                || dwarf_sidecar.is_some()
                || (task_runtime && options.emit == Emit::Object && !merge_debug_ir)
            {
                &object
            } else {
                &artifact
            },
        );
        if options.emit == Emit::Executable && !cfg!(windows) && dwarf_sidecar.is_none() {
            clang.arg("-lm");
        }
        if task_runtime && options.emit == Emit::Executable && dwarf_sidecar.is_none() {
            clang
                .args(["-x", "none"])
                .arg(&runtime_object)
                .arg("-pthread");
        }
        collect_message(
            &mut messages,
            run_tool(
                &mut clang,
                "install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable",
            )?,
        );
        if dwarf_sidecar.is_some() {
            let mut linker = Command::new(tool("TSUZURI_CLANG", "clang"));
            linker.arg(&object).args(["-g", "-lm"]);
            if task_runtime {
                linker.arg(&runtime_object).arg("-pthread");
            }
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
        if task_runtime && options.emit == Emit::Object && !merge_debug_ir {
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
        if options.emit == Emit::Wasm {
            let mut linker = Command::new(tool("TSUZURI_WASM_LD", "wasm-ld"));
            linker
                .arg("--no-entry")
                .arg("--stack-first")
                .arg("-z")
                .arg("stack-size=1048576")
                .arg("--max-memory=16777216");
            if !options.debug_info {
                linker.arg("--strip-all");
            }
            if debug_import {
                linker.arg("--export-memory");
            }
            if llvm::uses_host_abi(module) {
                linker.args([
                    "--export=tsuzuri_alloc",
                    "--export=tsuzuri_free",
                    "--export-memory",
                ]);
            }
            if options.trap_info {
                linker.arg("--export=tsuzuri_trap_site");
            }
            for function in &module.functions {
                if function.exported {
                    linker.arg(format!("--export=tz_{}", function.name));
                }
            }
            linker.arg(&object).arg("-o").arg(&artifact);
            collect_message(
                &mut messages,
                run_tool(
                    &mut linker,
                    "install LLVM LLD or set TSUZURI_WASM_LD to the wasm-ld executable",
                )?,
            );
        }
    }
    let sidecars: Vec<_> = sidecar
        .as_ref()
        .map(|path| (&staged_sidecar, path))
        .into_iter()
        .chain(dwarf_sidecar.as_ref().map(|path| (&staged_dwarf, path)))
        .collect();
    publish_outputs(project, &artifact, output, &sidecars, &mut temporary)?;
    temporary.close()?;
    Ok((messages, trap_sites))
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

pub fn trap_sidecar_path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_owned();
    name.push(".trap.json");
    PathBuf::from(name)
}

pub fn run(
    module: &CheckedModule,
    project: &Project,
    options: BuildOptions,
) -> Result<Vec<String>, Diagnostic> {
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
    )?;
    let result = Command::new(&output)
        .output()
        .map_err(|error| io_error("run executable", &output, error))?;
    temporary.close()?;
    io::stdout()
        .write_all(&result.stdout)
        .map_err(|error| io_error("relay program stdout", &output, error))?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let span = project
            .with_trap_sources(|sources| {
                sites
                    .iter()
                    .find(|site| {
                        site.message(sources)
                            .is_ok_and(|message| stderr.contains(&message))
                    })
                    .map(|site| site.span)
            })
            .unwrap_or_default();
        let message = if stderr.trim().is_empty() {
            format!(
                "program terminated with {}; integer division, indexing, assert, or allocation may have trapped",
                result.status
            )
        } else {
            format!(
                "program terminated with {}:\n{}",
                result.status,
                stderr.trim_end()
            )
        };
        return Err(Diagnostic::new("E2005", message, span));
    }
    io::stderr()
        .write_all(&result.stderr)
        .map_err(|error| io_error("relay program stderr", &output, error))?;
    Ok(messages)
}

fn tool(variable: &str, fallback: &str) -> OsString {
    env::var_os(variable).unwrap_or_else(|| fallback.into())
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
    for source in &project.sources {
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
            if source == destination || same_file(input, &metadata)? {
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
fn same_file(input: &Path, output: &fs::Metadata) -> Result<bool, Diagnostic> {
    use std::os::unix::fs::MetadataExt;
    let input_metadata =
        fs::metadata(input).map_err(|error| io_error("inspect source", input, error))?;
    Ok(input_metadata.dev() == output.dev() && input_metadata.ino() == output.ino())
}

#[cfg(not(unix))]
fn same_file(_input: &Path, _output: &fs::Metadata) -> Result<bool, Diagnostic> {
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

struct TemporaryDirectory {
    path: PathBuf,
    removed: bool,
}

impl TemporaryDirectory {
    fn new(parent: &Path) -> Result<Self, Diagnostic> {
        let parent = fs::canonicalize(parent)
            .map_err(|error| io_error("resolve temporary directory parent", parent, error))?;
        for _ in 0..128 {
            let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".tsuzuri-{}-{id}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
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

    fn close(mut self) -> Result<(), Diagnostic> {
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
        directory.close().unwrap();
    }

    #[test]
    fn validates_native_cpu_tuning_and_selects_architecture_flags() {
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
        assert_eq!(std.len(), crate::stdlib::SOURCES.len());
        assert!(
            std.iter()
                .all(|source| source.origin == ModuleOrigin::Std && source.path.starts_with("std"))
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
                ("Main.tz", "fn main() -> i64 { Other.value() }"),
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
