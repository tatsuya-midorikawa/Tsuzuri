//! G25: typed holes `?` and `?name`. A hole is one `E1030` (kind `typed_hole`) that reports the
//! expected type, the locals in scope and (Phase 2) candidates; the checker recovers from it, so it
//! never cascades and a program with a hole is never built.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::analyze_all;
use tsuzuri::diagnostic::{Applicability, Diagnostic, location};

fn errors(source: &str) -> Vec<Diagnostic> {
    analyze_all(source).expect_err(source).diagnostics
}

fn one(source: &str) -> Diagnostic {
    let mut errors = errors(source);
    assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
    errors.remove(0)
}

fn scope_names(diagnostic: &Diagnostic) -> Vec<&str> {
    diagnostic
        .detail
        .scope
        .iter()
        .map(|entry| entry.name.as_str())
        .collect()
}

fn candidate_names(diagnostic: &Diagnostic) -> Vec<&str> {
    diagnostic
        .detail
        .candidates
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect()
}

/// A new empty directory for one project, removed by `Drop`.
struct Project(PathBuf);

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-typed-holes-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        for (name, text) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tsuzuri(arguments: &[&std::ffi::OsStr], threads: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command.args(arguments);
    if let Some(threads) = threads {
        command.env("TSUZURI_THREADS", threads);
    }
    command.output().unwrap()
}

#[test]
fn a_hole_is_one_error_and_does_not_cascade() {
    let source = "def f :: i64 -> i64 = \\n -> n + ?\nf 1\n";
    let hole = one(source);
    assert_eq!(hole.code, "E1030");
    assert_eq!(hole.kind, Some("typed_hole"));
    assert_eq!((hole.span.start, hole.span.end), (32, 33));
    assert_eq!(
        hole.message,
        "typed hole '?' of type 'i64'; replace it with an expression of that type"
    );
    assert_eq!(
        hole.help.as_deref(),
        Some("fill the hole with a value of type 'i64'")
    );
    assert_eq!(hole.detail.expected.as_deref(), Some("i64"));
    assert_eq!(hole.detail.found, None);
    let scope: Vec<(&str, &str)> = hole
        .detail
        .scope
        .iter()
        .map(|entry| (entry.name.as_str(), entry.ty.as_str()))
        .collect();
    assert_eq!(scope, [("n", "i64")]);
}

#[test]
fn a_named_hole_keeps_its_name_and_reports_unknown_type() {
    let hole = one("let x = ?foo\nx\n");
    assert_eq!(hole.code, "E1030");
    assert_eq!((hole.span.start, hole.span.end), (8, 12));
    assert_eq!(
        hole.message,
        "typed hole '?foo' of unknown type; replace it with an expression"
    );
    assert_eq!(
        hole.help.as_deref(),
        Some("fill the hole with an expression; its type is not known here")
    );
    assert_eq!(hole.detail.expected.as_deref(), Some("_"));
    assert!(hole.detail.scope.is_empty());
    assert!(hole.detail.candidates.is_empty());
}

#[test]
fn holes_and_independent_errors_are_all_reported() {
    let source = "def f :: i64 -> i64 = \\n ->\n    let a: i64 = ?\n    let b: bool = 1\n    let c = a + ?\n    n\nf 1\n";
    let errors = errors(source);
    let found: Vec<(&str, (usize, usize))> = errors
        .iter()
        .map(|error| (error.code, location(source, error.span.start)))
        .collect();
    assert_eq!(
        found,
        [("E1030", (2, 18)), ("E1003", (3, 19)), ("E1030", (4, 17))]
    );
    assert_eq!(errors[1].message, "expected bool, found i32");
    // `a` is the error's recovery, so its type is open and `a + ?` expects any type.
    assert_eq!(errors[2].detail.expected.as_deref(), Some("_"));
}

