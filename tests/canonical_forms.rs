//! A22 Phase 3: the noncanonical spellings that the parser records, their rewrites and `W1007`.
//! The rewritten sources are written by hand from the decision table (appendix G.2).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tsuzuri::parser::parse_with_source_all;
use tsuzuri::syntax::{Edition, FixLevel, Noncanonical, NoncanonicalKind};

fn findings(source: &str) -> Vec<Noncanonical> {
    parse_with_source_all(source, 0, Edition::E2026)
        .unwrap_or_else(|errors| panic!("{source}\n{}", errors[0].message))
        .noncanonical
}

/// The source with the edits of the findings at `minimum` or surer applied.
fn apply(source: &str, findings: &[Noncanonical], minimum: FixLevel) -> String {
    let mut edits: Vec<_> = findings
        .iter()
        .filter(|finding| finding.level <= minimum)
        .flat_map(|finding| finding.edits.iter())
        .collect();
    edits.sort_by_key(|edit| std::cmp::Reverse((edit.span.start, edit.span.end)));
    let mut out = source.to_owned();
    for edit in edits {
        out.replace_range(edit.span.start..edit.span.end, &edit.text);
    }
    out
}

/// One row of appendix G.2: the source, the findings and the rewritten source.
fn row(source: &str, expected: &[(NoncanonicalKind, &str, &str, FixLevel)], rewritten: &str) {
    let found = findings(source);
    let summary: Vec<_> = found
        .iter()
        .map(|finding| {
            (
                finding.kind,
                finding.spelling.as_str(),
                finding.canonical.as_str(),
                finding.level,
            )
        })
        .collect();
    assert_eq!(summary, expected, "{source}");
    assert_eq!(
        apply(source, &found, FixLevel::Likely),
        rewritten,
        "{source}"
    );
    // The rewrite is canonical, unless there is none.
    if expected
        .iter()
        .all(|(.., level)| *level != FixLevel::Unavailable)
    {
        assert!(findings(rewritten).is_empty(), "{rewritten}");
    }
}

