use std::process::{Command, Output};

use tsuzuri::analyze_modules_all;
use tsuzuri::diagnostic_kinds::DiagnosticKind;
use tsuzuri::explain::{self, CODE_PAGES, KIND_PAGES, Target};

const KIND_PAGE_LIMIT: usize = 1200;

/// The kinds that a source reports: the errors of a failed analysis, else the warnings.
fn reported_kinds(source: &str) -> (bool, Vec<&'static str>) {
    match analyze_modules_all(&[("Main", source)]) {
        Ok(module) => (
            true,
            module
                .warnings
                .iter()
                .filter_map(|warning| warning.kind)
                .collect(),
        ),
        Err(set) => (
            false,
            set.diagnostics
                .iter()
                .filter_map(|diagnostic| diagnostic.kind)
                .collect(),
        ),
    }
}

fn assert_sorted_unique(pages: &[(&str, &str)]) {
    assert!(
        pages.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "{:?}",
        pages.iter().map(|page| page.0).collect::<Vec<_>>()
    );
}

/// The kinds that a `## See also` section lists.
fn see_also(section: &str) -> Vec<&str> {
    section
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.strip_prefix("- `")
                .and_then(|line| line.strip_suffix('`'))
                .unwrap_or_else(|| panic!("a See also line is \"- `kind`\": {line}"))
        })
        .collect()
}

#[test]
fn pages_have_the_required_shape() {
    assert_sorted_unique(KIND_PAGES);
    for (id, page) in KIND_PAGES {
        assert!(DiagnosticKind::from_id(id).is_some(), "{id} is not a kind");
        assert!(
            page.chars().count() <= KIND_PAGE_LIMIT,
            "{id}: {} characters",
            page.chars().count()
        );
        let title = explain::page_title(page).unwrap_or_else(|| panic!("{id}: no title"));
        assert!(
            title.len() <= 60
                && title.starts_with(|c: char| c.is_ascii_uppercase())
                && !title.ends_with('.'),
            "{id}: {title}"
        );
        let headings: Vec<&str> = explain::sections(page)
            .iter()
            .map(|(heading, _)| *heading)
            .collect();
        assert!(
            headings == ["Why", "Bad", "Good"] || headings == ["Why", "Bad", "Good", "See also"],
            "{id}: {headings:?}"
        );
        for (heading, section) in explain::sections(page) {
            if heading == "Bad" || heading == "Good" {
                assert_eq!(explain::examples(section).len(), 1, "{id}: {heading}");
            }
            if heading == "See also" {
                for kind in see_also(section) {
                    assert!(
                        DiagnosticKind::from_id(kind).is_some(),
                        "{id}: {kind} is not a kind"
                    );
                }
            }
        }
    }
}

