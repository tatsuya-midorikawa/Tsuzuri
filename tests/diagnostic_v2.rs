//! G22: the extras of diagnostics v2 (related locations, help, fixes, expected/found, scope) and
//! the output of diagnostics that have none, which stays what it was before them.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tsuzuri::analyze_all;
use tsuzuri::diagnostic::{
    Detail, Diagnostic, Edit, RelatedRole, ScopeEntry, Span, apply_edits, json_string, location,
};

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

const FIELD1: &str = r#"record Point { x: i64, y: i64 }

def f :: Point -> i64 = \p -> p.z

def g :: unit -> Point = \_ -> Point { x: 1, z: 2 }

0
"#;
const NOFN: &str = r#"def f :: i64 = 1 2

def g :: i64 -> i64 = \n -> n 3

def h :: string -> i64 = \s -> s "x"

0
"#;
const STD2: &str = r#"def f :: i64 = Array.length 5

0
"#;
const MISMATCH1: &str = r#"def f :: [i64; 2] -> i64 = \a -> 0

def g :: [i64; 3] -> i64 = \a -> f a

0
"#;
const MISMATCH2: &str = r#"def f :: Maybe<i64> -> i64 = \m -> 0

def g :: Maybe<string> -> i64 = \m -> f m

0
"#;
const FIX2: &str = r#"def f :: bool -> i64 = \b ->
    match b with
    | true -> 1

def g :: bool -> bool -> i64 = \a b ->
    match (a, b) with
    | (true, true) -> 1
    | (false, _) -> 2

0
"#;
const FIX3: &str = r#"record P { x: i64, y: i64 }

def f :: P -> i64 = \p ->
    match p with
    | P { x = 0, y = n } -> n

def g :: [i64] -> i64 = \xs ->
    match xs with
    | [] -> 0

def h :: [|i64|] -> i64 = \ls ->
    match ls with
    | [||] -> 0

0
"#;
const FIX4: &str = r#"def classify :: bool -> i64
fn classify flag
    | true -> 1

0
"#;
const ARMS5: &str = r#"union Shape = Circle of f64 | Square of f64 | Triangle of f64

def area :: Shape -> Shape -> f64 = \s t ->
    match s with
    | Circle r -> r
    | Square w -> match t with | Circle q -> q | Square z -> z | Triangle _ -> unreachable ()

0
"#;
const ARMS6: &str = r#"union Shape = Circle of f64 | Square of f64 | Triangle of f64

def area :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w ->
        let a = w * w
        a

def area2 :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w ->
        if w > 1.0 then
            w
        else
            2.0

0
"#;
const DUP2: &str = r#"record Point { x: i64, y: i64 }

record Point { a: i64 }

0
"#;
const DUP6: &str = r#"def add :: i64 -> i64 -> i64 = \a a -> a + a

record Pair { left: i64, left: i64 }

0
"#;
const DUP7: &str = r#"def add :: i64 -> i64 -> i64 = \a a -> a + a

0
"#;
const DUP8: &str = r#"def twice :: i64 -> i64

fn twice n = n * 2

fn twice n = n + n

0
"#;
const DUP9: &str = r#"fn twice n = n * 2

def twice :: i64 -> i64 = \n -> n + n

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

