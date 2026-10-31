//! `tsuzuri explain`: the explanation pages of the diagnostic kinds and codes, embedded in the
//! compiler, and their text and JSON renderings.

use std::fmt::Write;

use crate::diagnostic::json_string;
use crate::diagnostic_kinds::DiagnosticKind;

/// The pages of single kinds, by id.
pub const KIND_PAGES: &[(&str, &str)] = &[
    ("ambiguous_name", include_str!("explain/ambiguous_name.md")),
    (
        "ambiguous_polymorphic_use",
        include_str!("explain/ambiguous_polymorphic_use.md"),
    ),
    (
        "and_without_recursive_group",
        include_str!("explain/and_without_recursive_group.md"),
    ),
    (
        "assign_to_immutable_place",
        include_str!("explain/assign_to_immutable_place.md"),
    ),
    (
        "borrow_conflict",
        include_str!("explain/borrow_conflict.md"),
    ),
    (
        "borrow_of_temporary",
        include_str!("explain/borrow_of_temporary.md"),
    ),
    (
        "borrow_outlives_block",
        include_str!("explain/borrow_outlives_block.md"),
    ),
    (
        "call_non_function",
        include_str!("explain/call_non_function.md"),
    ),
    (
        "case_pattern_missing_payload",
        include_str!("explain/case_pattern_missing_payload.md"),
    ),
    (
        "char_literal_not_one_unit",
        include_str!("explain/char_literal_not_one_unit.md"),
    ),
    (
        "class_has_no_method",
        include_str!("explain/class_has_no_method.md"),
    ),
    (
        "concrete_type_required",
        include_str!("explain/concrete_type_required.md"),
    ),
    (
        "definition_parameter_count_mismatch",
        include_str!("explain/definition_parameter_count_mismatch.md"),
    ),
    (
        "definition_without_signature",
        include_str!("explain/definition_without_signature.md"),
    ),
    ("deprecated_use", include_str!("explain/deprecated_use.md")),
    (
        "deref_non_reference",
        include_str!("explain/deref_non_reference.md"),
    ),
    (
        "duplicate_function_definition",
        include_str!("explain/duplicate_function_definition.md"),
    ),
    (
        "duplicate_module",
        include_str!("explain/duplicate_module.md"),
    ),
    ("duplicate_name", include_str!("explain/duplicate_name.md")),
    (
        "duplicate_option",
        include_str!("explain/duplicate_option.md"),
    ),
    (
        "empty_list_needs_annotation",
        include_str!("explain/empty_list_needs_annotation.md"),
    ),
    (
        "entry_point_missing",
        include_str!("explain/entry_point_missing.md"),
    ),
    (
        "expected_binding_end",
        include_str!("explain/expected_binding_end.md"),
    ),
    (
        "expected_expression",
        include_str!("explain/expected_expression.md"),
    ),
    (
        "expected_identifier",
        include_str!("explain/expected_identifier.md"),
    ),
    (
        "expected_line_break",
        include_str!("explain/expected_line_break.md"),
    ),
    (
        "expected_pattern",
        include_str!("explain/expected_pattern.md"),
    ),
    ("expected_token", include_str!("explain/expected_token.md")),
    (
        "field_access_non_record",
        include_str!("explain/field_access_non_record.md"),
    ),
    (
        "implicit_copy_of_large_value",
        include_str!("explain/implicit_copy_of_large_value.md"),
    ),
    (
        "index_non_indexable",
        include_str!("explain/index_non_indexable.md"),
    ),
    (
        "infinite_inferred_type",
        include_str!("explain/infinite_inferred_type.md"),
    ),
    (
        "input_extension_invalid",
        include_str!("explain/input_extension_invalid.md"),
    ),
    (
        "instance_missing_method",
        include_str!("explain/instance_missing_method.md"),
    ),
    (
        "integer_literal_out_of_range",
        include_str!("explain/integer_literal_out_of_range.md"),
    ),
    (
        "invalid_module_name",
        include_str!("explain/invalid_module_name.md"),
    ),
    (
        "invalid_numeric_literal",
        include_str!("explain/invalid_numeric_literal.md"),
    ),
    (
        "invalid_string_escape",
        include_str!("explain/invalid_string_escape.md"),
    ),
    (
        "let_function_needs_lambda",
        include_str!("explain/let_function_needs_lambda.md"),
    ),
    (
        "match_without_arms",
        include_str!("explain/match_without_arms.md"),
    ),
    (
        "misplaced_doc_comment",
        include_str!("explain/misplaced_doc_comment.md"),
    ),
    ("missing_input", include_str!("explain/missing_input.md")),
    (
        "missing_rec_marker",
        include_str!("explain/missing_rec_marker.md"),
    ),
    (
        "missing_record_fields",
        include_str!("explain/missing_record_fields.md"),
    ),
    (
        "move_out_of_collection_element",
        include_str!("explain/move_out_of_collection_element.md"),
    ),
    (
        "mut_as_expression",
        include_str!("explain/mut_as_expression.md"),
    ),
    (
        "mutate_immutable_value",
        include_str!("explain/mutate_immutable_value.md"),
    ),
    (
        "mutate_through_shared_reference",
        include_str!("explain/mutate_through_shared_reference.md"),
    ),
    ("no_instance", include_str!("explain/no_instance.md")),
    (
        "non_exhaustive_match",
        include_str!("explain/non_exhaustive_match.md"),
    ),
    (
        "option_conflict",
        include_str!("explain/option_conflict.md"),
    ),
    (
        "option_requires_build",
        include_str!("explain/option_requires_build.md"),
    ),
    (
        "or_pattern_binding_mismatch",
        include_str!("explain/or_pattern_binding_mismatch.md"),
    ),
    (
        "overlapping_instance",
        include_str!("explain/overlapping_instance.md"),
    ),
    (
        "private_name_access",
        include_str!("explain/private_name_access.md"),
    ),
    (
        "private_type_in_public_api",
        include_str!("explain/private_type_in_public_api.md"),
    ),
    (
        "record_has_no_field",
        include_str!("explain/record_has_no_field.md"),
    ),
    (
        "reference_without_owner",
        include_str!("explain/reference_without_owner.md"),
    ),
    (
        "reserved_std_namespace",
        include_str!("explain/reserved_std_namespace.md"),
    ),
    ("shadowed_local", include_str!("explain/shadowed_local.md")),
    (
        "signature_uses_def",
        include_str!("explain/signature_uses_def.md"),
    ),
    (
        "signature_uses_double_colon",
        include_str!("explain/signature_uses_double_colon.md"),
    ),
    (
        "signature_without_definition",
        include_str!("explain/signature_without_definition.md"),
    ),
    (
        "slice_must_be_borrowed",
        include_str!("explain/slice_must_be_borrowed.md"),
    ),
    (
        "source_too_large",
        include_str!("explain/source_too_large.md"),
    ),
    (
        "syntax_nesting_too_deep",
        include_str!("explain/syntax_nesting_too_deep.md"),
    ),
    (
        "task_capture_retains_borrow",
        include_str!("explain/task_capture_retains_borrow.md"),
    ),
    (
        "too_many_arguments_applied",
        include_str!("explain/too_many_arguments_applied.md"),
    ),
    (
        "type_argument_count",
        include_str!("explain/type_argument_count.md"),
    ),
    ("type_mismatch", include_str!("explain/type_mismatch.md")),
    (
        "unexpected_character",
        include_str!("explain/unexpected_character.md"),
    ),
    ("unknown_field", include_str!("explain/unknown_field.md")),
    (
        "unknown_function",
        include_str!("explain/unknown_function.md"),
    ),
    (
        "unknown_module_member",
        include_str!("explain/unknown_module_member.md"),
    ),
    ("unknown_option", include_str!("explain/unknown_option.md")),
    (
        "unknown_type_name",
        include_str!("explain/unknown_type_name.md"),
    ),
    (
        "unknown_union_case",
        include_str!("explain/unknown_union_case.md"),
    ),
    ("unknown_value", include_str!("explain/unknown_value.md")),
    (
        "unreachable_match_arm",
        include_str!("explain/unreachable_match_arm.md"),
    ),
    (
        "unterminated_string_literal",
        include_str!("explain/unterminated_string_literal.md"),
    ),
    ("unused_local", include_str!("explain/unused_local.md")),
    (
        "unused_private_function",
        include_str!("explain/unused_private_function.md"),
    ),
    (
        "update_drop_type_value",
        include_str!("explain/update_drop_type_value.md"),
    ),
    ("use_after_move", include_str!("explain/use_after_move.md")),
];