#[test]
fn every_page_example_behaves() {
    for (id, page) in KIND_PAGES {
        for (heading, section) in explain::sections(page) {
            for (info, example) in explain::examples(section) {
                if info != "tsuzuri" {
                    assert_eq!(info, "text", "{id}: {heading}");
                    continue;
                }
                let (accepted, kinds) = reported_kinds(&example);
                match heading {
                    "Bad" => assert!(
                        kinds.contains(id),
                        "{id}: the Bad example reports {kinds:?}\n{example}"
                    ),
                    "Good" => assert!(
                        accepted,
                        "{id}: the Good example reports {kinds:?}\n{example}"
                    ),
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn resolve_accepts_kinds_and_codes() {
    assert_eq!(
        explain::resolve("use_after_move"),
        Ok(Target::Kind(DiagnosticKind::UseAfterMove))
    );
    assert_eq!(explain::resolve("E1012"), Ok(Target::Code("E1012")));
    assert!(explain::resolve("e1012").is_err());
    assert!(explain::resolve("E9999").is_err());
    assert!(explain::resolve("").is_err());
}

#[test]
fn resolve_suggests_close_names() {
    assert_eq!(
        explain::resolve("e1012")
            .unwrap_err()
            .first()
            .map(String::as_str),
        Some("E1012")
    );
    assert_eq!(
        explain::resolve("use_after_mov").unwrap_err(),
        ["use_after_move"]
    );
    assert!(explain::resolve("zzzz").unwrap_err().is_empty());
    let suggestions = explain::suggest("unknown");
    assert!(suggestions.len() <= explain::MAX_SUGGESTIONS);
    assert!(suggestions.contains(&"unknown_value".to_string()));
    assert_eq!(
        explain::unknown_target_message("use_after_mov", &["use_after_move".into()]),
        "unknown diagnostic 'use_after_mov'; did you mean 'use_after_move'? run 'tsuzuri explain --catalog' to list every kind"
    );
    assert_eq!(
        explain::unknown_target_message("no_such_kind", &[]),
        "unknown diagnostic 'no_such_kind'; run 'tsuzuri explain --catalog' to list every kind"
    );
}

#[test]
fn humanize_titles() {
    assert_eq!(
        explain::humanize("option_requires_build"),
        "Option requires build"
    );
    assert_eq!(explain::humanize("x"), "X");
    assert_eq!(explain::humanize(""), "");
    assert_eq!(
        explain::title(DiagnosticKind::UseAfterMove),
        "Use of a moved value"
    );
    assert_eq!(explain::title(DiagnosticKind::UsageError), "Usage error");
}

#[test]
fn render_kind_text_and_json() {
    let text = explain::render_kind(DiagnosticKind::UseAfterMove, false);
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("use_after_move  E1012  error  since 0.1.0")
    );
    assert_eq!(lines.next(), Some("Use of a moved value"));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some("## Why"));
    let page = explain::kind_page(DiagnosticKind::UseAfterMove).unwrap();
    assert!(text.ends_with(explain::page_body(page)));
    let json: serde_json::Value =
        serde_json::from_str(&explain::render_kind(DiagnosticKind::UseAfterMove, true)).unwrap();
    let keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    let mut expected = [
        "kind",
        "code",
        "severity",
        "since",
        "title",
        "page",
        "code_page",
        "siblings",
    ];
    expected.sort_unstable();
    assert_eq!(keys, expected);
    assert_eq!(json["kind"], "use_after_move");
    assert_eq!(json["page"], explain::page_body(page));
    let json = explain::render_kind(DiagnosticKind::UseAfterMove, true);
    assert!(
        json.starts_with(
            "{\"kind\":\"use_after_move\",\"code\":\"E1012\",\"severity\":\"error\",\"since\":\"0.1.0\",\"title\":\"Use of a moved value\",\"page\":\""
        ),
        "{json}"
    );
    assert!(json.contains("\",\"code_page\":"), "{json}");
    let usage = explain::render_kind(DiagnosticKind::UsageError, true);
    assert!(usage.contains("\"page\":null"), "{usage}");
    assert!(usage.contains("\"siblings\":[\""), "{usage}");
}

#[test]
fn render_catalog_is_sorted() {
    let text = explain::render_catalog(false);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), DiagnosticKind::ALL.len());
    let keys: Vec<(&str, &str)> = lines
        .iter()
        .map(|line| {
            let mut parts = line.splitn(3, "  ");
            (parts.next().unwrap(), parts.next().unwrap())
        })
        .collect();
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(lines.contains(&"E1012  use_after_move  Use of a moved value"));
    let json = explain::render_catalog(true);
    assert_eq!(json.lines().count(), DiagnosticKind::ALL.len());
    assert!(json.lines().any(|line| line
        == "{\"kind\":\"use_after_move\",\"code\":\"E1012\",\"severity\":\"error\",\"since\":\"0.1.0\",\"title\":\"Use of a moved value\",\"page\":true}"));
    assert_eq!(text, explain::render_catalog(false));
}

fn tsuzuri(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn cli_explains_a_kind() {
    let output = tsuzuri(&["explain", "use_after_move"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("use_after_move  E1012  error  since 0.1.0\nUse of a moved value\n\n"),
        "{stdout}"
    );
    assert_eq!(
        stdout,
        explain::render_kind(DiagnosticKind::UseAfterMove, false)
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn cli_explains_a_code() {
    let output = tsuzuri(&["explain", "E1012"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("E1012  error\n"), "{stdout}");
    assert!(
        stdout.contains("\nkinds:\n  use_after_move  Use of a moved value\n"),
        "{stdout}"
    );
    if let Some(page) = explain::code_page("E1012") {
        assert!(stdout.ends_with(explain::page_body(page)), "{stdout}");
    }
}

#[test]
fn cli_json_has_the_documented_keys() {
    let output = tsuzuri(&["explain", "use_after_move", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    assert_eq!(
        stdout,
        explain::render_kind(DiagnosticKind::UseAfterMove, true)
    );
    let output = tsuzuri(&["explain", "--json", "E1012", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("{\"code\":\"E1012\",\"severity\":\"error\",\"title\":"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            ",\"kinds\":[{\"kind\":\"use_after_move\",\"title\":\"Use of a moved value\",\"page\":true}"
        ),
        "{stdout}"
    );
}

#[test]
fn cli_catalog_lists_every_kind() {
    for arguments in [
        &["explain", "--catalog"][..],
        &["explain", "--catalog", "--catalog"],
    ] {
        let output = tsuzuri(arguments);
        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.lines().count(), DiagnosticKind::ALL.len());
        assert_eq!(stdout, explain::render_catalog(false));
    }
    let output = tsuzuri(&["explain", "--catalog", "--json"]);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        explain::render_catalog(true)
    );
}

#[test]
fn cli_rejects_bad_usage() {
    let cases: &[(&[&str], &str, &str)] = &[
        (&["explain"], "usage_error", "usage: tsuzuri explain TARGET"),
        (
            &["explain", "a", "b"],
            "usage_error",
            "usage: tsuzuri explain TARGET",
        ),
        (
            &["explain", "--catalog", "use_after_move"],
            "option_conflict",
            "--catalog cannot be combined with a target",
        ),
        (
            &["explain", "--nope"],
            "unknown_option",
            "unknown option '--nope'; use --help",
        ),
        (
            &["explain", "no_such_kind"],
            "unknown_explain_target",
            "unknown diagnostic 'no_such_kind'; run 'tsuzuri explain --catalog' to list every kind",
        ),
        (
            &["explain", "e1012"],
            "unknown_explain_target",
            "unknown diagnostic 'e1012'; did you mean 'E1012'",
        ),
    ];
    for (arguments, kind, message) in cases {
        let mut json = arguments.to_vec();
        json.push("--json");
        let output = tsuzuri(&json);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        let line: serde_json::Value = serde_json::from_str(stderr.trim_end()).unwrap();
        assert_eq!(line["code"], "E2000", "{arguments:?}");
        assert_eq!(line["kind"], *kind, "{arguments:?}");
        assert_eq!(line["path"], "<command line>", "{arguments:?}");
        assert!(
            line["message"].as_str().unwrap().starts_with(message),
            "{arguments:?}: {stderr}"
        );
        let output = tsuzuri(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.starts_with(&format!("<command line>:1:1: error[E2000]: {message}")),
            "{arguments:?}: {stderr}"
        );
    }
}

#[test]
fn code_pages_are_sorted() {
    assert_sorted_unique(CODE_PAGES);
}