/// The diagnostic with `code` whose primary span starts on `line`.
fn at_line(source: &str, code: &str, line: usize) -> Diagnostic {
    errors(source)
        .into_iter()
        .find(|diagnostic| diagnostic.code == code && line_of(source, diagnostic.span) == line)
        .unwrap_or_else(|| panic!("no {code} on line {line} in {source}"))
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
fn moved_in_a_while_condition_says_previous_iteration() {
    // The condition is checked before the loop's body; it runs again too.
    let source = "def consume :: own string -> bool = \\s -> s.length > 0\n\ndef f :: unit -> i64 = \\_ ->\n    let text = \"a\"\n    while consume text do\n        ()\n    0\n\n0\n";
    let diagnostic = first(source, "E1012");
    assert_eq!(
        diagnostic.related[0].message,
        "value moved here, in the previous iteration of the loop"
    );
    assert_eq!(diagnostic.related[0].span, diagnostic.span);
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

#[test]
fn related_in_another_file_is_rendered() {
    let lib = "def add :: i64 -> i64 -> i64 = \\a b -> a + b\n";
    let main = "def total :: unit -> i64 = \\_ -> Lib.add 1 \"two\"\n\n0\n";
    let files = [("Lib.tz", lib), ("Main.tz", main)];
    let (_, text) = check_cli(&files, false);
    assert!(
        text.contains("Main.tz:1:44: error[E1003]: expected i64, found string"),
        "{text}"
    );
    assert!(
        text.contains("  note: parameter 2 of 'add' is declared here\n    --> "),
        "{text}"
    );
    assert!(text.contains("Lib.tz:1:19\n  1 | def add :: i64 -> i64 -> i64 = \\a b -> a + b\n    |                   ^"), "{text}");
    let (_, stderr) = check_cli(&files, true);
    let diagnostic = &json_lines(&stderr)[0];
    assert!(diagnostic["path"].as_str().unwrap().ends_with("Main.tz"));
    let related = &diagnostic["related"][0];
    assert_eq!(related["role"], "expected_because");
    assert!(related["path"].as_str().unwrap().ends_with("Lib.tz"));
    assert_eq!(related["span"]["line"], 1);
    assert_eq!(related["span"]["column"], 19);
}

#[test]
fn mismatch_has_expected_and_found() {
    let diagnostic = at_line(TYPE1, "E1003", 1);
    assert_eq!(diagnostic.detail.expected.as_deref(), Some("i64"));
    assert_eq!(diagnostic.detail.found.as_deref(), Some("bool"));
    assert!(diagnostic.related.is_empty());
    let diagnostic = first(MISMATCH1, "E1003");
    assert_eq!(diagnostic.detail.expected.as_deref(), Some("[i64; 2]"));
    assert_eq!(diagnostic.detail.found.as_deref(), Some("[i64; 3]"));
    let diagnostic = first(MISMATCH2, "E1003");
    assert_eq!(diagnostic.detail.expected.as_deref(), Some("Maybe<i64>"));
    assert_eq!(diagnostic.detail.found.as_deref(), Some("Maybe<string>"));
    // The message is the same text as before.
    assert_eq!(
        diagnostic.message,
        "expected Maybe<i64>, found Maybe<string>"
    );
}

#[test]
fn annotation_mismatch_names_the_annotation() {
    let diagnostic = at_line(TYPE1, "E1003", 5);
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].role, RelatedRole::ExpectedBecause);
    assert_eq!(
        diagnostic.related[0].message,
        "the type annotation declares this type"
    );
    assert_eq!(text_at(TYPE1, diagnostic.related[0].span), "i64");
    assert_eq!(line_of(TYPE1, diagnostic.related[0].span), 5);
}

#[test]
fn result_mismatch_names_the_result_type() {
    let diagnostic = at_line(TYPE1, "E1003", 3);
    assert_eq!(
        diagnostic.related[0].message,
        "the result type of 'pick' is declared here"
    );
    assert_eq!(text_at(TYPE1, diagnostic.related[0].span), "i64");
    assert_eq!(line_of(TYPE1, diagnostic.related[0].span), 3);
    let diagnostic = at_line(CALL1, "E1003", 3);
    assert_eq!(
        diagnostic.related[0].message,
        "the result type of 'f' is declared here"
    );
    assert_eq!(text_at(CALL1, diagnostic.related[0].span), "i64");
}

