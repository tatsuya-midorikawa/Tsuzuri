//! NS06: `if let` and `elif let` conditional pattern bindings across ordinary code,
//! computation expressions, `const def`, ownership/borrowing, `W1003`, and `tsuzuri fmt`.

use tsuzuri::formatter::format_source;
use tsuzuri::syntax::{Edition, SourceKind};
use tsuzuri::{analyze, analyze_with_edition, llvm};

#[test]
fn if_let_evaluates_and_moves_like_two_arm_match() {
    let if_let_src = r#"def greet :: own Maybe<string> -> string = \name ->
    if let Some text = name when text.length > 0 then
        $"Hello, {text}!"
    else
        "Hello!"
greet (Some "world")
"#;
    let match_src = r#"def greet :: own Maybe<string> -> string = \name ->
    match name with
    | Some text when text.length > 0 -> $"Hello, {text}!"
    | _ -> "Hello!"
greet (Some "world")
"#;
    for edition in [Edition::E2026, Edition::E2027] {
        let if_let_mod = analyze_with_edition(if_let_src, edition).unwrap();
        let match_mod = analyze_with_edition(match_src, edition).unwrap();
        let if_let_ir = llvm::emit(&if_let_mod, llvm::Entry::Library).unwrap();
        let match_ir = llvm::emit(&match_mod, llvm::Entry::Library).unwrap();
        assert_eq!(if_let_ir, match_ir, "{edition:?}");
        assert_eq!(
            llvm::emit_target(&if_let_mod, llvm::Entry::Library, true).unwrap(),
            llvm::emit_target(&match_mod, llvm::Entry::Library, true).unwrap(),
            "{edition:?}"
        );
    }

    // Shared borrow scrutinee binds read-only view; original value remains usable after, and
    // original owned value remains usable in `else` when `when` guard is false.
    let borrow_and_guard_else = r#"def inspect :: ref Maybe<string> -> i64 = \m ->
    if let Some text = deref m when text.length > 2 then
        text.length
    else
        0

def keep_on_failed_guard :: own Maybe<string> -> i64 = \m ->
    if let Some s = m when s.length > 10 then
        s.length
    else
        match m with
        | Some rest -> rest.length
        | None -> -1

let mut acc = 0
if let Some n = Some 5 then
    acc = acc + n
if let Some x = Some 7 {
    acc = acc + x
} else {
    acc = acc - 1
}
let u: (i64 | string) = "abc"
let tag_len = if let :? string as s = u then s.length else 0
let or_pat = if let Some a | Some a = Some 9 then a else 0
acc + inspect (ref (Some "hello")) + keep_on_failed_guard (Some "hi") + tag_len + or_pat
"#;
    for edition in [Edition::E2026, Edition::E2027] {
        analyze_with_edition(borrow_and_guard_else, edition)
            .unwrap_or_else(|e| panic!("{edition:?}: {}: {}", e.code, e.message));
    }

    // Moving a non-Copy pattern binding inside `when` guard is rejected just like `match`.
    let bad_guard_move = r#"def take :: own string -> bool = \_s -> true
def bad :: own Maybe<string> -> i64 = \m ->
    if let Some s = m when take s then s.length else 0
bad (Some "x")
"#;
    let err = analyze_with_edition(bad_guard_move, Edition::E2027).unwrap_err();
    assert!(
        matches!(err.code, "E1012" | "E1014"),
        "{}: {}",
        err.code,
        err.message
    );

    // Pattern bindings are not in scope in `else` or after `if let`.
    let leaked_in_else = "if let Some x = Some 1 then x else x\n";
    assert_eq!(analyze(leaked_in_else).unwrap_err().code, "E1002");
}

