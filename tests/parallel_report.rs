//! `tsuzuri check --report parallel` (F15 Phase 3). The fixtures are in tests/fixtures/parallel_report/.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/parallel_report")
        .join(name)
}

/// Runs the compiler in `directory`; the fixtures are projects, so the knowledge graph is turned off to keep the
/// source tree clean.
fn run(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(directory)
        .args(arguments)
        .env("TSUZURI_KG", "0")
        .output()
        .unwrap()
}

fn report(directory: &Path, json: bool) -> String {
    report_of(directory, "Main.tz", json)
}

/// The report of `check TARGET --report parallel` in `directory`.
fn report_of(directory: &Path, target: &str, json: bool) -> String {
    let mut arguments = vec!["check", target, "--report", "parallel"];
    if json {
        arguments.push("--json");
    }
    let output = run(directory, &arguments);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    with_slashes(&String::from_utf8(output.stdout).unwrap())
}

/// `text` with the path of the fixture written with `/`: the report shows the path as the diagnostics do, which is
/// `.\Main.tz` on Windows (`.\\Main.tz` in JSON), and the expectations below were written with `./Main.tz`.
fn with_slashes(text: &str) -> String {
    text.replace(r".\\Main.tz", "./Main.tz")
        .replace(r".\Main.tz", "./Main.tz")
}

const BASIC_TEXT: &str = "parallel report: 2 suggestions, 2 sites
./Main.tz:6:19: suggestion: Array.map can be Parallel.map: the callback is pure
  fix (maybe): replace 'Array.map' with 'Parallel.map'
./Main.tz:7:5: suggestion: Array.sum can be Parallel.sum: integer sums are the same in any order
  fix (maybe): replace 'Array.sum' with 'Parallel.sum'
./Main.tz:13:21: site: Parallel.map callback is pure (parallel_safe: true)
./Main.tz:16:19: site: Parallel.map callback is not pure (parallel_safe: false): the callback reaches 'Debug.__print_string' (effect: debug)
";

const BASIC_JSON: &str = r#"{"schema":"tz-report-parallel/1","suggestions":2,"sites":2,"truncated":0}
{"kind":"suggestion","call":"Array.map","replacement":"Parallel.map","parallel_safe":true,"path":"./Main.tz","span":{"start":134,"end":143,"line":6,"column":19,"end_line":6,"end_column":28},"fixes":[{"title":"replace 'Array.map' with 'Parallel.map'","applicability":"maybe","edits":[{"path":"./Main.tz","span":{"start":134,"end":143,"line":6,"column":19,"end_line":6,"end_column":28},"text":"Parallel.map"}]}]}
{"kind":"suggestion","call":"Array.sum","replacement":"Parallel.sum","parallel_safe":true,"path":"./Main.tz","span":{"start":162,"end":171,"line":7,"column":5,"end_line":7,"end_column":14},"fixes":[{"title":"replace 'Array.sum' with 'Parallel.sum'","applicability":"maybe","edits":[{"path":"./Main.tz","span":{"start":162,"end":171,"line":7,"column":5,"end_line":7,"end_column":14},"text":"Parallel.sum"}]}]}
{"kind":"site","call":"Parallel.map","parallel_safe":true,"path":"./Main.tz","span":{"start":341,"end":382,"line":13,"column":21,"end_line":13,"end_column":62}}
{"kind":"site","call":"Parallel.map","parallel_safe":false,"reason":"the callback reaches 'Debug.__print_string' (effect: debug)","path":"./Main.tz","span":{"start":434,"end":483,"line":16,"column":19,"end_line":16,"end_column":68}}
"#;

const RULES_TEXT: &str = "parallel report: 4 suggestions, 3 sites
./Main.tz:2:24: suggestion: Array.sum can be Parallel.sum: integer sums are the same in any order
  fix (maybe): replace 'Array.sum' with 'Parallel.sum'
./Main.tz:14:28: suggestion: Array.map can be Parallel.map: the callback is pure
  fix (maybe): replace 'Array.map' with 'Parallel.map'
./Main.tz:17:20: suggestion: Array.mapr can be Parallel.map_ref: the callback is pure
  fix (maybe): replace 'Array.mapr' with 'Parallel.map_ref'
./Main.tz:20:18: suggestion: Array.fold_monoid can be Parallel.fold: the result is the same by the laws of Monoid
  fix (maybe): replace 'Array.fold_monoid' with 'Parallel.fold'
./Main.tz:26:21: site: Parallel.sum callback is pure (parallel_safe: true)
./Main.tz:29:20: site: Parallel.fold callback is pure (parallel_safe: true)
./Main.tz:32:19: site: Parallel.init callback is pure (parallel_safe: true)
";