#[test]
fn argument_mismatch_names_the_parameter() {
    let diagnostic = at_line(CALL1, "E1003", 7);
    assert_eq!(diagnostic.related[0].role, RelatedRole::ExpectedBecause);
    assert_eq!(
        diagnostic.related[0].message,
        "parameter 2 of 'add' is declared here"
    );
    assert_eq!(text_at(CALL1, diagnostic.related[0].span), "i64");
    assert_eq!(line_of(CALL1, diagnostic.related[0].span), 1);
    let diagnostic = at_line(CALL1, "E1003", 9);
    assert_eq!(
        diagnostic.related[0].message,
        "parameter 1 of 'add' is declared here"
    );
    assert_eq!(
        diagnostic.related[0].span.start,
        CALL1.find("i64 -> i64 -> i64").unwrap()
    );
}

#[test]
fn inner_mismatch_has_no_because() {
    assert!(at_line(TYPE1, "E1003", 1).related.is_empty());
}

#[test]
fn std_parameter_note_points_into_std() {
    let (code, stderr) = check_cli(&[("Main.tz", STD2)], true);
    assert_eq!(code, 1);
    let diagnostic = &json_lines(&stderr)[0];
    assert_eq!(diagnostic["code"], "E1003");
    let related = &diagnostic["related"][0];
    assert_eq!(related["role"], "expected_because");
    assert_eq!(
        related["message"],
        "parameter 1 of 'length' is declared here"
    );
    assert_eq!(
        related["path"].as_str().unwrap().replace('\\', "/"),
        "std/Array.tz"
    );
    let std = include_str!("../std/Array.tz");
    let declared = std[..std.find("def length ::").unwrap()]
        .matches('\n')
        .count()
        + 1;
    assert_eq!(related["span"]["line"], declared);
}

#[test]
fn partial_application_gets_a_help() {
    let diagnostic = at_line(CALL1, "E1003", 3);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some(
            "a call with too few arguments evaluates to a function; add the missing argument(s) (1 more needed)"
        )
    );
    assert_eq!(diagnostic.detail.found.as_deref(), Some("i64 -> i64"));
    // A mismatch that is not a partial application has no such help.
    assert!(at_line(CALL1, "E1003", 7).help.is_none());
}

#[test]
fn unknown_value_lists_the_scope() {
    let diagnostic = first(NAME1, "E1002");
    assert_eq!(text_at(NAME1, diagnostic.span), "countt");
    assert_eq!(
        diagnostic.detail.scope,
        [ScopeEntry {
            name: "count".into(),
            ty: "i64".into()
        }]
    );
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("check the spelling, or define the name with 'let' or 'def'; names in scope: count")
    );
    let diagnostic = errors(NAME1)
        .into_iter()
        .find(|diagnostic| diagnostic.message == "unknown value 'length_off'")
        .unwrap();
    assert!(diagnostic.detail.scope.is_empty());
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("check the spelling, or define the name with 'let' or 'def'")
    );
    // The unknown type and the unknown module function are not changed.
    for diagnostic in errors(NAME1) {
        if diagnostic.code == "E1004" || diagnostic.message.starts_with("module 'Array'") {
            assert!(!has_extras(&diagnostic), "{}", diagnostic.message);
        }
    }
}

#[test]
fn unknown_field_lists_the_fields() {
    for diagnostic in errors(FIELD1) {
        assert_eq!(diagnostic.code, "E1007");
        assert_eq!(
            diagnostic.help.as_deref(),
            Some("the fields of 'Main.Point' are: x, y")
        );
    }
    assert_eq!(errors(FIELD1).len(), 2);
}

#[test]
fn call_helps() {
    let diagnostic = first(CALL1, "E1006");
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("remove the 1 extra argument(s)")
    );
    let helps: Vec<_> = errors(NOFN)
        .into_iter()
        .map(|diagnostic| diagnostic.into_data().help.unwrap())
        .collect();
    assert_eq!(
        helps,
        [
            "a number is not a function; remove the argument(s) or call a function",
            "a value of type i64 is not a function; remove the argument(s) or call a function",
            "a value of type string is not a function; remove the argument(s) or call a function",
        ]
    );
}

/// Applies the first fix of every diagnostic in `source` at once.
fn apply_first_fixes(source: &str) -> String {
    let diagnostics = errors(source);
    let edits: Vec<&Edit> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "E1021")
        .flat_map(|diagnostic| &diagnostic.fixes[0].edits)
        .collect();
    apply_edits(source, &edits).unwrap()
}

