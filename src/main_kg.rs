//! `tsuzuri kg`: building, checking and reading a project's knowledge graph (G27 §8, D-60).
//! `build`, `verify`, `clean` and `export` work on the files; the queries (G28) and the diagrams
//! (G30) read the graph through `tsuzuri::kg::store`.

use super::*;

use std::collections::BTreeMap;

use tsuzuri::kg::manifest::{self, Freshness};
use tsuzuri::kg::open::{self, Refresh};
use tsuzuri::kg::store::KgData;

/// The commands of `tsuzuri kg` that this compiler has, in the order of the usage message.
const COMMANDS: &[&str] = &["build", "verify", "clean", "export"];

/// Whether an option takes a value.
const OPTIONS: &[(&str, bool)] = &[
    ("--kg-dir", true),
    ("--json", false),
    ("--no-refresh", false),
    ("--std", false),
    ("--force", false),
    ("--nodes", false),
    ("--edges", false),
    ("--facts", false),
];

/// The command line of one `tsuzuri kg` command.
pub(crate) struct KgArguments {
    pub(crate) command: String,
    pub(crate) positionals: Vec<OsString>,
    pub(crate) options: BTreeMap<&'static str, Option<OsString>>,
}

impl KgArguments {
    pub(crate) fn has(&self, option: &str) -> bool {
        self.options.contains_key(option)
    }

    pub(crate) fn value(&self, option: &str) -> Option<&OsString> {
        self.options.get(option).and_then(Option::as_ref)
    }

    pub(crate) fn json(&self) -> bool {
        self.has("--json")
    }
}

fn usage_error(message: impl Into<String>) -> Diagnostic {
    command_line_error(DiagnosticKind::UsageError, message)
}

fn commands_list() -> String {
    match COMMANDS.split_last() {
        Some((last, rest)) => format!("{}, or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Parses `tsuzuri kg COMMAND ...`: options anywhere before `--`, positional arguments elsewhere.
pub(crate) fn parse_kg_arguments(
    arguments: &[OsString],
    options: &[(&'static str, bool)],
) -> Result<KgArguments, Diagnostic> {
    let Some(command) = arguments.first() else {
        return Err(usage_error(format!(
            "usage: tsuzuri kg COMMAND [arguments]; use {}",
            commands_list()
        )));
    };
    let command = command.to_string_lossy().into_owned();
    let mut parsed = KgArguments {
        command,
        positionals: Vec::new(),
        options: BTreeMap::new(),
    };
    let mut position = 1;
    let mut paths_only = false;
    while position < arguments.len() {
        let argument = &arguments[position];
        position += 1;
        let text = argument.to_string_lossy();
        if paths_only || !text.starts_with('-') || text == "-" {
            parsed.positionals.push(argument.clone());
            continue;
        }
        if text == "--" {
            paths_only = true;
            continue;
        }
        let Some(&(name, takes_value)) = options.iter().find(|(name, _)| *name == text) else {
            return Err(command_line_error(
                DiagnosticKind::UnknownOption,
                format!("unknown option '{text}'; use --help"),
            ));
        };
        if parsed.options.contains_key(name) {
            return Err(command_line_error(
                DiagnosticKind::DuplicateOption,
                format!("{name} specified more than once"),
            ));
        }
        let value = if takes_value {
            Some(next_value(arguments, &mut position, name)?.clone())
        } else {
            None
        };
        parsed.options.insert(name, value);
    }
    if let Some(dir) = parsed.value("--kg-dir")
        && dir.is_empty()
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidValue,
            "--kg-dir must not be empty",
        ));
    }
    Ok(parsed)
}

/// Exit code and stderr output of an error of a `kg` command.
fn fail(error: &Diagnostic, json: bool, code: u8) -> ExitCode {
    print_diagnostic(error, Path::new("<command line>"), "", json);
    ExitCode::from(code)
}

/// The exit code of an error: usage errors are 2, the others 1.
pub(crate) fn exit_of(error: &Diagnostic) -> u8 {
    let usage = [
        DiagnosticKind::UsageError,
        DiagnosticKind::UnknownOption,
        DiagnosticKind::DuplicateOption,
        DiagnosticKind::OptionMissingValue,
        DiagnosticKind::OptionInvalidValue,
        DiagnosticKind::OptionInvalidForCommand,
    ];
    if error.code == "E2000" && usage.iter().any(|kind| error.kind == Some(kind.id())) {
        EXIT_USAGE
    } else {
        EXIT_FAILURE
    }
}

/// The project directory of a command: its one positional argument, or `.`.
fn project_path(arguments: &KgArguments, usage: &str) -> Result<PathBuf, Diagnostic> {
    match arguments.positionals.as_slice() {
        [] => Ok(PathBuf::from(".")),
        [path] => Ok(PathBuf::from(path)),
        _ => Err(usage_error(usage)),
    }
}

/// `tsuzuri kg ARGS`.
pub(crate) fn kg_command(arguments: &[OsString]) -> ExitCode {
    let json = arguments
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json");
    if let Some(code) = invalid_thread_setting(&mut io::stderr(), json) {
        return ExitCode::from(code);
    }
    let parsed = match parse_kg_arguments(arguments, OPTIONS) {
        Ok(parsed) => parsed,
        Err(error) => return fail(&error, json, EXIT_USAGE),
    };
    let result = match parsed.command.as_str() {
        "build" => kg_build(&parsed),
        "verify" => kg_verify(&parsed),
        "clean" => kg_clean(&parsed),
        "export" => kg_export(&parsed),
        other => Err(usage_error(format!(
            "unknown kg command '{other}'; use {}",
            commands_list()
        ))),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let code = exit_of(&error);
            fail(&error, json, code)
        }
    }
}

/// Rejects the options of `allowed`'s complement for `command`.
fn only(arguments: &KgArguments, allowed: &[&str], usage: &str) -> Result<(), Diagnostic> {
    match arguments
        .options
        .keys()
        .find(|option| !allowed.contains(option))
    {
        Some(option) => Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            format!(
                "{option} is not valid with kg {}; {usage}",
                arguments.command
            ),
        )),
        None => Ok(()),
    }
}

