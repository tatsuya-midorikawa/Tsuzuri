use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::check::{CheckedModule, ModuleOrigin};
use crate::diagnostic::{Diagnostic, Span};
use crate::llvm::{self, Entry};
use crate::syntax::{MAX_SOURCE_BYTES, SourceKind};

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
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            emit: Emit::Executable,
            optimization: 3,
            cpu: Cpu::Generic,
        }
    }
}

impl BuildOptions {
    pub fn validate(self) -> Result<(), Diagnostic> {
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
            name: name.to_owned(),
            text,
            origin: ModuleOrigin::User,
        })
    }
}

impl Project {
    pub fn load(input: &Path) -> Result<Self, SourceError> {
        let metadata = fs::metadata(input)
            .map_err(|error| SourceError::new(input, io_error("inspect source", input, error)))?;
        let input = if metadata.is_dir() {
            input.join("Main.tz")
        } else {
            input.to_owned()
        };
        let root = SourceFile::read(&input)?;
        let parent = input
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let entries = fs::read_dir(parent).map_err(|error| {
            SourceError::new(parent, io_error("list module directory", parent, error))
        })?;
        let mut paths = Vec::new();
        let mut found_root = false;
        for entry in entries {
            let path = entry
                .map_err(|error| {
                    SourceError::new(parent, io_error("list module directory", parent, error))
                })?
                .path();
            if path.file_name() == input.file_name() {
                found_root = true;
            } else if source_kind(&path).is_some() {
                paths.push(path);
            }
        }
        if !found_root {
            return Err(SourceError::new(
                &input,
                driver_error(
                    "E1011",
                    "source filename must match its directory entry exactly, including case",
                ),
            ));
        }
        paths.sort();
        let mut sources = vec![root];
        for path in paths {
            sources.push(SourceFile::read(&path)?);
        }
        sources.sort_by(|left, right| left.path.file_name().cmp(&right.path.file_name()));
        let root = sources
            .iter()
            .position(|source| source.path == input)
            .unwrap();
        // The embedded standard library follows the user sources.
        sources.extend(crate::stdlib::SOURCES.iter().map(|(path, text)| {
            SourceFile {
                path: PathBuf::from(path),
                name: crate::stdlib::module_name(path)
                    .expect("embedded std paths are flat")
                    .to_owned(),
                text: (*text).to_owned(),
                origin: ModuleOrigin::Std,
            }
        }));
        Ok(Self { sources, root })
    }

    pub fn input(&self) -> &Path {
        &self.sources[self.root].path
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
                    ModuleOrigin::User => source.path.file_name().unwrap().to_str().unwrap(),
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
    let mut text = if options.emit == Emit::Header {
        llvm::header(module)
    } else {
        llvm::emit_target(
            module,
            if options.emit == Emit::Executable {
                Entry::Console
            } else {
                Entry::Library
            },
            options.target == Target::Wasm32,
        )?
    };
    if options.target == Target::Wasm32 && options.emit != Emit::Header {
        text.push_str(include_str!("runtime/wasm.ll"));
    }
    let task_runtime =
        options.target == Target::Native && text.contains("declare void @tsuzuri_task_parallel(");
    if task_runtime && !cfg!(unix) && !matches!(options.emit, Emit::Llvm | Emit::Header) {
        return Err(driver_error(
            "E2002",
            "native parallel tasks require a POSIX pthread toolchain; wasm32 provides the portable sequential backend",
        ));
    }
    protect_sources(project, output)?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create output directory", parent, error))?;
    let temporary = TemporaryDirectory::new(parent)?;
    let artifact = temporary
        .path
        .join(if options.emit == Emit::Executable && cfg!(windows) {
            "artifact.exe"
        } else {
            "artifact"
        });
    let mut messages = Vec::new();
    if matches!(options.emit, Emit::Llvm | Emit::Header) {
        fs::write(&artifact, text).map_err(|error| io_error("write output", &artifact, error))?;
    } else {
        let ir = temporary.path.join("module.ll");
        fs::write(&ir, text).map_err(|error| io_error("write LLVM IR", &ir, error))?;
        let runtime_object = temporary.path.join("task.o");
        if task_runtime {
            let runtime_source = temporary.path.join("task.c");
            fs::write(&runtime_source, include_str!("runtime/task.c"))
                .map_err(|error| io_error("write task runtime", &runtime_source, error))?;
            let mut runtime = Command::new(tool("TSUZURI_CLANG", "clang"));
            runtime
                .args(["-std=c11", "-fPIC", "-pthread", "-c"])
                .arg(format!("-O{}", options.optimization));
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
        }
        let mut clang = Command::new(tool("TSUZURI_CLANG", "clang"));
        clang
            .arg("-x")
            .arg("ir")
            .arg("-Wno-override-module")
            .arg(format!("-O{}", options.optimization));
        if options.target == Target::Wasm32 {
            clang
                .arg("--target=wasm32-unknown-unknown")
                .arg("-mbulk-memory");
        } else {
            clang.arg("-fPIC");
            if options.cpu == Cpu::Native {
                clang.arg(native_cpu_flag(env::consts::ARCH)?);
            }
        }
        if options.emit != Emit::Executable {
            clang.arg("-c");
        }
        let object = temporary.path.join("module.o");
        clang.arg(&ir).arg("-o").arg(
            if options.emit == Emit::Wasm || (task_runtime && options.emit == Emit::Object) {
                &object
            } else {
                &artifact
            },
        );
        if options.emit == Emit::Executable && !cfg!(windows) {
            clang.arg("-lm");
        }
        if task_runtime && options.emit == Emit::Executable {
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
        if task_runtime && options.emit == Emit::Object {
            let mut linker = Command::new(tool("TSUZURI_CLANG", "clang"));
            linker.args(["-r", "-nostdlib"]);
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
                .arg("--strip-all")
                .arg("--stack-first")
                .arg("-z")
                .arg("stack-size=1048576")
                .arg("--max-memory=16777216");
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
    // Recheck immediately before publishing; never replace a source through a path alias.
    protect_sources(project, output)?;
    fs::rename(&artifact, output).map_err(|error| io_error("publish output", output, error))?;
    temporary.close()?;
    Ok(messages)
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
    let messages = build(module, project, &output, options)?;
    let status = Command::new(&output)
        .status()
        .map_err(|error| io_error("run executable", &output, error))?;
    temporary.close()?;
    if !status.success() {
        return Err(driver_error(
            "E2005",
            format!(
                "program terminated with {status}; integer division, indexing, assert, or allocation may have trapped"
            ),
        ));
    }
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
        fs::create_dir(directory.path.join("nested")).unwrap();
        fs::write(directory.path.join("nested/Hidden.tz"), "invalid").unwrap();
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