#[test]
fn non_exhaustive_match_offers_a_fix_that_compiles() {
    for (source, count) in [(MATCH1, 2), (FIX2, 2), (ARMS6, 2)] {
        let diagnostics: Vec<_> = errors(source)
            .into_iter()
            .filter(|diagnostic| diagnostic.code == "E1021")
            .collect();
        assert_eq!(diagnostics.len(), count, "{source}");
        for diagnostic in &diagnostics {
            assert!(
                diagnostic
                    .help
                    .as_deref()
                    .unwrap()
                    .starts_with("add an arm for '")
            );
            assert_eq!(diagnostic.fixes.len(), 1);
            let fix = &diagnostic.fixes[0];
            assert_eq!(fix.applicability.as_str(), "maybe");
            assert_eq!(fix.edits.len(), 1);
            let edit = &fix.edits[0];
            assert_eq!(edit.span.start, edit.span.end);
            assert_eq!(edit.span.start, diagnostic.span.end);
            assert!(edit.text.starts_with(" | ") && edit.text.ends_with(" -> unreachable ()"));
            assert_eq!(
                fix.title,
                format!("Add the arm '{}'", edit.text.trim_start())
            );
        }
        let fixed = apply_first_fixes(source);
        analyze_all(&fixed).unwrap_or_else(|set| panic!("{fixed}\n{:?}", set.diagnostics));
    }
    // The titles and the inserted arms.
    let titles: Vec<_> = errors(FIX2)
        .iter()
        .map(|diagnostic| diagnostic.fixes[0].title.clone())
        .collect();
    assert_eq!(
        titles,
        [
            "Add the arm '| false -> unreachable ()'",
            "Add the arm '| (true, false) -> unreachable ()'"
        ]
    );
    assert_eq!(
        apply_first_fixes(ARMS6),
        ARMS6
            .replace("        a\n", "        a | Triangle _ -> unreachable ()\n")
            .replace(
                "            2.0\n",
                "            2.0 | Triangle _ -> unreachable ()\n"
            )
    );
}

#[test]
fn no_fix_for_unwritable_witnesses() {
    let diagnostics = errors(FIX3);
    assert_eq!(diagnostics.len(), 3);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.help.is_some())
    );
    let fixes: Vec<usize> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.fixes.len())
        .collect();
    assert_eq!(fixes, [0, 1, 0]);
    assert_eq!(
        diagnostics[1].fixes[0].title,
        "Add the arm '| _ -> unreachable ()'"
    );
    let diagnostic = first(FIX4, "E1021");
    assert!(diagnostic.fixes.is_empty());
    assert!(diagnostic.help.is_some());
}

#[test]
fn nested_inline_match_fix_is_not_verified_here() {
    let diagnostic = first(ARMS5, "E1021");
    assert_eq!(diagnostic.fixes.len(), 1);
    let fixed = apply_first_fixes(ARMS5);
    // The new arm attaches to the nested match, so the outer match is still not exhaustive.
    let remaining = analyze_all(&fixed).expect_err(&fixed).diagnostics;
    assert!(
        remaining
            .iter()
            .any(|diagnostic| diagnostic.code == "E1021"),
        "{fixed}"
    );
}

#[test]
fn duplicates_name_the_first_declaration() {
    let cases = [
        (DUP2, "E1001", "Point", 1),
        (DUP3, "E1024", "Circle", 1),
        (DUP5, "E1001", "twice", 1),
        (DUP6, "E1001", "left", 3),
        (DUP7, "E1001", "a", 1),
        (DUP8, "E1001", "twice", 3),
        (DUP9, "E1001", "twice", 1),
    ];
    for (source, code, name, line) in cases {
        let diagnostic = first(source, code);
        assert_eq!(diagnostic.related.len(), 1, "{source}");
        let related = &diagnostic.related[0];
        assert_eq!(related.role, RelatedRole::FirstDefinedHere);
        assert_eq!(related.message, "first defined here");
        assert_eq!(text_at(source, related.span), name, "{source}");
        assert_eq!(line_of(source, related.span), line, "{source}");
        assert!(related.span.start < diagnostic.span.start, "{source}");
    }
}

