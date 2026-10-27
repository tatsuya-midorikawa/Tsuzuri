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
    assert!(stdout.contains("\nkinds:\n"), "{stdout}");
    assert!(
        stdout.contains("\n  use_after_move  Use of a moved value\n"),
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
            "{\"kind\":\"use_after_move\",\"title\":\"Use of a moved value\",\"page\":true}"
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

const CODE_PAGE_LIMIT: usize = 1500;

#[test]
fn every_code_has_a_page() {
    let pages: Vec<&str> = CODE_PAGES.iter().map(|(code, _)| *code).collect();
    assert_eq!(pages, DiagnosticKind::codes());
    for (code, page) in CODE_PAGES {
        assert!(
            page.chars().count() <= CODE_PAGE_LIMIT,
            "{code}: {} characters",
            page.chars().count()
        );
        let title = explain::page_title(page).unwrap_or_else(|| panic!("{code}: no title"));
        assert!(
            title.len() <= 60
                && title.starts_with(|c: char| c.is_ascii_uppercase())
                && !title.ends_with('.'),
            "{code}: {title}"
        );
        let sections = explain::sections(page);
        let headings: Vec<&str> = sections.iter().map(|(heading, _)| *heading).collect();
        assert!(
            headings == ["What it covers", "Fixes"]
                || headings == ["What it covers", "Fixes", "See also"],
            "{code}: {headings:?}"
        );
        for (heading, section) in sections {
            assert!(explain::examples(section).is_empty(), "{code}: {heading}");
            if heading == "Fixes" {
                let fixes = section
                    .lines()
                    .filter(|line| line.starts_with("- "))
                    .count();
                assert!((2..=6).contains(&fixes), "{code}: {fixes} fixes");
            }
            if heading == "See also" {
                for kind in see_also(section) {
                    assert_eq!(
                        DiagnosticKind::from_id(kind).map(DiagnosticKind::code),
                        Some(*code),
                        "{code}: {kind}"
                    );
                }
            }
        }
    }
}

/// The kinds that have pages of their own (appendix E.5 of G23), the 8 of Phase 1 included.
const CURATED: &[&str] = &[
    "expected_token",
    "expected_identifier",
    "expected_expression",
    "expected_pattern",
    "expected_binding_end",
    "expected_line_break",
    "unexpected_character",
    "unterminated_string_literal",
    "invalid_string_escape",
    "char_literal_not_one_unit",
    "invalid_numeric_literal",
    "source_too_large",
    "syntax_nesting_too_deep",
    "match_without_arms",
    "definition_without_signature",
    "signature_without_definition",
    "let_function_needs_lambda",
    "misplaced_doc_comment",
    "signature_uses_def",
    "signature_uses_double_colon",
    "mut_as_expression",
    "slice_must_be_borrowed",
    "duplicate_name",
    "duplicate_function_definition",
    "unknown_value",
    "unknown_function",
    "unknown_module_member",
    "unknown_type_name",
    "unknown_field",
    "unknown_union_case",
    "ambiguous_name",
    "type_mismatch",
    "call_non_function",
    "too_many_arguments_applied",
    "definition_parameter_count_mismatch",
    "record_has_no_field",
    "missing_record_fields",
    "field_access_non_record",
    "integer_literal_out_of_range",
    "ambiguous_polymorphic_use",
    "concrete_type_required",
    "infinite_inferred_type",
    "no_instance",
    "instance_missing_method",
    "overlapping_instance",
    "class_has_no_method",
    "index_non_indexable",
    "deref_non_reference",
    "type_argument_count",
    "empty_list_needs_annotation",
    "use_after_move",
    "move_out_of_collection_element",
    "assign_to_immutable_place",
    "borrow_conflict",
    "mutate_immutable_value",
    "mutate_through_shared_reference",
    "borrow_outlives_block",
    "borrow_of_temporary",
    "task_capture_retains_borrow",
    "reference_without_owner",
    "update_drop_type_value",
    "non_exhaustive_match",
    "or_pattern_binding_mismatch",
    "case_pattern_missing_payload",
    "and_without_recursive_group",
    "missing_rec_marker",
    "private_name_access",
    "invalid_module_name",
    "duplicate_module",
    "reserved_std_namespace",
    "private_type_in_public_api",
    "unused_local",
    "unused_private_function",
    "unreachable_match_arm",
    "shadowed_local",
    "deprecated_use",
    "implicit_copy_of_large_value",
    "option_requires_build",
    "option_conflict",
    "unknown_option",
    "duplicate_option",
    "missing_input",
    "entry_point_missing",
    "input_extension_invalid",
];

#[test]
fn curated_kinds_have_pages() {
    assert_eq!(CURATED.len(), 84);
    for id in CURATED {
        let kind = DiagnosticKind::from_id(id).unwrap_or_else(|| panic!("{id} is not a kind"));
        assert!(explain::kind_page(kind).is_some(), "{id} has no page");
    }
}

/// The table of every kind by code that `compiler/diagnostics.md` holds between its markers.
fn kind_table() -> String {
    let mut table = String::from("| コード | kind |\n| --- | --- |\n");
    for code in DiagnosticKind::codes() {
        let kinds: Vec<String> = DiagnosticKind::kinds_with_code(code)
            .iter()
            .map(|kind| format!("`{}`", kind.id()))
            .collect();
        table.push_str(&format!("| `{code}` | {} |\n", kinds.join(", ")));
    }
    table
}

#[test]
fn the_docs_list_every_kind() {
    let page = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/_tsuzuri/language-reference/compiler/diagnostics.md"
    ))
    .unwrap();
    let (start, end) = (
        "<!-- generated:diagnostic-kinds -->\n",
        "<!-- /generated:diagnostic-kinds -->",
    );
    let begin = page.find(start).expect("the start marker") + start.len();
    let finish = page[begin..].find(end).expect("the end marker") + begin;
    let expected = kind_table();
    assert!(
        page[begin..finish] == expected,
        "replace the block between the markers of compiler/diagnostics.md with:\n{expected}"
    );
}
