//! The I/O half of `tsuzuri bindgen` (E11): reads the header, runs Clang, and writes
//! the module that `crate::bindgen::generate` produces.

use super::*;
use crate::bindgen::{Arguments, Extras, Failure, HeaderInfo, MARKER, MAX_AST_BYTES};
use crate::cache::Sha256;

const CLANG_HINT: &str = "install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable";
const HEADER_HINT: &str =
    "fix the header, or pass the directories that it includes with --include-dir";

/// Generates `arguments.output` from `arguments.header`. Returns the header text, for
/// showing the `W2002` warnings at their declarations, and the warnings. Nothing is
/// written when this fails.
pub fn bindgen(arguments: &Arguments) -> Result<(String, Vec<Diagnostic>), SourceError> {
    let header = &arguments.header;
    let at_header = |diagnostic| SourceError::new(header, diagnostic);
    let metadata = fs::metadata(header)
        .map_err(|error| at_header(io_error("inspect header", header, error)))?;
    if !metadata.is_file() {
        return Err(at_header(driver_error(
            "E2001",
            "the header must be a regular file",
        )));
    }
    let mut bytes = Vec::new();
    fs::File::open(header)
        .and_then(|file| file.take(MAX_AST_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|error| at_header(io_error("read header", header, error)))?;
    if bytes.len() > MAX_AST_BYTES {
        return Err(at_header(driver_error(
            "E2001",
            format!("the header exceeds {MAX_AST_BYTES} bytes"),
        )));
    }
    let path = crate::cache::real_path(header)
        .map_err(|error| at_header(io_error("resolve header", header, error)))?;
    let path_text = path
        .to_str()
        .ok_or_else(|| at_header(driver_error("E2001", "the header path must be valid UTF-8")))?;
    let mut hash = Sha256::new();
    hash.update(&bytes);
    let sha256 = hash.hex();

    let clang = tool("TSUZURI_CLANG", "clang");
    let version = run_tool(Command::new(&clang).arg("--version"), CLANG_HINT).map_err(at_header)?;
    let clang_version = version.lines().next().unwrap_or("").trim().to_owned();
    let target = run_tool(Command::new(&clang).arg("-dumpmachine"), CLANG_HINT)
        .map_err(at_header)?
        .trim()
        .to_owned();
    if !crate::bindgen::lp64_target(&target) {
        return Err(at_header(driver_error(
            "E2002",
            format!(
                "bindgen supports only 64-bit LP64 targets (Linux and macOS); '{target}' is not supported"
            ),
        )));
    }
    let mut command = Command::new(&clang);
    command.args([
        "-x",
        "c",
        "-std=gnu17",
        "-fsyntax-only",
        "-Xclang",
        "-ast-dump=json",
    ]);
    for directory in &arguments.include_dirs {
        command.arg("-I").arg(directory);
    }
    command.arg(&path);
    let ast = capture(&mut command, "clang AST output", HEADER_HINT).map_err(at_header)?;
    // `-dD` keeps each `#define` with line markers that place it in its file; `-dM`
    // prints the macros as they end, after `#pragma pop_macro` too.
    let mut macros = Vec::new();
    for listing in ["-dD", "-dM"] {
        let mut command = Command::new(&clang);
        command.args(["-x", "c", "-std=gnu17", "-E", listing]);
        for directory in &arguments.include_dirs {
            command.arg("-I").arg(directory);
        }
        command.arg(&path);
        let output =
            capture(&mut command, "clang preprocessor output", HEADER_HINT).map_err(at_header)?;
        macros.push(String::from_utf8_lossy(&output).into_owned());
    }
    let unreadable = |error: &dyn std::fmt::Display| {
        at_header(driver_error(
            "E2002",
            format!(
                "clang produced an unreadable AST ({error}); use LLVM/Clang 17+ or bind a simpler header"
            ),
        ))
    };
    let ast: serde_json::Value =
        serde_json::from_slice(&ast).map_err(|error| unreadable(&error))?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let generated = crate::bindgen::generate_with(
        &ast,
        &HeaderInfo {
            path: path_text,
            file_name: &file_name,
            sha256: &sha256,
            clang_version: &clang_version,
            target: &target,
        },
        &Extras {
            preprocessed: Some(&macros[0]),
            final_macros: Some(&macros[1]),
            header_text: &bytes,
            buffers: &arguments.buffers,
            consumes: &arguments.consumes,
        },
    )
    .map_err(|failure| match failure {
        Failure::Ast(error) => unreadable(&error),
        Failure::Annotation(message) => {
            SourceError::new(Path::new("<command line>"), driver_error("E2000", message))
        }
    })?;
    publish(&path, &arguments.output, &generated.text)
        .map_err(|error| SourceError::new(&arguments.output, error))?;
    let warnings = generated
        .skipped
        .iter()
        .map(|skipped| {
            Diagnostic::warning(
                "W2002",
                format!(
                    "skipped C declaration '{}': {}; declare it by hand or wrap it in a C function with a supported signature",
                    skipped.name, skipped.reason
                ),
                Span::new(skipped.offset, skipped.offset + skipped.length),
            )
        })
        .collect();
    Ok((String::from_utf8_lossy(&bytes).into_owned(), warnings))
}

/// Runs a tool whose stdout is data. Stdout stops at `MAX_AST_BYTES`; stderr, which
/// holds Clang's warnings, is read on another thread so a full pipe cannot block it.
fn capture(command: &mut Command, what: &str, failure_hint: &str) -> Result<Vec<u8>, Diagnostic> {
    let name = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            driver_error(
                "E2002",
                format!("cannot execute '{name}': {error}; {CLANG_HINT}"),
            )
        })?;
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let errors = std::thread::spawn(move || {
        let mut errors = Vec::new();
        let _ = stderr.read_to_end(&mut errors);
        errors
    });
    let mut output = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("stdout is piped")
        .take(MAX_AST_BYTES as u64 + 1)
        .read_to_end(&mut output);
    if output.len() > MAX_AST_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        let _ = errors.join();
        return Err(driver_error(
            "E2002",
            format!(
                "{what} exceeds {MAX_AST_BYTES} bytes; bind a smaller header that includes only the declarations you need"
            ),
        ));
    }
    let status = child
        .wait()
        .map_err(|error| driver_error("E2002", format!("cannot wait for '{name}': {error}")))?;
    let errors = errors.join().unwrap_or_default();
    read.map_err(|error| driver_error("E2002", format!("cannot read from '{name}': {error}")))?;
    if !status.success() {
        return Err(driver_error(
            "E2002",
            format!(
                "'{name}' failed ({status}):\n{}\n{failure_hint}",
                String::from_utf8_lossy(&errors).trim()
            ),
        ));
    }
    Ok(output)
}