#[test]
fn expected_type_follows_the_position() {
    let source = r#"record P { x: i64, y: bool }
def g :: i64 -> bool -> i64 = \n b -> n
def p01 :: i64 -> i64 = \n -> n + ?
def p02 :: i64 -> i64 = \n -> ?
def p03 :: i64 -> i64 = \n -> g n ?
def p04 :: i64 -> i64 = \n -> if n > 0 then ? else 0
def p05 :: i64 -> P = \n -> P { x: ?, y: true }
def p06 :: i64 -> i64 = \n -> match n with | 0 -> ? | _ -> 1
def p07 :: bool -> bool = \b -> ? && b
def p08 :: i64 -> i64 = \n ->
    let a: i64 = ?
    a
def p09 :: i64 -> i64 = \n ->
    let b = ?
    n
def p10 :: i64 -> i64 = \n ->
    let c: [i64] = [1, ?]
    n
def p11 :: i64 -> i64 = \n ->
    let d = (1, ?)
    n
def p12 :: i64 -> i64 = \n ->
    let h = \z -> ?
    n
def p13 :: [i64] -> i64 = \xs -> Array.length ?
def p14 :: i64 -> i64 = \n -> g ? true
def p15 :: i64 -> i64 = \n -> ? n
p01 1
"#;
    let errors = errors(source);
    assert!(errors.iter().all(|error| error.code == "E1030"));
    let found: Vec<(usize, &str)> = errors
        .iter()
        .map(|error| {
            (
                location(source, error.span.start).0,
                error.detail.expected.as_deref().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (3, "i64"),
            (4, "i64"),
            (5, "bool"),
            (6, "i64"),
            (7, "i64"),
            (8, "i64"),
            (9, "_"),
            (11, "i64"),
            (14, "_"),
            (17, "i64"),
            (20, "_"),
            (23, "_"),
            (25, "ref [_]"),
            (26, "i64"),
            (27, "_"),
        ]
    );
}

#[test]
fn scope_lists_user_locals_in_a_fixed_order() {
    // The match subject's generated local and the hidden parameter `n` are not shown.
    let hole = one(
        "def f :: i64 -> bool -> i64 = \\n flag ->\n    let s = \"a\"\n    let n = 1.5\n    match flag with\n    | true -> ?\n    | false -> 0\nf 1 true\n",
    );
    assert_eq!(hole.detail.expected.as_deref(), Some("i64"));
    assert_eq!(scope_names(&hole), ["flag", "s", "n"]);
    let types: Vec<&str> = hole
        .detail
        .scope
        .iter()
        .map(|entry| entry.ty.as_str())
        .collect();
    assert_eq!(types, ["bool", "string", "f64"]);
    // The locals that fit `i64` come first, each group in declaration order.
    let hole = one(
        "def g :: string -> i64 -> i64 = \\s k ->\n    let t = \"b\"\n    let m = 2\n    ?\ng \"a\" 1\n",
    );
    assert_eq!(scope_names(&hole), ["k", "m", "s", "t"]);
    let mut source = "def h :: i64 -> i64 = \\n ->\n".to_owned();
    for index in 0..40 {
        source.push_str(&format!("    let v{index} = {index}\n"));
    }
    source.push_str("    ?\nh 1\n");
    let hole = one(&source);
    let names = scope_names(&hole);
    assert_eq!(names.len(), 32);
    assert_eq!(names.first(), Some(&"n"));
    assert_eq!(names.last(), Some(&"v30"));
}

#[test]
fn a_program_with_a_hole_is_not_built() {
    let project = Project::new(&[("Main.tz", "def f :: i64 -> i64 = \\n -> n + ?\nf 1\n")]);
    let out = project.path().join("out");
    let output = tsuzuri(
        &[
            "build".as_ref(),
            project.path().as_os_str(),
            "-o".as_ref(),
            out.as_os_str(),
        ],
        None,
    );
    assert_ne!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("E1030"));
    assert!(!out.exists());
}

