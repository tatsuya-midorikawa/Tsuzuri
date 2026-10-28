//! `tsuzuri kg`: building, checking and reading a project's knowledge graph (G27 §8, D-60).
//! `build`, `verify`, `clean` and `export` work on the files; the queries (G28) and the diagrams
//! (G30) read the graph through `tsuzuri::kg::store`.

use super::*;

use std::collections::BTreeMap;

use tsuzuri::kg::manifest::{self, Freshness};
use tsuzuri::kg::open::{self, Refresh};
use tsuzuri::kg::store::KgData;

/// The commands of `tsuzuri kg` that this compiler has, in the order of the usage message.
const COMMANDS: &[&str] = &[
    "build",
    "verify",
    "clean",
    "export",
    "brief",
    "show",
    "callers",
    "callees",
    "uses",
    "tests-for",
    "search",
    "find",
    "impact",
    "path",
];

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
    ("--path", true),
    ("--budget", true),
    ("--depth", true),
    ("--module", true),
    ("--project", false),
    ("--git", true),
    ("--from", true),
    ("--to", true),
    ("--rel", true),
    ("--result", true),
    ("--takes", true),
    ("--limit", true),
];

/// The options that may be written more than once (each value is kept).
const REPEATED: &[&str] = &["--takes"];

/// The command line of one `tsuzuri kg` command.
pub(crate) struct KgArguments {
    pub(crate) command: String,
    pub(crate) positionals: Vec<OsString>,
    pub(crate) options: BTreeMap<&'static str, Option<OsString>>,
    /// The values of the options of `REPEATED`, in order.
    pub(crate) repeated: Vec<(&'static str, OsString)>,
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
        repeated: Vec::new(),
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
        if REPEATED.contains(&name) {
            let value = next_value(arguments, &mut position, name)?.clone();
            parsed.repeated.push((name, value));
            parsed.options.insert(name, None);
            continue;
        }
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
        "brief" | "show" | "callers" | "callees" | "uses" | "tests-for" | "search" | "find"
        | "path" => kg_query(&parsed),
        "impact" => kg_impact(&parsed),
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

/// The options that belong to some commands only, with the message for the others (G28 §1).
fn misplaced(arguments: &KgArguments) -> Option<&'static str> {
    let command = arguments.command.as_str();
    let rules: [(&str, &[&str], &str); 12] = [
        (
            "--depth",
            &["callers", "callees", "path"],
            "--depth is only valid with kg callers, kg callees, or kg path",
        ),
        (
            "--std",
            &["search", "build", "clean"],
            "--std is only valid with kg search, kg build, or kg clean",
        ),
        (
            "--module",
            &["brief", "find"],
            "--module is only valid with kg brief or kg find",
        ),
        (
            "--project",
            &["brief"],
            "--project is only valid with kg brief",
        ),
        ("--git", &["impact"], "--git is only valid with kg impact"),
        ("--from", &["impact"], "--from is only valid with kg impact"),
        ("--to", &["impact"], "--to is only valid with kg impact"),
        ("--rel", &["path"], "--rel is only valid with kg path"),
        ("--result", &["find"], "--result is only valid with kg find"),
        ("--takes", &["find"], "--takes is only valid with kg find"),
        ("--limit", &["find"], "--limit is only valid with kg find"),
        ("--force", &["build"], "--force is only valid with kg build"),
    ];
    for (option, commands, message) in rules {
        if arguments.has(option) && !commands.contains(&command) {
            return Some(message);
        }
    }
    if command == "impact" && arguments.has("--budget") {
        return Some("--budget is not valid with kg impact");
    }
    None
}

/// Rejects the options of `allowed`'s complement for `command`.
fn only(arguments: &KgArguments, allowed: &[&str], usage: &str) -> Result<(), Diagnostic> {
    if let Some(message) = misplaced(arguments) {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidForCommand,
            message,
        ));
    }
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
    if !tsuzuri::kg::write::has_marker(&dir) {
        return Err(tsuzuri::kg::write::not_ours(&dir));
    }
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
    let data = open::open_project(&project, &dir, refresh, &mut |set, project| {
        print_diagnostics(set, project, false)
    })?;
    let wanted: &dyn Fn(&str) -> bool = match chosen[0] {
        "--nodes" => &|file| file == "nodes.jsonl",
        "--edges" => &|file| file == "edges.jsonl",
        _ => &|file| file.starts_with("facts/") && file.ends_with(".jsonl"),
    };
    // The files of one publication: the graph that was opened, or a complete one of the same
    // sources that a writer put in its place since.
    let (manifest, files) =
        tsuzuri::kg::store::read_snapshot(&dir, wanted).map_err(|freshness| {
            manifest::diagnostic(&dir, &freshness)
                .unwrap_or_else(|| manifest::broken(&dir, "it cannot be read"))
        })?;
    if !manifest.complete || manifest.source_hash != data.meta.source_hash {
        return Err(manifest::broken(&dir, "it changed while it was read"));
    }
    let mut out = io::stdout().lock();
    for bytes in files.0.values() {
        let _ = out.write_all(bytes);
    }
    Ok(EXIT_SUCCESS)
}

