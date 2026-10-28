//! The strict check of the Mermaid text that G30 writes (G30 "構造検証器", D9): only the forms of
//! the generated flowcharts, class diagrams and state diagrams pass, so a generated diagram that
//! Mermaid would read differently is caught without Mermaid. It is no general Mermaid parser; the
//! rules are those of `validate_ref.py` (G30 appendix E), written without regular expressions.

use std::collections::BTreeSet;

use super::{MAX_EDGES, MAX_TEXT};

const MAX_NODES: usize = 500;
const RESERVED: [&str; 16] = [
    "end",
    "graph",
    "flowchart",
    "subgraph",
    "style",
    "class",
    "classdef",
    "click",
    "call",
    "linkstyle",
    "default",
    "direction",
    "state",
    "note",
    "classdiagram",
    "statediagram",
];
const FLOW_CLASSES: [&str; 12] = [
    "pure", "io", "alloc", "trap", "ext", "cycle", "dep", "region", "cell", "note", "moved",
    "borrowed",
];
const STATE_CLASSES: [&str; 2] = ["moved", "borrowed"];
const ANNOTATIONS: [&str; 6] = ["record", "union", "class", "instance", "builtin", "module"];

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// `n12` with `prefix` `n`.
fn numbered(text: &str, prefix: char) -> bool {
    text.strip_prefix(prefix).is_some_and(digits)
}

/// An ID of a generated diagram: `n<digits>`, `s<digits>` or `v<digits>[_<digits>]`.
fn is_id(text: &str) -> bool {
    numbered(text, 'n')
        || numbered(text, 's')
        || text
            .strip_prefix('v')
            .is_some_and(|rest| match rest.split_once('_') {
                Some((first, second)) => digits(first) && digits(second),
                None => digits(rest),
            })
}

/// What is wrong with a label: a raw special character, or a `#` that starts no entity code.
fn bad_label(label: &str) -> Option<&'static str> {
    if label
        .chars()
        .any(|character| "\"<>&`\\".contains(character) || (character as u32) < 0x20)
    {
        return Some("raw special character in label");
    }
    for (at, _) in label.match_indices('#') {
        let rest = &label.as_bytes()[at + 1..];
        let run = if rest.first().is_some_and(u8::is_ascii_digit) {
            rest.iter().take_while(|byte| byte.is_ascii_digit()).count()
        } else {
            rest.iter()
                .take_while(|byte| byte.is_ascii_lowercase())
                .count()
        };
        if run == 0 || rest.get(run) != Some(&b';') {
            return Some("'#' that is not an entity code");
        }
    }
    None
}

/// The properties of a `classDef`: `name:value` pairs joined by `,`.
fn properties(text: &str) -> bool {
    text.split(',').all(|pair| {
        pair.split_once(':').is_some_and(|(name, value)| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
                && !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"# .".contains(&byte))
        })
    })
}

fn id_list(text: &str) -> Option<Vec<&str>> {
    let ids: Vec<&str> = text.split(',').collect();
    ids.iter().all(|id| is_id(id)).then_some(ids)
}

/// `classDef NAME PROPS` with NAME among `names`.
fn class_def<'a>(rest: &'a str, names: &[&str]) -> Option<&'a str> {
    let (name, props) = rest.strip_prefix("classDef ")?.split_once(' ')?;
    (names.contains(&name) && properties(props)).then_some(name)
}

/// `class ID,ID NAME` with NAME among `names`.
fn class_use<'a>(rest: &'a str, names: &[&str]) -> Option<(Vec<&'a str>, &'a str)> {
    let (ids, name) = rest.strip_prefix("class ")?.split_once(' ')?;
    if !names.contains(&name) {
        return None;
    }
    Some((id_list(ids)?, name))
}

