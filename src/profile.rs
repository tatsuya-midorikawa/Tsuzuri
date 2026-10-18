//! Profile-guided optimization (PR07): `--profile-generate DIR` and `--profile-use PATH`.
//!
//! Clang does the work on the module IR (`-x ir`): `-fprofile-generate=DIR` instruments it and
//! `-fprofile-use=FILE` applies an indexed profile. This module checks the options, stages the
//! profile for a build (merging a directory of raw profiles with `llvm-profdata`), and summarizes
//! the warnings of a profile that no longer matches. The IR that Tsuzuri generates is the same
//! with and without a profile.

use super::*;

/// The `llvm-profdata` that merges raw profiles (D3).
pub(crate) const PROFDATA_VARIABLE: &str = "TSUZURI_LLVM_PROFDATA";

/// The first 8 bytes of an indexed profile (`.profdata`).
const INDEXED_MAGIC: [u8; 8] = *b"\xfflprofi\x81";
/// The first 8 bytes of a raw profile (`.profraw`) of a 64-bit program, and of a 32-bit one.
const RAW_MAGICS: [[u8; 8]; 2] = [*b"\x81rforpl\xff", *b"\x81Rforpl\xff"];
/// The staged profile that Clang reads, in the build's temporary directory.
const STAGED: &str = "profile.profdata";
/// What `llvm-profdata merge --input-files` reads the raw profiles from, in the same directory.
const INPUT_LIST: &str = "profile-inputs.txt";

const MERGE_HINT: &str = "llvm-profdata must come from the same LLVM as the Clang that built the instrumented program; set TSUZURI_LLVM_PROFDATA";
const READ_HINT: &str = "the profile must be merged by the llvm-profdata that matches this Clang; regenerate it with --profile-generate";
const RUNTIME_HINT: &str = "--profile-generate links Clang's profile runtime (libclang_rt.profile): install compiler-rt, for example libclang-rt-21-dev, or use a Clang that has it";

/// How a build uses profile data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    /// No profile: the build is the one that there was before PGO.
    #[default]
    None,
    /// `--profile-generate DIR`: an instrumented program that writes raw profiles under this
    /// absolute directory when it exits.
    Generate(PathBuf),
    /// `--profile-use PATH`: an indexed profile, or a directory of raw profiles to merge.
    Use(PathBuf),
}

