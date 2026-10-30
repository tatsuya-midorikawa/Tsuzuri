//! G25 Phase 3: `tsuzuri find`, the type search of `tsuzuri kg find` (D-60) over std's graph and,
//! with `--project DIR`, the project's. The matching itself is tested with `kg find` in
//! `tests/kg_query.rs`; these tests cover the command line, the outputs and the exit codes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh directory under the temporary directory, removed by `Drop`.
struct Root(PathBuf);

impl Root {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-find-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// A project in `name` with a manifest whose namespace is `Finder`.
    fn project(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = self.0.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Tsuzuri.toml"),
            "[package]\nname = \"finder\"\nversion = \"0.1.0\"\nedition = \"2026\"\nnamespace = \"Finder\"\n",
        )
        .unwrap();
        for (file, text) in files {
            std::fs::write(dir.join(file), text).unwrap();
        }
        dir
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs the compiler in `root` with a cache of its own there.
fn run(root: &Root, arguments: &[&str], threads: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command
        .current_dir(root.path())
        .args(arguments)
        .env("TSUZURI_CACHE_DIR", root.path().join("cache"))
        .env_remove("TSUZURI_KG")
        .env_remove("TSUZURI_KG_DIR");
    if let Some(threads) = threads {
        command.env("TSUZURI_THREADS", threads);
    }
    command.output().unwrap()
}

fn find(root: &Root, arguments: &[&str]) -> Output {
    let mut all = vec!["find"];
    all.extend_from_slice(arguments);
    run(root, &all, None)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const MAIN: &str = "record Point { x: i64, y: i64 }\nunion Shape = Circle of f64 | Empty\nconst limit: i64 = 10\ndef double :: i64 -> i64 = \\n -> n * 2\ndouble limit\n";
const HELPER: &str = "def shared :: i64 -> i64 = \\n -> n\n";

#[test]
fn std_search_ranks_exact_before_borrow_before_partial() {
    let root = Root::new("tiers");
    let output = find(&root, &["--limit", "100", "ref [i64] -> i64"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.contains(&"exact   Array.length :: ref ['a] -> i64"),
        "{text}"
    );
    assert!(lines.contains(&"exact   Array.sum :: Numeric<'a> => ref ['a] -> 'a"));
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("partial Array.countr ") && line.ends_with("  (+1)"))
    );
    // `Float<i64>` does not hold.
    assert!(!text.contains("Array.sum_pairwise"));
    let tiers: Vec<usize> = lines
        .iter()
        .map(|line| match &line[..8] {
            "exact   " => 0,
            "borrow  " => 1,
            _ => 2,
        })
        .collect();
    assert!(tiers.windows(2).all(|pair| pair[0] <= pair[1]), "{text}");
    // Nothing of the current directory is read or written.
    assert!(!root.path().join("target").exists());
}

#[test]
fn the_borrow_tier_ignores_one_outer_ref() {
    let root = Root::new("borrow");
    let text = stdout(&find(&root, &["string -> i64"]));
    assert!(
        text.contains("borrow  String.length :: ref string -> i64\n"),
        "{text}"
    );
    assert!(text.contains("borrow  String.char_count :: ref string -> i64\n"));
    let text = stdout(&find(&root, &["ref string -> string -> bool"]));
    for name in ["String.contains", "String.ends_with", "String.starts_with"] {
        assert!(text.contains(&format!("borrow  {name} :: ")), "{text}");
    }
    assert!(!text.contains("exact "));
}

#[test]
fn wildcards_and_type_variables_match_any_type() {
    let root = Root::new("variables");
    let text = stdout(&find(&root, &["--limit", "100", "_ -> bool"]));
    assert!(
        text.contains("exact   Array.is_empty :: ref ['a] -> bool\n"),
        "{text}"
    );
    let text = stdout(&find(&root, &["--limit", "100", "'a -> 'a"]));
    assert!(
        text.contains("exact   Char.to_ascii_upper :: own char -> char\n"),
        "{text}"
    );
    let text = stdout(&find(&root, &["(ref 'a -> bool) -> ref ['a] -> i64"]));
    assert!(
        text.starts_with("exact   Array.countr :: own (ref 'a -> bool) -> ref ['a] -> i64\n"),
        "{text}"
    );
}

#[test]
fn result_and_takes_options() {
    let root = Root::new("parts");
    let text = stdout(&find(&root, &["--limit", "100", "--result", "bool"]));
    assert!(text.contains("exact   Array.is_empty :: "), "{text}");
    let text = stdout(&find(&root, &["--takes", "string", "--result", "i64"]));
    assert!(text.contains("borrow  String.length :: "), "{text}");
    let text = stdout(&find(&root, &["--takes", "ref string", "--result", "i64"]));
    assert!(text.contains("exact   String.length :: "), "{text}");
}

#[test]
fn module_filter_and_unknown_module() {
    let root = Root::new("module");
    let output = find(&root, &["--module", "Array", "ref [i64] -> i64"]);
    let text = stdout(&output);
    assert!(!text.is_empty());
    assert!(
        text.lines()
            .filter(|line| !line.starts_with("... "))
            .all(|line| line[8..].starts_with("Array.")),
        "{text}"
    );
    let output = find(&root, &["--module", "Arrey", "i64"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("error[E2000]: no loaded module is named 'Arrey'"));
    let output = find(&root, &["--json", "--module", "Arrey", "i64"]);
    assert!(stderr(&output).contains("\"kind\":\"find_unknown_module\""));
}

#[test]
fn limit_and_the_truncation_line() {
    let root = Root::new("limit");
    let output = find(&root, &["--limit", "2", "ref [i64] -> i64"]);
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    let more = lines[2]
        .strip_prefix("... ")
        .and_then(|rest| rest.strip_suffix(" more; raise --limit (at most 100)"))
        .unwrap();
    assert!(more.parse::<usize>().unwrap() > 0);
    for limit in ["0", "101", "x"] {
        let output = find(&root, &["--limit", limit, "i64"]);
        assert_eq!(output.status.code(), Some(2), "{limit}");
        assert!(stderr(&output).contains("--limit needs a number from 1 to 100"));
    }
}

#[test]
fn json_output_has_result_events_and_a_summary() {
    let root = Root::new("json");
    let output = find(&root, &["--json", "--limit", "3", "ref [i64] -> i64"]);
    let rows: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 4);
    assert!(rows[..3].iter().all(|row| row["event"] == "result"));
    assert!(rows[..3].iter().all(|row| row["origin"] == "std"));
    let summary = &rows[3];
    assert_eq!(summary["event"], "summary");
    assert_eq!(summary["query"], "ref [i64] -> i64");
    assert_eq!(summary["limit"], 3);
    assert_eq!(summary["shown"], 3);
    assert!(summary["matches"].as_u64().unwrap() > 3);
    assert!(summary["searched"].as_u64().unwrap() > 500);
    let output = find(&root, &["--json", "--takes", "string", "--result", "i64"]);
    let last = stdout(&output).lines().last().unwrap().to_owned();
    let summary: serde_json::Value = serde_json::from_str(&last).unwrap();
    assert_eq!(summary["query"], "--result i64 --takes string");
}

#[test]
fn no_matches_exits_zero() {
    let root = Root::new("none");
    let query = ["bool"; 13].join(" -> ");
    let output = find(&root, &[query.as_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "no matches\n");
    let output = find(&root, &["--json", query.as_str()]);
    let text = stdout(&output);
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("\"matches\":0"));
}

#[test]
fn usage_and_query_errors_exit_two() {
    let root = Root::new("usage");
    for (arguments, message) in [
        (
            &[][..],
            "find takes one type query, or --result and --takes",
        ),
        (
            &["a", "b"],
            "find takes one type query, or --result and --takes",
        ),
        (
            &["--result", "i64", "x"],
            "find takes one type query, or --result and --takes",
        ),
        (&["--bogus", "x"], "unknown option '--bogus' for find"),
        (&["--limit"], "--limit needs a value"),
        (
            &["--module", "A", "--module", "B", "x"],
            "--module specified more than once",
        ),
        (&["i64 ->"], "cannot parse the type query"),
    ] {
        let output = find(&root, arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        let error = stderr(&output);
        assert!(
            error.contains("error[E2000]") && error.contains(message),
            "{error}"
        );
        let mut json = vec!["--json"];
        json.extend_from_slice(arguments);
        let error = stderr(&find(&root, &json));
        assert!(error.contains("\"kind\":\"find_bad_arguments\""), "{error}");
    }
}

#[test]
fn project_declarations_are_searched() {
    let root = Root::new("project");
    let project = root.project("app", &[("Main.tz", MAIN), ("Helper.tz", HELPER)]);
    let path = project.to_str().unwrap();
    let text = stdout(&find(
        &root,
        &["--project", path, "--limit", "100", "i64 -> i64"],
    ));
    assert!(
        text.contains("exact   Finder::Main.double :: i64 -> i64\n"),
        "{text}"
    );
    assert!(text.contains("exact   Finder::Helper.shared :: i64 -> i64\n"));
    let output = find(
        &root,
        &["--project", path, "--json", "--module", "Main", "i64"],
    );
    let rows: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let limit = rows
        .iter()
        .find(|row| row["name"] == "Finder::Main.limit")
        .unwrap();
    assert_eq!(
        (&limit["kind"], &limit["origin"]),
        (&"const".into(), &"user".into())
    );
    let text = stdout(&find(
        &root,
        &[
            "--project",
            path,
            "--json",
            "--module",
            "Main",
            "ref Point -> i64",
        ],
    ));
    assert!(
        text.contains("\"kind\":\"field\",\"name\":\"Finder::Main.Point.x\""),
        "{text}"
    );
    let text = stdout(&find(&root, &["--project", path, "f64 -> Shape"]));
    assert!(
        text.contains("exact   Finder::Main.Shape.Circle :: f64 -> Shape\n"),
        "{text}"
    );
}

#[test]
fn find_and_kg_find_print_the_same() {
    let root = Root::new("same");
    let project = root.project("app", &[("Main.tz", MAIN), ("Helper.tz", HELPER)]);
    let path = project.to_str().unwrap();
    for query in ["i64 -> i64", "ref [i64] -> i64"] {
        let ours = find(&root, &["--project", path, "--limit", "100", query]);
        let kg = run(
            &root,
            &[
                "kg", "find", "--path", path, "--limit", "100", "--budget", "100000", query,
            ],
            None,
        );
        assert_eq!(ours.status.code(), Some(0));
        assert_eq!(stdout(&ours), stdout(&kg), "{query}");
    }
}

#[test]
fn a_project_with_errors_fails_with_its_diagnostics() {
    let root = Root::new("errors");
    let project = root.project(
        "app",
        &[("Main.tz", "def bad :: Strng -> i64 = \\n -> 0\n0\n")],
    );
    let output = find(&root, &["--project", project.to_str().unwrap(), "i64"]);
    assert_eq!(output.status.code(), Some(1));
    let error = stderr(&output);
    assert!(
        error.contains("error[E1004]") && error.contains("error[E2009]"),
        "{error}"
    );
    let missing = root.path().join("missing");
    let output = find(&root, &["--project", missing.to_str().unwrap(), "i64"]);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn output_is_deterministic_across_thread_counts() {
    let root = Root::new("threads");
    let project = root.project("app", &[("Main.tz", MAIN), ("Helper.tz", HELPER)]);
    let path = project.to_str().unwrap();
    let arguments = ["find", "--project", path, "--limit", "100", "i64 -> i64"];
    let outputs: Vec<Vec<u8>> = ["1", "8", "1", "8"]
        .into_iter()
        .map(|threads| run(&root, &arguments, Some(threads)).stdout)
        .collect();
    assert!(!outputs[0].is_empty());
    assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn a_module_with_only_types_is_known() {
    let root = Root::new("types-only");
    let project = root.project(
        "app",
        &[("Main.tz", "0\n"), ("Types.tz", "type Count = i64\n")],
    );
    let output = find(
        &root,
        &[
            "--project",
            project.to_str().unwrap(),
            "--module",
            "Types",
            "i64",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "no matches\n");
}