/// The pages of codes, by code.
pub const CODE_PAGES: &[(&str, &str)] = &[
    ("E0001", include_str!("explain/codes/E0001.md")),
    ("E0002", include_str!("explain/codes/E0002.md")),
    ("E0003", include_str!("explain/codes/E0003.md")),
    ("E0004", include_str!("explain/codes/E0004.md")),
    ("E1001", include_str!("explain/codes/E1001.md")),
    ("E1002", include_str!("explain/codes/E1002.md")),
    ("E1003", include_str!("explain/codes/E1003.md")),
    ("E1004", include_str!("explain/codes/E1004.md")),
    ("E1005", include_str!("explain/codes/E1005.md")),
    ("E1006", include_str!("explain/codes/E1006.md")),
    ("E1007", include_str!("explain/codes/E1007.md")),
    ("E1008", include_str!("explain/codes/E1008.md")),
    ("E1009", include_str!("explain/codes/E1009.md")),
    ("E1010", include_str!("explain/codes/E1010.md")),
    ("E1011", include_str!("explain/codes/E1011.md")),
    ("E1012", include_str!("explain/codes/E1012.md")),
    ("E1013", include_str!("explain/codes/E1013.md")),
    ("E1014", include_str!("explain/codes/E1014.md")),
    ("E1015", include_str!("explain/codes/E1015.md")),
    ("E1016", include_str!("explain/codes/E1016.md")),
    ("E1017", include_str!("explain/codes/E1017.md")),
    ("E1018", include_str!("explain/codes/E1018.md")),
    ("E1019", include_str!("explain/codes/E1019.md")),
    ("E1020", include_str!("explain/codes/E1020.md")),
    ("E1021", include_str!("explain/codes/E1021.md")),
    ("E1022", include_str!("explain/codes/E1022.md")),
    ("E1023", include_str!("explain/codes/E1023.md")),
    ("E1024", include_str!("explain/codes/E1024.md")),
    ("E1025", include_str!("explain/codes/E1025.md")),
    ("E1026", include_str!("explain/codes/E1026.md")),
    ("E1027", include_str!("explain/codes/E1027.md")),
    ("E1028", include_str!("explain/codes/E1028.md")),
    ("E1029", include_str!("explain/codes/E1029.md")),
    ("E1030", include_str!("explain/codes/E1030.md")),
    ("E1031", include_str!("explain/codes/E1031.md")),
    ("E1032", include_str!("explain/codes/E1032.md")),
    ("E1033", include_str!("explain/codes/E1033.md")),
    ("E1034", include_str!("explain/codes/E1034.md")),
    ("E1036", include_str!("explain/codes/E1036.md")),
    ("E1037", include_str!("explain/codes/E1037.md")),
    ("E1039", include_str!("explain/codes/E1039.md")),
    ("E2000", include_str!("explain/codes/E2000.md")),
    ("E2001", include_str!("explain/codes/E2001.md")),
    ("E2002", include_str!("explain/codes/E2002.md")),
    ("E2003", include_str!("explain/codes/E2003.md")),
    ("E2004", include_str!("explain/codes/E2004.md")),
    ("E2005", include_str!("explain/codes/E2005.md")),
    ("E2006", include_str!("explain/codes/E2006.md")),
    ("E2007", include_str!("explain/codes/E2007.md")),
    ("E2008", include_str!("explain/codes/E2008.md")),
    ("E2009", include_str!("explain/codes/E2009.md")),
    ("E2010", include_str!("explain/codes/E2010.md")),
    ("E2011", include_str!("explain/codes/E2011.md")),
    ("W1001", include_str!("explain/codes/W1001.md")),
    ("W1002", include_str!("explain/codes/W1002.md")),
    ("W1003", include_str!("explain/codes/W1003.md")),
    ("W1004", include_str!("explain/codes/W1004.md")),
    ("W1005", include_str!("explain/codes/W1005.md")),
    ("W1006", include_str!("explain/codes/W1006.md")),
    ("W1007", include_str!("explain/codes/W1007.md")),
    ("W1008", include_str!("explain/codes/W1008.md")),
    ("W1009", include_str!("explain/codes/W1009.md")),
    ("W1010", include_str!("explain/codes/W1010.md")),
    ("W2001", include_str!("explain/codes/W2001.md")),
    ("W2002", include_str!("explain/codes/W2002.md")),
];