#[test]
fn reserved_names_have_no_first_definition() {
    let diagnostic = first(DUP12, "E1001");
    assert_eq!(diagnostic.message, "duplicate or reserved name 'Array'");
    assert!(diagnostic.related.is_empty());
}

#[test]
fn cli_json_of_a_fix_has_edits_with_spans() {
    let (_, stderr) = check_cli(&[("Main.tz", FIX2)], true);
    let diagnostic = &json_lines(&stderr)[0];
    assert_eq!(diagnostic["code"], "E1021");
    let fix = &diagnostic["fixes"][0];
    assert_eq!(fix["applicability"], "maybe");
    assert_eq!(fix["title"], "Add the arm '| false -> unreachable ()'");
    let edit = &fix["edits"][0];
    assert_eq!(edit["text"], " | false -> unreachable ()");
    assert_eq!(edit["span"]["start"], edit["span"]["end"]);
    assert!(edit["path"].as_str().unwrap().ends_with("Main.tz"));
    let keys: Vec<&str> = diagnostic
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert!(keys.contains(&"help") && keys.contains(&"fixes"));
}

#[test]
fn parenthesized_match_fix_goes_inside_the_parentheses() {
    let source = "def f :: bool -> i64 = \\b -> (match b with | true -> 1)\n\n0\n";
    let diagnostic = first(source, "E1021");
    let edit = &diagnostic.fixes[0].edits[0];
    // The match's span includes the parentheses; the arm goes after the last arm's body.
    assert_eq!(edit.span.start, source.find("1)").unwrap() + 1);
    assert_eq!(&source[edit.span.start..], ")\n\n0\n");
    let fixed = apply_first_fixes(source);
    analyze_all(&fixed).unwrap_or_else(|set| panic!("{fixed}\n{:?}", set.diagnostics));
}

const HELD: &str = "def both :: ref i64 -> ref mut i64 -> unit = \\a b -> ()\n\nlet mut n = 0\nlet r = ref n\nboth r (ref mut n)\n";
const NEXT_ITERATION: &str =
    "let mut n = 0\nlet r = ref n\nfor i in 0 .. 2 do\n    let x = deref r\n    n = x + 1\nn\n";

#[test]
fn an_earlier_use_is_named_only_when_a_loop_repeats_it() {
    // The argument 'r' holds the borrow while the call evaluates 'ref mut n': there is no loop.
    let diagnostic = first(HELD, "E1014");
    let roles: Vec<_> = diagnostic
        .related
        .iter()
        .map(|related| related.role)
        .collect();
    assert_eq!(roles, [RelatedRole::BorrowedHere]);
    let diagnostic = first(NEXT_ITERATION, "E1014");
    assert_eq!(diagnostic.related.len(), 2);
    let used = &diagnostic.related[1];
    assert_eq!(used.role, RelatedRole::BorrowUsedHere);
    assert_eq!(
        used.message,
        "the borrow is used here, in the next iteration of the loop, so it is still alive"
    );
    assert_eq!(text_at(NEXT_ITERATION, used.span), "r");
    assert_eq!(line_of(NEXT_ITERATION, used.span), 4);
}

#[test]
fn reserved_fields_have_no_first_definition() {
    let source = "record R { _: i64, _: i64 }\n\n0\n";
    let diagnostic = first(source, "E1001");
    assert_eq!(diagnostic.span.start, source.find('_').unwrap());
    assert!(diagnostic.related.is_empty(), "{:?}", diagnostic.related);
}