fn kg_dir_of(arguments: &KgArguments) -> Option<PathBuf> {
    arguments.value("--kg-dir").map(PathBuf::from)
}

fn load_project(path: &Path) -> Result<Project, Diagnostic> {
    Project::load_for_tests(path).map_err(|error| error.diagnostic)
}

/// `tsuzuri kg build [PATH] [--kg-dir DIR] [--std] [--force] [--json]`.
fn kg_build(arguments: &KgArguments) -> Result<u8, Diagnostic> {
    const USAGE: &str = "usage: tsuzuri kg build [PATH] [--kg-dir DIR] [--std] [--force] [--json]";
    only(
        arguments,
        &["--kg-dir", "--std", "--force", "--json"],
        USAGE,
    )?;
    let json = arguments.json();
    if arguments.has("--std") {
        if !arguments.positionals.is_empty() || arguments.has("--kg-dir") {
            return Err(usage_error(
                "kg build --std takes no PATH and no --kg-dir; the std KG lives in the compiler's cache",
            ));
        }
        let (dir, written) = tsuzuri::kg::std_kg::build(arguments.has("--force"))?;
        report_build(
            &dir,
            if written == 0 { "unchanged" } else { "written" },
            written,
            true,
            0,
            json,
        );
        return Ok(EXIT_SUCCESS);
    }
    let path = project_path(arguments, USAGE)?;
    let project = load_project(&path)?;
    let dir = open::directory(&project, kg_dir_of(arguments).as_deref());
    match tsuzuri::kg::build::target_for(&dir, &project, arguments.has("--force"))? {
        tsuzuri::kg::build::Plan::Build(target) => {
            let plan = tsuzuri::kg::build::Plan::Build(target);
            let (analysis, pending) =
                tsuzuri::kg::build::analyze(&project, true, driver::AnalysisKind::Tests, plan);
            let pending = pending.expect("a build plan captures");
            match analysis {
                Ok(module) => {
                    let warnings = tsuzuri::diagnostic::DiagnosticSet::from_diagnostics(
                        module.warnings.iter().cloned(),
                        0,
                    );
                    print_diagnostics(&warnings, &project, json);
                    if let Some(notice) =
                        tsuzuri::kg::build::complete(pending, &project, Ok(&module))
                    {
                        return Err(Diagnostic::of(
                            DiagnosticKind::IoFailure,
                            notice.trim_start_matches("kg not updated: ").to_owned(),
                            Span::default(),
                        ));
                    }
                    let files = written_files(&dir);
                    report_build(&dir, "written", files, true, 0, json);
                    Ok(EXIT_SUCCESS)
                }
                Err(errors) => {
                    print_diagnostics(&errors, &project, json);
                    if let Some(notice) =
                        tsuzuri::kg::build::complete(pending, &project, Err(&errors))
                    {
                        return Err(Diagnostic::of(
                            DiagnosticKind::IoFailure,
                            notice.trim_start_matches("kg not updated: ").to_owned(),
                            Span::default(),
                        ));
                    }
                    let count = errors
                        .diagnostics
                        .iter()
                        .filter(|error| error.severity.as_str() == "error")
                        .count();
                    report_build(&dir, "incomplete", written_files(&dir), false, count, json);
                    Ok(EXIT_FAILURE)
                }
            }
        }
        tsuzuri::kg::build::Plan::Fresh | tsuzuri::kg::build::Plan::Off => {
            // An unchanged graph of a failed analysis stays incomplete, and so does the status.
            let manifest = tsuzuri::kg::store::read_manifest(&dir);
            match manifest.filter(|manifest| !manifest.complete) {
                Some(manifest) => {
                    let errors = manifest.count("errors").unwrap_or(0) as usize;
                    report_build(&dir, "incomplete", 0, false, errors, json);
                    Ok(EXIT_FAILURE)
                }
                None => {
                    report_build(&dir, "unchanged", 0, true, 0, json);
                    Ok(EXIT_SUCCESS)
                }
            }
        }
    }
}