/// The ID of a flowchart node line and its label: `n1["L"]`, `n1("L")`, `n1(["L"])`,
/// `n1[["L"]]` or `n1{{"L"}}`.
fn flow_node(rest: &str) -> Option<(&str, &str)> {
    let end = 1 + rest
        .get(1..)?
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let (id, shape) = rest.split_at(end);
    if !numbered(id, 'n') {
        return None;
    }
    for (open, close) in [
        ("[\"", "\"]"),
        ("(\"", "\")"),
        ("([\"", "\"])"),
        ("[[\"", "\"]]"),
        ("{{\"", "\"}}"),
    ] {
        if let Some(label) = shape
            .strip_prefix(open)
            .and_then(|inner| inner.strip_suffix(close))
        {
            return Some((id, label));
        }
    }
    None
}

/// A flowchart edge: its ends and its label (`n1 -->|"L"| n2`).
fn flow_edge(rest: &str) -> Option<(&str, &str, Option<&str>)> {
    let (src, after) = rest.split_once(' ')?;
    let after = after
        .strip_prefix("-->")
        .or_else(|| after.strip_prefix("-.->"))?;
    let (label, after) = match after.strip_prefix("|\"") {
        Some(inner) => {
            let (label, after) = inner.split_once("\"|")?;
            if label.contains('"') {
                return None;
            }
            (Some(label), after)
        }
        None => (None, after),
    };
    let dst = after.strip_prefix(' ')?;
    (numbered(src, 'n') && numbered(dst, 'n')).then_some((src, dst, label))
}

/// A class diagram relation: `n1 --> n2`, `n1 ..|> n2` or `n1 ..> n2`, with ` : label`.
fn relation(rest: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = rest.splitn(3, ' ');
    let (src, arrow, tail) = (parts.next()?, parts.next()?, parts.next()?);
    if !matches!(arrow, "-->" | "..|>" | "..>") {
        return None;
    }
    let (dst, label) = match tail.split_once(" : ") {
        Some((dst, label)) => (dst, Some(label)),
        None => (tail, None),
    };
    if label.is_some_and(|label| label.contains('"')) {
        return None;
    }
    (numbered(src, 'n') && numbered(dst, 'n')).then_some((src, dst, label))
}

/// `%% omitted: N edges`.
fn omitted(rest: &str) -> bool {
    rest.strip_prefix("%% omitted: ")
        .and_then(|tail| tail.strip_suffix(" edges"))
        .is_some_and(digits)
}

struct Check {
    errors: Vec<String>,
    declared: BTreeSet<String>,
    uses: Vec<(usize, String)>,
}

impl Check {
    fn label(&mut self, number: usize, label: &str) {
        if let Some(message) = bad_label(label) {
            self.errors.push(format!("line {number}: {message}"));
        }
    }

    fn id(&mut self, number: usize, id: &str) {
        if RESERVED.contains(&id.to_lowercase().as_str()) || !is_id(id) {
            self.errors.push(format!("line {number}: bad id {id}"));
        }
    }

    fn unknown(&mut self, number: usize, line: &str) {
        self.errors
            .push(format!("line {number}: unknown form: '{line}'"));
    }

    fn uses<'a>(&mut self, number: usize, ids: impl IntoIterator<Item = &'a str>) {
        self.uses
            .extend(ids.into_iter().map(|id| (number, id.to_owned())));
    }
}

