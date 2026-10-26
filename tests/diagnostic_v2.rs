//! G22: the extras of diagnostics v2 (related locations, help, fixes, expected/found, scope) and
//! the output of diagnostics that have none, which stays what it was before them.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tsuzuri::analyze_all;
use tsuzuri::diagnostic::{Detail, Diagnostic, RelatedRole, Span, json_string, location};

const TYPE1: &str = r#"def add :: i64 -> i64 = \n -> n + true

def pick :: bool -> i64 = \flag -> if flag then 1 else "two"

let x: i64 = "text"
add 1
"#;
const CALL1: &str = r#"def add :: i64 -> i64 -> i64 = \a b -> a + b

def f :: unit -> i64 = \_ -> add 1

def g :: unit -> i64 = \_ -> add 1 2 3

def h :: unit -> i64 = \_ -> add 1 "two"

def k :: unit -> i64 = \_ -> add true 2

0
"#;
const NAME1: &str = r#"record Point { x: i64, y: i64 }

def length_of :: string -> i64 = \s -> s.length

def f :: i64 -> i64 = \count -> countt + 1

def g :: i64 -> Pointt = \n -> Point { x: n, y: n }

def h :: Point -> i64 = \p -> p.z

def k :: unit -> i64 = \_ -> Array.lenght [1, 2, 3]

def m :: unit -> i64 = \_ -> length_off "abc"

0
"#;
const MATCH1: &str = r#"union Shape = Circle of f64 | Square of f64 | Triangle of f64

def area :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w -> w * w

def pick :: i64 -> string = \n ->
    match n with
    | 0 -> "zero"
    | 1 -> "one"

0
"#;
const DUP3: &str = r#"union Shape = Circle of f64 | Square of f64 | Circle of f64

record Pair { a: i64, a: i64 }

0
"#;
const DUP5: &str = r#"def twice :: i64 -> i64 = \n -> n * 2

def twice :: i64 -> i64 = \n -> n + n

0
"#;
const MOVE1: &str = r#"def consume :: own string -> i64 = \s -> s.length

def main_value :: unit -> i64 = \_ ->
    let text = "hello"
    let a = consume text
    let b = consume text
    a + b

main_value ()
"#;
const LOOP1: &str = r#"def consume :: own string -> i64 = \s -> s.length

def total :: unit -> i64 = \_ ->
    let text = "hello"
    let mut sum = 0
    for i in 0 .. 2 do
        sum = sum + consume text
    sum

0
"#;
const LOOP2: &str = r#"def consume :: own string -> i64 = \s -> s.length

def f :: bool -> i64 =
    \c ->
        let text = "a"
        let mut n = 0
        for i in 0 .. 2 do
            if c then
                n = n + consume text
            else
                n = n + 1
        n

0
"#;
const BRANCH1: &str = r#"def consume :: own string -> i64 = \s -> s.length

def total :: bool -> i64 = \flag ->
    let text = "sample"
    if flag then
        let a = consume text
        ()
    else
        ()
    text.length

0
"#;
const BRANCH2: &str = r#"def consume :: own string -> i64 = \s -> s.length

def total :: bool -> i64 = \flag ->
    let text = "sample"
    let n = if flag then consume text else consume text + 1
    consume text + n

0
"#;
const PARTIAL1: &str = r#"record Package { title: string, body: string, count: i64 }

def take :: own string -> i64 = \s -> s.length

def size :: own Package -> i64 = \pkg ->
    let t = pkg.title
    let n = take pkg.body
    pkg.count + take pkg.title + t.length + n

0
"#;
const BORROW1: &str = r#"let mut score = 100
let r1 = ref mut score
let r2 = ref score
deref r1
"#;
const BORROW2: &str = r#"def bump :: ref mut i64 -> unit = \r -> deref r = deref r + 1

def run :: unit -> i64 = \_ ->
    let mut score = 100
    let view = ref score
    score = 5
    deref view

0
"#;
const BORROW3: &str = r#"let score = 100
let r = ref mut score
deref r
"#;
const LIFE1: &str = r#"def invalid_ref :: unit -> ref string = \_ ->
    let local = "temporary"
    ref local

0
"#;
const LIFE3: &str = r#"def f :: own string -> ref string = \s -> ref s

0
"#;
const ESC1: &str = r#"def f :: unit -> i64 =
    \_ ->
        let x = 1
        let mut r = ref x
        if true then
            let y = 2
            r = ref y
        else
            ()
        deref r

0
"#;
const ESC3: &str = r#"union Box = Full of i64 | Empty