#[test]
fn generated_declarations_have_no_because() {
    // A test's implicit `unit` result and a bench's generated binding are not written anywhere.
    for source in ["test \"bad\" = true\n", "bench \"a\" = 1\n"] {
        let diagnostic = first(source, "E1003");
        assert!(
            diagnostic.related.is_empty(),
            "{source}{:?}",
            diagnostic.related
        );
        assert!(diagnostic.detail.expected.is_some(), "{source}");
    }
}

#[test]
fn a_reassigned_holder_is_not_a_later_use() {
    // `r = ref m` ends the borrow of `n` that `r` held, and `deref r` reads the new one.
    let source = "def both :: ref i64 -> ref mut i64 -> unit = \\a b -> ()\n\nlet mut n = 0\nlet m = 1\nlet mut r = ref n\nboth r (ref mut n)\nr = ref m\nderef r\n";
    let diagnostic = first(source, "E1014");
    let roles: Vec<_> = diagnostic
        .related
        .iter()
        .map(|related| related.role)
        .collect();
    assert_eq!(roles, [RelatedRole::BorrowedHere]);
    // Read before it is replaced, the holder still keeps the borrow alive.
    let source =
        "let mut n = 0\nlet m = 1\nlet mut r = ref n\nn = 2\nr = ref (deref r + m)\nderef r\n";
    let diagnostic = first(source, "E1014");
    let used = diagnostic
        .related
        .iter()
        .find(|related| related.role == RelatedRole::BorrowUsedHere)
        .unwrap_or_else(|| panic!("{:?}", diagnostic.related));
    assert_eq!(
        used.message,
        "the borrow is used here, so it is still alive"
    );
    assert_eq!(line_of(source, used.span), 5);
    assert_eq!(text_at(source, used.span), "r");
}

#[test]
fn each_holder_keeps_its_own_later_use() {
    // Replacing `r` does not end the borrow that `keep` holds.
    let source = "let mut n = 0\nlet m = 1\nlet mut r = ref n\nlet keep = r\nn = 2\nr = ref m\nderef keep + deref r\n";
    let diagnostic = first(source, "E1014");
    let used = diagnostic
        .related
        .iter()
        .find(|related| related.role == RelatedRole::BorrowUsedHere)
        .unwrap_or_else(|| panic!("{:?}", diagnostic.related));
    assert_eq!(text_at(source, used.span), "keep");
    assert_eq!(line_of(source, used.span), 7);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some(
            "end the borrow before this access: stop using 'keep' first, or move this access after its last use"
        )
    );
}

#[test]
fn a_reborrow_of_a_parameter_names_where_it_was_taken() {
    let source = "def f :: ref mut i64 -> i64 = \\p ->\n    let q = ref (deref p)\n    deref p = 1\n    deref q\n\n0\n";
    let diagnostic = first(source, "E1014");
    assert_eq!(diagnostic.related[0].role, RelatedRole::BorrowedHere);
    assert_eq!(text_at(source, diagnostic.related[0].span), "ref (deref p)");
}

#[test]
fn an_operator_in_the_tail_is_not_explained_by_the_declaration() {
    // `!` expects a bool; the annotation declares the result, `i64`.
    let source = "let x: i64 = !1i64\nx\n";
    let diagnostic = first(source, "E1003");
    assert_eq!(diagnostic.detail.expected.as_deref(), Some("bool"));
    assert!(diagnostic.related.is_empty(), "{:?}", diagnostic.related);
}

#[test]
fn a_fix_adds_every_missing_case_or_none() {
    let source =
        "union U = A | B | C\n\ndef f :: U -> i64 = \\u ->\n    match u with\n    | A -> 1\n\n0\n";
    let diagnostic = first(source, "E1021");
    assert_eq!(diagnostic.fixes.len(), 1);
    assert_eq!(
        diagnostic.fixes[0].title,
        "Add the arms '| B -> unreachable () | C -> unreachable ()'"
    );
    let fixed = apply_first_fixes(source);
    analyze_all(&fixed).unwrap_or_else(|set| panic!("{fixed}\n{:?}", set.diagnostics));
    // Nine missing cases are more arms than a fix adds.
    let cases: Vec<String> = (0..10).map(|index| format!("K{index}")).collect();
    let source = format!(
        "union U = {}\n\ndef f :: U -> i64 = \\u ->\n    match u with\n    | K0 -> 1\n\n0\n",
        cases.join(" | ")
    );
    let diagnostic = first(&source, "E1021");
    assert!(diagnostic.fixes.is_empty());
    assert!(diagnostic.help.is_some());
}

