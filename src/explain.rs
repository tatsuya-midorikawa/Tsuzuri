//! `tsuzuri explain`: the explanation pages of the diagnostic kinds and codes, embedded in the
//! compiler, and their text and JSON renderings.

use std::fmt::Write;

use crate::diagnostic::json_string;
use crate::diagnostic_kinds::DiagnosticKind;

/// The pages of single kinds, by id.
pub const KIND_PAGES: &[(&str, &str)] = &[
    (
        "borrow_conflict",
        include_str!("explain/borrow_conflict.md"),
    ),
    (
        "borrow_outlives_block",
        include_str!("explain/borrow_outlives_block.md"),
    ),
    (
        "mutate_immutable_value",
        include_str!("explain/mutate_immutable_value.md"),
    ),
    (
        "non_exhaustive_match",
        include_str!("explain/non_exhaustive_match.md"),
    ),
    ("type_mismatch", include_str!("explain/type_mismatch.md")),
    (
        "unknown_module_member",
        include_str!("explain/unknown_module_member.md"),
    ),
    ("unknown_value", include_str!("explain/unknown_value.md")),
    ("use_after_move", include_str!("explain/use_after_move.md")),
];

/// The pages of codes, by code.
pub const CODE_PAGES: &[(&str, &str)] = &[];

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
