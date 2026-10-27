use std::path::Path;
use std::process::Command;

use tsuzuri::diagnostic::{Diagnostic, Severity, Span};
use tsuzuri::diagnostic_kinds::DiagnosticKind;

fn upper_camel(id: &str) -> String {
    id.split('_')
        .map(|word| {
            let mut characters = word.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect()
}

#[test]
fn registry_is_consistent() {
    let mut ids = std::collections::BTreeSet::new();
    for kind in DiagnosticKind::ALL {
        let id = kind.id();
        assert!(
            id.len() <= 48
                && id.starts_with(|c: char| c.is_ascii_lowercase())
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{id}"
        );
        assert!(ids.insert(id), "duplicate kind {id}");
        assert_eq!(DiagnosticKind::from_id(id), Some(*kind));
        let code = kind.code().as_bytes();
        assert!(
            code.len() == 5
                && matches!(code[0], b'E' | b'W')
                && code[1..].iter().all(u8::is_ascii_digit),
            "{id}: {}",
            kind.code()
        );
        let expected = if code[0] == b'W' {
            Severity::Warning
        } else {
            Severity::Error
        };
        assert_eq!(kind.severity(), expected, "{id}");
        let since: Vec<&str> = kind.since().split('.').collect();
        assert!(
            since.len() == 3
                && since
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())),
            "{id}: {}",
            kind.since()
        );
        assert_eq!(format!("{kind:?}"), upper_camel(id));
    }
    assert_eq!(DiagnosticKind::from_id("no_such_kind"), None);
    // Each run of kinds with the same `since` is in (code, id) order.
    for pair in DiagnosticKind::ALL.windows(2) {
        if pair[0].since() == pair[1].since() {
            assert!(
                (pair[0].code(), pair[0].id()) < (pair[1].code(), pair[1].id()),
                "{} before {}",
                pair[0].id(),
                pair[1].id()
            );
        }
    }
}

#[test]
fn kinds_with_code_is_sorted() {
    let codes = DiagnosticKind::codes();
    assert!(codes.windows(2).all(|pair| pair[0] < pair[1]), "{codes:?}");
    let mut total = 0;
    for code in &codes {
        let kinds = DiagnosticKind::kinds_with_code(code);
        assert!(!kinds.is_empty(), "{code}");
        assert!(kinds.iter().all(|kind| kind.code() == *code));
        assert!(kinds.windows(2).all(|pair| pair[0].id() < pair[1].id()));
        total += kinds.len();
    }
    assert_eq!(total, DiagnosticKind::ALL.len());
    assert!(DiagnosticKind::kinds_with_code("E9999").is_empty());
    assert!(
        DiagnosticKind::kinds_with_code("E1012")
            .iter()
            .any(|kind| kind.id() == "use_after_move")
    );
}

#[test]
fn of_sets_code_severity_and_kind() {
    let diagnostic = Diagnostic::of(DiagnosticKind::UseAfterMove, "m", Span::new(1, 2));
    assert_eq!(diagnostic.code, "E1012");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.kind, Some("use_after_move"));
    assert_eq!(diagnostic.message, "m");
    assert_eq!(diagnostic.span, Span::new(1, 2));
    assert_eq!(diagnostic.kind_of(), Some(DiagnosticKind::UseAfterMove));
    assert!(diagnostic.help.is_none() && diagnostic.related.is_empty());
}

#[test]
fn recode_replaces_code_and_kind_only() {
    let original = Diagnostic::of(DiagnosticKind::UnknownValue, "m", Span::new(3, 4))
        .with_help("h")
        .expected_found("a", "b");
    let recoded = original.clone().recode(DiagnosticKind::TypeMismatch);
    assert_eq!(recoded.code, "E1003");
    assert_eq!(recoded.kind, Some("type_mismatch"));
    assert_eq!(recoded.severity, Severity::Error);
    assert_eq!(recoded.message, original.message);
    assert_eq!(recoded.span, original.span);
    assert_eq!(recoded.help, original.help);
    assert_eq!(recoded.detail, original.detail);
}

/// The diagnostics of `check . --json` in `directory`, one JSON line each.
fn check_json(directory: &Path) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["check", ".", "--json"])
        .current_dir(directory)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", directory.display());
    String::from_utf8(output.stderr)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

fn baseline_root() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/diagnostic_baseline"
    ))
}

/// (case, diagnostics) of `baseline.jsonl`, written by the compiler before G23.
fn baseline() -> Vec<(String, Vec<serde_json::Value>)> {
    std::fs::read_to_string(baseline_root().join("baseline.jsonl"))
        .unwrap()
        .lines()
        .map(|line| {
            let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
            let case = value["case"].as_str().unwrap().to_string();
            let diagnostics = value["diagnostics"].as_array_mut().unwrap().clone();
            (case, diagnostics)
        })
        .collect()
}