/// A profile that a build has staged: what Clang is told, and what the cache key records.
pub(crate) struct PreparedProfile {
    flag: OsString,
    key: (&'static str, Vec<u8>),
    generate: bool,
    /// A WebAssembly module has no profile runtime and no value-profiling runtime.
    wasm: bool,
    /// The staged copy that Clang reads, and the path that the user gave for it.
    staged: Option<(PathBuf, PathBuf)>,
}

impl Profile {
    /// An instrumented build whose program writes under `directory`, made absolute because the
    /// program decides where it runs.
    pub fn generate(directory: &Path) -> Result<Self, Diagnostic> {
        std::path::absolute(directory)
            .map(Self::Generate)
            .map_err(|error| io_error("resolve profile directory", directory, error))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The checks that need no tool: where the build runs and what it makes. Only native
    /// executables and objects of the machine that builds them, and WebAssembly modules of
    /// `wasm32`, can be instrumented and trained; every other output is rejected, so that a new
    /// kind of output is never accepted by default.
    pub fn validate(&self, options: BuildOptions) -> Result<(), Diagnostic> {
        if self.is_none() {
            return Ok(());
        }
        // Instrumentation and the use of a profile are Clang's work on the IR (PB05 does not make them).
        if options.backend == Backend::Fast {
            return Err(driver_error(
                "E2000",
                "--backend fast cannot instrument or optimize with a profile; use --backend llvm with --profile-generate and --profile-use",
            ));
        }
        if options.target.is_wasm() {
            return self.validate_wasm(options);
        }
        if !matches!(options.emit, Emit::Executable | Emit::Object) {
            return Err(driver_error(
                "E2000",
                "profile-guided optimization needs --emit exe or object; remove the profile option for other output kinds",
            ));
        }
        let platform = options.platform();
        if platform.windows() {
            return Err(driver_error(
                "E2000",
                "--profile-generate and --profile-use are not supported on Windows yet; build on macOS or Linux",
            ));
        }
        if let Some(triple) = options.triple.filter(|_| platform.cross()) {
            return Err(driver_error(
                "E2000",
                format!(
                    "--profile-generate and --profile-use apply to the machine that builds; remove --target {} for another machine",
                    triple.name()
                ),
            ));
        }
        if let Self::Generate(directory) = self {
            if options.freestanding {
                return Err(driver_error(
                    "E2000",
                    "--profile-generate cannot be combined with --freestanding: the profile runtime needs the C library",
                ));
            }
            if directory.to_string_lossy().contains('%') {
                return Err(driver_error(
                    "E2000",
                    "--profile-generate directory must not contain '%'; choose another directory",
                ));
            }
        }
        Ok(())
    }

    /// A `wasm32` module has no profile runtime: its host writes the counters (`profile-wasm.mjs`),
    /// so only a single-threaded module of `--emit wasm` is instrumented.
    fn validate_wasm(&self, options: BuildOptions) -> Result<(), Diagnostic> {
        let refuse = |message: &str| Err(driver_error("E2000", message));
        if options.target != Target::Wasm32 {
            return refuse(
                "profile-guided optimization supports --target wasm32 only; remove --target wasm64 or the profile option",
            );
        }
        if options.emit != Emit::Wasm {
            return refuse(
                "profile-guided optimization needs --emit wasm for WebAssembly; remove the profile option for other output kinds",
            );
        }
        if options.wasm_threads {
            return refuse(
                "--profile-generate and --profile-use cannot be combined with --wasm-feature threads: the counters are not atomic",
            );
        }
        if matches!(self, Self::Generate(_)) && options.wasm_opt {
            return refuse(
                "--profile-generate cannot be combined with --wasm-opt: Binaryen may drop what the profile writer reads",
            );
        }
        // The toolchain that builds and trains a module is only verified on macOS and Linux.
        if cfg!(windows) {
            return refuse(
                "--profile-generate and --profile-use are not supported on Windows yet; build on macOS or Linux",
            );
        }
        Ok(())
    }

    /// The bundled launcher `tsuzuri-clang` passes LLVM Clang only a fixed set of options, so it
    /// would build the program without the profile options and report success.
    pub(crate) fn require_llvm_clang(&self) -> Result<(), Diagnostic> {
        self.reject_launcher(&tool("TSUZURI_CLANG", "clang"))
    }

    fn reject_launcher(&self, clang: &OsStr) -> Result<(), Diagnostic> {
        if !self.is_none() && bundled_launcher(clang) {
            return Err(driver_error(
                "E2002",
                "profile options need an LLVM Clang: the bundled Clang launcher (tsuzuri-clang) passes no profile options to LLVM Clang and has no profile runtime or llvm-profdata; set TSUZURI_CLANG to an LLVM 21 Clang",
            ));
        }
        Ok(())
    }
}

/// Stages `profile` in `temporary` for one build of a native program, or of a `wasm32` module when
/// `wasm`; `None` for a build without a profile.
pub(crate) fn prepare_profile(
    profile: &Profile,
    wasm: bool,
    temporary: &Path,
    messages: &mut Vec<String>,
) -> Result<Option<PreparedProfile>, Diagnostic> {
    prepare_for(profile, wasm, temporary, messages, &find_profdata)
}

#[cfg(test)]
fn prepare_with(
    profile: &Profile,
    temporary: &Path,
    messages: &mut Vec<String>,
    profdata: &dyn Fn() -> Result<OsString, Diagnostic>,
) -> Result<Option<PreparedProfile>, Diagnostic> {
    prepare_for(profile, false, temporary, messages, profdata)
}

fn prepare_for(
    profile: &Profile,
    wasm: bool,
    temporary: &Path,
    messages: &mut Vec<String>,
    profdata: &dyn Fn() -> Result<OsString, Diagnostic>,
) -> Result<Option<PreparedProfile>, Diagnostic> {
    match profile {
        Profile::None => Ok(None),
        Profile::Generate(directory) => {
            let directory = std::path::absolute(directory)
                .map_err(|error| io_error("resolve profile directory", directory, error))?;
            // The directory of a WebAssembly module is only the default of its profile writer.
            let mut flag = OsString::from("-fprofile-generate");
            if !wasm {
                flag.push("=");
                flag.push(&directory);
            }
            Ok(Some(PreparedProfile {
                flag,
                key: (
                    "profile-generate-dir",
                    directory.as_os_str().as_encoded_bytes().to_vec(),
                ),
                generate: true,
                wasm,
                staged: None,
            }))
        }
        Profile::Use(path) => {
            let staged = temporary.join(STAGED);
            let metadata =
                fs::metadata(path).map_err(|error| io_error("read profile", path, error))?;
            if metadata.is_dir() {
                merge_directory(path, temporary, &staged, messages, profdata)?;
            } else {
                // Copying freezes what Clang reads and what the cache key hashes.
                fs::copy(path, &staged).map_err(|error| io_error("read profile", path, error))?;
            }
            check_indexed(path, &staged)?;
            let digest = crate::cache::file_digest(&staged)
                .map_err(|error| io_error("read profile", &staged, error))?;
            let mut flag = OsString::from("-fprofile-use=");
            flag.push(&staged);
            Ok(Some(PreparedProfile {
                flag,
                key: ("profile-use-sha256", digest.to_vec()),
                generate: false,
                wasm,
                staged: Some((staged, path.clone())),
            }))
        }
    }
}

/// The input list of `llvm-profdata merge --input-files` for `files`: one `1,<path>` line each. The
/// weight in front keeps a comma in a path from being read as the end of a weight, which a plain
/// path would be; only a line break cannot be carried.
fn input_list(files: &[PathBuf]) -> Result<Vec<u8>, Diagnostic> {
    let mut list = Vec::new();
    for file in files {
        let bytes = file.as_os_str().as_encoded_bytes();
        if bytes.contains(&b'\n') {
            return Err(driver_error(
                "E2000",
                format!(
                    "--profile-use cannot merge {file:?}: the name of a file has a line break; rename it"
                ),
            ));
        }
        list.extend_from_slice(b"1,");
        list.extend_from_slice(bytes);
        list.push(b'\n');
    }
    Ok(list)
}

/// Merges the raw profiles of `directory`, in the order of their names, into `staged`. The paths go to
/// `llvm-profdata` in a file of `temporary`: a directory may hold more raw profiles than the arguments
/// of a process can name.
fn merge_directory(
    directory: &Path,
    temporary: &Path,
    staged: &Path,
    messages: &mut Vec<String>,
    profdata: &dyn Fn() -> Result<OsString, Diagnostic>,
) -> Result<(), Diagnostic> {
    let entries = fs::read_dir(directory)
        .map_err(|error| io_error("read profile directory", directory, error))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| io_error("read profile directory", directory, error))?
            .path();
        if path.extension() == Some(OsStr::new("profraw"))
            && fs::metadata(&path).is_ok_and(|metadata| metadata.is_file())
        {
            files.push(path);
        }
    }
    if files.is_empty() {
        return Err(driver_error(
            "E2000",
            format!(
                "--profile-use directory '{}' has no .profraw files; run the program built with --profile-generate first",
                directory.display()
            ),
        ));
    }
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    let list = input_list(&files)?;
    let tool = profdata()?;
    let path = temporary.join(INPUT_LIST);
    fs::write(&path, list).map_err(|error| io_error("write profile list", &path, error))?;
    let mut input_files = OsString::from("--input-files=");
    input_files.push(&path);
    let mut command = Command::new(tool);
    command.arg("merge").arg("-o").arg(staged).arg(input_files);
    collect_message(messages, run_tool(&mut command, MERGE_HINT)?);
    Ok(())
}