def f :: Box -> i64 =
    \b ->
        let zero = 0
        let mut keep = ref zero
        match b with
        | Full n -> keep = ref n
        | Empty -> ()
        deref keep

0
"#;
const NO1: &str = r#"extern def drop_log :: i64 -> unit
record Resource { id: i64, name: string }
instance Drop<Resource> { fn drop value = drop_log value.id }
def name_of :: Resource -> string
fn name_of value = value.name

0
"#;
const NO2: &str = r#"def take :: string -> i64
fn take text = text.length
let text = "owned"
let r = ref text
take r
"#;
const NO3: &str = r#"let lock = Mutex.create 5i64
let kept = Mutex.with_lock (ref lock) (\value -> { let view = ref (deref value); \x -> deref view + x })
0
"#;
const NO4: &str = r#"match Channel.bounded 2 with
| (sender, receiver) ->
    let table = [40i64, 41i64]
    let view = ref table
    let _sent = Channel.send (ref sender) (\x -> x + view[0])
    match Channel.recv (ref receiver) with
    | Maybe.Some _item -> 1
    | Maybe.None -> 0
"#;
const NO5: &str = r#"def take :: string -> i64 = \s -> s.length

let names = ["a", "b"]
take names[0]
"#;
const NO6: &str = r#"extern def drop_log :: i64 -> unit
record Resource { id: i64, name: string }
instance Drop<Resource> { fn drop value = drop_log value.id }
def rename :: Resource -> Resource
fn rename value = { value with name = "c" }

0
"#;
const DUP12: &str = r#"record Array { x: i64 }

0
"#;

fn errors(source: &str) -> Vec<Diagnostic> {
    analyze_all(source).expect_err(source).diagnostics
}

fn has_extras(diagnostic: &Diagnostic) -> bool {
    diagnostic.kind.is_some()
        || has_text_extras(diagnostic)
        || diagnostic.detail != Detail::default()
}

/// The extras that the text form prints: `expected` and `found` are in the JSON only.
fn has_text_extras(diagnostic: &Diagnostic) -> bool {
    !diagnostic.related.is_empty() || diagnostic.help.is_some() || !diagnostic.fixes.is_empty()
}

/// The text form that HEAD printed, written out here as the reference.
fn old_text(diagnostic: &Diagnostic, path: &str, source: &str) -> String {
    let (line, column) = location(source, diagnostic.span.start);
    let mut output = format!(
        "{path}:{line}:{column}: {}[{}]: {}",
        diagnostic.severity.as_str(),
        diagnostic.code,
        diagnostic.message
    );
    let text = source.lines().nth(line - 1).unwrap_or("");
    if !text.is_empty() {
        let skip = column.saturating_sub(41);
        let snippet: String = text.chars().skip(skip).take(120).collect();
        output.push_str(&format!(
            "\n  {line} | {snippet}\n    | {}^",
            " ".repeat(column - 1 - skip)
        ));
    }
    output
}

/// The JSON form that HEAD printed, written out here as the reference.
fn old_json(diagnostic: &Diagnostic, path: &str, source: &str) -> String {
    let (line, column) = location(source, diagnostic.span.start);
    let (end_line, end_column) = location(source, diagnostic.span.end);
    format!(
        "{{\"severity\":{},\"code\":{},\"message\":{},\"path\":{},\"span\":{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\"end_line\":{end_line},\"end_column\":{end_column}}}}}",
        json_string(diagnostic.severity.as_str()),
        json_string(diagnostic.code),
        json_string(&diagnostic.message),
        json_string(path),
        diagnostic.span.start,
        diagnostic.span.end,
    )
}

#[test]
fn json_without_extras_is_byte_identical() {
    let sources = [
        TYPE1, CALL1, NAME1, MATCH1, DUP3, DUP5, BORROW3, LIFE1, MOVE1, NO1, NO2, NO3, NO4, NO5,
        NO6, DUP12,
    ];
    let mut plain = 0;
    for source in sources {
        for diagnostic in errors(source) {
            let (old_text, old_json) = (
                old_text(&diagnostic, "Main.tz", source),
                old_json(&diagnostic, "Main.tz", source),
            );
            let (text, json) = (
                diagnostic.render("Main.tz", source),
                diagnostic.json("Main.tz", source),
            );
            if has_text_extras(&diagnostic) {
                assert!(text.starts_with(&old_text) && text.len() > old_text.len());
            } else {
                assert_eq!(text, old_text);
            }
            if has_extras(&diagnostic) {
                assert!(json.starts_with(&old_json[..old_json.len() - 1]));
                assert!(json.len() > old_json.len());
            } else {
                plain += 1;
                assert_eq!(json, old_json);
            }
        }
    }
    assert!(plain >= 8, "{plain}");
}