/// The number of files of the graph at `dir`: those of its manifest, the manifest and the marker.
fn written_files(dir: &Path) -> usize {
    tsuzuri::kg::store::read_manifest(dir).map_or(0, |manifest| manifest.files.len() + 2)
}

fn report_build(dir: &Path, status: &str, files: usize, complete: bool, errors: usize, json: bool) {
    if json {
        println!(
            "{{\"type\":\"kg_build\",\"status\":\"{status}\",\"dir\":{},\"files\":{files},\"complete\":{complete}}}",
            json_string(&dir.to_string_lossy())
        );
    } else {
        match status {
            "written" => println!("kg: wrote {files} files to {}", dir.display()),
            "unchanged" => println!("kg: up to date ({})", dir.display()),
            _ => println!("kg: incomplete, {errors} errors ({})", dir.display()),
        }
    }
}

/// `tsuzuri kg verify [PATH] [--kg-dir DIR] [--json]`: the graph is fresh, every file has the
/// digest of the manifest, and the graph keeps its structural invariants (D-60).
fn kg_verify(arguments: &KgArguments) -> Result<u8, Diagnostic> {
    const USAGE: &str = "usage: tsuzuri kg verify [PATH] [--kg-dir DIR] [--json]";
    only(arguments, &["--kg-dir", "--json"], USAGE)?;
    let path = project_path(arguments, USAGE)?;
    let project = load_project(&path)?;
    let dir = open::directory(&project, kg_dir_of(arguments).as_deref());
    let hash = tsuzuri::kg::input::hex(&tsuzuri::kg::input::source_hash(&project));
    let stamp = tsuzuri::kg::build::current_stamp()?;
    let freshness = manifest::inspect(&dir, &hash, &stamp);
    match freshness {
        Freshness::Fresh => {}
        Freshness::NotOurs if dir.is_dir() => return Err(manifest::missing(&dir, true)),
        other => return Err(manifest::diagnostic(&dir, &other).expect("not fresh")),
    }
    // Every file of one publication, and the graph's invariants over those same bytes.
    let broken = |freshness: Freshness| {
        manifest::diagnostic(&dir, &freshness)
            .unwrap_or_else(|| manifest::broken(&dir, "it cannot be read"))
    };
    let (manifest, files) = tsuzuri::kg::store::read_snapshot(&dir, &|_| true).map_err(broken)?;
    // What is verified is that publication: one that replaced the graph since `inspect` must be
    // fresh as well.
    match manifest::judge(&dir, &manifest, &hash, &stamp) {
        Freshness::Fresh => {}
        other => return Err(manifest::diagnostic(&dir, &other).expect("not fresh")),
    }
    let data =
        KgData::from_parts(&manifest, &files).map_err(|reason| manifest::broken(&dir, &reason))?;
    if let Some(violation) = data.violation() {
        return Err(manifest::broken(&dir, &violation));
    }
    let nodes = data.nodes.len();
    if arguments.json() {
        println!(
            "{{\"type\":\"kg_verify\",\"status\":\"ok\",\"source_hash\":{},\"nodes\":{nodes},\"files\":{}}}",
            json_string(&manifest.source_hash),
            manifest.files.len()
        );
    } else {
        println!(
            "kg: ok ({nodes} nodes, {} files, source {})",
            manifest.files.len(),
            &manifest.source_hash[..12.min(manifest.source_hash.len())]
        );
    }
    Ok(EXIT_SUCCESS)
}

