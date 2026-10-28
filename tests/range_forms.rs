//! A22 Phase 2: the range operators `..=` (closed) and `..<` (half-open) in `for` loops and
//! slices, in every edition. The values themselves are checked by the E2E suite `range_forms`
//! against a reference enumeration; these tests pin the syntax, the errors and the IR.

use tsuzuri::lexer::lex;
use tsuzuri::llvm::{self, Entry};
use tsuzuri::syntax::{Edition, TokenKind};
use tsuzuri::{analyze_with_edition, check::CheckedModule};

fn checked(source: &str, edition: Edition) -> CheckedModule {
    analyze_with_edition(source, edition)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn ir(source: &str, edition: Edition) -> String {
    llvm::emit(&checked(source, edition), Entry::Library).unwrap()
}

/// Whether some instruction of `ir` is `= OPERATION i64 ..., 1`.
fn by_one(ir: &str, operation: &str) -> bool {
    ir.lines()
        .any(|line| line.contains(&format!("= {operation} i64 ")) && line.ends_with(", 1"))
}

fn loop_over(range: &str) -> String {
    format!(
        "export def run :: i64 -> i64 -> i64 = \\a b ->\n    let mut t: i64 = 0\n    for i in {range} do\n        t = t * 31 + i\n    t\n"
    )
}

#[test]
fn half_open_and_closed_ranges_are_accepted_in_every_edition() {
    let source = "export def run :: i64 -> i64 = \\n ->\n    let xs = [1i64, 2, 3, 4]\n    let mut t: i64 = 0\n    \
                  for i in 0 ..< n do\n        t = t + i\n    \
                  for i in 0 ..= n do\n        t = t + i\n    \
                  for i in 0 .. 2 ..< n do\n        t = t + i\n    \
                  for i in n .. -1 ..= 0 do\n        t = t + i\n    \
                  let a = ref xs[1..=2]\n    let b = ref xs[..<2]\n    let c = ref xs[..=1]\n    \
                  t + a.length + b.length + c.length\n";
    for edition in Edition::SUPPORTED {
        checked(source, *edition);
    }
}

#[test]
fn bare_and_closed_ranges_have_identical_ir() {
    assert_eq!(
        ir(&loop_over("a .. b"), Edition::E2026),
        ir(&loop_over("a ..= b"), Edition::E2026)
    );
    assert_eq!(
        ir(&loop_over("a .. 3 .. b"), Edition::E2026),
        ir(&loop_over("a .. 3 ..= b"), Edition::E2026)
    );
    // The edition changes nothing that both accept.
    assert_eq!(
        ir(&loop_over("a ..= b"), Edition::E2026),
        ir(&loop_over("a ..= b"), Edition::E2027)
    );
}

#[test]
fn lexer_longest_match_for_range_operators() {
    let kinds = |text: &str| -> Vec<TokenKind> {
        lex(text, Edition::E2027)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .filter(|kind| *kind != TokenKind::End)
            .collect()
    };
    let name = |text: &str| TokenKind::Ident(text.into());
    assert_eq!(
        kinds("a..=b"),
        [name("a"), TokenKind::DotDotEqual, name("b")]
    );
    assert_eq!(
        kinds("a ..< b"),
        [name("a"), TokenKind::DotDotLess, name("b")]
    );
    assert_eq!(kinds("a..b"), [name("a"), TokenKind::DotDot, name("b")]);
    assert_eq!(
        kinds("1..<2"),
        [
            TokenKind::Integer("1".into()),
            TokenKind::DotDotLess,
            TokenKind::Integer("2".into())
        ]
    );
    assert_eq!(
        kinds("a.. =b"),
        [name("a"), TokenKind::DotDot, TokenKind::Equal, name("b")]
    );
}

#[test]
fn rejects_malformed_range_forms() {
    for (body, message) in [
        (
            "let s = ref xs[1..=]",
            "an inclusive slice needs an end bound; write 'ref xs[start..=end]', or 'ref xs[start..]' to slice to the end",
        ),
        (
            "let s = ref xs[..=]",
            "an inclusive slice needs an end bound; write 'ref xs[start..=end]', or 'ref xs[start..]' to slice to the end",
        ),
        (
            "let s = ref xs[1..<]",
            "a half-open slice with '..<' needs an end bound; write 'ref xs[start..<end]', or 'ref xs[start..]' to slice to the end",
        ),
        (
            "for i in 0 ..= 3 .. 6 do ()",
            "a range takes at most one step; write 'start .. step ..= finish'",
        ),
        (
            "for i in 0 ..< 3 ..= 6 do ()",
            "a range takes at most one step; write 'start .. step ..= finish'",
        ),
        (
            "for i in 0 .. 1 .. 3 ..= 6 do ()",
            "a range takes at most one step; write 'start .. step ..= finish'",
        ),
    ] {
        let source = format!("let xs = [1, 2, 3]\n{body}\n0\n");
        let error = analyze_with_edition(&source, Edition::E2026).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E0002", message),
            "{body}"
        );
    }
    // A slice still needs a bound, and a range is still only a `for` source.
    let error = analyze_with_edition("let r = 1 ..< 3\n0\n", Edition::E2026).unwrap_err();
    assert_eq!(error.code, "E1005");
}

