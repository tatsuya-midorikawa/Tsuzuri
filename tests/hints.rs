use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::analyze_modules_all;
use tsuzuri::diagnostic::{Applicability, Diagnostic, apply_edits};
use tsuzuri::diagnostic_kinds::DiagnosticKind;
use tsuzuri::hints::{Hint, TABLE};

fn errors(source: &str) -> Vec<Diagnostic> {
    analyze_modules_all(&[("Main", source)])
        .expect_err(source)
        .diagnostics
}

fn hint(id: &str) -> &'static Hint {
    TABLE.iter().find(|hint| hint.id == id).unwrap()
}

#[test]
fn every_hint_matches_its_bad_source() {
    assert_eq!(TABLE.len(), 40);
    for hint in TABLE {
        let diagnostics = errors(hint.bad);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == Some(hint.kind)
                    && diagnostic.help.as_deref() == Some(hint.help)),
            "{}: no {} diagnostic with the hint's help\n{}\n{diagnostics:#?}",
            hint.id,
            hint.kind,
            hint.bad
        );
    }
}

#[test]
fn every_good_source_is_accepted() {
    for hint in TABLE {
        if let Err(set) = analyze_modules_all(&[("Main", hint.good)]) {
            panic!("{}: {}\n{:#?}", hint.id, hint.good, set.diagnostics);
        }
    }
}

#[test]
fn hint_ids_are_unique_and_kinds_are_registered() {
    let mut ids = std::collections::BTreeSet::new();
    for hint in TABLE {
        assert!(ids.insert(hint.id), "duplicate hint {}", hint.id);
        assert!(
            DiagnosticKind::from_id(hint.kind).is_some(),
            "{}: {} is not a kind",
            hint.id,
            hint.kind
        );
        for pattern in hint.patterns {
            assert_eq!(
                pattern
                    .iter()
                    .filter(|element| element.starts_with('@'))
                    .count(),
                1,
                "{}: one main token per pattern",
                hint.id
            );
        }
        if let Some(fix) = &hint.fix {
            assert_ne!(fix.applicability, Applicability::Certain, "{}", hint.id);
        }
    }
}

#[test]
fn hint_text_follows_the_style() {
    for hint in TABLE {
        assert!(
            !hint.id.is_empty()
                && hint
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{}",
            hint.id
        );
        assert!(
            hint.help.chars().count() <= 160
                && hint
                    .help
                    .starts_with(|c: char| c.is_ascii_lowercase() || c == '\'')
                && !hint.help.ends_with('.'),
            "{}: {}",
            hint.id,
            hint.help
        );
        // A title starts with a capital or with an identifier, as in `to_string and Display.display`.
        assert!(
            !hint.title.is_empty() && hint.title.len() <= 60 && !hint.title.ends_with('.'),
            "{}: {}",
            hint.id,
            hint.title
        );
    }
}

#[test]
fn unrelated_diagnostics_get_no_help() {
    let diagnostics = errors("let x: i64 = true\n");
    let mismatch = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.kind == Some("type_mismatch"))
        .unwrap();
    assert!(
        TABLE
            .iter()
            .all(|hint| mismatch.help.as_deref() != Some(hint.help)),
        "{mismatch:?}"
    );
}

#[test]
fn hints_are_deterministic() {
    for hint in TABLE {
        assert_eq!(errors(hint.bad), errors(hint.bad), "{}", hint.id);
    }
}

#[test]
fn fix_edits_replace_only_the_main_token() {
    let hint = hint("match-fat-arrow");
    let diagnostics = errors(hint.bad);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.help.as_deref() == Some(hint.help))
        .unwrap();
    assert_eq!(diagnostic.fixes.len(), 1);
    let fix = &diagnostic.fixes[0];
    assert_eq!(fix.applicability, Applicability::Likely);
    assert_eq!(fix.title, "replace with '->'");
    assert_eq!(fix.edits.len(), 1);
    let edit = &fix.edits[0];
    assert_eq!(&hint.bad[edit.span.start..edit.span.end], "=>");
    assert_eq!(edit.span.start, diagnostic.span.start);
    assert_eq!(edit.text, "->");
}

#[test]
fn a_fix_removes_its_diagnostic() {
    let mut fixed = 0;
    for hint in TABLE.iter().filter(|hint| hint.fix.is_some()) {
        let diagnostics = errors(hint.bad);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.help.as_deref() == Some(hint.help))
            .unwrap_or_else(|| panic!("{}", hint.id));
        let edits: Vec<_> = diagnostic.fixes.last().unwrap().edits.iter().collect();
        let after = apply_edits(hint.bad, &edits).unwrap();
        if let Err(set) = analyze_modules_all(&[("Main", after.as_str())]) {
            assert!(
                !set.diagnostics
                    .iter()
                    .any(|other| other.kind == diagnostic.kind
                        && other.span.start == diagnostic.span.start),
                "{}: {after}\n{:#?}",
                hint.id,
                set.diagnostics
            );
        }
        fixed += 1;
    }
    assert_eq!(fixed, 7);
}

#[test]
fn table_is_public_for_the_guide() {
    let hint: &Hint = &TABLE[0];
    let fields = (
        hint.id, hint.title, hint.kind, hint.bad, hint.good, hint.help,
    );
    assert_eq!(fields.0, "tuple-dot-index");
}

/// A fresh project directory with `Main.tz`.
fn project(source: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-hints-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    root
}

#[test]
fn help_reaches_the_json_and_the_library_result() {
    let hint = hint("tuple-dot-index");
    let root = project(hint.bad);
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["check", ".", "--json"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&format!(
            ",\"kind\":\"expected_identifier\",\"help\":\"{}\"",
            hint.help
        )),
        "{stderr}"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["check", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&format!("\n  help: {}", hint.help)),
        "{stderr}"
    );
    std::fs::remove_dir_all(root).unwrap();
}