/// The staged profile must be an indexed one: Clang reads a raw one as "bad magic".
fn check_indexed(path: &Path, staged: &Path) -> Result<(), Diagnostic> {
    let mut magic = [0u8; 8];
    let read = fs::File::open(staged)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok();
    if read && magic == INDEXED_MAGIC {
        return Ok(());
    }
    if read && RAW_MAGICS.contains(&magic) {
        return Err(driver_error(
            "E2000",
            format!(
                "'{}' is a raw profile; pass its directory to --profile-use or merge it with llvm-profdata",
                path.display()
            ),
        ));
    }
    Err(driver_error(
        "E2000",
        format!(
            "--profile-use expects an indexed .profdata file or a directory of .profraw files; '{}' is neither",
            path.display()
        ),
    ))
}

/// The `llvm-profdata` of this build's Clang (D3): the variable, the directory of Clang's
/// executable, then the toolchain that Clang reports. `PATH` is never searched: another LLVM's
/// tool reads the raw profile of this Clang's runtime only by chance.
fn find_profdata() -> Result<OsString, Diagnostic> {
    locate_profdata(
        env::var_os(PROFDATA_VARIABLE),
        &tool("TSUZURI_CLANG", "clang"),
    )
}

fn locate_profdata(variable: Option<OsString>, clang: &OsStr) -> Result<OsString, Diagnostic> {
    if let Some(value) = variable {
        return Ok(value);
    }
    let name = format!("llvm-profdata{}", env::consts::EXE_SUFFIX);
    // Beside the real executable of Clang, behind the links that name it (Homebrew's, apt's).
    let beside = crate::cache::executable_path(clang)
        .ok()
        .and_then(|path| path.parent().map(|directory| directory.join(&name)));
    if let Some(found) = beside.filter(|candidate| candidate.is_file()) {
        return Ok(found.into_os_string());
    }
    // Apple's `/usr/bin/clang` is a shim with nothing beside it, and a wrapper script is not Clang's
    // executable; Clang knows its own toolchain.
    toolchain_root(clang)
        .map(|root| root.join("bin").join(&name))
        .filter(|candidate| candidate.is_file())
        .map(PathBuf::into_os_string)
        .ok_or_else(|| {
            driver_error(
                "E2002",
                "cannot find llvm-profdata; set TSUZURI_LLVM_PROFDATA to the llvm-profdata that belongs to the Clang in TSUZURI_CLANG",
            )
        })
}

/// The root of Clang's toolchain, three levels above its resource directory
/// (`<root>/lib/clang/<version>`).
fn toolchain_root(clang: &OsStr) -> Option<PathBuf> {
    let started = crate::timings::start();
    let output = Command::new(clang)
        .arg("-print-resource-dir")
        .stdin(Stdio::null())
        .output();
    crate::timings::finish("tool.clang", started);
    let output = output.ok().filter(|output| output.status.success())?;
    let resource = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    resource.ancestors().nth(3).map(Path::to_path_buf)
}

impl PreparedProfile {
    /// The tag and bytes that the whole-build cache key records for this profile.
    pub(crate) fn cache_field(&self) -> (&'static str, &[u8]) {
        (self.key.0, &self.key.1)
    }

    /// The Clang option of the IR compile.
    pub(crate) fn flag(&self) -> &OsStr {
        &self.flag
    }

    /// The Clang options of the IR compile of a program or a module. A WebAssembly module has no
    /// runtime that records indirect-call targets, so neither the instrumentation nor the use of
    /// the profile deals with them (`-disable-vp`); the two must agree, or Clang calls the
    /// profile stale.
    pub(crate) fn args(&self) -> Vec<&OsStr> {
        let mut args = vec![self.flag.as_os_str()];
        if self.wasm {
            args.extend(["-mllvm", "-disable-vp"].map(OsStr::new));
        }
        args
    }

    /// Whether the program is instrumented, which makes the link read Clang's profile runtime.
    pub(crate) fn instruments(&self) -> bool {
        self.generate
    }

    /// Whether the build makes an instrumented WebAssembly module, which needs the writer of
    /// `profile-wasm.mjs` instead of a runtime.
    pub(crate) fn instruments_wasm(&self) -> bool {
        self.generate && self.wasm
    }

    /// The hint of a failing Clang run: `base`, and what this profile adds to the causes.
    pub(crate) fn hint(&self, base: String) -> String {
        match (self.generate, self.wasm) {
            (true, true) => base,
            (true, false) => format!("{base}; {RUNTIME_HINT}"),
            (false, _) => format!("{base}; {READ_HINT}"),
        }
    }

    /// What Clang printed, with the warnings of a profile that no longer matches summarized.
    pub(crate) fn clang_output(&self, text: String) -> String {
        if self.generate {
            text
        } else {
            summarize_profile_warnings(self.shown(text))
        }
    }

    /// The failure of a Clang run, with the profile named as the user gave it.
    pub(crate) fn clang_error(&self, mut error: Diagnostic) -> Diagnostic {
        error.message = self.shown(error.message);
        error
    }

    /// `text` with the staged copy of the profile, which is gone when the build ends, named as the
    /// user gave it.
    fn shown(&self, text: String) -> String {
        match &self.staged {
            Some((staged, given)) => {
                text.replace(&*staged.to_string_lossy(), &given.to_string_lossy())
            }
            None => text,
        }
    }
}

/// The warning of an instrumented `run` that ended normally but left no raw profile in `directory`
/// since `started`, for example because the directory cannot be written. `None` when a profile was
/// written, or when `LLVM_PROFILE_FILE` sent it elsewhere.
pub(crate) fn unwritten_profile(
    directory: &Path,
    started: std::time::SystemTime,
) -> Option<String> {
    if env::var_os("LLVM_PROFILE_FILE").is_some() {
        return None;
    }
    // A file system may round a time down, so a profile written at the very start still counts.
    let since = started - std::time::Duration::from_secs(2);
    let written = fs::read_dir(directory).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.path().extension() == Some(OsStr::new("profraw"))
                && entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .is_ok_and(|modified| modified >= since)
        })
    });
    (!written).then(|| {
        format!(
            "the program wrote no profile under '{}'; check that the directory can be written (a program that traps or is killed writes none)",
            directory.display()
        )
    })
}