#[test]
fn an_operand_type_is_not_explained_by_the_declaration() {
    // `&&` needs bool operands whatever the annotation says, even when it says bool too.
    let source = "let x: bool = 1i64 && 2i64\nx\n";
    let diagnostic = first(source, "E1003");
    assert!(diagnostic.related.is_empty(), "{:?}", diagnostic.related);
}

#[test]
fn borrow_uses_follow_the_control_flow() {
    let used = |source: &str| {
        let diagnostic = first(source, "E1014");
        diagnostic
            .related
            .iter()
            .find(|related| related.role == RelatedRole::BorrowUsedHere)
            .map(|related| {
                (
                    line_of(source, related.span),
                    related.message.contains("next iteration"),
                )
            })
    };
    // A use before the loop does not hide the one that the next iteration runs.
    let source = "let mut n = 0\nlet r = ref n\nlet _before = deref r\nfor i in 0 .. 2 do\n    let x = deref r\n    n = x + 1\nn\n";
    assert_eq!(used(source), Some((5, true)));
    // The condition of a `while` runs again too.
    let source = "let mut n = 0\nlet r = ref n\nwhile deref r < 2 do\n    n = n + 1\n0\n";
    assert_eq!(used(source), Some((3, true)));
    // A replacement before the loop does not stop the loop from using the borrow again.
    let source = "let mut n = 0\nlet mut m = 5\nlet mut r = ref m\nr = ref n\nfor i in 0 .. 2 do\n    let x = deref r\n    n = x + 1\nn\n";
    assert_eq!(used(source), Some((6, true)));
    // A replacement in one branch only does not end the borrow for the other.
    let source = "def f :: bool -> i64 = \\flag ->\n    let mut n = 0\n    let m = 1\n    let mut r = ref n\n    n = 2\n    if flag then r = ref m else ()\n    deref r\n\n0\n";
    assert_eq!(used(source), Some((7, false)));
    // A false guard falls through to the later arms, which may still use the borrow.
    let source = "def set :: ref mut i64 -> bool = \\p -> false\n\ndef f :: i64 -> i64 = \\k ->\n    let mut n = 0\n    let r = ref n\n    match k with\n    | 0 when set (ref mut n) -> 0\n    | _ -> deref r\n\n0\n";
    assert_eq!(used(source), Some((8, false)));
    // The other branch of the `if` does not run after the access.
    let source = "def both :: ref i64 -> ref mut i64 -> unit = \\a b -> ()\ndef discard :: i64 -> unit = \\a -> ()\n\ndef f :: bool -> unit = \\flag ->\n    let mut n = 0\n    let r = ref n\n    if flag then both r (ref mut n) else discard (deref r)\n\n0\n";
    assert_eq!(used(source), None);
}

#[test]
fn evaluation_order_decides_what_runs_later() {
    // The `where` bindings of a guarded function run before its arms: no loop is involved.
    let source = "def take :: own string -> i64 = \\s -> s.length\n\ndef f :: own string -> i64\nfn f text\n    | otherwise -> take text + n\n    where\n        n = take text\n\n0\n";
    let diagnostic = first(source, "E1012");
    assert_eq!(diagnostic.related[0].message, "value moved here");
    assert_eq!(line_of(source, diagnostic.related[0].span), 7);
    // The arm uses the borrow after the `where` binding that conflicts with it.
    let source = "def f :: ref mut i64 -> i64\nfn f p\n    | otherwise -> deref q\n    where\n        q = ref (deref p)\n        changed = { deref p = 1; 0 }\n\n0\n";
    let diagnostic = first(source, "E1014");
    let used = diagnostic
        .related
        .iter()
        .find(|related| related.role == RelatedRole::BorrowUsedHere)
        .unwrap_or_else(|| panic!("{:?}", diagnostic.related));
    assert_eq!(line_of(source, used.span), 3);
    assert_eq!(text_at(source, used.span), "q");
}