/// Writes the output through a staged file and a rename, checking the protection rules
/// before staging and again just before the rename.
fn publish(header: &Path, output: &Path, text: &str) -> Result<(), Diagnostic> {
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    protect_bindgen_output(header, output)?;
    fs::create_dir_all(parent).map_err(|error| io_error("create output parent", parent, error))?;
    let temporary = TemporaryDirectory::new(parent)?;
    let staged = temporary.path.join("generated.tz");
    fs::write(&staged, text).map_err(|error| io_error("write bindgen output", &staged, error))?;
    protect_bindgen_output(header, output)?;
    fs::rename(&staged, output).map_err(|error| io_error("write bindgen output", output, error))?;
    temporary.close()
}

/// bindgen writes only a `.tz` file that is not the header and is either new or a
/// regular file whose first line is [`MARKER`], so it never replaces hand-written code.
fn protect_bindgen_output(header: &Path, output: &Path) -> Result<(), Diagnostic> {
    if output.extension() != Some(OsStr::new("tz")) {
        return Err(driver_error(
            "E2000",
            "bindgen output must have the .tz extension",
        ));
    }
    let metadata = match fs::symlink_metadata(output) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error("inspect output", output, error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(driver_error(
            "E2003",
            "the output must not be a symlink, directory, or special file",
        ));
    }
    let header_path =
        fs::canonicalize(header).map_err(|error| io_error("resolve header", header, error))?;
    let destination =
        fs::canonicalize(output).map_err(|error| io_error("resolve output", output, error))?;
    if header_path == destination || same_file(header, output, &metadata)? {
        return Err(driver_error(
            "E2003",
            "bindgen output must not overwrite the header",
        ));
    }
    let mut first = Vec::new();
    fs::File::open(output)
        .and_then(|file| file.take(MARKER.len() as u64 + 2).read_to_end(&mut first))
        .map_err(|error| io_error("read output", output, error))?;
    let marked = first.strip_prefix(MARKER.as_bytes()).is_some_and(|rest| {
        rest.is_empty() || rest.starts_with(b"\n") || rest.starts_with(b"\r\n")
    });
    if !marked {
        return Err(driver_error(
            "E2003",
            format!(
                "refusing to overwrite '{}': it does not start with the tsuzuri bindgen marker; delete it or choose another output",
                output.display()
            ),
        ));
    }
    Ok(())
}