#[test]
fn elif_chains_mix_boolean_and_let_conditions() {
    let source = r#"def classify :: bool -> Maybe<i64> -> Result<string, i64> -> i64 = \flag m r ->
    let inline = if flag then 1 else if let Some z = m then z else 0
    let braced = if flag { 1 } else if let Some z = m { z } else { 0 }
    let chain =
        if flag then
            100
        elif let Some n = m when n > 0 then
            n * 10
        elif let Ok s = deref r then
            s.length
        elif let Error code = deref r then
            -code
        else
            0
    inline + braced + chain
classify false None (Ok "abcd")
"#;
    let module = analyze_with_edition(source, Edition::E2027).unwrap();
    assert!(module.warnings.is_empty(), "{:?}", module.warnings);
    llvm::emit(&module, llvm::Entry::Library).unwrap();
}

#[test]
fn if_let_works_in_computations_and_const_def() {
    let source = r#"const def unwrap_pos :: Maybe<i64> -> i64 = \m ->
    if let Some n = m when n > 0 then n * 2 else 1

const C1: i64 = unwrap_pos (Some 21)
const C2: i64 = unwrap_pos (Some (-3))

def maybe_chain :: Maybe<i64> -> Maybe<i64> = \m ->
    Maybe {
        if let Some x = m when x > 0 then
            let! y = Some (x + 1)
            return y
        else
            return 0
    }

def io_unit_chain :: Maybe<i64> -> IO<i64> = \m ->
    IO {
        if let Some x = m when x > 0 then
            do! IO.pure ()
        return 1
    }

def result_chain :: Result<i64, string> -> Result<i64, string> = \r ->
    Result {
        if let Ok v = r then
            return v + 10
        else
            return 0
    }

def io_chain :: own Maybe<string> -> IO<i64> = \m ->
    IO {
        if let Some s = m then
            do! IO.pure ()
            return s.length
        else
            return 0
    }

C1 + C2 + Maybe.default_value 0 (maybe_chain (Some 4)) + Result.default_value 0 (result_chain (Ok 5))
"#;
    for edition in [Edition::E2026, Edition::E2027] {
        let module = analyze_with_edition(source, edition)
            .unwrap_or_else(|e| panic!("{edition:?}: {}: {}", e.code, e.message));
        llvm::emit(&module, llvm::Entry::Console).unwrap();
        llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    }
}

#[test]
fn irrefutable_if_let_warns_w1003_and_formatter_is_idempotent() {
    let with_else = "let pair = (10, 20)\nif let (a, b) = pair then a + b else 0\n";
    let m1 = analyze_with_edition(with_else, Edition::E2027).unwrap();
    assert_eq!(m1.warnings.len(), 1, "{:?}", m1.warnings);
    assert_eq!(m1.warnings[0].code, "W1003");
    assert!(m1.warnings[0].message.contains("irrefutable 'if let'"));
    assert_eq!(m1.warnings[0].span.start, with_else.rfind("0").unwrap());

    let without_else =
        "let pair = (10, 20)\nlet mut s = 0\nif let (a, b) = pair then s = a + b\ns\n";
    let m2 = analyze_with_edition(without_else, Edition::E2027).unwrap();
    assert_eq!(m2.warnings.len(), 1, "{:?}", m2.warnings);
    assert_eq!(m2.warnings[0].code, "W1003");
    assert_eq!(
        m2.warnings[0].span.start,
        without_else.find("(a, b)").unwrap()
    );

    let formatted_sample = r#"def demo :: bool -> Maybe<i64> -> Maybe<string> -> i64 = \flag a b ->
    if flag then
        1
    elif let Some n = a when n > 0 then
        n
    elif let Some text = b then
        text.length
    else
        0
"#;
    let once = format_source("Main.tz", formatted_sample, SourceKind::Code).unwrap();
    assert!(
        once.formatted
            .contains("elif let Some n = a when n > 0 then")
    );
    assert!(once.formatted.contains("elif let Some text = b then"));
    let twice = format_source("Main.tz", &once.formatted, SourceKind::Code).unwrap();
    assert_eq!(once.formatted, twice.formatted);
}