#[test]
fn jumps_and_iterations_decide_what_runs_again() {
    let used = |source: &str| {
        let diagnostic = first(source, "E1014");
        diagnostic
            .related
            .iter()
            .find(|related| related.role == RelatedRole::BorrowUsedHere)
            .map(|related| {
                (
                    line_of(source, related.span),
                    related.message.contains("next iteration"),
                )
            })
    };
    // `break` leaves the loop: the read after the `if` does not run after the access.
    let source = "def both :: ref i64 -> ref mut i64 -> unit = \\a b -> ()\n\ndef f :: bool -> i64 = \\flag ->\n    let mut n = 0\n    let r = ref n\n    for i in 0 .. 2 do\n        if flag then\n            both r (ref mut n)\n            break\n        else\n            ()\n        let _use = deref r\n        ()\n    0\n\n0\n";
    assert_eq!(used(source), None);
    // The next iteration may take the other branch.
    let source = "let mut n = 0\nlet r = ref n\nfor i in 0 .. 2 do\n    if i == 0 then\n        n = 1\n    else\n        let _x = deref r\n        ()\n0\n";
    assert_eq!(used(source), Some((7, true)));
    // A `break` ends only the inner loop; the outer one reads the borrow again.
    let source = "let mut n = 0\nlet r = ref n\nfor i in 0 .. 2 do\n    let x = deref r\n    for j in 0 .. 2 do\n        n = 1\n        break\n0\n";
    assert_eq!(used(source), Some((4, true)));
    // The arm of the access replaces the holder before the read after the `if`.
    let source = "def both :: ref i64 -> ref mut i64 -> unit = \\a b -> ()\n\ndef f :: bool -> i64 = \\flag ->\n    let mut n = 0\n    let m = 1\n    let mut r = ref n\n    if flag then\n        both r (ref mut n)\n        r = ref m\n    else\n        ()\n    deref r\n\n0\n";
    assert_eq!(used(source), None);
    // The `break` after the conflict skips the replacement: the read after the loop reads `n`.
    let source = "def both :: ref i64 -> ref mut i64 -> unit = \\a b -> ()\n\ndef f :: bool -> i64 = \\flag ->\n    let mut n = 0\n    let m = 1\n    let mut r = ref n\n    for i in 0 .. 2 do\n        if flag then\n            both r (ref mut n)\n            break\n        else\n            ()\n        r = ref m\n    deref r\n\n0\n";
    assert_eq!(used(source), Some((14, false)));
    // A read after an unconditional `break` never runs, in any iteration.
    let source = "let mut n = 0\nlet r = ref n\nfor i in 0 .. 2 do\n    for j in 0 .. 2 do\n        n = 1\n        break\n        let _x = deref r\n        ()\n0\n";
    assert_eq!(used(source), None);
    // A conflict in the condition of a `while`, which runs again.
    let source =
        "let mut n = 0\nlet r = ref n\nwhile { let x = deref r; n = x + 1; x < 2 } do\n    ()\n0\n";
    assert_eq!(used(source), Some((3, true)));
}

#[test]
fn a_parenthesized_function_is_a_first_definition() {
    for source in [
        "fn f() -> i64 { 1 }\n\nfn f() -> i64 { 2 }\n\n0\n",
        "fn f() -> i64 { 1 }\n\ndef f :: unit -> i64 = \\_ -> 2\n\n0\n",
    ] {
        let diagnostic = first(source, "E1001");
        assert_eq!(diagnostic.related.len(), 1, "{source}");
        assert_eq!(line_of(source, diagnostic.related[0].span), 1);
        assert_eq!(text_at(source, diagnostic.related[0].span), "f");
    }
}