#[test]
fn fmt_keeps_holes() {
    let project = Project::new(&[("Main.tz", "let x = f   ?a ?\nx\n")]);
    let output = tsuzuri(&["fmt".as_ref(), project.path().as_os_str()], None);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.path().join("Main.tz")).unwrap(),
        "let x = f ?a ?\nx\n"
    );
    let output = tsuzuri(
        &[
            "fmt".as_ref(),
            "--check".as_ref(),
            project.path().as_os_str(),
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn question_mark_outside_an_expression_is_a_syntax_error() {
    for source in ["match 1 with\n| ? -> 0\n| _ -> 1\n", "let y: ? = 1\ny\n"] {
        assert_eq!(errors(source)[0].code, "E0002", "{source}");
    }
}

const ARRAY_HOLE: &str = "def f :: [i64] -> i64 = \\xs -> ?\nf [1, 2]\n";

#[test]
fn candidates_use_locals_and_are_ranked() {
    let hole = one(ARRAY_HOLE);
    let names = candidate_names(&hole);
    assert_eq!(names.len(), 10, "{names:?}");
    // `xs` is an owned `[i64]` (edition 2026), lent with `ref`. A result of a known type comes
    // before a result that is a type variable, then the names in order.
    assert_eq!(names[0], "Array.length (ref xs)");
    assert_eq!(
        hole.detail.candidates[0].signature,
        "Array.length :: ref ['a] -> i64"
    );
    let product = names
        .iter()
        .position(|name| *name == "Array.product (ref xs)");
    let sum = names.iter().position(|name| *name == "Array.sum (ref xs)");
    assert!(
        product.is_some() && sum.is_some() && product < sum,
        "{names:?}"
    );
    assert!(
        hole.detail
            .candidates
            .iter()
            .all(|candidate| candidate.how == "call")
    );
    assert!(hole.fixes.is_empty());
}

#[test]
fn the_function_being_checked_is_not_a_candidate() {
    let hole = one(ARRAY_HOLE);
    let names = candidate_names(&hole);
    assert!(!names.iter().any(|name| name.starts_with("Main.f ")));
    // `Integer<[i64]>` does not hold, so `xs` does not fill `Int.count_ones`.
    assert!(
        !names
            .iter()
            .any(|name| name.starts_with("Int.count_ones xs"))
    );
}

#[test]
fn constraints_remove_candidates() {
    let hole = one(ARRAY_HOLE);
    let names = candidate_names(&hole);
    // `Float<i64>` does not hold.
    assert!(
        !names
            .iter()
            .any(|name| name.starts_with("Array.sum_pairwise"))
    );
    assert!(names.contains(&"Array.sum (ref xs)"));
}

#[test]
fn one_local_of_the_type_gives_a_maybe_fix() {
    let source = "def f :: i64 -> i64 = \\n -> ?\nf 1\n";
    let hole = one(source);
    let first = &hole.detail.candidates[0];
    assert_eq!(
        (
            first.name.as_str(),
            first.how.as_str(),
            first.signature.as_str()
        ),
        ("n", "value", "n: i64")
    );
    assert_eq!(hole.fixes.len(), 1);
    let fix = &hole.fixes[0];
    assert_eq!(fix.title, "Replace '?' with 'n'");
    assert_eq!(fix.applicability, Applicability::Maybe);
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].span, hole.span);
    assert_eq!(fix.edits[0].text, "n");
    // A named hole names itself in the title.
    let hole = one("def f :: i64 -> i64 = \\n -> ?it\nf 1\n");
    assert_eq!(hole.fixes[0].title, "Replace '?it' with 'n'");
}

#[test]
fn two_locals_of_the_type_give_no_fix() {
    let hole = one("def f :: i64 -> i64 -> i64 = \\a b -> ?\nf 1 2\n");
    let first: Vec<(&str, &str)> = hole.detail.candidates[..2]
        .iter()
        .map(|candidate| (candidate.name.as_str(), candidate.how.as_str()))
        .collect();
    assert_eq!(first, [("a", "value"), ("b", "value")]);
    assert!(hole.fixes.is_empty());
}

#[test]
fn union_cases_are_candidates() {
    let hole = one("union Shape = Circle of f64 | Square of f64 | Empty\nlet s: Shape = ?\ns\n");
    let first: Vec<(&str, &str)> = hole.detail.candidates[..3]
        .iter()
        .map(|candidate| (candidate.name.as_str(), candidate.how.as_str()))
        .collect();
    assert_eq!(
        first,
        [
            ("Main.Empty", "value"),
            ("Main.Circle ?", "call"),
            ("Main.Square ?", "call")
        ]
    );
}

