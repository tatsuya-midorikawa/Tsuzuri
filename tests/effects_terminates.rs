//! A20 Phase 5: `@terminates`, a structural sufficient condition: no `while`, no recursion, no
//! built-in function that waits for an outside event, and no call that the analysis cannot follow.

use tsuzuri::analyze;

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";

fn verdict(prelude: &str, function: &str) -> Result<(), (String, Option<&'static str>)> {
    let source = format!("{prelude}@terminates\n{function}\n{MAIN}");
    match analyze(&source) {
        Ok(_) => Ok(()),
        Err(error) => {
            assert_eq!(error.code, "E1031", "{source}\n{}", error.message);
            Err((error.message.clone(), error.kind))
        }
    }
}

#[test]
fn terminates_accepts_for_loops_and_acyclic_calls() {
    let prelude = "def square :: i64 -> i64 = \\x -> x * x\n";
    verdict(prelude, "def f :: ref [i64] -> i64 = \\xs ->\n    let mut sum = 0\n    for x in xs do\n        sum = sum + square x\n    for i in 0 .. 10 do\n        sum = sum + i\n    sum").unwrap();
    verdict(
        "",
        "def f :: ref [i64] -> i64 = \\xs -> Array.sum xs + Array.fold (\\a x -> a + x) 0 xs",
    )
    .unwrap();
}

#[test]
fn terminates_rejects_while() {
    let (message, kind) = verdict(
        "",
        "def f :: i64 -> i64 = \\n ->\n    let mut i = n\n    while i > 0 do\n        i = i - 1\n    i",
    )
    .unwrap_err();
    assert_eq!(kind, Some("effect_diverge_in_terminates"));
    assert!(
        message.contains("contains a 'while' loop, which may not terminate"),
        "{message}"
    );
}

#[test]
fn terminates_rejects_self_tail_recursion_and_mutual_recursion() {
    let (message, _) = verdict(
        "",
        "def rec count :: i64 -> i64 = \\n -> if n <= 0 then 0 else count (n - 1)",
    )
    .unwrap_err();
    assert!(
        message.contains("this expression is recursive (through 'Main.count' -> 'Main.count')"),
        "{message}"
    );
    let prelude = "def rec ping :: i64 -> i64 = \\n -> if n <= 0 then 0 else pong (n - 1)\nand pong :: i64 -> i64 = \\n -> ping n\n";
    let (message, _) = verdict(prelude, "def f :: i64 -> i64 = \\n -> ping n").unwrap_err();
    assert!(
        message.contains(
            "this call reaches 'Main.ping', which is recursive (through 'Main.ping' -> 'Main.pong')"
        ),
        "{message}"
    );
}

#[test]
fn terminates_rejects_blocking_builtins() {
    let (message, kind) = verdict(
        "",
        "def f :: unit -> IO<Result<string, Os.Error>> = \\() -> File.read_text \"a.txt\"",
    )
    .unwrap_err();
    assert_eq!(kind, Some("effect_diverge_in_terminates"), "{message}");
    assert!(
        message.contains("waits for an event outside the program"),
        "{message}"
    );
}

#[test]
fn terminates_rejects_unknown_callees() {
    let prelude = "record Box { run: i64 -> i64 }\n";
    let (message, kind) =
        verdict(prelude, "def f :: ref Box -> i64 -> i64 = \\b x -> b.run x").unwrap_err();
    assert_eq!(kind, Some("effect_unknown_callee"));
    assert!(
        message.contains("runs a function or task value that its caller can choose"),
        "{message}"
    );
}

#[test]
fn terminates_rejects_extern_calls() {
    let prelude = "extern \"abs\" def c_abs :: i32 -> i32\n";
    let (message, kind) = verdict(prelude, "def f :: i32 -> i32 = \\n -> c_abs n").unwrap_err();
    assert_eq!(kind, Some("effect_extern_in_attribute"));
    assert!(message.contains("reaches the extern function"), "{message}");
}