/// The most candidates that an unknown target lists.
pub const MAX_SUGGESTIONS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Kind(DiagnosticKind),
    Code(&'static str),
}

fn lookup(pages: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    pages
        .binary_search_by(|(name, _)| (*name).cmp(key))
        .ok()
        .map(|index| pages[index].1)
}

pub fn kind_page(kind: DiagnosticKind) -> Option<&'static str> {
    lookup(KIND_PAGES, kind.id())
}

pub fn code_page(code: &str) -> Option<&'static str> {
    lookup(CODE_PAGES, code)
}

/// The `# ` line of a page.
pub fn page_title(page: &str) -> Option<&str> {
    page.lines().next()?.strip_prefix("# ")
}

/// A page without its `# ` line and the blank line after it.
pub fn page_body(page: &str) -> &str {
    let Some(rest) = page
        .strip_prefix("# ")
        .and_then(|page| page.split_once('\n'))
        .map(|(_, rest)| rest)
    else {
        return page;
    };
    rest.strip_prefix('\n').unwrap_or(rest)
}

/// `option_requires_build` → `Option requires build`.
pub fn humanize(id: &str) -> String {
    let text = id.replace('_', " ");
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + characters.as_str()
    })
}

/// The title of the kind's page, or its humanized id.
pub fn title(kind: DiagnosticKind) -> String {
    kind_page(kind)
        .and_then(page_title)
        .map_or_else(|| humanize(kind.id()), str::to_string)
}