#[test]
fn candidates_are_capped_and_deterministic() {
    let project = Project::new(&[("Main.tz", ARRAY_HOLE)]);
    let arguments = [
        "check".as_ref(),
        "--json".as_ref(),
        project.path().as_os_str(),
    ];
    let one = tsuzuri(&arguments, Some("1"));
    let eight = tsuzuri(&arguments, Some("8"));
    assert_eq!(one.status.code(), Some(1));
    assert_eq!(one.stderr, eight.stderr);
    let stderr = String::from_utf8(one.stderr).unwrap();
    let line: serde_json::Value = serde_json::from_str(stderr.lines().next().unwrap()).unwrap();
    assert_eq!(line["candidates"].as_array().unwrap().len(), 10);
    // Without a manifest the CLI reads the latest edition, where the parameter is borrowed
    // (`ref [i64]`) and is passed on as it is.
    assert_eq!(line["candidates"][0]["name"], "Array.length xs");
    assert_eq!(line["scope"][0]["type"], "ref [i64]");
}

#[test]
fn only_the_first_sixteen_holes_of_a_function_get_candidates() {
    let mut source = "def f :: i64 -> i64 = \\n ->\n".to_owned();
    for index in 0..17 {
        source.push_str(&format!("    let a{index}: i64 = ?\n"));
    }
    source.push_str("    n\nf 1\n");
    let errors = errors(&source);
    assert_eq!(errors.len(), 17);
    assert!(
        errors[..16]
            .iter()
            .all(|error| !error.detail.candidates.is_empty())
    );
    assert!(errors[16].detail.candidates.is_empty());
    assert!(errors[16].fixes.is_empty());
}

#[test]
fn an_unknown_expected_type_has_no_candidates() {
    let hole = one("let x = ?\nx\n");
    assert!(hole.detail.candidates.is_empty());
    assert!(hole.fixes.is_empty());
}

#[test]
fn a_users_generic_function_is_not_a_candidate() {
    // `bits` requires `Integer<'a>` through its body, which is not known when the hole is checked.
    let hole = one(
        "def bits :: 'a -> i64 = \\x -> Int.count_ones x\ndef f :: string -> i64 = \\text -> ?\nf \"a\"\n",
    );
    assert!(
        !candidate_names(&hole)
            .iter()
            .any(|name| name.starts_with("Main.bits"))
    );
}

#[test]
fn an_open_numeric_local_keeps_its_class() {
    // `n` is an `Integer`, so it neither fits a `bool` hole nor gives a fix.
    let hole = one("def f :: unit -> bool = \\_ ->\n    let n = 1\n    ?\nf ()\n");
    assert_eq!(hole.detail.expected.as_deref(), Some("bool"));
    assert!(!candidate_names(&hole).contains(&"n"));
    assert!(hole.fixes.is_empty());
    assert_eq!(scope_names(&hole), ["n"]);
}

#[test]
fn fields_are_projections_of_readable_records() {
    let hole = one("record P { x: i64 }\ndef f :: P -> i64 = \\p -> ?\nf (P { x: 1 })\n");
    let names = candidate_names(&hole);
    assert!(names.contains(&"p.x"), "{names:?}");
    assert!(!names.iter().any(|name| name.starts_with("Main.P.x")));
    // A std record whose representation is private to its module has no field candidates.
    let hole = one("def f :: Map<i64, i64> -> i64 = \\m -> ?\nf (Map.empty())\n");
    assert!(
        !candidate_names(&hole)
            .iter()
            .any(|name| name.starts_with("m."))
    );
}

#[test]
fn a_function_without_parameters_is_called() {
    let hole = one(
        "def answer :: i64 = 42\nconst limit: i64 = 7\ndef f :: unit -> i64 = \\_ -> ?\nf ()\n",
    );
    let found: Vec<(&str, &str)> = hole
        .detail
        .candidates
        .iter()
        .map(|candidate| (candidate.name.as_str(), candidate.how.as_str()))
        .collect();
    assert!(found.contains(&("Main.answer()", "call")), "{found:?}");
    assert!(found.contains(&("Main.limit", "value")), "{found:?}");
}

#[test]
fn a_hole_in_a_constant_reports_its_type() {
    let hole = one("const answer: i64 = ?\nanswer\n");
    assert_eq!(hole.code, "E1030");
    assert_eq!(hole.detail.expected.as_deref(), Some("i64"));
}

