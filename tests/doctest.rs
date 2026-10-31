//! G34: `tsuzuri test --doc`, which checks the ```` ```tsuzuri ```` examples of `///` comments.
//! The fixtures and their expected outputs are those of the ticket's appendix B.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tsuzuri::doctest;
use tsuzuri::driver::Project;

/// How a trapped test is reported (see `tests/bench.rs`).
const TRAPPED: &str = if cfg!(windows) {
    "trapped or exited with code -1073741795"
} else {
    "trapped or terminated by signal"
};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> String {
    format!("tests/fixtures/doctest/{name}")
}

fn tsuzuri(arguments: &[&str], cache: &Path, env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command
        .current_dir(repository())
        .args(arguments)
        .env("TSUZURI_CACHE_DIR", cache);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

/// A fresh directory with `files`, removed by `Drop`; its cache is beside it.
struct Package(PathBuf);

impl Package {
    fn new(label: &str, files: &[(&str, &str)]) -> Self {
        let root =
            std::env::temp_dir().join(format!("tsuzuri-doctest-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (name, text) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }

    fn cache(&self) -> PathBuf {
        self.0.with_extension("cache")
    }
}

impl Drop for Package {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        let _ = fs::remove_dir_all(self.cache());
    }
}

fn cache(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tsuzuri-doctest-cache-{}-{label}",
        std::process::id()
    ))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn library_api_judges_check_fails_and_ignore_without_a_toolchain() {
    let package = Package::new(
        "library",
        &[(
            "Lib.tz",
            "/// One.\n///\n/// ```tsuzuri\n/// let _x = Lib.one()\n/// ```\n///\n/// ```tsuzuri fails=E1003\n/// let _y: bool = Lib.one()\n/// ```\n///\n/// ```tsuzuri fails=E1003\n/// let _z = Lib.one()\n/// ```\n///\n/// ```tsuzuri ignore\n/// Lib.one\n/// ```\ndef one :: i64 = 1\n",
        )],
    );
    let project = Project::load_for_docs(&package.0).unwrap();
    let examples = doctest::collect(&project).unwrap();
    assert_eq!(examples.len(), 4);
    let report = doctest::run(
        &project,
        &package.0,
        &examples,
        &doctest::Options::default(),
    );
    let failures: Vec<Option<&str>> = report
        .results
        .iter()
        .map(|result| result.failure.as_deref())
        .collect();
    assert_eq!(
        failures,
        [
            None,
            None,
            Some("expected E1003, but the example was accepted")
        ]
    );
    assert_eq!(report.ignored, 1);
}

#[test]
fn list_shows_every_example_with_its_kind() {
    let cache = cache("list");
    let output = tsuzuri(&["test", "--doc", &fixture("pass"), "--list"], &cache, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "0 Counter.add_one#1 [run]\n1 Counter.add_one#2 [run]\n2 Counter.add_one#3 [fails]\n3 Counter.double#1 [test]\n4 Counter.double#2 [check]\n5 Counter.double#3 [ignore]\n"
    );
    let output = tsuzuri(
        &["test", "--doc", &fixture("pass"), "--list", "--json"],
        &cache,
        &[],
    );
    let text = stdout(&output);
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 6);
    let first = text.lines().next().unwrap();
    assert!(
        first.starts_with("{\"index\":0,\"kind\":\"run\",\"line\":3,\"module\":\"Counter\",\"name\":\"add_one#1\",\"path\":\""),
        "{first}"
    );
    assert!(first.ends_with("Counter.tz\",\"type\":\"doctest\"}"));
    let lines: Vec<u64> = rows
        .iter()
        .map(|row| row["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, [3, 9, 14, 22, 26, 30]);
    let _ = fs::remove_dir_all(cache);
}

const PASS_OUTPUT: &str = "ok 1 - Counter add_one#1\nok 2 - Counter add_one#2\nok 3 - Counter add_one#3\nok 4 - Counter double#1\nok 5 - Counter double#2\n\n5 passed; 0 failed; 1 ignored\n";

#[test]
fn passing_fixture_passes_at_o0_and_o3() {
    let cache = cache("pass");
    for (level, threads) in [("-O0", "1"), ("-O3", "8")] {
        let output = tsuzuri(
            &["test", "--doc", &fixture("pass"), level],
            &cache,
            &[("TSUZURI_THREADS", threads)],
        );
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(stdout(&output), PASS_OUTPUT);
        assert_eq!(stderr(&output), "");
    }
    let _ = fs::remove_dir_all(cache);
}

#[test]
fn passing_fixture_json_has_the_documented_keys() {
    let cache = cache("json");
    let output = tsuzuri(&["test", "--doc", &fixture("pass"), "--json"], &cache, &[]);
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 6);
    assert!(
        lines[0].starts_with("{\"type\":\"doctest\",\"index\":0,\"module\":\"Counter\",\"name\":\"add_one#1\",\"status\":\"passed\",\"path\":\""),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains("Counter.tz\",\"line\":3,\"duration_ms\":"));
    assert!(lines[5].starts_with(
        "{\"type\":\"summary\",\"passed\":5,\"failed\":0,\"ignored\":1,\"duration_ms\":"
    ));
    let _ = fs::remove_dir_all(cache);
}

#[test]
fn failing_fixture_reports_each_failure_kind() {
    let cache = cache("fail");
    let output = tsuzuri(&["test", "--doc", &fixture("fail"), "--json"], &cache, &[]);
    assert_eq!(output.status.code(), Some(1));
    let rows: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let failures: Vec<&str> = rows[..7]
        .iter()
        .map(|row| row["failure"].as_str().unwrap())
        .collect();
    assert_eq!(failures[0], "expected output \"4\", found \"3\"");
    assert_eq!(
        failures[1],
        "E1003: expected bool, found i64 (example line 1, column 19)"
    );
    assert_eq!(failures[2], "expected E1003, but the example was accepted");
    assert_eq!(
        failures[3],
        "expected E1003, found E1005: no instance for Numeric<string>; define an instance or use a supported type (example line 1, column 1)"
    );
    assert_eq!(
        failures[4],
        format!("test \"fails on purpose\" failed: {TRAPPED}")
    );
    assert_eq!(failures[5], "the example declares no test");
    assert!(failures[6].starts_with("E2005: "), "{}", failures[6]);
    assert_eq!(
        rows[7],
        serde_json::json!({"type": "summary", "passed": 0, "failed": 7, "ignored": 0, "duration_ms": rows[7]["duration_ms"]})
    );
    let error = stderr(&output);
    let last: serde_json::Value = serde_json::from_str(error.lines().last().unwrap()).unwrap();
    assert_eq!(last["code"], "E2006");
    assert_eq!(last["message"], "7 doc tests failed");
    assert_eq!(last["span"]["line"], 3);
    assert!(last["path"].as_str().unwrap().ends_with("Counter.tz"));
    let _ = fs::remove_dir_all(cache);
}

#[test]
fn notation_errors_are_e2011_and_nothing_runs() {
    let cache = cache("notation");
    let output = tsuzuri(
        &["test", "--doc", &fixture("notation"), "--json"],
        &cache,
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "");
    let rows: Vec<serde_json::Value> = stderr(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(rows.iter().all(|row| row["code"] == "E2011"));
    let lines: Vec<u64> = rows
        .iter()
        .map(|row| row["span"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, [3, 7, 11, 15, 19, 22]);
    let starts = [
        "unknown option 'sloppy' in a tsuzuri code fence",
        "'%' in run= must be followed by two hexadecimal digits",
        "a tsuzuri code fence takes one of run=, test, ignore, and fails=; found 'run=' and 'test'",
        "a tsuzuri code fence must start at column 1 of the comment",
        "a tsuzuri example needs at least one line of code",
        "the tsuzuri code fence is not closed",
    ];
    for (row, start) in rows.iter().zip(starts) {
        assert!(row["message"].as_str().unwrap().starts_with(start), "{row}");
        assert_eq!(row["kind"], "doc_example_invalid");
    }
    let _ = fs::remove_dir_all(cache);
}

#[test]
fn main_tz_examples_are_ignored_with_a_note() {
    let cache = cache("main");
    let output = tsuzuri(&["test", "--doc", &fixture("main")], &cache, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "ok 1 - Lib add_one#1\n\n1 passed; 0 failed; 1 ignored\n"
    );
    assert_eq!(
        stderr(&output),
        "1 doc example in Main.tz not run; move the documented declaration to another module\n"
    );
    let _ = fs::remove_dir_all(cache);
}

#[test]
fn filter_and_index_select_examples() {
    let cache = cache("filter");
    for selection in [["--filter", "add_one#2"], ["--index", "3"]] {
        let mut arguments = vec!["test", "--doc"];
        let pass = fixture("pass");
        arguments.push(&pass);
        arguments.extend(selection);
        let output = tsuzuri(&arguments, &cache, &[]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(
            stdout(&output).ends_with("1 passed; 0 failed; 5 ignored\n"),
            "{}",
            stdout(&output)
        );
    }
    let output = tsuzuri(
        &["test", "--doc", &fixture("pass"), "--index", "99"],
        &cache,
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("error[E2000]: doc example index is out of range"));
    let _ = fs::remove_dir_all(cache);
}

#[test]
fn check_only_examples_need_no_native_toolchain() {
    let source = "/// One.\n///\n/// ```tsuzuri\n/// let _x = Lib.one()\n/// ```\n///\n/// ```tsuzuri fails=E1003\n/// let _y: bool = Lib.one()\n/// ```\n///\n/// ```tsuzuri ignore\n/// nothing\n/// ```\ndef one :: i64 = 1\n";
    let package = Package::new("no-toolchain", &[("Lib.tz", source)]);
    let missing = [("TSUZURI_CLANG", "/nonexistent/clang")];
    let output = tsuzuri(
        &["test", "--doc", package.path()],
        &package.cache(),
        &missing,
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).ends_with("2 passed; 0 failed; 1 ignored\n"));
    let with_run = source.replace(
        "def one",
        "///\n/// ```tsuzuri run=1\n/// Lib.one()\n/// ```\ndef one",
    );
    let package = Package::new("no-toolchain-run", &[("Lib.tz", &with_run)]);
    let output = tsuzuri(
        &["test", "--doc", package.path()],
        &package.cache(),
        &missing,
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("not ok 4 - Lib one#4"), "{text}");
    assert!(text.ends_with("2 passed; 1 failed; 1 ignored\n"), "{text}");
}

#[test]
fn rejected_option_combinations_exit_with_2() {
    let cache = cache("options");
    let pass = fixture("pass");
    for (arguments, message) in [
        (
            vec!["check", "--doc", &pass],
            "--doc is only valid with test",
        ),
        (
            vec!["test", "--doc", "--doc", &pass],
            "doc specified more than once",
        ),
        (
            vec!["test", "--doc", &pass, "--coverage", "x"],
            "--doc cannot be combined with --coverage",
        ),
        (
            vec!["test", "--doc", &pass, "--seed", "1"],
            "--doc cannot be combined with --seed",
        ),
        (
            vec!["test", "--doc", &pass, "-g", "-o", "x"],
            "--doc cannot be combined with -g or -o",
        ),
        (
            vec!["test", "--doc", &pass, "--target", "native"],
            "--doc does not use --target; examples run on this machine",
        ),
        (
            vec!["test", "--doc", &pass, "-l", "m"],
            "--doc does not use --link, -l, or -L; list native inputs in Tsuzuri.toml",
        ),
    ] {
        let output = tsuzuri(&arguments, &cache, &[]);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(stderr(&output).contains(message), "{}", stderr(&output));
    }
    let _ = fs::remove_dir_all(cache);
}

#[test]
fn examples_obey_the_capability_policy_of_their_package() {
    // E15: an example is code of the package, so `[capabilities]` binds it as it binds `check`.
    let source = "/// One.\n///\n/// ```tsuzuri\n/// let _text = File.read_text \"notes.txt\"\n/// ```\n///\n/// ```tsuzuri fails=E1036\n/// let _text = File.read_text \"notes.txt\"\n/// ```\ndef one :: i64 = 1\n";
    let package = Package::new(
        "capabilities",
        &[
            (
                "Tsuzuri.toml",
                "[package]\nname = \"lib\"\nversion = \"0.1.0\"\nedition = \"2027\"\n\n[capabilities]\nallow = [\"stdio\"]\n",
            ),
            ("Lib.tz", source),
        ],
    );
    let output = tsuzuri(&["test", "--doc", package.path()], &package.cache(), &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("not ok 1 - Lib one#1\n  failure: E1036: "),
        "{text}"
    );
    assert!(text.contains("ok 2 - Lib one#2\n"), "{text}");
    assert!(text.ends_with("1 passed; 1 failed; 0 ignored\n"), "{text}");
}

#[test]
fn examples_keep_the_edition_that_edition_chose() {
    // `1l` is accepted in edition 2026 and rejected (E0004) in 2027; the example project is
    // loaded again, and keeps the `--edition` of a project without a manifest.
    let source = "/// One.\n///\n/// ```tsuzuri\n/// let _x = 1l\n/// ```\ndef one :: i64 = 1\n";
    let package = Package::new("edition", &[("Lib.tz", source)]);
    let output = tsuzuri(
        &["test", "--doc", package.path(), "--edition", "2026"],
        &package.cache(),
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).ends_with("1 passed; 0 failed; 0 ignored\n"));
    let output = tsuzuri(&["test", "--doc", package.path()], &package.cache(), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("failure: E0004: "),
        "{}",
        stdout(&output)
    );
}

#[test]
fn deny_warnings_fails_an_example_with_a_warning() {
    let source = "/// One.\n///\n/// ```tsuzuri\n/// def f :: unit -> i64 = \\() -> {\n///     let unused = 1;\n///     2\n/// }\n/// let _x = f ()\n/// ```\ndef one :: i64 = 1\n";
    let package = Package::new("deny-warnings", &[("Lib.tz", source)]);
    let output = tsuzuri(&["test", "--doc", package.path()], &package.cache(), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let output = tsuzuri(
        &["test", "--doc", package.path(), "--deny-warnings"],
        &package.cache(),
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(
        text.contains("failure: W1001: unused local 'unused'"),
        "{text}"
    );
    assert!(text.contains("(example line 2, column 9)"), "{text}");
}

#[test]
fn a_test_example_reports_wrapped_arithmetic_as_tsuzuri_test_does() {
    // A23: `tsuzuri test` fails a test whose integer arithmetic wraps, unless `--allow-overflow`.
    let source = "/// One.\n///\n/// ```tsuzuri test\n/// def twice :: i32 -> i32 = \\n -> n * 2\n/// test \"wraps\" = Test.equal (ref (twice 2000000000)) (ref (-294967296))\n/// ```\ndef one :: i64 = 1\n";
    let package = Package::new("overflow", &[("Lib.tz", source)]);
    let output = tsuzuri(&["test", "--doc", package.path()], &package.cache(), &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("integer overflow wrapped at"),
        "{}",
        stdout(&output)
    );
    let output = tsuzuri(
        &["test", "--doc", package.path(), "--allow-overflow"],
        &package.cache(),
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(text.ends_with("1 passed; 0 failed; 0 ignored\n"), "{text}");
    // The example passes and its report stays visible, in text and in JSON.
    assert!(
        text.contains("ok 1 - Lib one#1\n  warning: integer overflow wrapped at"),
        "{text}"
    );
    let output = tsuzuri(
        &[
            "test",
            "--doc",
            package.path(),
            "--allow-overflow",
            "--json",
        ],
        &package.cache(),
        &[],
    );
    let line = stdout(&output)
        .lines()
        .find(|line| line.contains("\"type\":\"doctest\""))
        .unwrap_or_default()
        .to_owned();
    assert!(
        line.contains("\"status\":\"passed\",\"overflow\":[\"integer overflow wrapped at"),
        "{line}"
    );
}

#[test]
fn a_blank_between_the_backticks_and_the_language_still_opens_an_example() {
    let source = "/// One.\n///\n/// ``` tsuzuri run=1\n/// Lib.one\n/// ```\ndef one :: i64 = 1\n";
    let package = Package::new("spaced-fence", &[("Lib.tz", source)]);
    let output = tsuzuri(
        &["test", "--doc", package.path(), "--list"],
        &package.cache(),
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "0 Lib.one#1 [run]\n");
    assert_eq!(
        tsuzuri::doctest::render_doc("One.\n\n``` tsuzuri run=1\n# let x = 1\nLib.one\n```"),
        "One.\n\n```tsuzuri\nLib.one\n```\n\nOutput:\n\n```text\n1\n```"
    );
}

#[test]
fn package_with_a_manifest_and_nested_modules_runs_its_examples() {
    let package = Package::new(
        "manifest",
        &[
            (
                "Tsuzuri.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2027\"\nnamespace = \"App\"\n",
            ),
            (
                "Main.tz",
                "def main :: unit -> i32 = \\_ -> 0\n\ntest \"own test\" = Test.equal (ref 1) (ref 2)\n",
            ),
            (
                "Geometry/Point.tz",
                "/// The x of the origin.\n///\n/// ```tsuzuri run=0\n/// Geometry::Point.origin_x()\n/// ```\ndef origin_x :: i64 = 0\n",
            ),
        ],
    );
    let output = tsuzuri(&["test", "--doc", package.path()], &package.cache(), &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        stdout(&output),
        "ok 1 - Geometry::Point origin_x#1\n\n1 passed; 0 failed; 0 ignored\n"
    );
}

#[test]
fn std_directory_is_read_as_std() {
    let cache = cache("std");
    let output = tsuzuri(&["test", "--doc", "std", "--list", "--json"], &cache, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let rows: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        rows.iter()
            .all(|row| row["path"].as_str().unwrap().contains("std"))
    );
    // Every public std function has an example (tests/std_examples.rs counts them).
    assert!(rows.len() > 800, "{} std examples", rows.len());
    assert!(rows.iter().any(|row| row["module"] == "Array"));
    let _ = fs::remove_dir_all(cache);
}