/// PR #75 review: the Windows spelling of the fixture's path, in text and in JSON, compares with the expectations.
#[test]
fn windows_paths_compare_with_slashes() {
    assert_eq!(
        with_slashes(
            ".\\Main.tz:6:19: suggestion: Array.map can be Parallel.map: the callback is pure\n"
        ),
        "./Main.tz:6:19: suggestion: Array.map can be Parallel.map: the callback is pure\n"
    );
    assert_eq!(
        with_slashes(r#"{"path":".\\Main.tz","span":{"start":134}}"#),
        r#"{"path":"./Main.tz","span":{"start":134}}"#
    );
    assert_eq!(with_slashes(BASIC_JSON), BASIC_JSON);
}

/// The byte offsets and columns of the expectations are those of the fixtures with LF line ends, as the repository
/// stores them (the Windows workflow checks out with `core.autocrlf=false`); say so instead of a confusing diff.
#[test]
fn the_fixtures_have_lf_line_ends() {
    for name in ["basic", "rules", "error"] {
        let text = fs::read_to_string(fixture(name).join("Main.tz")).unwrap();
        assert!(
            !text.contains('\r'),
            "{name}/Main.tz has CR line ends; check out with core.autocrlf=false"
        );
    }
}

#[test]
fn the_basic_report_in_text_and_json() {
    assert_eq!(report(&fixture("basic"), false), BASIC_TEXT);
    assert_eq!(report(&fixture("basic"), true), BASIC_JSON);
}

#[test]
fn only_the_replaceable_calls_are_suggested() {
    assert_eq!(report(&fixture("rules"), false), RULES_TEXT);
}

#[test]
fn the_report_is_deterministic() {
    for name in ["basic", "rules"] {
        for json in [false, true] {
            assert_eq!(
                report(&fixture(name), json),
                report(&fixture(name), json),
                "{name} {json}"
            );
        }
    }
}

#[test]
fn without_the_flag_the_output_is_unchanged() {
    for name in ["basic", "rules"] {
        let output = run(&fixture(name), &["check", "Main.tz"]);
        assert!(output.status.success(), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
        assert!(output.stderr.is_empty(), "{name}");
    }
}

#[test]
fn an_erroneous_program_prints_no_report() {
    let output = run(
        &fixture("error"),
        &["check", "Main.tz", "--report", "parallel"],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn the_flag_is_checked() {
    let directory = fixture("basic");
    for (arguments, message) in [
        (
            vec!["build", "Main.tz", "--report", "parallel"],
            "--report is only valid with check",
        ),
        (
            vec!["check", "Main.tz", "--report"],
            "--report requires a report name; supported: parallel",
        ),
        (
            vec!["check", "Main.tz", "--report", "copies"],
            "unknown report 'copies' for --report; supported: parallel",
        ),
        (
            vec![
                "check", "Main.tz", "--report", "parallel", "--report", "parallel",
            ],
            "report parallel specified more than once",
        ),
        // PR #75 review: the proof's records would come before the report's header on stdout.
        (
            vec![
                "check",
                "Main.tz",
                "--contracts",
                "prove",
                "--report",
                "parallel",
                "--json",
            ],
            "--report parallel cannot be combined with --contracts prove",
        ),
    ] {
        let output = run(&directory, &arguments);
        assert!(!output.status.success(), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{arguments:?}\n{stderr}");
    }
}

/// Writes `files` into a fresh directory `name` under the test's temporary directory.
fn project(name: &str, files: &[(String, String)]) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&directory);
    for (path, text) in files {
        let path = directory.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    directory
}

/// Applies every suggestion of the report of `check TARGET` over the project `files` (in the directory `name`), and
/// returns the replacements in the order of the report and the summary line of the report of the edited project, which
/// must check.
fn apply_suggestions_in(name: &str, files: &[(&str, &str)], target: &str) -> (Vec<String>, String) {
    let mut files: Vec<(String, String)> = files
        .iter()
        .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
        .collect();
    let directory = project(name, &files);
    let mut edits = Vec::new();
    for line in report_of(&directory, target, true).lines().skip(1) {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        if value["kind"] == "suggestion" {
            let edit = &value["fixes"][0]["edits"][0];
            let path = edit["path"].as_str().unwrap();
            edits.push((
                path.replace('\\', "/").trim_start_matches("./").to_owned(),
                edit["span"]["start"].as_u64().unwrap() as usize,
                edit["span"]["end"].as_u64().unwrap() as usize,
                edit["text"].as_str().unwrap().to_string(),
            ));
        }
    }
    let replacements = edits.iter().map(|edit| edit.3.clone()).collect();
    // From the end, so that each edit keeps the offsets of the ones before it in its file.
    edits.sort_by(|left, right| (&left.0, right.1).cmp(&(&right.0, left.1)));
    for (path, start, end, text) in edits {
        let file = files
            .iter_mut()
            .find(|(name, _)| *name == path)
            .unwrap_or_else(|| panic!("{path}"));
        file.1.replace_range(start..end, &text);
    }
    // A sibling, not a subdirectory: a project reads the sources under its directory too.
    let fixed = project(&format!("{name}_fixed"), &files);
    let json = report_of(&fixed, target, true);
    (replacements, json.lines().next().unwrap().to_owned())
}

fn apply_suggestions(name: &str, source: &str) -> (Vec<String>, String) {
    apply_suggestions_in(name, &[("Main.tz", source)], "Main.tz")
}

#[test]
fn the_suggested_edits_compile_and_leave_no_suggestions() {
    let source = fs::read_to_string(fixture("rules").join("Main.tz")).unwrap();
    let (replacements, summary) = apply_suggestions("parallel_report_rules", &source);
    assert_eq!(replacements.len(), 4);
    assert_eq!(
        summary,
        r#"{"schema":"tz-report-parallel/1","suggestions":0,"sites":7,"truncated":0}"#
    );
}

/// PR #75 review: edition 2027 reads the elements of `Array.map` through a reference, so for elements that own heap
/// memory the suggestion keeps that with `Parallel.map_ref`, which `Parallel.map` would copy into its callback. Its
/// callback gets a `ref` parameter, so only a function whose parameter is already a shared reference keeps its type;
/// a lambda (`\row -> row` would return `ref [i64]`) is not suggested. Numbers keep `Parallel.map`, and the edited
/// source checks.
#[test]
fn borrowed_heap_elements_are_suggested_as_map_ref() {
    let source = "def lengths :: ref [[i64]] -> [i64]\n\
                  fn lengths rows = Array.map Array.length rows\n\
                  def sizes :: ref [string] -> [i64]\n\
                  fn sizes texts = Array.map String.length texts\n\
                  def copies :: ref [[i64]] -> [[i64]]\n\
                  fn copies rows = Array.map (\\row -> row) rows\n\
                  def firsts :: ref [[i64]] -> [i64]\n\
                  fn firsts rows = Array.map (\\row -> row[0] + 1) rows\n\
                  def doubled :: ref [i64] -> [i64]\n\
                  fn doubled values = Array.map (\\value -> value * 2) values\n";
    let (replacements, summary) = apply_suggestions("parallel_report_rows", source);
    assert_eq!(
        replacements,
        ["Parallel.map_ref", "Parallel.map_ref", "Parallel.map"]
    );
    assert_eq!(
        summary,
        r#"{"schema":"tz-report-parallel/1","suggestions":0,"sites":3,"truncated":0}"#
    );
}

/// PR #75 review: a user module `Parallel` (here `Tools/Parallel.tz`) comes before std's in its namespace, so the
/// edits spell `std::Parallel.*`; the edited project calls std's functions (they are sites of the report).
#[test]
fn a_user_module_parallel_gets_qualified_edits() {
    let files = [
        (
            "Tools/Parallel.tz",
            "def sum :: ref [i64] -> i64\nfn sum _values = 42\n",
        ),
        (
            "Tools/Use.tz",
            "def total :: ref [i64] -> i64\n\
             fn total values = Array.sum values\n\
             def doubled :: ref [i64] -> [i64]\n\
             fn doubled values = Array.map (\\value -> value * 2) values\n",
        ),
        (
            "Main.tz",
            "let values = new [i64](5000, \\index -> index)\nTools::Use.total (ref values)\n",
        ),
    ];
    let (replacements, summary) = apply_suggestions_in("parallel_report_shadowed", &files, ".");
    assert_eq!(replacements, ["std::Parallel.sum", "std::Parallel.map"]);
    assert_eq!(
        summary,
        r#"{"schema":"tz-report-parallel/1","suggestions":0,"sites":2,"truncated":0}"#
    );
    // Without a user module `Parallel`, the edits keep the short spelling.
    let (replacements, _) = apply_suggestions("parallel_report_unshadowed", files[1].1);
    assert_eq!(replacements, ["Parallel.sum", "Parallel.map"]);
}

#[test]
fn the_entries_stop_at_one_thousand() {
    let mut source = String::new();
    for index in 0..1001 {
        source.push_str(&format!(
            "def f{index} :: i64 -> [i64]\nfn f{index} count = Parallel.init count (\\item -> item)\n\n"
        ));
    }
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parallel_report_limit");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("Main.tz"), &source).unwrap();
    let json = report(&directory, true);
    assert_eq!(
        json.lines().next().unwrap(),
        r#"{"schema":"tz-report-parallel/1","suggestions":0,"sites":1000,"truncated":1}"#
    );
    assert_eq!(json.lines().count(), 1001);
}
