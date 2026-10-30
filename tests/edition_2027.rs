//! A22 Phase 4 and 5: edition 2027 rejects the noncanonical spellings with `E0004`, and
//! `tsuzuri fix --edition 2027` migrates a 2026 package without changing its generated code.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tsuzuri::parser::parse_with_source_all;
use tsuzuri::syntax::Edition;

const SAMPLES: [&str; 8] = [
    "for i in 1 .. 3 do ()",
    "let a = 86uy",
    "let a = 1.5f",
    "let r = &x",
    "let r = &mut y",
    "let r = ref x\nlet a = *r + 1",
    "let g = \\(r: &i64) -> deref r",
    "let g = a -> b -> a + b",
];

fn source(body: &str) -> String {
    format!("let x = 1\nlet mut y = 2\n{body}\n0\n")
}

#[test]
fn edition_2027_rejects_every_decision_row() {
    for body in SAMPLES {
        let text = source(body);
        let findings = parse_with_source_all(&text, 0, Edition::E2026)
            .unwrap()
            .noncanonical;
        assert_eq!(findings.len(), 1, "{body}");
        let errors = parse_with_source_all(&text, 0, Edition::E2027).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}");
        let error = &errors[0];
        assert_eq!(error.code, "E0004", "{body}");
        assert!(
            error.kind.is_some_and(|kind| kind.ends_with("_rejected")),
            "{body}"
        );
        assert_eq!(
            error.message,
            format!(
                "'{}' is not allowed in edition 2027; write '{}'",
                findings[0].spelling, findings[0].canonical
            )
        );
        assert_eq!(error.span, findings[0].span);
    }
}

#[test]
fn edition_2027_reports_every_occurrence_in_one_run() {
    let text = source(&SAMPLES.join("\n"));
    let errors = parse_with_source_all(&text, 0, Edition::E2027).unwrap_err();
    assert_eq!(errors.len(), SAMPLES.len());
    assert!(errors.iter().all(|error| error.code == "E0004"));
    // In parse order.
    assert!(
        errors
            .windows(2)
            .all(|pair| pair[0].span.start < pair[1].span.start)
    );
}

#[test]
fn edition_2027_accepts_the_canonical_forms() {
    let text = "let x = 1\nlet mut y = 2\nfor i in 1 ..= 3 do ()\nfor i in 1 ..< 3 do ()\nlet a = 86i8u\n\
                let b = 1.5f32\nlet r = ref x\nlet m = ref mut y\nlet c = deref r + 1\n\
                let g = \\(r: ref i64) -> deref r\nlet h = \\(a: i64) (b: i64) -> a + b\n\
                let xs = [1, 2, 3]\nlet s = ref xs[0..2]\nlet t = ref xs[..=1]\n\
                h c (g r) + s.length + t.length + (a as i64) + (b as i64)\n";
    let program = parse_with_source_all(text, 0, Edition::E2027).unwrap();
    assert!(program.noncanonical.is_empty());
    tsuzuri::analyze_with_edition(text, Edition::E2027)
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
}

#[test]
fn edition_2027_diagnostics_have_no_fixes() {
    let errors =
        parse_with_source_all(&source(&SAMPLES.join("\n")), 0, Edition::E2027).unwrap_err();
    assert!(errors.iter().all(|error| error.fixes.is_empty()));
    assert!(errors.iter().all(|error| error.help.is_some()));
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-edition-2027-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn tsuzuri(arguments: &[&str], directory: &Path, cache: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .current_dir(directory)
        .env("TSUZURI_CACHE_DIR", cache)
        .output()
        .unwrap()
}

#[test]
fn edition_2027_does_not_reject_std_or_2026_dependencies() {
    let root = scratch("dependency");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2027\"\n\n[dependencies]\nlegacy = { path = \"../legacy\" }\n",
    );
    write(
        &root,
        "app/Main.tz",
        "let xs = Array.init 3 (\\i -> i)\nLegacy::Old.total (ref xs) + Array.sum (ref xs)\n",
    );
    write(
        &root,
        "legacy/Tsuzuri.toml",
        "[package]\nname = \"legacy\"\nversion = \"1.0.0\"\n",
    );
    write(
        &root,
        "legacy/Old.tz",
        "def total :: &[i64] -> i64 = \\xs ->\n    let mut sum = 0l\n    for i in 0 .. xs.length - 1 do\n        sum = sum + xs[i]\n    sum\n",
    );
    let output = tsuzuri(&["run", "app"], &root, &root.join("cache"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "6\n");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fmt_stops_on_noncanonical_forms_in_2027() {
    let root = scratch("fmt");
    let text = "let x  =  1\nlet r = &x\nderef r\n";
    write(&root, "Main.tz", text);
    let output = tsuzuri(&["fmt", "Main.tz"], &root, &root.join("cache"));
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("error[E0004]"));
    assert_eq!(std::fs::read_to_string(root.join("Main.tz")).unwrap(), text);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_2027_project_uses_the_frontend_cache_after_fixing() {
    let root = scratch("cache");
    let cache = root.join("cache");
    write(
        &root,
        "app/Main.tz",
        "let x = 41\nlet r = &x\nderef r + 1\n",
    );
    let failed = tsuzuri(&["check", "app"], &root, &cache);
    assert_eq!(failed.status.code(), Some(1));
    write(
        &root,
        "app/Main.tz",
        "let x = 41\nlet r = ref x\nderef r + 1\n",
    );
    for _ in 0..2 {
        let output = tsuzuri(&["check", "app"], &root, &cache);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

/// Phase 5: `tsuzuri fix --edition 2027 --from 2026` applies the `certain` rewrites with A18's
/// migration; the migrated package is 2027, holds no noncanonical spelling, and builds the
/// same IR. A second run changes nothing.
#[test]
fn corpus_migrates_without_changing_the_ir() {
    let root = scratch("migrate");
    let cache = root.join("cache");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/canonical_2026");
    for file in ["Tsuzuri.toml", "Main.tz"] {
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::copy(fixture.join(file), root.join("app").join(file)).unwrap();
    }
    let ir = |name: &str| {
        let output = tsuzuri(
            &["build", "app", "--emit", "llvm", "-O0", "-o", name],
            &root,
            &cache,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read_to_string(root.join(name)).unwrap()
    };
    let before = ir("before.ll");
    let migrate = tsuzuri(
        &["fix", "--edition", "2027", "--from", "2026", "app"],
        &root,
        &cache,
    );
    assert!(
        migrate.status.success(),
        "{}",
        String::from_utf8_lossy(&migrate.stderr)
    );
    let manifest = std::fs::read_to_string(root.join("app/Tsuzuri.toml")).unwrap();
    assert!(manifest.contains("edition = \"2027\""), "{manifest}");
    let migrated = std::fs::read_to_string(root.join("app/Main.tz")).unwrap();
    assert!(
        parse_with_source_all(&migrated, 0, Edition::E2027).is_ok(),
        "{migrated}"
    );
    assert_eq!(ir("after.ll"), before);
    let again = tsuzuri(&["fix", "--edition", "2027", "app"], &root, &cache);
    assert!(again.status.success());
    assert_eq!(
        std::fs::read_to_string(root.join("app/Main.tz")).unwrap(),
        migrated
    );
    let _ = std::fs::remove_dir_all(root);
}