/// `Ok` when `text` is one of the forms that G30 generates; the problems by line otherwise.
pub fn validate_mermaid(text: &str) -> Result<(), Vec<String>> {
    let mut check = Check {
        errors: Vec::new(),
        declared: BTreeSet::new(),
        uses: Vec::new(),
    };
    if text.chars().count() > MAX_TEXT {
        check
            .errors
            .push("text longer than 45,000 characters".to_owned());
    }
    if !text.ends_with('\n') || text.contains('\r') || text.contains('\t') || text.contains("\n\n")
    {
        check
            .errors
            .push("must end with one newline and have no CR, tab or blank line".to_owned());
    }
    let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    let header = lines[0];
    let body = &lines[1..];
    let mut edges = 0;
    let mut styles: Vec<(usize, usize)> = Vec::new();
    match header {
        "flowchart TD" | "flowchart LR" => {
            let mut after_nodes = false;
            let (mut defined, mut used) = (BTreeSet::new(), BTreeSet::new());
            for (index, line) in body.iter().enumerate() {
                let number = index + 2;
                let Some(rest) = line.strip_prefix("    ") else {
                    check.unknown(number, line);
                    continue;
                };
                if let Some((id, label)) = flow_node(rest) {
                    if after_nodes {
                        check.errors.push(format!("line {number}: node after edge"));
                    }
                    check.id(number, id);
                    if !check.declared.insert(id.to_owned()) {
                        check.errors.push(format!("line {number}: duplicate node"));
                    }
                    check.label(number, label);
                } else if let Some((src, dst, label)) = flow_edge(rest) {
                    after_nodes = true;
                    edges += 1;
                    check.uses(number, [src, dst]);
                    if let Some(label) = label {
                        check.label(number, label);
                    }
                } else if let Some(name) = class_def(rest, &FLOW_CLASSES) {
                    after_nodes = true;
                    defined.insert(name);
                } else if let Some((ids, name)) = class_use(rest, &FLOW_CLASSES) {
                    after_nodes = true;
                    check.uses(number, ids);
                    used.insert(name);
                } else if let Some((id, "stroke-width:3px")) = rest
                    .strip_prefix("style ")
                    .and_then(|tail| tail.split_once(' '))
                    .filter(|(id, _)| numbered(id, 'n'))
                {
                    check.uses(number, [id]);
                } else if let Some((index, "stroke:#cf222e,stroke-width:2px")) = rest
                    .strip_prefix("linkStyle ")
                    .and_then(|tail| tail.split_once(' '))
                    .filter(|(index, _)| digits(index))
                {
                    styles.push((number, index.parse().unwrap_or(usize::MAX)));
                } else if !omitted(rest) {
                    check.unknown(number, line);
                }
            }
            if defined != used {
                check
                    .errors
                    .push("classDef and class do not match".to_owned());
            }
            if check.declared.len() > MAX_NODES || edges > MAX_EDGES {
                check.errors.push("too many nodes or edges".to_owned());
            }
        }
        "classDiagram" => {
            if body.first() != Some(&"    direction LR") {
                check
                    .errors
                    .push("the second line must be '    direction LR'".to_owned());
            }
            let mut open = false;
            for (index, line) in body.iter().enumerate().skip(1) {
                let number = index + 2;
                let class = line.strip_prefix("    class ").and_then(|rest| {
                    let (id, tail) = rest.split_once("[\"")?;
                    let label = tail.strip_suffix("\"] {")?;
                    (numbered(id, 'n') && !label.contains('"')).then_some((id, label))
                });
                if let Some((id, label)) = class {
                    if open {
                        check.errors.push(format!("line {number}: nested block"));
                    }
                    open = true;
                    check.id(number, id);
                    check.declared.insert(id.to_owned());
                    check.label(number, label);
                } else if open
                    && line
                        .strip_prefix("        <<")
                        .and_then(|rest| rest.strip_suffix(">>"))
                        .is_some_and(|name| ANNOTATIONS.contains(&name))
                {
                } else if let Some(member) = line.strip_prefix("        +").filter(|_| open) {
                    check.label(number, &member.replace(['(', ')'], ""));
                } else if *line == "    }" && open {
                    open = false;
                } else if let Some((src, dst, label)) = line.strip_prefix("    ").and_then(relation)
                {
                    check.uses(number, [src, dst]);
                    if let Some(label) = label.filter(|label| !label.is_empty()) {
                        check.label(number, label);
                    }
                } else if !line.strip_prefix("    ").is_some_and(omitted) {
                    check.unknown(number, line);
                }
            }
            if open {
                check.errors.push("unclosed class block".to_owned());
            }
        }
        "stateDiagram-v2" => {
            if body.first() != Some(&"    direction LR") {
                check
                    .errors
                    .push("the second line must be '    direction LR'".to_owned());
            }
            let mut depth = 0;
            for (index, line) in body.iter().enumerate().skip(1) {
                let number = index + 2;
                let pad = "    ".repeat(depth + 1);
                let state = line
                    .strip_prefix(&pad)
                    .and_then(|rest| rest.strip_prefix("state \""))
                    .and_then(|rest| {
                        let (label, tail) = rest.split_once("\" as ")?;
                        let (id, opens) = match tail.strip_suffix(" {") {
                            Some(id) => (id, true),
                            None => (tail, false),
                        };
                        is_id(id).then_some((label, id, opens))
                    });
                let transition = line.strip_prefix(&pad).and_then(|rest| {
                    let (src, tail) = rest.split_once(" --> ")?;
                    let (dst, label) = match tail.split_once(" : ") {
                        Some((dst, label)) => (dst, Some(label)),
                        None => (tail, None),
                    };
                    let end = |id: &str| id == "[*]" || is_id(id);
                    (end(src) && end(dst)).then_some((src, dst, label))
                });
                if let Some((label, id, opens)) = state {
                    check.label(number, label);
                    check.declared.insert(id.to_owned());
                    if opens {
                        depth += 1;
                    }
                } else if depth > 0 && *line == format!("{pad}direction LR") {
                } else if let Some((src, dst, label)) = transition {
                    check.uses(number, [src, dst].into_iter().filter(|id| *id != "[*]"));
                    if let Some(label) = label.filter(|label| !label.is_empty()) {
                        check.label(number, label);
                        // Mermaid 12.1.0 reads `::` in the unquoted label as a new description.
                        if label.contains(':') {
                            check
                                .errors
                                .push(format!("line {number}: raw ':' in a transition label"));
                        }
                    }
                } else if depth > 0 && *line == format!("{}}}", "    ".repeat(depth)) {
                    depth -= 1;
                } else if line
                    .strip_prefix("    ")
                    .is_some_and(|rest| class_def(rest, &STATE_CLASSES).is_some())
                {
                } else if let Some((ids, _)) = line
                    .strip_prefix("    ")
                    .and_then(|rest| class_use(rest, &STATE_CLASSES))
                {
                    check.uses(number, ids);
                } else if !line.strip_prefix("    ").is_some_and(omitted) {
                    check.unknown(number, line);
                }
            }
            if depth > 0 {
                check.errors.push("unclosed state block".to_owned());
            }
        }
        other => check.errors.push(format!("unsupported header '{other}'")),
    }
    let undeclared: Vec<String> = check
        .uses
        .iter()
        .filter(|(_, id)| !check.declared.contains(id))
        .map(|(number, id)| format!("line {number}: {id} is not declared"))
        .collect();
    check.errors.extend(undeclared);
    if header.starts_with("flowchart") {
        for (number, index) in styles {
            if index >= edges {
                check
                    .errors
                    .push(format!("line {number}: linkStyle index out of range"));
            }
        }
    }
    if check.errors.is_empty() {
        Ok(())
    } else {
        Err(check.errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_take_entity_codes_only() {
        assert_eq!(bad_label("a#35;b#quot;c"), None);
        assert!(bad_label("a#b").is_some());
        assert!(bad_label("a#;").is_some());
        assert!(bad_label("say \"hi\"").is_some());
        assert!(bad_label("Array<Point>").is_some());
    }

    #[test]
    fn ids_are_numbered_and_never_reserved() {
        assert!(is_id("n1") && is_id("s0") && is_id("v3_2"));
        assert!(!is_id("end") && !is_id("n") && !is_id("v3_") && !is_id("n1a"));
    }

    #[test]
    fn transition_labels_take_no_colon() {
        let text =
            "stateDiagram-v2\n    direction LR\n    state \"A\" as s1\n    s1 --> s1 : a::b\n";
        assert_eq!(
            validate_mermaid(text).unwrap_err(),
            ["line 4: raw ':' in a transition label"]
        );
    }

    #[test]
    fn validator_rejects_sequence_diagram() {
        let errors = validate_mermaid("sequenceDiagram\n    a->>b: hi\n").unwrap_err();
        assert_eq!(errors, ["unsupported header 'sequenceDiagram'"]);
    }
}