#[test]
fn slice_forms_lower_as_documented() {
    let slice = |form: &str| {
        format!(
            "export def run :: i64 -> i64 -> i64 = \\a b ->\n    let xs = [10i64, 20, 30, 40, 50]\n    let s = ref xs[{form}]\n    s.length\n"
        )
    };
    assert_eq!(
        ir(&slice("a..b"), Edition::E2026),
        ir(&slice("a..<b"), Edition::E2026)
    );
    let inclusive = ir(&slice("a..=b"), Edition::E2026);
    assert!(by_one(&inclusive, "add"), "{inclusive}");
    // `..=` keeps the slice's two unsigned checks on the end `b + 1`.
    let half_open = ir(&slice("a..<b"), Edition::E2026);
    assert_eq!(
        inclusive.matches("icmp ule").count(),
        half_open.matches("icmp ule").count()
    );
}

#[test]
fn half_open_for_is_a_new_typed_loop() {
    let module = checked(&loop_over("a ..< b"), Edition::E2026);
    let run = module
        .functions
        .iter()
        .find(|function| function.name == "run")
        .unwrap();
    let mut found = Vec::new();
    let mut stack = vec![&run.body];
    while let Some(expression) = stack.pop() {
        if let tsuzuri::check::TypedExprKind::ForRange { exclusive, .. } = &expression.kind {
            found.push(*exclusive);
        }
        stack.extend(expression.children());
    }
    assert_eq!(found, [true]);
}

#[test]
fn half_open_for_ir_is_deterministic_and_closed_ir_is_unchanged() {
    let open = ir(&loop_over("a ..< b"), Edition::E2027);
    assert_eq!(open, ir(&loop_over("a ..< b"), Edition::E2027));
    // An empty half-open range skips the closed loop, whose end is one below `b`.
    assert!(open.contains("icmp slt i64"), "{open}");
    assert!(by_one(&open, "sub"), "{open}");
    let closed = ir(&loop_over("a ..= b"), Edition::E2027);
    assert!(closed.contains("icmp sle i64"), "{closed}");
    assert!(!by_one(&closed, "sub"), "{closed}");
}

#[test]
fn half_open_const_loops_fold() {
    let source = "const def triangle :: i64 -> i64 = \\n ->\n    let mut t: i64 = 0\n    for i in 0 ..< n do\n        t = t + i\n    t\n\
                  const def down :: i64 -> i64 = \\n ->\n    let mut t: i64 = 0\n    for i in n .. -1 ..< 0 do\n        t = t * 10 + i\n    t\n\
                  const A: i64 = triangle 101\nconst B: i64 = triangle 0\nconst C: i64 = down 3\n\
                  export def run :: i64 -> i64 = \\n -> A * 1000000 + B * 1000 + C + n\n";
    let text = ir(source, Edition::E2027);
    // 5050 (0 to 100), 0 (an empty range) and 321 (3 down to 1), folded at compile time.
    assert!(text.contains("mul i64 5050, 1000000"), "{text}");
    assert!(text.contains("mul i64 0, 1000"), "{text}");
    assert!(text.lines().any(|line| line.ends_with(", 321")), "{text}");
}
