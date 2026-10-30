//! `tsuzuri find` (G25 Phase 3): searching declarations by type. The search is the one of
//! `tsuzuri kg find` (`tsuzuri::kg::find`, D-60) over std's knowledge graph and, with
//! `--project DIR`, the project's; only the command line differs. Without `--project` nothing of
//! the current directory is read.

use super::*;

use tsuzuri::kg::find;
use tsuzuri::kg::open::{self, Refresh};
use tsuzuri::kg::store::{KgView, StdSource};

const USAGE: &str =
    "usage: tsuzuri find [--project DIR] [--module NAME] [--limit N] [--json] QUERY";

/// The command line of `tsuzuri find`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FindOptions {
    pub(crate) query: Option<String>,
    pub(crate) result: Option<String>,
    pub(crate) takes: Vec<String>,
    pub(crate) module: Option<String>,
    pub(crate) project: Option<PathBuf>,
    pub(crate) limit: usize,
    pub(crate) json: bool,
}

impl FindOptions {
    /// The query as the JSON summary repeats it.
    fn text(&self) -> String {
        if let Some(query) = &self.query {
            return query.clone();
        }
        let result = self
            .result
            .iter()
            .map(|result| format!("--result {result}"));
        let takes = self.takes.iter().map(|take| format!("--takes {take}"));
        result.chain(takes).collect::<Vec<_>>().join(" ")
    }
}

fn bad_arguments(message: impl Into<String>) -> Diagnostic {
    command_line_error(DiagnosticKind::FindBadArguments, message)
}

/// Parses the arguments after `find`: options anywhere before `--`, one query or `--result` and
/// `--takes`.
pub(crate) fn parse_find_arguments(arguments: &[OsString]) -> Result<FindOptions, Diagnostic> {
    let mut options = FindOptions {
        limit: find::DEFAULT_LIMIT,
        ..FindOptions::default()
    };
    let mut queries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut words = arguments.iter().map(|argument| argument.to_string_lossy());
    let mut paths_only = false;
    while let Some(word) = words.next() {
        if paths_only || !word.starts_with('-') || word == "-" {
            queries.push(word.into_owned());
            continue;
        }
        if word == "--" {
            paths_only = true;
            continue;
        }
        let option = word.into_owned();
        if !matches!(
            option.as_str(),
            "--project" | "--module" | "--limit" | "--result" | "--takes" | "--json"
        ) {
            return Err(bad_arguments(format!(
                "unknown option '{option}' for find; supported: --project, --module, --limit, --result, --takes, --json"
            )));
        }
        if option != "--takes" && !seen.insert(option.clone()) {
            return Err(bad_arguments(format!("{option} specified more than once")));
        }
        if option == "--json" {
            options.json = true;
            continue;
        }
        let Some(value) = words.next() else {
            return Err(bad_arguments(format!("{option} needs a value")));
        };
        let value = value.into_owned();
        match option.as_str() {
            "--project" => options.project = Some(PathBuf::from(value)),
            "--module" => options.module = Some(value),
            "--result" => options.result = Some(value),
            "--takes" => options.takes.push(value),
            _ => {
                options.limit = value
                    .parse::<usize>()
                    .ok()
                    .filter(|limit| (find::LIMIT_RANGE.0..=find::LIMIT_RANGE.1).contains(limit))
                    .ok_or_else(|| bad_arguments("--limit needs a number from 1 to 100"))?;
            }
        }
    }
    let parts = options.result.is_some() || !options.takes.is_empty();
    match (queries.len(), parts) {
        (1, false) => options.query = queries.pop(),
        (0, true) => {}
        _ => {
            return Err(bad_arguments(format!(
                "find takes one type query, or --result and --takes; {USAGE}"
            )));
        }
    }
    Ok(options)
}

/// The view that the search reads: std's graph, behind the project's with `--project`.
fn view(options: &FindOptions) -> Result<KgView, Diagnostic> {
    let Some(path) = &options.project else {
        return Ok(KgView::new(None, StdSource::Cache));
    };
    let project = Project::load_for_tests(path).map_err(|error| error.diagnostic)?;
    let dir = open::directory(&project, None);
    let json = options.json;
    let data = open::open_project(&project, &dir, Refresh::Auto, &mut |set, project| {
        print_diagnostics(set, project, json)
    })?;
    Ok(KgView::new(
        Some(std::sync::Arc::new(data)),
        StdSource::Cache,
    ))
}