fn first(source: &str, code: &str) -> Diagnostic {
    errors(source)
        .into_iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("no {code} in {source}"))
}

fn text_at(source: &str, span: Span) -> &str {
    &source[span.start..span.end]
}

fn line_of(source: &str, span: Span) -> usize {
    source[..span.start].matches('\n').count() + 1
}

/// Runs `tsuzuri check [--json]` on a project made of `files`; returns the exit code and stderr.
fn check_cli(files: &[(&str, &str)], json: bool) -> (i32, String) {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-diagnostic-v2-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for (name, text) in files {
        fs::write(root.join(name), text).unwrap();
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command.arg("check").arg(&root);
    if json {
        command.arg("--json");
    }
    let output = command.output().unwrap();
    fs::remove_dir_all(&root).unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn json_lines(stderr: &str) -> Vec<serde_json::Value> {
    stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("{line}")))
        .collect()
}

#[test]
fn cli_json_keeps_old_keys_first() {
    let (code, stderr) = check_cli(&[("Main.tz", MOVE1)], true);
    assert_eq!(code, 1);
    let lines = json_lines(&stderr);
    assert_eq!(lines.len(), 1, "{stderr}");
    let line = stderr.lines().next().unwrap();
    assert!(line.starts_with(
        "{\"severity\":\"error\",\"code\":\"E1012\",\"message\":\"use of moved or partially moved value 'text'\",\"path\":"
    ));
    let keys: Vec<&str> = lines[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "code", "help", "message", "path", "related", "severity", "span"
        ]
    );
    assert_eq!(lines[0]["related"][0]["role"], "moved_here");
    assert_eq!(lines[0]["related"][0]["span"]["line"], 5);
    assert_eq!(lines[0]["related"][0]["span"]["end_column"], 25);
    assert_eq!(lines[0]["related"][0]["span"].as_object().unwrap().len(), 6);
    // The old keys are first, in the old order, then `related`, then `help`.
    let order = |key: &str| line.find(&format!("\"{key}\":")).unwrap();
    assert!(order("severity") < order("code") && order("code") < order("message"));
    assert!(order("message") < order("path") && order("path") < order("span"));
    assert!(order("span") < order("related") && order("related") < order("help"));
}

#[test]
fn moved_value_names_the_move() {
    let diagnostic = first(MOVE1, "E1012");
    assert_eq!(diagnostic.related.len(), 1);
    let related = &diagnostic.related[0];
    assert_eq!(related.role, RelatedRole::MovedHere);
    assert_eq!(related.message, "value moved here");
    assert_eq!(text_at(MOVE1, related.span), "text");
    assert_eq!(line_of(MOVE1, related.span), 5);
    assert_eq!(line_of(MOVE1, diagnostic.span), 6);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("borrow the value with 'ref text' or move it last")
    );
}

#[test]
fn moved_in_a_loop_says_previous_iteration() {
    for source in [LOOP1, LOOP2] {
        let diagnostic = first(source, "E1012");
        assert_eq!(diagnostic.related.len(), 1);
        assert_eq!(diagnostic.related[0].role, RelatedRole::MovedHere);
        assert_eq!(
            diagnostic.related[0].message,
            "value moved here, in the previous iteration of the loop"
        );
        assert_eq!(diagnostic.related[0].span, diagnostic.span);
    }
}

#[test]
fn moved_in_branches_names_the_later_move() {
    let diagnostic = first(BRANCH1, "E1012");
    assert_eq!(text_at(BRANCH1, diagnostic.related[0].span), "text");
    assert_eq!(line_of(BRANCH1, diagnostic.related[0].span), 6);
    let diagnostic = first(BRANCH2, "E1012");
    let later = BRANCH2.find("else consume text").unwrap() + "else consume ".len();
    assert_eq!(diagnostic.related[0].span.start, later);
    assert_eq!(text_at(BRANCH2, diagnostic.related[0].span), "text");
}

#[test]
fn partial_move_says_part_of_the_value() {
    let diagnostic = first(PARTIAL1, "E1012");
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(
        diagnostic.related[0].message,
        "part of the value moved here"
    );
    assert_eq!(text_at(PARTIAL1, diagnostic.related[0].span), "pkg.title");
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("borrow the field with 'ref' instead of moving it out of 'pkg', or move it last")
    );
}