#[test]
fn every_decision_row_is_reported() {
    use FixLevel::*;
    use NoncanonicalKind::*;
    let prelude = "let xs = [1, 2, 3, 4]\nlet x = 1\nlet mut y = 2\nlet f = \\v -> v\nlet add = \\a b -> a + b\n";
    let check = |body: &str, expected: &[(NoncanonicalKind, &str, &str, FixLevel)], after: &str| {
        row(
            &format!("{prelude}{body}\n0\n"),
            expected,
            &format!("{prelude}{after}\n0\n"),
        )
    };
    check(
        "for i in 1 .. 3 do ()",
        &[(BareRange, "..", "..=", Certain)],
        "for i in 1 ..= 3 do ()",
    );
    check(
        "for i in 1 .. 2 .. 9 do ()",
        &[(BareRange, "..", "..=", Certain)],
        "for i in 1 .. 2 ..= 9 do ()",
    );
    check(
        "for i in 1 ..= 3 do ()\nlet a = ref xs[1..3]\nfor i = 1 to 3 do ()",
        &[],
        "for i in 1 ..= 3 do ()\nlet a = ref xs[1..3]\nfor i = 1 to 3 do ()",
    );
    check(
        "let a = 86uy",
        &[(ShortSuffix, "86uy", "86i8u", Certain)],
        "let a = 86i8u",
    );
    for (short, long) in [
        ("86y", "86i8"),
        ("86uy", "86i8u"),
        ("86s", "86i16"),
        ("86us", "86i16u"),
        ("86u", "86i32u"),
        ("86l", "86i64"),
        ("86ul", "86i64u"),
        ("86L", "86i128"),
        ("86UL", "86i128u"),
        ("1hf", "1f16"),
        ("1f", "1f32"),
        ("1F", "1f128"),
        ("1hm", "1d32"),
        ("1m", "1d64"),
        ("1M", "1d128"),
        ("1_000l", "1_000i64"),
        ("0x10l", "0x10i64"),
        ("0b101uy", "0b101i8u"),
        ("1.5f", "1.5f32"),
        ("2e3f", "2e3f32"),
    ] {
        check(
            &format!("let a = {short}"),
            &[(ShortSuffix, short, long, Certain)],
            &format!("let a = {long}"),
        );
    }
    for canonical in ["86i32u", "86ubyte", "86sbyte", "86I", "1f32", "0xFFf", "3"] {
        check(
            &format!("let a = {canonical}"),
            &[],
            &format!("let a = {canonical}"),
        );
    }
    check(
        "let r = &x",
        &[(SymbolicBorrow, "&", "ref", Certain)],
        "let r = ref x",
    );
    check(
        "let r = &mut y",
        &[(SymbolicBorrow, "&mut", "ref mut", Certain)],
        "let r = ref mut y",
    );
    check(
        "let a = f &x",
        &[(SymbolicBorrow, "&", "ref", Certain)],
        "let a = f (ref x)",
    );
    check(
        "let r = &f x",
        &[(SymbolicBorrow, "&", "ref", Likely)],
        "let r = ref (f x)",
    );
    check(
        "let a = f (&[1, 2, 3])",
        &[(SymbolicBorrow, "&", "ref", Likely)],
        "let a = f (ref [1, 2, 3])",
    );
    check(
        "let a = f (&xs[1..3])\nlet b = f &xs[1..3]",
        &[
            (SymbolicBorrow, "&", "ref", Certain),
            (SymbolicBorrow, "&", "ref", Certain),
        ],
        "let a = f (ref xs[1..3])\nlet b = f (ref xs[1..3])",
    );
    check(
        "let r = ref x\nlet a = *r + 1",
        &[(SymbolicDeref, "*", "deref", Certain)],
        "let r = ref x\nlet a = deref r + 1",
    );
    check(
        "let r = ref x\nlet a = *f r",
        &[(SymbolicDeref, "*", "deref", Certain)],
        "let r = ref x\nlet a = deref (f r)",
    );
    check(
        "let r = ref x\nlet a = add *r *r",
        &[
            (SymbolicDeref, "*", "deref", Certain),
            (SymbolicDeref, "*", "deref", Certain),
        ],
        "let r = ref x\nlet a = add (deref r) (deref r)",
    );
    check(
        "let r = ref x\nlet rr = ref r\nlet a = **rr\nlet b = -*r",
        &[
            (SymbolicDeref, "**", "deref deref", Certain),
            (SymbolicDeref, "*", "deref", Certain),
        ],
        "let r = ref x\nlet rr = ref r\nlet a = deref deref rr\nlet b = -deref r",
    );
    check(
        "let g = \\(r: &i64) -> deref r",
        &[(SymbolicReferenceType, "&", "ref", Certain)],
        "let g = \\(r: ref i64) -> deref r",
    );
    check(
        "let p: &mut i64 = ref mut y\nlet q: &&i64 = ref ref x",
        &[
            (SymbolicReferenceType, "&", "ref", Certain),
            (SymbolicReferenceType, "&&", "ref ref", Certain),
        ],
        "let p: ref mut i64 = ref mut y\nlet q: ref ref i64 = ref ref x",
    );
    check(
        "let g = v -> v + 1",
        &[(LegacyLambda, "v ->", "\\v ->", Certain)],
        "let g = \\v -> v + 1",
    );
    check(
        "let g = a -> b -> a + b",
        &[(LegacyLambda, "a -> b ->", "\\a b ->", Certain)],
        "let g = \\a b -> a + b",
    );
    check(
        "let g = a -> \\b -> a + b",
        &[(LegacyLambda, "a ->", "\\a b ->", Certain)],
        "let g = \\a b -> a + b",
    );
    check(
        "let g = a -> b -> c -> a + b + c",
        &[(LegacyLambda, "a -> b -> c ->", "\\a b c ->", Certain)],
        "let g = \\a b c -> a + b + c",
    );
    check(
        "let g = a -> (\\b -> a + b)",
        &[(LegacyLambda, "a ->", "\\a b ->", Unavailable)],
        "let g = a -> (\\b -> a + b)",
    );
    check(
        "let g = _ -> 0",
        &[(LegacyLambda, "_ ->", "\\_ ->", Likely)],
        "let g = \\_ -> 0",
    );
    check(
        "let g = \\v -> v + 1\nlet h = \\a b -> a + b",
        &[],
        "let g = \\v -> v + 1\nlet h = \\a b -> a + b",
    );
    check(
        "let s = $\"{&x}\"",
        &[(SymbolicBorrow, "&", "ref", Certain)],
        "let s = $\"{ref x}\"",
    );
    check(
        "let n = 2l\nlet m = match n with | 1l -> 0 | _ -> 1",
        &[
            (ShortSuffix, "2l", "2i64", Certain),
            (ShortSuffix, "1l", "1i64", Certain),
        ],
        "let n = 2i64\nlet m = match n with | 1i64 -> 0 | _ -> 1",
    );
}

#[test]
fn reference_operand_borrows_have_no_fix() {
    let module =
        tsuzuri::analyze("let x = 41\nlet r = ref x\nlet rr = &r\nlet y = &x\n0\n").unwrap();
    let levels: Vec<_> = module
        .noncanonical
        .iter()
        .map(|finding| (finding.level, finding.edits.is_empty()))
        .collect();
    assert_eq!(
        levels,
        [(FixLevel::Unavailable, true), (FixLevel::Certain, false)]
    );
    let warnings = tsuzuri::canonical::warnings(&module);
    assert_eq!(
        warnings[0].help.as_deref(),
        Some(
            "a reference to a reference cannot be written with 'ref'; 'ref r' reborrows r, so pass 'r' itself or restructure the code"
        )
    );
    assert!(warnings[0].fixes.is_empty());
    assert_eq!(warnings[1].fixes.len(), 1);
    // Review of #66: parentheses widen the borrow's span to the `(`; the `&` inside is still
    // found, and a parenthesized borrow of a plain value keeps its rewrite.
    let module = tsuzuri::analyze(
        "let x = 41\nlet r = ref x\nlet rr = (&r)\nlet rs = Maybe.Some (&r)\nlet y = (&x)\nlet z = ((&x))\n0\n",
    )
    .unwrap();
    let levels: Vec<_> = module
        .noncanonical
        .iter()
        .map(|finding| finding.level)
        .collect();
    assert_eq!(
        levels,
        [
            FixLevel::Unavailable,
            FixLevel::Unavailable,
            FixLevel::Certain,
            FixLevel::Certain
        ]
    );
}