/// `tsuzuri find`: the matches on standard output, best first. The exit status is 0 with matches
/// or without, 1 when the project or its graph cannot be read, and 2 for a usage error.
pub(crate) fn find_command(arguments: &[OsString]) -> ExitCode {
    let json = arguments
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json");
    let usage = |error: &Diagnostic| {
        print_diagnostic(error, Path::new("<command line>"), "", json);
        ExitCode::from(EXIT_USAGE)
    };
    let options = match parse_find_arguments(arguments) {
        Ok(options) => options,
        Err(error) => return usage(&error),
    };
    if let Some(code) = invalid_thread_setting(&mut io::stderr(), json) {
        return ExitCode::from(code);
    }
    let view = match view(&options) {
        Ok(view) => view,
        Err(error) => {
            print_diagnostic(&error, Path::new("<command line>"), "", json);
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    if let Some(module) = &options.module
        && !find::has_module(&view, module)
    {
        return usage(&command_line_error(
            DiagnosticKind::FindUnknownModule,
            format!("no loaded module is named '{module}'"),
        ));
    }
    let query = find::Query {
        text: options.query.clone(),
        result: options.result.clone(),
        takes: options.takes.clone(),
        module: options.module.clone(),
        limit: options.limit,
    };
    let (found, searched) = match find::search(&view, &query) {
        Ok(found) => found,
        Err(error) => {
            return usage(&bad_arguments(format!(
                "cannot parse the type query: {}",
                error.0
            )));
        }
    };
    let budget = tsuzuri::kg::query::BUDGET_RANGE.1;
    let (text, rows) = find::render(&options.text(), &query, &found, searched, Some(budget));
    if let Err(error) = view.intact() {
        print_diagnostic(&error, Path::new("<command line>"), "", json);
        return ExitCode::from(EXIT_FAILURE);
    }
    let mut out = io::stdout().lock();
    if json {
        for row in &rows {
            let _ = writeln!(out, "{row}");
        }
    } else {
        let _ = out.write_all(text.as_bytes());
    }
    ExitCode::from(EXIT_SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(words: &[&str]) -> Result<FindOptions, Diagnostic> {
        let arguments: Vec<OsString> = words.iter().map(OsString::from).collect();
        parse_find_arguments(&arguments)
    }

    fn message(words: &[&str]) -> String {
        let error = parse(words).unwrap_err();
        assert_eq!(error.code, "E2000");
        assert_eq!(error.kind, Some("find_bad_arguments"));
        error.message.clone()
    }

    #[test]
    fn find_arguments_read_a_query() {
        let options = parse(&["ref [i64] -> i64"]).unwrap();
        assert_eq!(options.query.as_deref(), Some("ref [i64] -> i64"));
        assert_eq!(options.limit, 20);
        assert!(!options.json && options.project.is_none());
        assert_eq!(options.text(), "ref [i64] -> i64");
    }

    #[test]
    fn find_arguments_read_options() {
        let options = parse(&[
            "--module",
            "Array",
            "--limit",
            "5",
            "--json",
            "--project",
            "d",
            "q",
        ])
        .unwrap();
        assert_eq!(options.module.as_deref(), Some("Array"));
        assert_eq!(options.limit, 5);
        assert!(options.json);
        assert_eq!(options.project, Some(PathBuf::from("d")));
        assert_eq!(options.query.as_deref(), Some("q"));
        let options = parse(&["--takes", "string", "--takes", "i64", "--result", "bool"]).unwrap();
        assert_eq!(options.takes, ["string", "i64"]);
        assert_eq!(options.text(), "--result bool --takes string --takes i64");
    }

    #[test]
    fn find_arguments_reject_unknown_options() {
        assert_eq!(
            message(&["--bogus", "x"]),
            "unknown option '--bogus' for find; supported: --project, --module, --limit, --result, --takes, --json"
        );
        assert_eq!(message(&["--limit"]), "--limit needs a value");
        for words in [&[][..], &["a", "b"], &["--result", "i64", "x"]] {
            assert!(message(words).starts_with("find takes one type query"));
        }
    }

    #[test]
    fn find_arguments_reject_a_bad_limit_and_repeats() {
        for limit in ["0", "101", "x", "-1"] {
            assert_eq!(
                message(&["--limit", limit, "q"]),
                "--limit needs a number from 1 to 100"
            );
        }
        assert_eq!(
            message(&["--module", "A", "--module", "B", "x"]),
            "--module specified more than once"
        );
    }
}