/// `tsuzuri kg clean [PATH] [--kg-dir DIR] [--std]`.
fn kg_clean(arguments: &KgArguments) -> Result<u8, Diagnostic> {
    const USAGE: &str = "usage: tsuzuri kg clean [PATH] [--kg-dir DIR] [--std]";
    only(arguments, &["--kg-dir", "--std"], USAGE)?;
    if arguments.has("--std") {
        if !arguments.positionals.is_empty() || arguments.has("--kg-dir") {
            return Err(usage_error("kg clean --std takes no PATH and no --kg-dir"));
        }
        if let Some(root) = tsuzuri::kg::std_kg::cache_root() {
            tsuzuri::kg::write::remove_tree(&root);
        }
        return Ok(EXIT_SUCCESS);
    }
    let path = project_path(arguments, USAGE)?;
    let project = load_project(&path)?;
    let dir = open::directory(&project, kg_dir_of(arguments).as_deref());
    if std::fs::symlink_metadata(&dir).is_err() {
        return Ok(EXIT_SUCCESS);
    }
    // Only a graph that the compiler wrote, and never a directory that holds the project's
    // sources (a marker copied to the root with `--kg-dir .`).
    tsuzuri::kg::build::protect(&dir, &project)?;
    std::fs::remove_dir_all(&dir).map_err(|error| {
        Diagnostic::of(
            DiagnosticKind::IoFailure,
            format!("cannot remove '{}': {error}", dir.display()),
            Span::default(),
        )
    })?;
    Ok(EXIT_SUCCESS)
}

/// `tsuzuri kg export [PATH] [--kg-dir DIR] (--nodes | --edges | --facts) [--no-refresh]`.
fn kg_export(arguments: &KgArguments) -> Result<u8, Diagnostic> {
    const USAGE: &str = "usage: tsuzuri kg export [PATH] [--kg-dir DIR] (--nodes | --edges | --facts) [--no-refresh]";
    only(
        arguments,
        &["--kg-dir", "--nodes", "--edges", "--facts", "--no-refresh"],
        USAGE,
    )?;
    let chosen: Vec<&str> = ["--nodes", "--edges", "--facts"]
        .into_iter()
        .filter(|flag| arguments.has(flag))
        .collect();
    if chosen.len() != 1 {
        return Err(usage_error(
            "kg export needs exactly one of --nodes, --edges, --facts",
        ));
    }
    let path = project_path(arguments, USAGE)?;
    let refresh = if arguments.has("--no-refresh") {
        Refresh::Never
    } else {
        Refresh::Auto
    };
    let project = load_project(&path)?;
    let dir = open::directory(&project, kg_dir_of(arguments).as_deref());
    open::open_project(&project, &dir, refresh, &mut |set, project| {
        print_diagnostics(set, project, false)
    })?;
    let wanted: &dyn Fn(&str) -> bool = match chosen[0] {
        "--nodes" => &|file| file == "nodes.jsonl",
        "--edges" => &|file| file == "edges.jsonl",
        _ => &|file| file.starts_with("facts/") && file.ends_with(".jsonl"),
    };
    // The files of one publication: the graph that was opened, or one that a writer put in its
    // place since, which must be fresh and complete as well.
    let (manifest, files) =
        tsuzuri::kg::store::read_snapshot(&dir, wanted).map_err(|freshness| {
            manifest::diagnostic(&dir, &freshness)
                .unwrap_or_else(|| manifest::broken(&dir, "it cannot be read"))
        })?;
    let hash = tsuzuri::kg::input::hex(&tsuzuri::kg::input::source_hash(&project));
    let stamp = tsuzuri::kg::build::current_stamp()?;
    match manifest::judge(&dir, &manifest, &hash, &stamp) {
        Freshness::Fresh => {}
        other => return Err(manifest::diagnostic(&dir, &other).expect("not fresh")),
    }
    let mut out = io::stdout().lock();
    for bytes in files.0.values() {
        let _ = out.write_all(bytes);
    }
    Ok(EXIT_SUCCESS)
}