/// The writer that a host runs to save the counters of an instrumented WebAssembly module, which
/// has no profile runtime: it is published beside the module as `app.wasm.profile.mjs`.
const WASM_WRITER: &str = include_str!("runtime/profile-wasm.mjs");

/// What `wasm-ld` must export for that writer: the bounds of the profile sections (the linker
/// makes them), the version of the raw format that the instrumentation wrote, and the memory.
pub(crate) const WASM_LINK_ARGS: [&str; 8] = [
    "--export=__start___llvm_prf_cnts",
    "--export=__stop___llvm_prf_cnts",
    "--export=__start___llvm_prf_data",
    "--export=__stop___llvm_prf_data",
    "--export=__start___llvm_prf_names",
    "--export=__stop___llvm_prf_names",
    "--export=__llvm_profile_raw_version",
    "--export-memory",
];

pub(crate) fn wasm_writer_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(".profile.mjs");
    PathBuf::from(path)
}

/// The writer, whose default directory is the one that `--profile-generate` named.
pub(crate) fn wasm_writer(directory: &Path) -> String {
    let directory = serde_json::to_string(&*directory.to_string_lossy())
        .expect("a string is JSON serializable");
    WASM_WRITER.replace("__TSUZURI_PROFILE_DIRECTORY__", &directory)
}

/// `ir` with the symbol that Clang's profile runtime would define. Without a definition, the
/// instrumentation refers to a runtime that a WebAssembly module does not have, and the link fails.
pub(crate) fn wasm_instrumented_ir(mut ir: String) -> String {
    ir.push_str("\n@__llvm_profile_runtime = hidden global i32 0\n");
    ir
}

/// Clang reports each function whose control flow no longer matches the profile on a line of its
/// own, which makes thousands of lines for a large program. One line says how many there are.
pub(crate) fn summarize_profile_warnings(text: String) -> String {
    const MISMATCH: &str = "(hash mismatch)";
    let functions = text.lines().filter(|line| line.contains(MISMATCH)).count();
    if functions == 0 {
        return text;
    }
    let mut summary = String::new();
    for line in text.lines() {
        if !line.contains(MISMATCH) && !is_warning_count(line) {
            summary.push_str(line);
            summary.push('\n');
        }
    }
    summary.push_str(&format!(
        "profile data does not match {functions} function(s) in this build; they are optimized without profile data. Rebuild with --profile-generate and rerun the training workload"
    ));
    summary
}