#[test]
fn borrow_conflict_names_borrow_and_use() {
    let diagnostic = first(BORROW1, "E1014");
    let roles: Vec<_> = diagnostic
        .related
        .iter()
        .map(|related| related.role)
        .collect();
    assert_eq!(
        roles,
        [RelatedRole::BorrowedHere, RelatedRole::BorrowUsedHere]
    );
    assert_eq!(diagnostic.related[0].message, "value mutably borrowed here");
    assert_eq!(
        text_at(BORROW1, diagnostic.related[0].span),
        "ref mut score"
    );
    assert_eq!(
        diagnostic.related[1].message,
        "the borrow is used here, so it is still alive"
    );
    assert_eq!(text_at(BORROW1, diagnostic.related[1].span), "r1");
    assert_eq!(line_of(BORROW1, diagnostic.related[1].span), 4);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some(
            "end the borrow before this access: stop using 'r1' first, or move this access after its last use"
        )
    );
    let diagnostic = first(BORROW2, "E1014");
    assert_eq!(diagnostic.related[0].message, "value borrowed here");
    assert_eq!(text_at(BORROW2, diagnostic.related[0].span), "ref score");
    assert_eq!(text_at(BORROW2, diagnostic.related[1].span), "view");
    assert_eq!(line_of(BORROW2, diagnostic.related[1].span), 7);
}

#[test]
fn immutable_access_names_the_declaration() {
    let diagnostic = first(BORROW3, "E1014");
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].role, RelatedRole::DeclaredHere);
    assert_eq!(
        diagnostic.related[0].message,
        "'score' is declared here without 'mut'"
    );
    assert_eq!(text_at(BORROW3, diagnostic.related[0].span), "score");
    assert_eq!(line_of(BORROW3, diagnostic.related[0].span), 1);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("declare 'score' with 'let mut', or take it by 'ref mut'")
    );
}

#[test]
fn escaping_borrow_names_owner_and_borrow() {
    let diagnostic = first(LIFE1, "E1013");
    assert_eq!(
        diagnostic.related.len(),
        1,
        "the borrow is the primary span"
    );
    assert_eq!(diagnostic.related[0].role, RelatedRole::DeclaredHere);
    assert_eq!(
        diagnostic.related[0].message,
        "'local' is declared here and dropped at the end of this block"
    );
    assert_eq!(text_at(LIFE1, diagnostic.related[0].span), "local");
    assert_eq!(
        diagnostic.help.as_deref(),
        Some(
            "return the value itself instead of a reference to it, or declare it in an outer scope"
        )
    );
    let diagnostic = first(LIFE3, "E1013");
    assert_eq!(
        diagnostic.related[0].message,
        "'s' is declared here and is dropped when the function returns"
    );
    assert_eq!(text_at(LIFE3, diagnostic.related[0].span), "s");
    let diagnostic = first(ESC1, "E1013");
    let roles: Vec<_> = diagnostic
        .related
        .iter()
        .map(|related| related.role)
        .collect();
    assert_eq!(
        roles,
        [RelatedRole::DeclaredHere, RelatedRole::BorrowedHere]
    );
    assert_eq!(text_at(ESC1, diagnostic.related[0].span), "y");
    assert_eq!(text_at(ESC1, diagnostic.related[1].span), "ref y");
    let diagnostic = first(ESC3, "E1013");
    assert_eq!(
        diagnostic.related[0].message,
        "'n' is bound here and ends with this arm or iteration"
    );
    assert_eq!(text_at(ESC3, diagnostic.related[1].span), "ref n");
}

#[test]
fn moves_in_loops_still_reach_the_fixpoint() {
    let codes: Vec<_> = errors(LOOP2)
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(codes, ["E1012"]);
}

#[test]
fn ownership_without_related_is_unchanged() {
    let cases = [
        (
            NO1,
            "E1012",
            "cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead",
        ),
        (
            NO2,
            "E1012",
            "cannot move a non-Copy value out of a reference",
        ),
        (
            NO3,
            "E1013",
            "Mutex.with_lock results must be proven free of borrowed environments",
        ),
        (NO4, "E1013", "a channel item cannot hold a borrow"),
        (
            NO5,
            "E1012",
            "cannot move a non-Copy element out of an array or list; borrow the element instead",
        ),
        (
            NO6,
            "E1012",
            "cannot update a value whose type implements Drop; construct a new value instead",
        ),
    ];
    for (source, code, message) in cases {
        let diagnostic = first(source, code);
        assert!(
            diagnostic.message.starts_with(message),
            "{}",
            diagnostic.message
        );
        assert!(diagnostic.related.is_empty(), "{source}");
        assert!(diagnostic.help.is_none(), "{source}");
    }
}