fn scratch(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("tsuzuri-canonical-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn tsuzuri(arguments: &[&str], directory: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .current_dir(directory)
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap()
}

/// A 2026 project, where the noncanonical spellings are still accepted.
fn legacy(root: &Path, main: &str) {
    write(
        root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    write(root, "app/Main.tz", main);
}

#[test]
fn warn_canonical_is_off_by_default() {
    let root = scratch("off");
    legacy(
        &root,
        "let mut total = 0\nfor i in 1 .. 3 do\n    total = total + i\ntotal\n",
    );
    let off = tsuzuri(&["check", "app"], &root);
    assert!(off.status.success());
    assert!(
        off.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&off.stderr)
    );
    let on = tsuzuri(&["check", "app", "--warn", "canonical"], &root);
    assert!(on.status.success());
    let stderr = String::from_utf8_lossy(&on.stderr);
    assert!(
        stderr.contains(
            "app/Main.tz:2:12: warning[W1007]: '..' is not canonical; write '..='\n  2 | for i in 1 .. 3 do\n    |            ^\n  help: write '..=' to include the end or '..<' to exclude it\n  fix (certain): Replace '..' with '..='\n"
        ),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn deny_warnings_counts_w1007() {
    let root = scratch("deny");
    legacy(&root, "let a = 86uy\n0\n");
    let denied = tsuzuri(
        &["check", "app", "--warn", "canonical", "--deny-warnings"],
        &root,
    );
    assert_eq!(denied.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn json_has_kind_help_and_fixes() {
    let root = scratch("json");
    legacy(
        &root,
        "let x = 1\nlet a = &x\nlet b = Array.length (&[1])\nlet c = &a\nb\n",
    );
    let output = tsuzuri(&["check", "app", "--warn", "canonical", "--json"], &root);
    let lines: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let fixes: Vec<_> = lines
        .iter()
        .filter(|line| line["code"] == "W1007")
        .map(|line| {
            assert_eq!(line["kind"], "symbolic_borrow");
            assert!(line["help"].is_string());
            line["fixes"]
                .as_array()
                .map(|fixes| fixes[0]["applicability"].as_str().unwrap().to_owned())
        })
        .collect();
    assert_eq!(
        fixes,
        [Some("certain".to_owned()), Some("likely".to_owned()), None]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn std_and_dependencies_report_nothing() {
    let root = scratch("dependency");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\nlib = { path = \"../lib\" }\n",
    );
    write(
        &root,
        "app/Main.tz",
        "let a = Lib::Util.twice 2l\nlet b = Array.length [1, 2]\na + b\n",
    );
    write(
        &root,
        "lib/Tsuzuri.toml",
        "[package]\nname = \"lib\"\nversion = \"1.0.0\"\nedition = \"2026\"\n",
    );
    write(
        &root,
        "lib/Util.tz",
        "def twice :: i64 -> i64 = n -> n * 2l\n",
    );
    let output = tsuzuri(&["check", "app", "--warn", "canonical"], &root);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("warning[W1007]").count(), 1, "{stderr}");
    assert!(stderr.contains("app/Main.tz:1:25"), "{stderr}");
    let _ = std::fs::remove_dir_all(root);
}

/// Appendix G: the `certain` rewrites of every fixture of the 2026 corpus keep the IR.
#[test]
fn corpus_certain_fixes_keep_the_ir() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/canonical_2026");
    let source = std::fs::read_to_string(directory.join("Main.tz")).unwrap();
    let module = tsuzuri::analyze_with_edition(&source, Edition::E2026).unwrap();
    let before = tsuzuri::llvm::emit(&module, tsuzuri::llvm::Entry::Library).unwrap();
    assert!(
        module.noncanonical.len() >= 20,
        "{}",
        module.noncanonical.len()
    );
    let mut text = source;
    for _ in 0..5 {
        let found = tsuzuri::parser::parse_with_source_all(&text, 0, Edition::E2026)
            .unwrap()
            .noncanonical;
        if found
            .iter()
            .all(|finding| finding.level != FixLevel::Certain)
        {
            break;
        }
        text = apply(&text, &found, FixLevel::Certain);
    }
    // The same edition, so that only the spelling differs (2027 also changes the parameter
    // conventions, A18); the rewritten source is one that 2027 accepts.
    let rewritten = tsuzuri::analyze_with_edition(&text, Edition::E2026)
        .unwrap_or_else(|error| panic!("{text}\n{}: {}", error.code, error.message));
    let after = tsuzuri::llvm::emit(&rewritten, tsuzuri::llvm::Entry::Library).unwrap();
    assert_eq!(before, after);
    tsuzuri::parser::parse_with_source_all(&text, 0, Edition::E2027)
        .unwrap_or_else(|errors| panic!("{text}\n{}", errors[0].message));
}