/// The top-level keys of a JSON object line, in order.
fn top_level_keys(line: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    let mut current = String::new();
    let mut last_string = String::new();
    for character in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
                current.push(character);
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
                last_string = std::mem::take(&mut current);
            } else {
                current.push(character);
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ':' if depth == 1 => keys.push(std::mem::take(&mut last_string)),
            _ => {}
        }
    }
    keys
}

/// The keys of a diagnostic line, in the order of D-51.
const KEY_ORDER: &[&str] = &[
    "severity",
    "code",
    "message",
    "path",
    "span",
    "kind",
    "related",
    "help",
    "expected",
    "found",
    "candidates",
    "scope",
    "fixes",
];

/// Checks one current line against its baseline: the same values except `kind` (added), `help`
/// (kept when the baseline has one, else possibly added by a hint), and `fixes` (the baseline's
/// fixes first); the keys in the order of D-51.
fn assert_same_but_added(case: &str, before: &serde_json::Value, line: &str) {
    let keys = top_level_keys(line);
    let positions: Vec<usize> = keys
        .iter()
        .map(|key| {
            KEY_ORDER
                .iter()
                .position(|known| known == key)
                .unwrap_or_else(|| panic!("{case}: unknown key {key} in {line}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{case}: keys out of order in {line}"
    );
    let mut after: serde_json::Value = serde_json::from_str(line).unwrap();
    let mut before = before.clone();
    let after = after.as_object_mut().unwrap();
    let before = before.as_object_mut().unwrap();
    after.remove("kind");
    let added_help = after.remove("help");
    if let Some(help) = before.remove("help") {
        assert!(
            added_help.is_some(),
            "{case}: the help {help} disappeared in {line}"
        );
    }
    let before_fixes = before.remove("fixes").unwrap_or_default();
    let after_fixes = after.remove("fixes").unwrap_or_default();
    let before_fixes = before_fixes.as_array().cloned().unwrap_or_default();
    let after_fixes = after_fixes.as_array().cloned().unwrap_or_default();
    assert!(
        after_fixes.starts_with(&before_fixes),
        "{case}: fixes changed in {line}"
    );
    assert_eq!(after, before, "{case}: {line}");
}

#[test]
fn baseline_diagnostics_are_unchanged() {
    let cases = baseline();
    assert_eq!(cases.len(), 48);
    for (case, before) in &cases {
        let lines = check_json(&baseline_root().join(case));
        assert_eq!(lines.len(), before.len(), "{case}: {lines:#?}");
        for (line, before) in lines.iter().zip(before) {
            assert_same_but_added(case, before, line);
        }
    }
}

/// `line` with `insertion` after its `span` object.
fn insert_after_span(line: &str, insertion: &str) -> String {
    let start = line.find("\"span\":{").unwrap();
    let end = start + line[start..].find('}').unwrap() + 1;
    format!("{}{insertion}{}", &line[..end], &line[end..])
}

#[test]
fn json_adds_only_the_kind_key() {
    let raw = std::fs::read_to_string(baseline_root().join("baseline.jsonl")).unwrap();
    let line = raw
        .lines()
        .find(|line| line.starts_with("{\"case\":\"use_after_move\""))
        .unwrap();
    let before = &line[line.find("[{").unwrap() + 1..line.len() - 2];
    let current = check_json(&baseline_root().join("use_after_move"));
    assert_eq!(
        current,
        [insert_after_span(before, ",\"kind\":\"use_after_move\"")]
    );
}

/// The `DiagnosticKind::Variant` names that the sources of `directory` use.
fn emitted_variants(directory: &Path, found: &mut std::collections::BTreeSet<String>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            emitted_variants(&path, found);
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        // The registry, the pages and the hints name kinds without emitting them.
        if !name.ends_with(".rs")
            || ["diagnostic_kinds.rs", "explain.rs", "hints.rs"].contains(&name.as_ref())
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (index, _) in text.match_indices("DiagnosticKind::") {
            let rest = &text[index + "DiagnosticKind::".len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(rest.len());
            if rest[..end].starts_with(|c: char| c.is_ascii_uppercase()) {
                found.insert(rest[..end].to_string());
            }
        }
    }
}

#[test]
fn every_kind_has_an_emission_site() {
    let mut found = std::collections::BTreeSet::new();
    emitted_variants(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut found,
    );
    let unemitted: Vec<&str> = DiagnosticKind::ALL
        .iter()
        .filter(|kind| !found.contains(&format!("{kind:?}")))
        .map(|kind| kind.id())
        .collect();
    assert!(
        unemitted.is_empty(),
        "kinds that no source emits: {unemitted:?}"
    );
}