/// Clang's closing line, `3 warnings generated.`
fn is_warning_count(line: &str) -> bool {
    line.strip_suffix(" generated.")
        .and_then(|rest| {
            rest.strip_suffix(" warnings")
                .or_else(|| rest.strip_suffix(" warning"))
        })
        .is_some_and(|count| !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> TemporaryDirectory {
        TemporaryDirectory::new(&env::temp_dir()).unwrap()
    }

    fn no_profdata() -> Result<OsString, Diagnostic> {
        panic!("llvm-profdata must not run for this profile")
    }

    fn message(result: Result<Option<PreparedProfile>, Diagnostic>) -> String {
        match result {
            Err(error) => format!("{}: {}", error.code, error.message),
            Ok(_) => panic!("the profile was accepted"),
        }
    }

    #[test]
    fn summarizes_profile_hash_mismatch_warnings() {
        let stale = "warning: module.ll: function control flow change detected (hash mismatch) f Hash = 1 up to 0 count discarded [-Wbackend-plugin]\n\
                     warning: module.ll: function control flow change detected (hash mismatch) g Hash = 2 up to 7 count discarded [-Wbackend-plugin]\n\
                     2 warnings generated.\n";
        let summary = "profile data does not match 2 function(s) in this build; they are optimized without profile data. Rebuild with --profile-generate and rerun the training workload";
        assert_eq!(summarize_profile_warnings(stale.to_owned()), summary);
        // Other warnings stay before the summary, in their order.
        let mixed = "clang: warning: unrelated\n\
                     warning: module.ll: function control flow change detected (hash mismatch) f Hash = 1 up to 0 count discarded [-Wbackend-plugin]\n\
                     2 warnings generated.\n";
        assert_eq!(
            summarize_profile_warnings(mixed.to_owned()),
            format!(
                "clang: warning: unrelated\n{}",
                summary.replace("2 function", "1 function")
            )
        );
        // Without a mismatch the text is returned as it is, closing line included.
        for text in ["", "warning: unrelated\n1 warning generated.\n"] {
            assert_eq!(summarize_profile_warnings(text.to_owned()), text);
        }
    }

    #[test]
    fn recognizes_the_closing_line_of_the_warnings() {
        for line in [
            "1 warning generated.",
            "2 warnings generated.",
            "120 warnings generated.",
        ] {
            assert!(is_warning_count(line), "{line}");
        }
        for line in [
            "warnings generated.",
            "x warnings generated.",
            "1 error generated.",
            "2 warnings generated",
            "note: 2 warnings generated.",
        ] {
            assert!(!is_warning_count(line), "{line}");
        }
    }

    #[test]
    fn validates_profile_options() {
        let generate = Profile::Generate(PathBuf::from("/prof"));
        let used = Profile::Use(PathBuf::from("p.profdata"));
        let wasm = BuildOptions {
            target: Target::Wasm32,
            emit: Emit::Wasm,
            ..BuildOptions::default()
        };
        // A build without a profile is never refused here.
        Profile::None.validate(wasm).unwrap();
        // A `wasm32` module is instrumented and trained by its host; `wasm64` is not.
        if cfg!(windows) {
            assert_eq!(
                generate.validate(wasm).unwrap_err().message,
                "--profile-generate and --profile-use are not supported on Windows yet; build on macOS or Linux"
            );
        } else {
            generate.validate(wasm).unwrap();
            used.validate(wasm).unwrap();
        }
        let wasm64 = BuildOptions {
            target: Target::Wasm64,
            ..wasm
        };
        for profile in [&generate, &used] {
            assert_eq!(
                profile.validate(wasm64).unwrap_err().message,
                "profile-guided optimization supports --target wasm32 only; remove --target wasm64 or the profile option"
            );
        }
        for emit in [Emit::Object, Emit::Llvm, Emit::Bitcode, Emit::BindingsJs] {
            let options = BuildOptions { emit, ..wasm };
            assert_eq!(
                used.validate(options).unwrap_err().message,
                "profile-guided optimization needs --emit wasm for WebAssembly; remove the profile option for other output kinds",
                "{emit:?}"
            );
        }
        let threads = BuildOptions {
            wasm_threads: true,
            ..wasm
        };
        for profile in [&generate, &used] {
            assert_eq!(
                profile.validate(threads).unwrap_err().message,
                "--profile-generate and --profile-use cannot be combined with --wasm-feature threads: the counters are not atomic"
            );
        }
        let optimized = BuildOptions {
            wasm_opt: true,
            ..wasm
        };
        assert_eq!(
            generate.validate(optimized).unwrap_err().message,
            "--profile-generate cannot be combined with --wasm-opt: Binaryen may drop what the profile writer reads"
        );
        // The merged profile is read by Clang before Binaryen runs, so a build that uses it may use Binaryen.
        if !cfg!(windows) {
            used.validate(optimized).unwrap();
        }
        // Only an executable or an object is accepted; any other output is refused by default,
        // including the bitcode of --emit bitcode (PR08), which a host's own link would instrument.
        for emit in [
            Emit::Llvm,
            Emit::Bitcode,
            Emit::Header,
            Emit::Shared,
            Emit::Wgsl,
            Emit::WgslRelaxed,
            Emit::Spirv,
            Emit::SpirvRelaxed,
            Emit::BindingsJs,
            Emit::BindingsCs,
            Emit::BindingsPy,
            Emit::BindingsCpp,
        ] {
            let options = BuildOptions {
                emit,
                ..BuildOptions::default()
            };
            assert_eq!(
                used.validate(options).unwrap_err().message,
                "profile-guided optimization needs --emit exe or object; remove the profile option for other output kinds",
                "{emit:?}"
            );
        }
        let windows = BuildOptions {
            triple: Triple::parse("x86_64-pc-windows-msvc"),
            ..BuildOptions::default()
        };
        assert_eq!(
            generate.validate(windows).unwrap_err().message,
            "--profile-generate and --profile-use are not supported on Windows yet; build on macOS or Linux"
        );
        if cfg!(windows) {
            return;
        }
        for emit in [Emit::Executable, Emit::Object] {
            let options = BuildOptions {
                emit,
                ..BuildOptions::default()
            };
            generate.validate(options).unwrap();
            used.validate(options).unwrap();
        }
        // Another machine's target: the program could not run where it is trained.
        let other = if Triple::host() == Triple::parse("x86_64-unknown-linux-gnu") {
            "aarch64-unknown-linux-gnu"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        let cross = BuildOptions {
            triple: Triple::parse(other),
            ..BuildOptions::default()
        };
        for profile in [&generate, &used] {
            assert_eq!(
                profile.validate(cross).unwrap_err().message,
                format!(
                    "--profile-generate and --profile-use apply to the machine that builds; remove --target {other} for another machine"
                )
            );
        }
        // The machine's own target is not another machine.
        if let Some(host) = Triple::host() {
            let own = BuildOptions {
                triple: Some(host),
                ..BuildOptions::default()
            };
            used.validate(own).unwrap();
        }
        let freestanding = BuildOptions {
            emit: Emit::Object,
            freestanding: true,
            ..BuildOptions::default()
        };
        assert_eq!(
            generate.validate(freestanding).unwrap_err().message,
            "--profile-generate cannot be combined with --freestanding: the profile runtime needs the C library"
        );
        used.validate(freestanding).unwrap();
        assert_eq!(
            Profile::Generate(PathBuf::from("/tmp/100%/prof"))
                .validate(BuildOptions::default())
                .unwrap_err()
                .message,
            "--profile-generate directory must not contain '%'; choose another directory"
        );
        // The directory of a profile to use is only read, so a '%' is harmless there.
        Profile::Use(PathBuf::from("/tmp/100%/prof"))
            .validate(BuildOptions::default())
            .unwrap();
    }

    #[test]
    fn refuses_the_bundled_launcher() {
        let launcher = OsStr::new("/dist/bin/tsuzuri-clang");
        let used = Profile::Use(PathBuf::from("p.profdata"));
        let error = used.reject_launcher(launcher).unwrap_err();
        assert_eq!(error.code, "E2002");
        assert!(
            error.message.starts_with(
                "profile options need an LLVM Clang: the bundled Clang launcher (tsuzuri-clang)"
            ),
            "{}",
            error.message
        );
        assert!(
            Profile::Generate(PathBuf::from("/p"))
                .reject_launcher(OsStr::new("tsuzuri-clang"))
                .is_err()
        );
        used.reject_launcher(OsStr::new("/usr/bin/clang")).unwrap();
        Profile::None.reject_launcher(launcher).unwrap();
    }

    #[test]
    fn prepares_profiles_and_rejects_raw_or_unknown_files() {
        let directory = scratch();
        let mut messages = Vec::new();
        // Instrumenting needs no file: the flag names the absolute directory, which the key records.
        let output = directory.path.join("out");
        let staged = prepare_with(
            &Profile::Generate(output.clone()),
            &directory.path,
            &mut messages,
            &no_profdata,
        )
        .unwrap()
        .unwrap();
        let mut flag = OsString::from("-fprofile-generate=");
        flag.push(&output);
        assert_eq!(staged.flag(), flag);
        assert!(staged.instruments());
        assert_eq!(
            staged.cache_field(),
            (
                "profile-generate-dir",
                output.as_os_str().as_encoded_bytes()
            )
        );
        assert!(
            !output.exists(),
            "the program creates the directory when it runs"
        );
        assert!(
            prepare_with(&Profile::None, &directory.path, &mut messages, &no_profdata)
                .unwrap()
                .is_none()
        );

        // An indexed profile is copied, and the cache key is the digest of the copy.
        let indexed = directory.path.join("p.profdata");
        let mut bytes = INDEXED_MAGIC.to_vec();
        bytes.extend_from_slice(b"profile body");
        fs::write(&indexed, &bytes).unwrap();
        let work = scratch();
        let staged = prepare_with(
            &Profile::Use(indexed.clone()),
            &work.path,
            &mut messages,
            &no_profdata,
        )
        .unwrap()
        .unwrap();
        let copy = work.path.join(STAGED);
        let mut flag = OsString::from("-fprofile-use=");
        flag.push(&copy);
        assert_eq!(staged.flag(), flag);
        assert!(!staged.instruments());
        assert_eq!(fs::read(&copy).unwrap(), bytes);
        let (tag, digest) = staged.cache_field();
        assert_eq!(tag, "profile-use-sha256");
        assert_eq!(digest, crate::cache::file_digest(&copy).unwrap());
        assert!(messages.is_empty());

        // A raw profile of either pointer size, and anything else that is not indexed, are refused.
        let refuse = |content: &[u8]| {
            let file = directory.path.join("other.profraw");
            fs::write(&file, content).unwrap();
            let work = scratch();
            message(prepare_with(
                &Profile::Use(file.clone()),
                &work.path,
                &mut Vec::new(),
                &no_profdata,
            ))
            .replace(&file.display().to_string(), "<file>")
        };
        for magic in RAW_MAGICS {
            assert_eq!(
                refuse(&magic),
                "E2000: '<file>' is a raw profile; pass its directory to --profile-use or merge it with llvm-profdata"
            );
        }
        for content in [&b"\x01\x02\x03\x04\x05\x06\x07\x08 more"[..], b"short", b""] {
            assert_eq!(
                refuse(content),
                "E2000: --profile-use expects an indexed .profdata file or a directory of .profraw files; '<file>' is neither"
            );
        }

        // A directory without a raw profile, and a path that is not there.
        let empty = directory.path.join("empty");
        fs::create_dir(&empty).unwrap();
        fs::write(empty.join("notes.txt"), "not a profile").unwrap();
        fs::create_dir(empty.join("nested.profraw")).unwrap();
        let work = scratch();
        assert_eq!(
            message(prepare_with(
                &Profile::Use(empty.clone()),
                &work.path,
                &mut Vec::new(),
                &no_profdata
            )),
            format!(
                "E2000: --profile-use directory '{}' has no .profraw files; run the program built with --profile-generate first",
                empty.display()
            )
        );
        let missing = directory.path.join("missing.profdata");
        let text = message(prepare_with(
            &Profile::Use(missing.clone()),
            &work.path,
            &mut Vec::new(),
            &no_profdata,
        ));
        assert!(
            text.starts_with(&format!(
                "E2001: cannot read profile '{}'",
                missing.display()
            )),
            "{text}"
        );
    }

    #[cfg(unix)]
    fn script(path: &Path, text: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A stand-in for llvm-profdata: it records its arguments in `merge.log` and the file that it is
    /// given with `--input-files` in `merge-list.log`, then writes an indexed profile.
    #[cfg(unix)]
    fn recording_profdata(directory: &Path) -> PathBuf {
        let tool = directory.join("fake-profdata");
        script(
            &tool,
            &format!(
                "#!/bin/sh\necho \"$@\" > '{log}'\nwhile [ $# -gt 0 ]; do\n  [ \"$1\" = -o ] && out=$2\n  case \"$1\" in --input-files=*) cp \"${{1#--input-files=}}\" '{list}' ;; esac\n  shift\ndone\nprintf '\\377lprofi\\201merged' > \"$out\"\necho 'warning: from the merge' >&2\n",
                log = directory.join("merge.log").display(),
                list = directory.join("merge-list.log").display()
            ),
        );
        tool
    }

    #[cfg(unix)]
    #[test]
    fn merges_a_directory_of_raw_profiles_in_name_order() {
        let directory = scratch();
        let tool = recording_profdata(&directory.path);
        let raw = directory.path.join("raw");
        fs::create_dir(&raw).unwrap();
        for name in ["b.profraw", "a.profraw", "c.txt", "d.PROFRAW"] {
            fs::write(raw.join(name), name).unwrap();
        }
        fs::create_dir(raw.join("z.profraw")).unwrap();
        let work = scratch();
        let mut messages = Vec::new();
        let profdata = || Ok(tool.clone().into_os_string());
        let staged = prepare_with(
            &Profile::Use(raw.clone()),
            &work.path,
            &mut messages,
            &profdata,
        )
        .unwrap()
        .unwrap();
        let copy = work.path.join(STAGED);
        // The paths are not arguments: a directory may hold more raw profiles than a process can be given.
        assert_eq!(
            fs::read_to_string(directory.path.join("merge.log"))
                .unwrap()
                .trim(),
            format!(
                "merge -o {} --input-files={}",
                copy.display(),
                work.path.join(INPUT_LIST).display()
            )
        );
        assert_eq!(
            fs::read_to_string(directory.path.join("merge-list.log")).unwrap(),
            format!(
                "1,{}\n1,{}\n",
                raw.join("a.profraw").display(),
                raw.join("b.profraw").display()
            )
        );
        assert_eq!(
            staged.cache_field().1,
            crate::cache::file_digest(&copy).unwrap()
        );
        // What the tool says is a warning of the build.
        assert_eq!(messages, ["warning: from the merge\n"]);

        // A failing merge is a tool failure with the hint that names the likely cause.
        let failing = directory.path.join("failing-profdata");
        script(
            &failing,
            "#!/bin/sh\necho 'error: no profile can be merged' >&2\nexit 1\n",
        );
        let work = scratch();
        let profdata = || Ok(failing.clone().into_os_string());
        let text = message(prepare_with(
            &Profile::Use(raw),
            &work.path,
            &mut Vec::new(),
            &profdata,
        ));
        assert!(text.starts_with("E2002: "), "{text}");
        assert!(text.contains("error: no profile can be merged"), "{text}");
        assert!(text.ends_with(MERGE_HINT), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn merges_more_raw_profiles_than_a_process_can_be_given_as_arguments() {
        let directory = scratch();
        let tool = recording_profdata(&directory.path);
        let raw = directory.path.join("raw");
        fs::create_dir(&raw).unwrap();
        // Long names make the paths exceed ARG_MAX (1 MiB on macOS, 2 MiB on Linux) with a few thousand files.
        let names: Vec<String> = (0..10_000)
            .map(|index| format!("{index:05}-{}.profraw", "x".repeat(230)))
            .collect();
        for name in &names {
            fs::write(raw.join(name), "").unwrap();
        }
        let paths: usize = names
            .iter()
            .map(|name| raw.join(name).as_os_str().len() + 1)
            .sum();
        assert!(paths > 2 << 20, "{paths} bytes of paths");
        let work = scratch();
        let profdata = || Ok(tool.clone().into_os_string());
        prepare_with(
            &Profile::Use(raw.clone()),
            &work.path,
            &mut Vec::new(),
            &profdata,
        )
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message))
        .unwrap();
        let arguments = fs::read_to_string(directory.path.join("merge.log")).unwrap();
        assert!(arguments.len() < 1024, "{} bytes", arguments.len());
        let list = fs::read_to_string(directory.path.join("merge-list.log")).unwrap();
        let expected: Vec<String> = names
            .iter()
            .map(|name| format!("1,{}", raw.join(name).display()))
            .collect();
        assert_eq!(list.lines().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn lists_raw_profiles_for_llvm_profdata_whatever_their_paths_hold() {
        // A path with a comma would be read as a weight without the "1," that every entry starts with.
        let files = [
            PathBuf::from("/work/a,b dir/x.profraw"),
            PathBuf::from("#odd/ y .profraw"),
            PathBuf::from("plain.profraw"),
        ];
        assert_eq!(
            input_list(&files).unwrap(),
            b"1,/work/a,b dir/x.profraw\n1,#odd/ y .profraw\n1,plain.profraw\n"
        );
        // A line break would split the entry in two, and no form of the list can carry it.
        let error = input_list(&[PathBuf::from("dir/a\nb.profraw")]).unwrap_err();
        assert_eq!(error.code, "E2000");
        assert_eq!(
            error.message,
            "--profile-use cannot merge \"dir/a\\nb.profraw\": the name of a file has a line break; rename it"
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_directory_with_a_raw_profile_whose_name_has_a_line_break() {
        let directory = scratch();
        let raw = directory.path.join("raw");
        fs::create_dir(&raw).unwrap();
        fs::write(raw.join("a\nb.profraw"), "").unwrap();
        let work = scratch();
        let text = message(prepare_with(
            &Profile::Use(raw),
            &work.path,
            &mut Vec::new(),
            &no_profdata,
        ));
        assert!(
            text.starts_with("E2000: --profile-use cannot merge \""),
            "{text}"
        );
        assert!(
            text.ends_with("a\\nb.profraw\": the name of a file has a line break; rename it"),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn finds_llvm_profdata_beside_clang_or_in_its_toolchain() {
        let directory = scratch();
        let root = fs::canonicalize(&directory.path).unwrap();
        let find = |variable: Option<&str>, clang: &Path| {
            locate_profdata(variable.map(OsString::from), clang.as_os_str()).map(PathBuf::from)
        };
        // The variable wins, whether or not it names anything.
        assert_eq!(
            find(Some("/nowhere/llvm-profdata"), Path::new("/nowhere/clang")).unwrap(),
            PathBuf::from("/nowhere/llvm-profdata")
        );

        // Beside the executable of Clang.
        let beside = root.join("beside");
        fs::create_dir_all(&beside).unwrap();
        script(&beside.join("clang"), "#!/bin/sh\nexit 1\n");
        let message = "cannot find llvm-profdata; set TSUZURI_LLVM_PROFDATA to the llvm-profdata that belongs to the Clang in TSUZURI_CLANG";
        let error = find(None, &beside.join("clang")).unwrap_err();
        assert_eq!((error.code, error.message.as_str()), ("E2002", message));
        fs::write(beside.join("llvm-profdata"), "").unwrap();
        assert_eq!(
            find(None, &beside.join("clang")).unwrap(),
            beside.join("llvm-profdata")
        );

        // Behind a link, as Homebrew and apt install it.
        let link = root.join("link");
        fs::create_dir_all(&link).unwrap();
        std::os::unix::fs::symlink(beside.join("clang"), link.join("clang")).unwrap();
        assert_eq!(
            find(None, &link.join("clang")).unwrap(),
            beside.join("llvm-profdata")
        );

        // A shim or a wrapper: Clang reports its toolchain, three levels above its resource directory.
        let toolchain = root.join("toolchain");
        fs::create_dir_all(toolchain.join("bin")).unwrap();
        fs::write(toolchain.join("bin/llvm-profdata"), "").unwrap();
        let shim = root.join("shim");
        fs::create_dir_all(&shim).unwrap();
        script(
            &shim.join("clang"),
            &format!(
                "#!/bin/sh\n[ \"$1\" = -print-resource-dir ] && echo '{}/lib/clang/21'\n",
                toolchain.display()
            ),
        );
        assert_eq!(
            find(None, &shim.join("clang")).unwrap(),
            toolchain.join("bin/llvm-profdata")
        );
        // A toolchain without the tool, and a Clang that cannot be run, both end in the error.
        fs::remove_file(toolchain.join("bin/llvm-profdata")).unwrap();
        assert_eq!(
            find(None, &shim.join("clang")).unwrap_err().message,
            message
        );
        assert_eq!(
            find(None, &root.join("absent/clang")).unwrap_err().message,
            message
        );
    }

    #[test]
    fn flags_and_cache_fields_follow_the_staged_profile() {
        let directory = scratch();
        let mut messages = Vec::new();
        let generate = prepare_with(
            &Profile::Generate(directory.path.join("a")),
            &directory.path,
            &mut messages,
            &no_profdata,
        )
        .unwrap()
        .unwrap();
        // The hint adds what likely went wrong, and only Clang's output of a profile in use is summarized.
        assert!(
            generate
                .hint("base".into())
                .starts_with("base; --profile-generate links Clang's profile runtime")
        );
        let text = "x (hash mismatch) y\n1 warning generated.\n".to_owned();
        assert_eq!(generate.clang_output(text.clone()), text);
        let indexed = directory.path.join("p.profdata");
        fs::write(&indexed, INDEXED_MAGIC).unwrap();
        let used = prepare_with(
            &Profile::Use(indexed),
            &directory.path,
            &mut messages,
            &no_profdata,
        )
        .unwrap()
        .unwrap();
        assert!(used.hint("base".into()).ends_with("; the profile must be merged by the llvm-profdata that matches this Clang; regenerate it with --profile-generate"));
        assert!(
            used.clang_output(text)
                .starts_with("profile data does not match 1 function(s)")
        );
        // The staged copy is gone when the build ends, so a failure names the profile that the user gave.
        let staged = directory.path.join(STAGED);
        let failure = driver_error(
            "E2002",
            format!("Error in reading profile {}: bad", staged.display()),
        );
        assert_eq!(
            used.clang_error(failure).message,
            format!(
                "Error in reading profile {}: bad",
                directory.path.join("p.profdata").display()
            )
        );
        let failure = driver_error("E2002", "not about a profile");
        assert_eq!(generate.clang_error(failure).message, "not about a profile");
    }

    #[test]
    fn prepares_webassembly_profiles_without_a_runtime() {
        let directory = scratch();
        let mut messages = Vec::new();
        let generate = prepare_for(
            &Profile::Generate(directory.path.join("prof")),
            true,
            &directory.path,
            &mut messages,
            &no_profdata,
        )
        .unwrap()
        .unwrap();
        // Nothing writes a file for a module, so Clang gets no directory; the writer carries it.
        assert_eq!(
            generate.args(),
            ["-fprofile-generate", "-mllvm", "-disable-vp"].map(OsStr::new)
        );
        assert!(generate.instruments() && generate.instruments_wasm());
        assert_eq!(generate.hint("base".into()), "base");
        let indexed = directory.path.join("p.profdata");
        fs::write(&indexed, INDEXED_MAGIC).unwrap();
        let used = prepare_for(
            &Profile::Use(indexed.clone()),
            true,
            &directory.path,
            &mut messages,
            &no_profdata,
        )
        .unwrap()
        .unwrap();
        let args = used.args();
        assert!(args[0].to_string_lossy().starts_with("-fprofile-use="));
        assert_eq!(args[1..], ["-mllvm", "-disable-vp"].map(OsStr::new));
        assert!(!used.instruments_wasm());
        // A native program keeps the one option that it had.
        let native = prepare_with(
            &Profile::Use(indexed),
            &directory.path,
            &mut messages,
            &no_profdata,
        )
        .unwrap()
        .unwrap();
        assert_eq!(native.args().len(), 1);
        assert!(!native.instruments_wasm());
    }

    #[test]
    fn the_wasm_writer_names_its_directory_and_the_module_defines_the_runtime_hook() {
        let text = wasm_writer(Path::new("/work/a \"b\"/prof"));
        assert!(text.contains("export const defaultDirectory = \"/work/a \\\"b\\\"/prof\";"));
        assert!(!text.contains("__TSUZURI_PROFILE_DIRECTORY__"));
        assert_eq!(
            wasm_writer_path(Path::new("out/app.wasm")),
            PathBuf::from("out/app.wasm.profile.mjs")
        );
        let ir = wasm_instrumented_ir("define void @f() {\n  ret void\n}\n".into());
        assert!(ir.starts_with("define void @f()"));
        assert!(ir.ends_with("@__llvm_profile_runtime = hidden global i32 0\n"));
        // Everything that the writer reads is exported: the bounds of its three sections, and the version.
        assert!(text.contains("__start___llvm_prf_${name}"));
        assert!(text.contains("__stop___llvm_prf_${name}"));
        assert!(text.contains("__llvm_profile_raw_version"));
        for section in ["cnts", "data", "names"] {
            for bound in ["start", "stop"] {
                let export = format!("--export=__{bound}___llvm_prf_{section}");
                assert!(WASM_LINK_ARGS.contains(&export.as_str()), "{export}");
            }
        }
        assert!(WASM_LINK_ARGS.contains(&"--export=__llvm_profile_raw_version"));
        assert!(WASM_LINK_ARGS.contains(&"--export-memory"));
    }

    #[test]
    fn warns_when_an_instrumented_run_wrote_no_profile() {
        if env::var_os("LLVM_PROFILE_FILE").is_some() {
            // The user chose where the profile goes, and the warning stays out of the way.
            assert_eq!(
                unwritten_profile(Path::new("absent"), std::time::SystemTime::now()),
                None
            );
            return;
        }
        let directory = scratch();
        let started = std::time::SystemTime::now();
        let missing = directory.path.join("absent");
        assert_eq!(
            unwritten_profile(&missing, started).unwrap(),
            format!(
                "the program wrote no profile under '{}'; check that the directory can be written (a program that traps or is killed writes none)",
                missing.display()
            )
        );
        assert!(unwritten_profile(&directory.path, started).is_some());
        // Other files are not profiles.
        fs::write(directory.path.join("notes.txt"), "x").unwrap();
        assert!(unwritten_profile(&directory.path, started).is_some());
        let profile = directory.path.join("default_1_0.profraw");
        fs::write(&profile, "x").unwrap();
        assert_eq!(unwritten_profile(&directory.path, started), None);
        // A profile of an earlier run is not the profile of this one.
        let hour = std::time::Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&profile)
            .unwrap()
            .set_modified(started - hour)
            .unwrap();
        assert!(unwritten_profile(&directory.path, started).is_some());
    }
}