/// An integer option within its bounds, or the error message of its range.
fn bounded(
    arguments: &KgArguments,
    option: &str,
    (low, high): (usize, usize),
    message: &str,
) -> Result<Option<usize>, Diagnostic> {
    match arguments.value(option) {
        None => Ok(None),
        Some(value) => value
            .to_str()
            .and_then(|text| text.trim().parse::<usize>().ok())
            .filter(|value| (low..=high).contains(value))
            .map(Some)
            .ok_or_else(|| command_line_error(DiagnosticKind::OptionInvalidValue, message)),
    }
}

/// The project path of a query: `--path`, or `.`.
fn query_path(arguments: &KgArguments) -> PathBuf {
    arguments
        .value("--path")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn refresh_of(arguments: &KgArguments) -> Refresh {
    if arguments.has("--no-refresh") {
        Refresh::Never
    } else {
        Refresh::Auto
    }
}

/// The graph of the project of a query, rebuilt unless `--no-refresh`, as a view with std behind it.
fn query_view(arguments: &KgArguments) -> Result<tsuzuri::kg::store::KgView, Diagnostic> {
    let project = load_project(&query_path(arguments))?;
    let dir = open::directory(&project, kg_dir_of(arguments).as_deref());
    let json = arguments.json();
    let data = open::open_project(
        &project,
        &dir,
        refresh_of(arguments),
        &mut |set, project| print_diagnostics(set, project, json),
    )?;
    // `--no-refresh` builds nothing, and reads the std graph only when the cache has it.
    let std = if arguments.has("--no-refresh") {
        tsuzuri::kg::store::StdSource::Cached
    } else {
        tsuzuri::kg::store::StdSource::Cache
    };
    Ok(tsuzuri::kg::store::KgView::new(
        Some(std::sync::Arc::new(data)),
        std,
    ))
}

/// Prints the answer of a query over `view`, unless a file that the view read on demand shows that
/// its graph was replaced or is broken.
fn print_output(
    view: &tsuzuri::kg::store::KgView,
    output: &tsuzuri::kg::query::Output,
    json: bool,
) -> Result<(), Diagnostic> {
    view.intact()?;
    let mut out = io::stdout().lock();
    if json {
        for row in &output.rows {
            let _ = writeln!(out, "{row}");
        }
    } else {
        let _ = out.write_all(output.text.as_bytes());
    }
    Ok(())
}

/// `kg brief|show|callers|callees|uses|tests-for|search|find|path` (G28 Phase 1 and 2, D-60).
fn kg_query(arguments: &KgArguments) -> Result<u8, Diagnostic> {
    use tsuzuri::kg::query::{self, BriefTarget, Options};
    let command = arguments.command.as_str();
    let usage = match command {
        "brief" => {
            "usage: tsuzuri kg brief [ID | --project | --module M] [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        "show" => {
            "usage: tsuzuri kg show ID [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        "callers" => {
            "usage: tsuzuri kg callers ID [--depth N] [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        "callees" => {
            "usage: tsuzuri kg callees ID [--depth N] [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        "uses" => {
            "usage: tsuzuri kg uses TYPE [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        "tests-for" => {
            "usage: tsuzuri kg tests-for ID [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        "search" => {
            "usage: tsuzuri kg search TEXT [--std] [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        "find" => {
            "usage: tsuzuri kg find TYPE | --result TYPE [--takes TYPE]... [--module M] [--limit N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
        _ => {
            "usage: tsuzuri kg path FROM TO [--rel R[,R...]] [--depth N] [--budget N] [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]"
        }
    };
    let common = ["--path", "--kg-dir", "--no-refresh", "--json", "--budget"];
    let extra: &[&str] = match command {
        "brief" => &["--project", "--module"],
        "callers" | "callees" => &["--depth"],
        "search" => &["--std"],
        "find" => &["--result", "--takes", "--module", "--limit"],
        "path" => &["--rel", "--depth"],
        _ => &[],
    };
    let allowed: Vec<&str> = common.iter().chain(extra).copied().collect();
    only(arguments, &allowed, usage)?;
    let budget = bounded(
        arguments,
        "--budget",
        query::BUDGET_RANGE,
        "--budget must be an integer from 100 to 100000",
    )?;
    let positional: Vec<String> = arguments
        .positionals
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let json = arguments.json();
    if command == "brief" {
        let chosen = usize::from(!positional.is_empty())
            + usize::from(arguments.has("--project"))
            + usize::from(arguments.has("--module"));
        if chosen > 1 || positional.len() > 1 {
            return Err(usage_error(
                "kg brief takes one of ID, --project, or --module",
            ));
        }
        let target = match (positional.first(), arguments.value("--module")) {
            (Some(id), _) => BriefTarget::Id(id.clone()),
            (None, Some(module)) => BriefTarget::Module(module.to_string_lossy().into_owned()),
            (None, None) => BriefTarget::Project,
        };
        let view = query_view(arguments)?;
        let output = query::brief(&view, &target, &Options { budget, depth: 1 })
            .map_err(|error| error.diagnostic())?;
        print_output(&view, &output, json)?;
        return Ok(EXIT_SUCCESS);
    }
    if command == "find" {
        return kg_find(arguments, &positional, usage);
    }
    let wanted = if command == "path" { 2 } else { 1 };
    if positional.len() != wanted {
        return Err(usage_error(usage));
    }
    let depth = if command == "path" {
        bounded(
            arguments,
            "--depth",
            query::PATH_DEPTH_RANGE,
            "--depth must be an integer from 1 to 16",
        )?
        .unwrap_or(query::PATH_DEPTH)
    } else {
        bounded(
            arguments,
            "--depth",
            query::DEPTH_RANGE,
            "--depth must be an integer from 1 to 8",
        )?
        .unwrap_or(1)
    };
    let relations: Vec<String> = match arguments.value("--rel") {
        Some(value) => value
            .to_string_lossy()
            .split(',')
            .map(|rel| rel.trim().to_owned())
            .collect(),
        None => vec!["calls".to_owned()],
    };
    if let Some(bad) = relations
        .iter()
        .find(|rel| !query::RELATIONS.contains(&rel.as_str()))
    {
        return Err(command_line_error(
            DiagnosticKind::OptionInvalidValue,
            format!(
                "unknown relation '{bad}' for --rel; use {}",
                query::RELATIONS.join(", ")
            ),
        ));
    }
    let view = query_view(arguments)?;
    let options = Options { budget, depth };
    let target = positional[0].as_str();
    let output = match command {
        "show" => query::show(&view, target, &options),
        "callers" => query::calls(&view, target, &options, false),
        "callees" => query::calls(&view, target, &options, true),
        "uses" => query::uses(&view, target, &options),
        "tests-for" => query::tests_for(&view, target, &options),
        "search" => query::search(&view, target, arguments.has("--std"), &options),
        _ => {
            let relations: Vec<&str> = relations.iter().map(String::as_str).collect();
            query::path(&view, target, &positional[1], &relations, depth, &options)
        }
    }
    .map_err(|error| error.diagnostic())?;
    print_output(&view, &output, json)?;
    Ok(EXIT_SUCCESS)
}

/// `kg find` (D-60: the type search of the knowledge graph).
fn kg_find(arguments: &KgArguments, positional: &[String], usage: &str) -> Result<u8, Diagnostic> {
    use tsuzuri::kg::find;
    let takes: Vec<String> = arguments
        .repeated
        .iter()
        .filter(|(option, _)| *option == "--takes")
        .map(|(_, value)| value.to_string_lossy().into_owned())
        .collect();
    let result = arguments
        .value("--result")
        .map(|value| value.to_string_lossy().into_owned());
    let parts = result.is_some() || !takes.is_empty();
    if positional.len() > 1 || (positional.len() == 1) == parts {
        return Err(usage_error(usage));
    }
    let limit = bounded(
        arguments,
        "--limit",
        find::LIMIT_RANGE,
        "--limit must be an integer from 1 to 100",
    )?
    .unwrap_or(find::DEFAULT_LIMIT);
    let query = find::Query {
        text: positional.first().cloned(),
        result: result.clone(),
        takes: takes.clone(),
        module: arguments
            .value("--module")
            .map(|value| value.to_string_lossy().into_owned()),
        limit,
    };
    let query_text = match &query.text {
        Some(text) => text.clone(),
        None => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(result) = &result {
                parts.push(format!("--result {result}"));
            }
            parts.extend(takes.iter().map(|take| format!("--takes {take}")));
            parts.join(" ")
        }
    };
    let view = query_view(arguments)?;
    let (found, searched) = find::search(&view, &query).map_err(|error| {
        command_line_error(
            DiagnosticKind::OptionInvalidValue,
            format!("cannot parse the type query: {}", error.0),
        )
    })?;
    let budget = bounded(
        arguments,
        "--budget",
        tsuzuri::kg::query::BUDGET_RANGE,
        "--budget must be an integer from 100 to 100000",
    )?;
    let (text, rows) = find::render(&query_text, &query, &found, searched, budget);
    let output = tsuzuri::kg::query::Output { text, rows };
    print_output(&view, &output, arguments.json())?;
    Ok(EXIT_SUCCESS)
}

/// A graph that `impact` compares: a graph directory as it is, or a project's graph in `dir`.
enum ImpactInput {
    Graph(PathBuf),
    Project(Box<Project>, PathBuf),
}

impl ImpactInput {
    /// The input at `path`. A project's graph is in `kg_dir`, else (for the working tree, `own`
    /// false) where `--kg-dir` and `TSUZURI_KG_DIR` say, else in its own `target/tz/kg`.
    fn at(path: &Path, kg_dir: Option<&Path>, own: bool) -> Result<Self, Diagnostic> {
        if tsuzuri::kg::write::has_marker(path) {
            return Ok(Self::Graph(path.to_path_buf()));
        }
        if !path.exists() {
            return Err(Diagnostic::of(
                DiagnosticKind::IoFailure,
                format!("cannot read '{}'", path.display()),
                Span::default(),
            ));
        }
        let project = load_project(path)?;
        let dir = if own {
            open::own_directory(&project)
        } else {
            open::directory(&project, kg_dir)
        };
        Ok(Self::Project(Box::new(project), dir))
    }

    fn directory(&self) -> &Path {
        match self {
            Self::Graph(dir) | Self::Project(_, dir) => dir,
        }
    }

    fn open(&self, refresh: Refresh, json: bool) -> Result<KgData, Diagnostic> {
        match self {
            Self::Graph(dir) => open::read(dir),
            Self::Project(project, dir) => {
                open::open_project(project, dir, refresh, &mut |set, project| {
                    print_diagnostics(set, project, json)
                })
            }
        }
    }
}

/// The base and head graphs of `impact`, opened in that order. When the head's graph is in the
/// base's directory, refreshing it would replace the base's files: the base reads them all first.
fn impact_graphs(
    base: &ImpactInput,
    head: &ImpactInput,
    refresh: Refresh,
    json: bool,
) -> Result<(KgData, KgData), Diagnostic> {
    let base_data = base.open(refresh, json)?;
    if open::same_directory(base.directory(), head.directory()) {
        base_data.read_all()?;
    }
    let head_data = head.open(refresh, json)?;
    Ok((base_data, head_data))
}

/// `kg impact (--git REV | --from PATH [--to PATH])` (G28 Phase 3).
fn kg_impact(arguments: &KgArguments) -> Result<u8, Diagnostic> {
    const USAGE: &str = "usage: tsuzuri kg impact (--git REV | --from PATH [--to PATH]) [--path PATH] [--kg-dir DIR] [--no-refresh] [--json]";
    only(
        arguments,
        &[
            "--git",
            "--from",
            "--to",
            "--path",
            "--kg-dir",
            "--no-refresh",
            "--json",
        ],
        USAGE,
    )?;
    if !arguments.positionals.is_empty() {
        return Err(usage_error(USAGE));
    }
    let json = arguments.json();
    let refresh = refresh_of(arguments);
    let (git, from, to) = (
        arguments.value("--git"),
        arguments.value("--from"),
        arguments.value("--to"),
    );
    let kg_dir = kg_dir_of(arguments);
    let impact = match (git, from, to) {
        (Some(rev), None, None) => {
            let path = query_path(arguments);
            let project = load_project(&path)?;
            let root = project
                .project_directory()
                .unwrap_or_else(|| PathBuf::from("."));
            // The checkout lives until the diff has read the base graph's files.
            let checkout = tsuzuri::kg::git::Checkout::create(&root, &rev.to_string_lossy())?;
            let base = ImpactInput::at(checkout.project_root(), None, true)?;
            let dir = open::directory(&project, kg_dir.as_deref());
            let head = ImpactInput::Project(Box::new(project), dir);
            let (base, head) = impact_graphs(&base, &head, refresh, json)?;
            let impact = tsuzuri::kg::impact::diff(
                &base,
                &head,
                &format!("git {}", checkout.sha12()),
                "working tree",
            );
            base.intact()?;
            head.intact()?;
            impact
        }
        (None, Some(from), to) => {
            let base = ImpactInput::at(Path::new(from), None, true)?;
            let (head, label) = match to {
                Some(to) => (
                    ImpactInput::at(Path::new(to), None, true)?,
                    to.to_string_lossy().into_owned(),
                ),
                None => (
                    ImpactInput::at(&query_path(arguments), kg_dir.as_deref(), false)?,
                    "working tree".to_owned(),
                ),
            };
            let (base, head) = impact_graphs(&base, &head, refresh, json)?;
            let impact = tsuzuri::kg::impact::diff(&base, &head, &from.to_string_lossy(), &label);
            base.intact()?;
            head.intact()?;
            impact
        }
        _ => return Err(usage_error(USAGE)),
    };
    if json {
        print!("{}", impact.render_json());
    } else {
        print!("{}", impact.render_text());
    }
    Ok(EXIT_SUCCESS)
}