/// The title of the code's page, or `Diagnostics with code E1012`.
pub fn code_title(code: &str) -> String {
    code_page(code)
        .and_then(page_title)
        .map_or_else(|| format!("Diagnostics with code {code}"), str::to_string)
}

fn is_code(target: &str) -> bool {
    let bytes = target.as_bytes();
    bytes.len() == 5 && matches!(bytes[0], b'E' | b'W') && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// A kind id or an upper-case code; `Err` holds the candidates for anything else.
pub fn resolve(target: &str) -> Result<Target, Vec<String>> {
    if is_code(target)
        && let Some(code) = DiagnosticKind::codes()
            .into_iter()
            .find(|code| *code == target)
    {
        return Ok(Target::Code(code));
    }
    DiagnosticKind::from_id(target)
        .map(Target::Kind)
        .ok_or_else(|| suggest(target))
}

fn levenshtein(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (i, l) in left.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, r) in right.iter().enumerate() {
            let substitution = previous[j] + usize::from(l != *r);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[right.len()]
}

/// The ids and codes close to `target`: within 2 edits (1 for a target of 4 characters or
/// fewer), ids that start with a target of 3 characters or more, and the upper-case form of a
/// lower-case code; by (distance, text), at most `MAX_SUGGESTIONS`.
pub fn suggest(target: &str) -> Vec<String> {
    let limit = if target.chars().count() >= 5 { 2 } else { 1 };
    let codes = DiagnosticKind::codes();
    let mut candidates: Vec<(usize, &str)> = Vec::new();
    for text in DiagnosticKind::ALL
        .iter()
        .map(|kind| kind.id())
        .chain(codes.iter().copied())
    {
        let distance = levenshtein(target, text);
        if distance <= limit || (target.len() >= 3 && text.starts_with(target)) {
            candidates.push((distance, text));
        }
    }
    let upper = target.to_ascii_uppercase();
    if upper != target
        && let Some(code) = codes.iter().find(|code| **code == upper)
    {
        candidates.push((0, code));
    }
    candidates.sort_unstable();
    let mut seen = std::collections::BTreeSet::new();
    candidates
        .into_iter()
        .filter(|(_, text)| seen.insert(*text))
        .take(MAX_SUGGESTIONS)
        .map(|(_, text)| text.to_string())
        .collect()
}

/// The message of `explain` for a target that is neither a kind nor a code.
pub fn unknown_target_message(target: &str, candidates: &[String]) -> String {
    let mut message = format!("unknown diagnostic '{target}'; ");
    if !candidates.is_empty() {
        let names: Vec<String> = candidates.iter().map(|name| format!("'{name}'")).collect();
        let _ = write!(message, "did you mean {}? ", names.join(", "));
    }
    message.push_str("run 'tsuzuri explain --catalog' to list every kind");
    message
}

fn severity(code: &str) -> &'static str {
    if code.starts_with('W') {
        "warning"
    } else {
        "error"
    }
}

fn json_option(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_string(), json_string)
}