#[test]
fn the_classes_of_the_expected_type_hold() {
    // `count_copies` needs `Copy<'a>`, which `string` lacks: only `numbers` fits.
    let hole = one(
        "def count_copies :: Copy<'a> => ref ['a] -> i64 = \\xs -> Array.length xs\ndef f :: ref [string] -> ref [i64] -> i64 = \\names numbers -> count_copies ?\nf (ref [\"a\"]) (ref [1])\n",
    );
    assert_eq!(scope_names(&hole), ["numbers", "names"]);
    let names = candidate_names(&hole);
    assert_eq!(names[0], "numbers");
    assert!(
        !names.iter().any(|name| name.contains("names")),
        "{names:?}"
    );
    assert_eq!(hole.fixes[0].title, "Replace '?' with 'numbers'");
}

#[test]
fn a_field_read_through_a_reference_is_copy() {
    let source = |result: &str| {
        format!(
            "record P {{ text: string, n: i64 }}\ndef f :: ref P -> {result} = \\p -> ?\nlet p = P {{ text: \"a\", n: 1 }}\nf (ref p)\n"
        )
    };
    assert!(!candidate_names(&one(&source("string"))).contains(&"p.text"));
    assert!(candidate_names(&one(&source("i64"))).contains(&"p.n"));
}

#[test]
fn edition_dependent_names_use_their_stable_spelling() {
    // `Vec.push` is `Vec.push_mut` in edition 2027; the index writes `Vec.push_owned`.
    let hole = one("def f :: Vec<i64> -> i64 -> Vec<i64> = \\v n -> ?\nf (Vec.empty()) 1\n");
    let names = candidate_names(&hole);
    assert!(
        !names.iter().any(|name| name.starts_with("Vec.push ")),
        "{names:?}"
    );
}

#[test]
fn user_candidates_carry_the_package_namespace() {
    let project = Project::new(&[
        (
            "Tsuzuri.toml",
            "[package]\nname = \"finder\"\nversion = \"0.1.0\"\nedition = \"2026\"\nnamespace = \"Finder\"\n",
        ),
        ("Main.tz", "0\n"),
        (
            "Shapes/Square.tz",
            "def answer :: bool -> i64 = \\_flag -> 1\n",
        ),
        (
            "Other/Use.tz",
            "namespace Other\n\ndef f :: bool -> i64 = \\flag -> ?\n",
        ),
    ]);
    let output = tsuzuri(
        &[
            "check".as_ref(),
            "--json".as_ref(),
            project.path().as_os_str(),
        ],
        None,
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line: serde_json::Value = serde_json::from_str(stderr.lines().next().unwrap()).unwrap();
    assert_eq!(
        line["candidates"][0]["name"],
        "Finder::Shapes::Square.answer flag"
    );
}

#[test]
fn a_parameter_read_through_its_reference_has_its_value_type() {
    // In edition 2027 a `mut i64` parameter reads as an `i64`.
    let project = Project::new(&[(
        "Main.tz",
        "def f :: mut i64 -> unit = \\n -> {\n    let r: ref mut i64 = ?;\n    ()\n}\ndef g :: mut i64 -> i64 = \\n -> ?\n0\n",
    )]);
    let output = tsuzuri(
        &[
            "check".as_ref(),
            "--json".as_ref(),
            project.path().as_os_str(),
        ],
        None,
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    let lines: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let names = |line: &serde_json::Value| -> Vec<String> {
        line["candidates"]
            .as_array()
            .map(|all| {
                all.iter()
                    .map(|candidate| candidate["name"].as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert!(!names(&lines[0]).contains(&"n".to_owned()));
    assert!(lines[0]["fixes"].is_null());
    assert_eq!(lines[1]["scope"][0]["type"], "i64");
    assert_eq!(names(&lines[1])[0], "n");
    assert_eq!(lines[1]["fixes"][0]["title"], "Replace '?' with 'n'");
}

#[test]
fn an_argument_keeps_the_classes_of_its_local() {
    // `positive` takes an `i32` (its literal's default): it is no `Integer`, and `xs` of `[bool]`
    // cannot follow it in `Array.count`.
    let hole =
        one("def f :: [bool] -> i64 = \\xs ->\n    let positive = \\n -> n > 0\n    ?\nf [true]\n");
    let names = candidate_names(&hole);
    assert!(
        !names.iter().any(|name| name.starts_with("Int.count_ones")),
        "{names:?}"
    );
    assert!(
        !names.contains(&"Array.count positive (ref xs)"),
        "{names:?}"
    );
}
