//! G22: the extras of diagnostics v2 (related locations, help, fixes, expected/found, scope) and
//! the output of diagnostics that have none, which stays what it was before them.

use tsuzuri::analyze_all;
use tsuzuri::diagnostic::{Detail, Diagnostic, json_string, location};

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
const BORROW3: &str = r#"let score = 100
let r = ref mut score
deref r
"#;
const LIFE1: &str = r#"def invalid_ref :: unit -> ref string = \_ ->
    let local = "temporary"
    ref local

0
"#;
const MOVE1: &str = r#"def consume :: string -> i64 = \s -> s.length

def main_value :: unit -> i64 = \_ ->
    let text = "hello"
    let a = consume text
    let b = consume text
    a + b

main_value ()
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