/// The explanation of one kind: a header, the title, and the kind's page, else the code's page.
pub fn render_kind(kind: DiagnosticKind, json: bool) -> String {
    let page = kind_page(kind).map(page_body);
    let code_page = code_page(kind.code()).map(page_body);
    if json {
        let siblings: Vec<String> = DiagnosticKind::kinds_with_code(kind.code())
            .into_iter()
            .filter(|sibling| *sibling != kind)
            .map(|sibling| json_string(sibling.id()))
            .collect();
        return format!(
            "{{\"kind\":{},\"code\":{},\"severity\":{},\"since\":{},\"title\":{},\"page\":{},\"code_page\":{},\"siblings\":[{}]}}\n",
            json_string(kind.id()),
            json_string(kind.code()),
            json_string(kind.severity().as_str()),
            json_string(kind.since()),
            json_string(&title(kind)),
            json_option(page),
            json_option(code_page),
            siblings.join(","),
        );
    }
    let mut text = format!(
        "{}  {}  {}  since {}\n{}\n\n",
        kind.id(),
        kind.code(),
        kind.severity().as_str(),
        kind.since(),
        title(kind)
    );
    match (page, code_page) {
        (Some(page), _) => text.push_str(page),
        (None, Some(code_page)) => {
            let _ = write!(
                text,
                "(no dedicated page for this kind; the page of {} follows)\n\n{code_page}",
                kind.code()
            );
        }
        (None, None) => text
            .push_str("(no explanation yet; the message of the diagnostic says what to change)\n"),
    }
    text
}

/// The explanation of one code: a header, the title, its kinds, and the code's page.
pub fn render_code(code: &str, json: bool) -> String {
    let kinds = DiagnosticKind::kinds_with_code(code);
    let page = code_page(code).map(page_body);
    if json {
        let kinds: Vec<String> = kinds
            .iter()
            .map(|kind| {
                format!(
                    "{{\"kind\":{},\"title\":{},\"page\":{}}}",
                    json_string(kind.id()),
                    json_string(&title(*kind)),
                    kind_page(*kind).is_some()
                )
            })
            .collect();
        return format!(
            "{{\"code\":{},\"severity\":{},\"title\":{},\"page\":{},\"kinds\":[{}]}}\n",
            json_string(code),
            json_string(severity(code)),
            json_string(&code_title(code)),
            json_option(page),
            kinds.join(","),
        );
    }
    let mut text = format!(
        "{code}  {}\n{}\n\nkinds:\n",
        severity(code),
        code_title(code)
    );
    for kind in &kinds {
        let _ = writeln!(text, "  {}  {}", kind.id(), title(*kind));
    }
    if let Some(page) = page {
        let _ = write!(text, "\n{page}");
    }
    text
}

/// Every kind, one line each, by code and then id.
pub fn render_catalog(json: bool) -> String {
    let mut kinds = DiagnosticKind::ALL.to_vec();
    kinds.sort_by_key(|kind| (kind.code(), kind.id()));
    let mut text = String::new();
    for kind in kinds {
        if json {
            let _ = writeln!(
                text,
                "{{\"kind\":{},\"code\":{},\"severity\":{},\"since\":{},\"title\":{},\"page\":{}}}",
                json_string(kind.id()),
                json_string(kind.code()),
                json_string(kind.severity().as_str()),
                json_string(kind.since()),
                json_string(&title(kind)),
                kind_page(kind).is_some()
            );
        } else {
            let _ = writeln!(text, "{}  {}  {}", kind.code(), kind.id(), title(kind));
        }
    }
    text
}

/// The `## ` sections of a page as (heading, content).
pub fn sections(page: &str) -> Vec<(&str, &str)> {
    let mut starts: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    let mut fenced = false;
    for line in page.split_inclusive('\n') {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if !fenced && let Some(heading) = line.strip_prefix("## ") {
            starts.push((offset, heading.trim_end()));
        }
        offset += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, (start, heading))| {
            let content_start = start + page[*start..].find('\n').map_or(0, |end| end + 1);
            let end = starts.get(index + 1).map_or(page.len(), |next| next.0);
            (*heading, &page[content_start.min(end)..end])
        })
        .collect()
}

/// The fenced blocks of a section as (info string, text).
pub fn examples(section: &str) -> Vec<(&str, String)> {
    let mut examples = Vec::new();
    let mut open: Option<(&str, String)> = None;
    for line in section.split_inclusive('\n') {
        match (&mut open, line.strip_prefix("```")) {
            (None, Some(info)) => open = Some((info.trim(), String::new())),
            (Some(_), Some(_)) => examples.extend(open.take()),
            (Some((_, text)), None) => text.push_str(line),
            (None, None) => {}
        }
    }
    examples
}
