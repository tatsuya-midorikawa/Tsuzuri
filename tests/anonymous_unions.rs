//! Anonymous unions `(A | B)` and type test patterns `:? T` (A24).

use tsuzuri::check::{CheckedModule, Type};
use tsuzuri::diagnostic::{Diagnostic, Edit, apply_edits};
use tsuzuri::driver::{Target, TestOptions, run_tests};
use tsuzuri::parser::{parse, parse_with_source};
use tsuzuri::syntax::{Edition, Program, TypeExpr, TypeExprKind};
use tsuzuri::{
    analyze, analyze_all, analyze_modules, analyze_modules_with_semantics, analyze_with_edition,
    llvm,
};

/// The type as written, with every anonymous union and its members spelled out, so that a test
/// reads the parse tree without the checker.
fn written(ty: &TypeExpr) -> String {
    let list = |types: &[TypeExpr], separator: &str| {
        types
            .iter()
            .map(written)
            .collect::<Vec<_>>()
            .join(separator)
    };
    match &ty.kind {
        TypeExprKind::Named(name) => name.clone(),
        TypeExprKind::Variable(name) => format!("'{name}"),
        TypeExprKind::Apply(head, arguments) => format!("{}<{}>", head.text, list(arguments, ", ")),
        TypeExprKind::AnonymousUnion(members) => format!("U({})", list(members, " | ")),
        TypeExprKind::Tuple(elements) => format!("T({})", list(elements, " * ")),
        TypeExprKind::Function(parameters, result) => {
            format!("F({} -> {})", list(parameters, " -> "), written(result))
        }
        TypeExprKind::Array(element) => format!("[{}]", written(element)),
        TypeExprKind::List(element) => format!("[|{}|]", written(element)),
        TypeExprKind::Reference(inner, mutable) => {
            format!(
                "ref{} {}",
                if *mutable { " mut" } else { "" },
                written(inner)
            )
        }
        TypeExprKind::Own(inner) => format!("own {}", written(inner)),
        other => format!("{other:?}"),
    }
}

fn parsed(source: &str) -> Program {
    parse(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn parse_error(source: &str) -> Diagnostic {
    parse(source).expect_err(source)
}

/// The written types of the first function's parameters and result.
fn signature(source: &str) -> Vec<String> {
    let program = parsed(source);
    let function = &program.functions[0];
    function
        .parameters
        .iter()
        .map(|parameter| written(&parameter.ty))
        .chain(std::iter::once(written(&function.result)))
        .collect()
}

#[test]
fn parses_anonymous_union_types() {
    assert_eq!(
        signature("def f :: (i64 | string) -> i64 = \\v -> 0\n"),
        ["U(i64 | string)", "i64"]
    );
    // Each member is a whole type: `->` and `*` bind tighter than `|`.
    assert_eq!(
        signature("def f :: (i64 -> i64 | string * bool) -> i64 = \\v -> 0\n"),
        ["U(F(i64 -> i64) | T(string * bool))", "i64"]
    );
    // Nested unions stay as written; the checker flattens them.
    assert_eq!(
        signature("def f :: (i64 | (string | bool)) -> Maybe<(i64 | f64)> = \\v -> None\n"),
        ["U(i64 | U(string | bool))", "Maybe<U(i64 | f64)>"]
    );
    assert_eq!(
        signature("def f :: own (i64 | [string]) -> [(i64 | string)] = \\v -> []\n"),
        ["own U(i64 | [string])", "[U(i64 | string)]"]
    );
    // A parenthesized single type is still only grouping.
    assert_eq!(
        signature("def f :: (i64) -> i64 = \\v -> v\n"),
        ["i64", "i64"]
    );
    // Lines may break inside the parentheses, with `|` at the start of a line.
    let program = parsed("type Value =\n    (i64\n    | f64\n    | string)\n");
    assert_eq!(
        written(&program.type_aliases[0].target),
        "U(i64 | f64 | string)"
    );
    assert!(program.uses_anonymous_unions);
    // Record fields, union payloads and type arguments take them too.
    let program = parsed(
        "record Cell { value: (i64 | string) }\nunion Shape = Dot | Text of (string | char)\n",
    );
    assert!(program.uses_anonymous_unions);
    assert_eq!(written(&program.records[0].fields[0].ty), "U(i64 | string)");
}

#[test]
fn flags_only_programs_that_write_the_syntax() {
    let program = parsed("def f :: (i64) -> i64 = \\v -> v\nlet x: i64 = 1\nx |> f\n");
    assert!(!program.uses_anonymous_unions);
    let program = parsed("union Shape = Dot | Line of (i64 * i64)\nlet x = 1\nx\n");
    assert!(!program.uses_anonymous_unions);
    // A use inside a body counts: the declarations behind it serve the whole program.
    let program = parsed("def f :: i64 -> i64 = \\x ->\n    let v: (i64 | string) = x\n    0\n");
    assert!(program.uses_anonymous_unions);
    let program =
        parsed("def f :: i64 -> i64 = \\x ->\n    match x with\n    | :? i64 -> 0\n    | _ -> 1\n");
    assert!(program.uses_anonymous_unions);
}

/// The debug text of the program's parse tree, where a test finds the patterns that it wrote.
fn tree(source: &str) -> String {
    format!("{:?}", parsed(source))
}

/// Asserts that `source` has a type test whose type's kind starts with `kind` and whose binder is
/// `binder`. In the debug text, the binder follows the end of the tested type: the first
/// `source: None } }, ` that a binder follows (the members of a tuple are followed by another type).
fn assert_type_test(source: &str, kind: &str, binder: Option<&str>) {
    let tree = tree(source);
    let start = format!("TypeTest(TypeExpr {{ kind: {kind}");
    let found = tree.match_indices(&start).any(|(at, _)| {
        let rest = &tree[at..];
        let named = rest.find("source: None } }, Some(Ident { text: \"");
        let none = rest.find("source: None } }, None)");
        match (binder, named, none) {
            (Some(name), Some(named), none) if none.is_none_or(|none| named < none) => rest
                [named..]
                .strip_prefix("source: None } }, Some(Ident { text: \"")
                .is_some_and(|text| text.starts_with(&format!("{name}\""))),
            (None, named, Some(none)) => named.is_none_or(|named| none < named),
            _ => false,
        }
    });
    assert!(
        found,
        "{source}\nno {start} with binder {binder:?} in\n{tree}"
    );
}

#[test]
fn parses_type_test_patterns() {
    let source = "def describe :: (i64 | string) -> i64 = \\value ->\n    match value with\n    | :? i64 as n -> n\n    | :? string -> 0\n";
    assert_type_test(source, "Named(\"i64\")", Some("n"));
    assert_type_test(source, "Named(\"string\")", None);
    assert!(parsed(source).uses_anonymous_unions);
    // The binder `_` stays a name; the checker treats it as a wildcard.
    assert_type_test(
        "def f :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as _ -> 1\n    | _ -> 0\n",
        "Named(\"i64\")",
        Some("_"),
    );
    // Two type tests in one arm are an OR pattern.
    let text = tree(
        "def f :: (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 | :? string -> 1\n    | _ -> 0\n",
    );
    assert!(text.contains("Or(Pattern { kind: TypeTest("), "{text}");
    // The type is a product type: a tuple without parentheses, generic types with `<`.
    assert_type_test(
        "def f :: (i64 * string | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 * string as p -> 1\n    | _ -> 0\n",
        "Tuple([",
        Some("p"),
    );
    // A function guard whose arm holds a `<` is still a pattern, also inside parentheses.
    let guard = "def f :: (Maybe<i64> | string) -> i64 = \\v ->\n    | :? Maybe<i64> as m -> 1\n    | :? string -> 0\n";
    assert_type_test(guard, "Apply(Ident { text: \"Maybe\"", Some("m"));
    let guard = "def f :: (Maybe<i64> | string) -> i64 = \\v ->\n    | (:? Maybe<i64> as m) -> 1\n    | (:? string) -> 0\n";
    assert_type_test(guard, "Apply(Ident { text: \"Maybe\"", Some("m"));
    // A group whose type test holds a `<` is no active pattern argument expression.
    let grouped = "def f :: ((Maybe<i64> | string) * i64) -> i64 = \\v ->\n    match v with\n    | ((:? Maybe<i64> as m), y) -> y\n    | (_, y) -> y\n";
    assert_type_test(grouped, "Apply(Ident { text: \"Maybe\"", Some("m"));
    // Inside a case pattern and a lambda parameter, and as the pattern of `for`.
    assert_type_test(
        "def f :: Maybe<(i64 | string)> -> i64 = \\v ->\n    match v with\n    | Some (:? i64 as n) -> n\n    | _ -> 0\n",
        "Named(\"i64\")",
        Some("n"),
    );
    assert_type_test(
        "def f :: (i64 | string) -> i64 = \\(:? i64 as n) -> n\n",
        "Named(\"i64\")",
        Some("n"),
    );
    assert_type_test(
        "def f :: (i64 | string) -> i64 = \\:? i64 as n -> n\n",
        "Named(\"i64\")",
        Some("n"),
    );
    assert_type_test(
        "def f :: [(i64 | string)] -> i64 = \\xs ->\n    let mut total = 0\n    for :? i64 as n in xs do\n        total <- total + n\n    total\n",
        "Named(\"i64\")",
        Some("n"),
    );
    // `as` after the parentheses binds the whole value, as for any pattern.
    let text = tree(
        "def f :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | (:? i64 as n) as whole -> n\n    | _ -> 0\n",
    );
    assert!(text.contains("As(Pattern { kind: TypeTest("), "{text}");
}

#[test]
fn rejects_double_bar() {
    let error = parse_error("let v: (i64 || string) = 1\nv\n");
    assert_eq!(error.code, "E0002");
    assert_eq!(error.kind, Some("anonymous_union_double_bar"));
    assert_eq!(
        error.message,
        "use a single '|' between the members of an anonymous union, as in (i64 | string)"
    );
    // The span is the `||`.
    assert_eq!((error.span.start, error.span.end), (12, 14));
    let error = parse_error("let v: (i64 | string || bool) = 1\nv\n");
    assert_eq!(error.kind, Some("anonymous_union_double_bar"));
    assert_eq!((error.span.start, error.span.end), (21, 23));
}

#[test]
fn keeps_existing_errors_for_other_bars() {
    for (source, kind, message, start) in [
        (
            "let v: (| i64 | string) = 1\nv\n",
            "expected_identifier",
            "expected an identifier",
            8,
        ),
        (
            "let v: (i64 |> string) = 1\nv\n",
            "expected_token",
            "expected ')' after the type",
            12,
        ),
        (
            "let v: (i64 ||| string) = 1\nv\n",
            "expected_token",
            "expected ')' after the type",
            12,
        ),
        (
            "def f :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | : ? i64 -> 0\n    | _ -> 1\n",
            "expected_pattern",
            "expected a pattern",
            62,
        ),
    ] {
        let error = parse_error(source);
        assert_eq!(error.code, "E0002", "{source}");
        assert_eq!(error.kind, Some(kind), "{source}");
        assert_eq!(error.message, message, "{source}");
        assert_eq!(error.span.start, start, "{source}");
    }
}

#[test]
fn limits_parsed_member_count() {
    // 128 members parse (the checker allows 32 after flattening); the 129th is an error.
    let members = |count: usize| {
        (0..count)
            .map(|index| format!("T{index}"))
            .collect::<Vec<_>>()
            .join(" | ")
    };
    parsed(&format!("type U = ({})\n", members(128)));
    let error = parse_error(&format!("type U = ({})\n", members(129)));
    assert_eq!(error.code, "E0002");
    assert_eq!(error.kind, Some("too_many_anonymous_union_members"));
    assert_eq!(error.message, "too many members in an anonymous union");
}

#[test]
fn parses_in_both_editions() {
    let source = "def f :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string -> 0\n";
    for edition in [Edition::E2026, Edition::E2027] {
        let program = parse_with_source(source, 0, edition)
            .unwrap_or_else(|error| panic!("{edition:?}: {}", error.message));
        assert!(program.uses_anonymous_unions, "{edition:?}");
        assert_eq!(
            written(&program.functions[0].parameters[0].ty),
            "U(i64 | string)"
        );
        let tree = format!("{program:?}");
        assert_eq!(tree.matches("TypeTest(").count(), 2, "{edition:?}");
    }
}

// Step 3: the declarations behind anonymous unions.

#[test]
fn programs_without_the_syntax_synthesize_nothing() {
    for source in [
        "def f :: i64 -> i64 = \\x -> x\n",
        "union Shape = Dot | Line of (i64 * i64)\ndef f :: (i64) -> i64 = \\x -> x\n",
    ] {
        let module = analyze(source).unwrap();
        assert!(
            module.unions.iter().all(|union| !union.is_anonymous()),
            "{source}"
        );
        assert!(
            module.unions.iter().all(|union| !union.name.contains('$')),
            "{source}"
        );
    }
}

// Step 3 and 4: the synthesized declarations, and the types that the checker makes of the syntax.

/// Checks `source` (edition 2026, as `analyze`), emits it twice to check determinism and checks
/// its symbols; returns the native IR.
fn accepts(source: &str) -> String {
    accepts_modules(&[("Main", source)])
}

fn accepts_modules(sources: &[(&str, &str)]) -> String {
    let module = analyze_modules(sources)
        .unwrap_or_else(|error| panic!("{sources:?}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    assert_identifier_symbols(&ir);
    ir
}

/// Fails for an unquoted global name of `ir` that is not an LLVM identifier (A29), as
/// `tests/dyn_dispatch.rs` checks: the synthesized names hold `$`, which identifiers allow.
fn assert_identifier_symbols(ir: &str) {
    let starts = |byte: u8| byte.is_ascii_alphabetic() || b"-$._".contains(&byte);
    let continues = |byte: u8| starts(byte) || byte.is_ascii_digit();
    for line in ir.lines() {
        let bytes = line.as_bytes();
        let mut quoted = false;
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'"' => quoted = !quoted,
                b';' if !quoted => break,
                b'@' if !quoted && bytes.get(at + 1) != Some(&b'"') => {
                    let end = bytes[at + 1..]
                        .iter()
                        .position(|byte| {
                            byte.is_ascii_whitespace() || b"(),[]{}<>\"".contains(byte)
                        })
                        .map_or(bytes.len(), |length| at + 1 + length);
                    let name = &bytes[at + 1..end];
                    let numbered = !name.is_empty() && name.iter().all(u8::is_ascii_digit);
                    let identifier = name.first().is_some_and(|&byte| starts(byte))
                        && name.iter().all(|&byte| continues(byte));
                    assert!(numbered || identifier, "not an LLVM identifier:\n{line}");
                    at = end;
                    continue;
                }
                _ => {}
            }
            at += 1;
        }
    }
}

fn rejects_modules(sources: &[(&str, &str)], code: &str, kind: &str) -> Diagnostic {
    let error = analyze_modules(sources).expect_err(&format!("{sources:?}"));
    assert_eq!(error.code, code, "{sources:?}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{sources:?}\n{}", error.message);
    error
}

fn rejects(source: &str, code: &str, kind: &str) -> Diagnostic {
    rejects_modules(&[("Main", source)], code, kind)
}

const PLAIN: &str = "def f :: i64 -> i64 = \\x -> x\n";

#[test]
fn synthesizes_declarations_only_when_used() {
    let plain = analyze(PLAIN).unwrap();
    assert!(
        plain
            .unions
            .iter()
            .all(|union| !union.name.starts_with('$'))
    );
    let used = analyze("def f :: (i64 | string) -> (string | i64) = \\x -> x\n").unwrap();
    assert_eq!(used.unions.len(), plain.unions.len() + 31);
    // After the user and std unions: `$anonymous.Union$n` for n = 2 to 32, with n type parameters
    // and n cases whose payloads are the parameters in order.
    let synthesized = &used.unions[plain.unions.len()..];
    for (index, union) in synthesized.iter().enumerate() {
        let members = index + 2;
        assert_eq!(union.name, format!("$anonymous.Union${members}"));
        assert!(union.is_anonymous(), "{}", union.name);
        assert_eq!(union.origin, tsuzuri::check::ModuleOrigin::Std);
        assert_eq!(union.parameters.len(), members);
        assert_eq!(union.cases.len(), members);
        for (case, (name, payload)) in union.cases.iter().enumerate() {
            assert_eq!(name, &format!("Union${members}${case}"));
            assert_eq!(
                payload,
                &Some(Type::Variable(format!("$t{case}"))),
                "{}",
                union.name
            );
        }
    }
    assert!(plain.unions.iter().all(|union| !union.is_anonymous()));
    // A use in a function body only, or in a type test, also synthesizes them.
    let body =
        analyze("def f :: i64 -> i64 = \\x ->\n    let g = \\(v: (i64 | string)) -> 0\n    x\n")
            .unwrap();
    assert_eq!(body.unions.len(), plain.unions.len() + 31);
}

#[test]
fn members_commute_and_flatten() {
    accepts("def f :: (i64 | string) -> (string | i64) = \\v -> v\n");
    accepts("def f :: (i64 | (string | bool)) -> ((i64 | string) | bool) = \\v -> v\n");
    accepts("def f :: (i64 | (string | bool)) -> (bool | string | i64) = \\v -> v\n");
    // Through an alias, and nested in another type.
    accepts(
        "type Text = (string | bool)\ndef f :: (i64 | Text) -> (bool | i64 | string) = \\v -> v\n",
    );
    accepts(
        "def f :: Maybe<(i64 | string)> -> Maybe<(string | i64)> = \\v -> v\ndef g :: [(f64 | bool)] -> [(bool | f64)] = \\v -> v\n",
    );
    // The same type in every module.
    accepts_modules(&[
        (
            "Main",
            "def g :: (i64 | string) -> (i64 | string) = \\v -> Other.id v\n",
        ),
        (
            "Other",
            "def id :: (string | i64) -> (string | i64) = \\v -> v\n",
        ),
    ]);
    // Different members make different types.
    rejects(
        "def f :: (i64 | string) -> (i64 | bool) = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
    rejects(
        "def f :: (i64 | string) -> (i64 | string | bool) = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
}

#[test]
fn displays_in_canonical_order() {
    // The members are ordered by the rank of their heads (integers, floats, bool, unit, char,
    // string, records, unions, arrays, tuples, functions), integers by width then signedness.
    let error = rejects(
        "def f :: (string | (i64 -> i64) | (i64 * i64) | [i64] | Maybe<i64> | char | unit | bool | f64 | i32 | i32u | i64) -> i64 = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(
        error.message,
        "expected i64, found (i32u | i32 | i64 | f64 | bool | unit | char | string | Maybe<i64> | [i64] | (i64 * i64) | (i64 -> i64))"
    );
    let error = rejects(
        "def f :: (string | i64) -> (bool | i64) = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected (i64 | bool), found (i64 | string)");
    // Records and unions by their qualified names, not by the order in which modules come.
    let error = rejects_modules(
        &[
            (
                "Main",
                "record A { x: i64 }\ndef f :: (A | Alpha.B) -> i64 = \\v -> v\n",
            ),
            ("Alpha", "record B { x: i64 }\n"),
        ],
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected i64, found (Alpha.B | Main.A)");
}

#[test]
fn rejects_type_variable_members() {
    let error = rejects(
        "def f :: ('a | i64) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_type_variable",
    );
    assert_eq!(
        error.message,
        "a type variable cannot be a member of an anonymous union: ('a | i64); members must differ without knowing type arguments"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("use a union with named cases, such as union OrInt<'a> = Value of 'a | Int of i64")
    );
    rejects(
        "type OrInt<'a> = ('a | i64)\ndef f :: OrInt<string> -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_type_variable",
    );
    // A member whose head is no type variable is fine.
    accepts("def f :: (Maybe<'a> | i64) -> i64 = \\v -> 0\n");
}

#[test]
fn rejects_indistinct_members() {
    let error = rejects(
        "def f :: (Maybe<'a> | Maybe<i64>) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_indistinct_members",
    );
    assert_eq!(
        error.message,
        "Maybe<'a> and Maybe<i64> may be the same type for some type arguments, so they cannot both be members of an anonymous union"
    );
    rejects(
        "def f :: (('a * i64) | (bool * 'b)) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_indistinct_members",
    );
    accepts("def f :: (Maybe<i64> | Maybe<string>) -> i64 = \\v -> 0\n");
    // Decided before the variable: the lengths of the tuples differ.
    accepts("def f :: (('a * i64) | (bool * 'a * i64)) -> i64 = \\v -> 0\n");
}

#[test]
fn rejects_duplicate_members_through_aliases() {
    let error = rejects(
        "def f :: (i64 | i64) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_duplicate_member",
    );
    assert_eq!(
        error.message,
        "i64 appears more than once in the anonymous union (i64 | i64)"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("a type alias names the same type as its definition")
    );
    rejects(
        "type Meters = f64\ndef f :: (Meters | f64) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_duplicate_member",
    );
    // Flattening may repeat a member.
    let error = rejects(
        "type Pair = (string | i64)\ndef f :: (i64 | Pair) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_duplicate_member",
    );
    assert_eq!(
        error.message,
        "i64 appears more than once in the anonymous union (i64 | i64 | string)"
    );
}

#[test]
fn function_members_ignore_conventions_and_currying() {
    // An impure curried function type is the same type however it is grouped.
    rejects(
        "def f :: ((i64 -> i64 -> i64) | (i64 -> (i64 -> i64))) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_duplicate_member",
    );
    // Purity, a pure result, the parameter count and a borrowed parameter make other types.
    accepts("def f :: ((i64 -> i64) | pure (i64 -> i64)) -> i64 = \\v -> 0\n");
    accepts("def f :: ((i64 -> pure (i64 -> i64)) | (i64 -> i64 -> i64)) -> i64 = \\v -> 0\n");
    accepts("def f :: ((i64 -> i64) | (i64 -> i64 -> i64)) -> i64 = \\v -> 0\n");
    accepts("def f :: ((ref i64 -> i64) | (i64 -> i64)) -> i64 = \\v -> 0\n");
    let error = rejects(
        "def f :: ((i64 -> i64) | pure (i64 -> i64) | (fn() -> i64)) -> i64 = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(
        error.message,
        "expected i64, found ((fn() -> i64) | (i64 -> i64) | pure (i64 -> i64))"
    );
}

#[test]
fn rejects_borrowed_members() {
    let error = rejects(
        "def f :: (ref string | i64) -> i64 = \\v -> 0\n",
        "E1013",
        "anonymous_union_borrowed_member",
    );
    assert_eq!(
        error.message,
        "an anonymous union holds owned values; ref string is borrowed"
    );
    rejects(
        "def f :: ((ref i64 * i64) | bool) -> i64 = \\v -> 0\n",
        "E1013",
        "anonymous_union_borrowed_member",
    );
}

/// `[[...[i64]...]]` with `depth` brackets: distinct types for as many members as a test needs.
fn nested_array(depth: usize) -> String {
    format!("{}i64{}", "[".repeat(depth), "]".repeat(depth))
}

fn members(range: std::ops::Range<usize>) -> String {
    range.map(nested_array).collect::<Vec<_>>().join(" | ")
}

#[test]
fn limits_member_count() {
    accepts(&format!(
        "def f :: ({}) -> i64 = \\v -> 0\n",
        members(0..32)
    ));
    let error = rejects(
        &format!("def f :: ({}) -> i64 = \\v -> 0\n", members(0..33)),
        "E1017",
        "anonymous_union_member_limit",
    );
    assert_eq!(
        error.message,
        "an anonymous union has at most 32 members, but this one has 33; declare a union with named cases instead"
    );
    // The limit counts the members after flattening.
    rejects(
        &format!(
            "def f :: ({} | ({})) -> i64 = \\v -> 0\n",
            members(0..20),
            members(20..34)
        ),
        "E1017",
        "anonymous_union_member_limit",
    );
}

#[test]
fn generic_members_stay_ordered_after_instantiation() {
    let ir = accepts(
        "def f :: (Maybe<'a> | i64) -> (i64 | Maybe<'a>) = \\v -> v\n\
         def g :: (Maybe<string> | i64) -> (i64 | Maybe<string>) = \\v -> f v\n\
         def h :: (i64 | Maybe<(f64 | bool)>) -> (Maybe<(bool | f64)> | i64) = \\v -> f v\n",
    );
    assert!(ir.contains("$anonymous.Union$2"), "{ir}");
    // An instance with a member type is another type than the generic one with a different member.
    rejects(
        "def f :: (Maybe<'a> | i64) -> (i64 | Maybe<'a>) = \\v -> v\n\
         def g :: (Maybe<string> | i64) -> (i64 | Maybe<bool>) = \\v -> f v\n",
        "E1003",
        "type_mismatch",
    );
}

#[test]
fn aliases_inside_members_expand() {
    // An alias in a member of another module's alias resolves in that module, not in the module
    // that uses the alias, where `Meters` is another type.
    accepts_modules(&[
        (
            "Main",
            "type Meters = i64\ndef f :: Units.Value -> (string | f64) = \\v -> v\n",
        ),
        (
            "Units",
            "type Meters = f64\ntype Value = (Meters | string)\n",
        ),
    ]);
    accepts("type Meters = f64\ndef f :: (Meters | string) -> (string | f64) = \\v -> v\n");
    accepts(
        "type Name<'a> = Maybe<'a>\ndef f :: (Name<string> | i64) -> (i64 | Maybe<string>) = \\v -> v\n",
    );
}

#[test]
fn inline_constraints_inside_members_are_kept() {
    accepts(
        "def f :: (Maybe<Add<'a>> | i64) -> i64 = \\v -> 0\ndef g :: (Maybe<string> | i64) -> i64 = \\v -> f v\n",
    );
    rejects(
        "def f :: (Maybe<Add<'a>> | i64) -> i64 = \\v -> 0\ndef g :: (Maybe<bool> | i64) -> i64 = \\v -> f v\n",
        "E1005",
        "no_instance",
    );
}

// Step 5: values injected where an anonymous union is expected.

/// Checks `source` in `edition`, which decides the parameter conventions (`analyze` is 2026).
fn accepts_in(source: &str, edition: Edition) -> CheckedModule {
    analyze_with_edition(source, edition)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn rejects_in(source: &str, edition: Edition, code: &str, kind: &str) -> Diagnostic {
    let error = analyze_with_edition(source, edition).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
    error
}

/// How often a test runs before a timeout counts, as in `tests/array_fsharp.rs`.
const ATTEMPTS: usize = 3;

/// Each test of `module` with its failure, run for `target` at `optimization`; a test that timed
/// out runs again, up to `ATTEMPTS` times in all.
fn results(
    module: &CheckedModule,
    target: Target,
    optimization: u8,
) -> Vec<(String, Option<String>)> {
    let mut results = Vec::new();
    for _ in 0..ATTEMPTS {
        results = run_tests(
            module,
            &TestOptions {
                target,
                optimization,
                ..Default::default()
            },
        )
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message))
        .results
        .into_iter()
        .map(|result| (result.case.name, result.failure))
        .collect();
        if !results.iter().any(|(_, failure)| {
            failure
                .as_deref()
                .is_some_and(|reason| reason.starts_with("timed out"))
        }) {
            break;
        }
    }
    results
}

/// Runs the tests of `source` (checked in `edition`) natively and on WASM at -O0 and -O3, and
/// fails for any test that does not pass.
fn passes(source: &str, edition: Edition) {
    let module = accepts_in(source, edition);
    let expected = source
        .lines()
        .filter(|line| line.starts_with("test \""))
        .count();
    for target in [Target::Native, Target::Wasm32] {
        for optimization in [0, 3] {
            let results = results(&module, target, optimization);
            assert_eq!(results.len(), expected, "{target:?} -O{optimization}");
            let failed: Vec<_> = results
                .iter()
                .filter_map(|(name, failure)| {
                    failure.as_ref().map(|reason| format!("{name}: {reason}"))
                })
                .collect();
            assert!(
                failed.is_empty(),
                "{target:?} -O{optimization}: {failed:?}\n{source}"
            );
        }
    }
}

/// Runs the tests of `source` natively at -O0 and -O3 and fails for any test that does not trap.
fn traps(source: &str, edition: Edition) {
    let module = accepts_in(source, edition);
    for optimization in [0, 3] {
        let results = results(&module, Target::Native, optimization);
        assert!(!results.is_empty());
        for (name, failure) in results {
            let reason = failure.unwrap_or_else(|| panic!("-O{optimization}: {name} did not trap"));
            assert!(
                !reason.starts_with("timed out"),
                "-O{optimization}: {name}: {reason}"
            );
        }
    }
}

/// `count`: a number for every member of `(i64 | string | bool)`, to read a value back.
const COUNT: &str = "def count :: (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n    | :? bool as b -> if b then 1 else 0\n";

#[test]
fn injects_at_annotated_bindings() {
    // The binding has the union type, in any member order.
    let error = rejects(
        "let v: (i64 | string) = 42\nlet x: bool = v\nx\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected bool, found (i64 | string)");
    passes(
        &format!(
            "{COUNT}test \"bindings\" =\n    let v: (i64 | string | bool) = 42\n    let w: (bool | string | i64) = v\n    let t: (string | bool | i64) = \"abc\"\n    let b: (bool | i64 | string) = true\n    let n: (i64 | string | bool) = -7\n    assert (count w == 42)\n    assert (count t == 3)\n    assert (count b == 1)\n    assert (count n == -7)\n"
        ),
        Edition::E2026,
    );
    let error = rejects(
        "let v: (i64 | string) = true\n0\n",
        "E1003",
        "anonymous_union_not_member",
    );
    assert_eq!(
        error.message,
        "expected (i64 | string), found bool; bool is not a member of the anonymous union"
    );
    assert_eq!(error.help.as_deref(), Some("the members are i64, string"));
    assert_eq!(error.detail.expected.as_deref(), Some("(i64 | string)"));
    assert_eq!(error.detail.found.as_deref(), Some("bool"));
    // A suffixed literal has its type: `16i32` is no member of `(i64 | string)`.
    let error = rejects(
        "let v: (i64 | string) = 16i32\n0\n",
        "E1003",
        "anonymous_union_not_member",
    );
    assert_eq!(
        error.message,
        "expected (i64 | string), found i32; i32 is not a member of the anonymous union"
    );
    // A number literal without a number member is named by its default type.
    let error = rejects(
        "let v: (bool | string) = 3\n0\n",
        "E1003",
        "anonymous_union_not_member",
    );
    assert_eq!(
        error.message,
        "expected (bool | string), found i32; i32 is not a member of the anonymous union"
    );
}

#[test]
fn injects_branches() {
    passes(
        &format!(
            "{COUNT}def pick :: i64 -> (i64 | string | bool) = \\x ->\n    if x > 0 then x else if x == 0 then \"zero\" else true\n\ndef choose :: i64 -> (string | bool | i64) = \\x ->\n    match x with\n    | 1 -> \"one\"\n    | 2 ->\n        let y = x * 10\n        y\n    | _ -> false\n\ntest \"branches\" =\n    assert (count (pick 5) == 5)\n    assert (count (pick 0) == 4)\n    assert (count (pick (-3)) == 1)\n    assert (count (choose 1) == 3)\n    assert (count (choose 2) == 20)\n    assert (count (choose 9) == 0)\n    let v: (i64 | string | bool) = if count (pick 1) > 0 then 1 else \"x\"\n    assert (count v == 1)\n"
        ),
        Edition::E2026,
    );
}

/// `measure`: `count` for `(i64 | string)` with an owned parameter in every edition.
const MEASURE: &str = "def measure :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n";

#[test]
fn injects_call_arguments_and_pipes() {
    // 1 + 1 + 4 + (-2) + 30 + 3 = 37, by hand.
    let program = format!(
        "{MEASURE}test \"arguments and pipes\" =\n    let name = \"abcd\"\n    let total = (\"x\" |> measure) + (1 |> measure) + (name |> measure) + (-2 |> measure) + measure 30 + measure \"abc\"\n    assert (total == 37)\n"
    );
    passes(&program, Edition::E2026);
    passes(&program, Edition::E2027);
    // A read parameter (2027) takes the union as a temporary, from a pipe as from an argument.
    let read = "def size :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n";
    passes(
        &format!(
            "{read}test \"read\" =\n    let name = \"abcd\"\n    let total = (\"x\" |> size) + (1 |> size) + (name |> size) + size 30 + size 1\n    assert (total == 37)\n"
        ),
        Edition::E2027,
    );
    // The piped value is checked before the parameter is known; a literal is still read as
    // its member, and two integer members are still ambiguous.
    passes(
        "def half :: own (f64 | string) -> f64 = \\v ->\n    match v with\n    | :? f64 as x -> x / 2.0\n    | :? string -> 0.0\n\ntest \"float\" =\n    assert ((3 |> half) == 1.5)\n    assert ((-3 |> half) == -1.5)\n    assert ((5.0 |> half) == 2.5)\n",
        Edition::E2026,
    );
    let error = rejects(
        "def f :: own (i32 | i64) -> i64 = \\v -> 0\nlet x = 16 |> f\nx\n",
        "E1041",
        "anonymous_union_ambiguous_literal",
    );
    assert_eq!(
        error.message,
        "the literal 16 could be i32 or i64 in (i32 | i64)"
    );
    // A value of an undetermined type is the union itself: the lambda's parameter here.
    passes(
        &format!(
            "{MEASURE}test \"open\" =\n    let forward = \\x -> measure x\n    let piped = \\x -> x |> measure\n    assert (forward \"abc\" + forward 1 + piped 2 + piped \"z\" == 7)\n"
        ),
        Edition::E2026,
    );
}

#[test]
fn injects_read_parameters() {
    // A read parameter (2027) of a union type is `ref (i64 | string)`: the injected value is a
    // temporary that the call borrows, as `f (Some x)`.
    let read = "def size :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n";
    passes(
        &format!(
            "{read}def pick :: own bool -> i64 = \\c -> size (if c then 40 else \"ab\")\n\ntest \"read\" =\n    assert (size 41 == 41)\n    assert (size \"abc\" == 3)\n    assert (pick true + pick false == 42)\n    let owned = \"xyz\"\n    assert (size owned == 3)\n"
        ),
        Edition::E2027,
    );
}

#[test]
fn injects_array_elements() {
    passes(
        &format!(
            "{COUNT}test \"elements\" =\n    let items: [(i64 | string | bool)] = [1, \"two\", true, -4]\n    let mut total = 0\n    for item in items do\n        total = total + (match item with\n            | :? i64 as n -> n\n            | :? string as s -> s.length\n            | :? bool as b -> if b then 1 else 0)\n    assert (total == 1)\n    let list: [|(i64 | string)|] = [|5, \"abc\"|]\n    assert (List.length list == 2)\n    let wrapped: Maybe<(i64 | string)> = Some 1\n    let pair: ((i64 | string) * (bool | string)) = (2, \"x\")\n    assert (match wrapped with\n        | Some (:? i64 as n) -> n == 1\n        | _ -> false)\n    assert (match pair with\n        | (:? i64 as n, :? string as s) -> n + s.length == 3\n        | _ -> false)\n"
        ),
        Edition::E2026,
    );
}

#[test]
fn injects_records_and_generic_constructors() {
    passes(
        "record Pair<'a> { left: 'a, right: i64 }\nrecord Cell { value: (i64 | string) }\nunion Shape = Dot | Text of (string | char)\n\ntest \"constructors\" =\n    let none: (Maybe<i64> | string) = None\n    let some: (Maybe<i64> | Maybe<string>) = Some 1\n    let text: (Maybe<i64> | Maybe<string>) = Some \"abc\"\n    let pair: (Pair<string> | i64) = Pair { left: \"x\", right: 2 }\n    let cell = Cell { value: 40 }\n    let shape = Text 'c'\n    assert (match none with\n        | :? Maybe<i64> as m -> (match m with | Some _ -> false | None -> true)\n        | :? string -> false)\n    assert (match some with\n        | :? Maybe<i64> as m -> (match m with | Some n -> n == 1 | None -> false)\n        | :? Maybe<string> -> false)\n    assert (match text with\n        | :? Maybe<string> as m -> (match m with | Some s -> s.length == 3 | None -> false)\n        | :? Maybe<i64> -> false)\n    assert (match (pair, cell.value) with\n        | (:? Pair<string> as p, :? i64 as n) -> p.right + n == 42\n        | _ -> false)\n    assert (match shape with\n        | Text (:? char) -> true\n        | _ -> false)\n",
        Edition::E2026,
    );
    // `None` may be either `Maybe`.
    let error = rejects(
        "let v: (Maybe<i64> | Maybe<string>) = None\n0\n",
        "E1041",
        "anonymous_union_ambiguous_member",
    );
    assert_eq!(
        error.message,
        "this value of type Maybe<an undetermined type> could be Maybe<i64> or Maybe<string> in (Maybe<i64> | Maybe<string>)"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("annotate the value, for example with a typed let binding")
    );
    // A number expression that is no literal may be either integer.
    let error = rejects(
        "let v: (i32 | i64 | string) = 1 + 2\n0\n",
        "E1041",
        "anonymous_union_ambiguous_member",
    );
    assert_eq!(
        error.message,
        "this number could be i32 or i64 in (i32 | i64 | string)"
    );
}

#[test]
fn literal_rules() {
    passes(
        "test \"literals\" =\n    let float: (f64 | string) = 2\n    let negative: (f64 | string) = -2\n    let integer: (i64 | f64) = 2\n    let fraction: (i64 | f64) = 2.5\n    let small: (i8u | string) = 255\n    assert (match float with | :? f64 as x -> x == 2.0 | :? string -> false)\n    assert (match negative with | :? f64 as x -> x == -2.0 | :? string -> false)\n    assert (match integer with | :? i64 as n -> n == 2 | :? f64 -> false)\n    assert (match fraction with | :? f64 as x -> x == 2.5 | :? i64 -> false)\n    assert (match small with | :? i8u as n -> n == 255i8u | :? string -> false)\n",
        Edition::E2026,
    );
    // A bigint member takes an integer literal when no integer member does.
    passes(
        "test \"bigint\" =\n    let big: (bigint | string) = 5\n    assert (match big with | :? bigint as b -> (match BigInt.to_i64 (ref b) with | Some n -> n == 5 | None -> false) | :? string -> false)\n",
        Edition::E2026,
    );
    let error = rejects(
        "let v: (i32 | i64 | string) = 16\n0\n",
        "E1041",
        "anonymous_union_ambiguous_literal",
    );
    assert_eq!(
        error.message,
        "the literal 16 could be i32 or i64 in (i32 | i64 | string)"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("add a suffix, for example 16i32")
    );
    assert_eq!((error.span.start, error.span.end), (30, 32));
    let error = rejects(
        "let v: (f32 | f64 | string) = -1.5\n0\n",
        "E1041",
        "anonymous_union_ambiguous_literal",
    );
    assert_eq!(
        error.message,
        "the literal -1.5 could be f32 or f64 in (f32 | f64 | string)"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("add a suffix, for example -1.5f32")
    );
    // Two floating-point members only: an integer literal is ambiguous there too.
    rejects(
        "let v: (f32 | f64) = 2\n0\n",
        "E1041",
        "anonymous_union_ambiguous_literal",
    );
    // A suffix decides, and the picked member keeps its range check.
    accepts("let v: (i32 | i64 | string) = 16i64\nlet w: (i32 | i64 | string) = 16i32\n0\n");
    rejects(
        "let v: (i8u | string) = -1\n0\n",
        "E1009",
        "integer_literal_out_of_range",
    );
}

#[test]
fn lambda_takes_the_only_function_member() {
    passes(
        "def apply :: own ((i64 -> i64) | i64) -> i64 = \\f ->\n    match f with\n    | :? (i64 -> i64) as g -> g 40\n    | :? i64 as n -> n\n\ntest \"lambda\" =\n    assert (apply (\\x -> x + 2) == 42)\n    let f: ((i64 -> i64) | string) = \\x -> x * 2\n    assert (match f with | :? (i64 -> i64) as g -> g 21 == 42 | :? string -> false)\n    let inc = \\(x: i64) -> x + 1\n    assert (apply inc == 41)\n    assert (apply 7 == 7)\n",
        Edition::E2026,
    );
    // Two function members of one parameter: the lambda's parameter decides, or nothing does.
    accepts("let f: ((i64 -> i64) | (string -> i64)) = \\(s: string) -> s.length\n0\n");
    rejects(
        "let f: ((i64 -> i64) | (string -> i64)) = \\x -> 0\n0\n",
        "E1041",
        "anonymous_union_ambiguous_member",
    );
}

#[test]
fn dyn_of_takes_the_only_dyn_member() {
    let class = "class Plain<'a> { def plain :: ref 'a -> i64 }\nrecord Point { x: i64 }\ninstance Plain<Point> { fn plain p = p.x }\n";
    let ir = accepts(&format!(
        "{class}def make :: i64 -> (dyn Plain | string) = \\x -> Dyn.of (Point {{ x: x }})\ndef read :: own (dyn Plain | string) -> i64 = \\v ->\n    match v with\n    | :? (dyn Plain) as d -> Plain.plain (ref d)\n    | :? string as s -> s.length\n"
    ));
    assert!(ir.contains("$anonymous.Union$2"), "{ir}");
    passes(
        &format!(
            "{class}test \"dyn\" =\n    let v: (dyn Plain | string) = Dyn.of (Point {{ x: 42 }})\n    assert (match v with | :? (dyn Plain) as d -> Plain.plain (ref d) == 42 | :? string -> false)\n"
        ),
        Edition::E2026,
    );
}

#[test]
fn no_injection_without_expected_type() {
    let error = rejects("let xs = [1, \"a\"]\n0\n", "E1003", "type_mismatch");
    assert_eq!(error.message, "expected an integer, found string");
    rejects(
        "def f :: bool -> i64 = \\c ->\n    let v = if c then 1 else \"x\"\n    0\n",
        "E1003",
        "type_mismatch",
    );
    // A type variable is not widened to a union: `'a` is `i64` from the first argument.
    rejects(
        "def same :: own 'a -> own 'a -> i64 = \\x y -> 0\nlet v: (i64 | string) = 1\nsame 1i64 v\n",
        "E1003",
        "type_mismatch",
    );
}

#[test]
fn no_injection_inside_containers() {
    rejects(
        "def f :: own [i64] -> [(i64 | string)] = \\xs -> xs\n",
        "E1003",
        "type_mismatch",
    );
    rejects(
        "def f :: own (i64 -> i64) -> ((i64 | string) -> i64) = \\g -> g\n",
        "E1003",
        "type_mismatch",
    );
    rejects(
        "def f :: own Maybe<i64> -> Maybe<(i64 | string)> = \\m -> m\n",
        "E1003",
        "type_mismatch",
    );
}

#[test]
fn no_injection_through_references() {
    rejects(
        "let x = 1i64\nlet r: ref (i64 | string) = ref x\n0\n",
        "E1003",
        "type_mismatch",
    );
    // An exclusive parameter takes a place of the union type, never an injected temporary.
    rejects(
        "def bump :: ref mut (i64 | string) -> unit = \\v -> ()\nlet mut x = 1i64\nbump (ref mut x)\n",
        "E1003",
        "type_mismatch",
    );
}

#[test]
fn injection_moves_the_value() {
    let size = "def size :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n";
    // The string moves into the union that the read parameter borrows (D10).
    let error = rejects_in(
        &format!(
            "{size}def twice :: own string -> i64 = \\name ->\n    let a = size name\n    a + name.length\n"
        ),
        Edition::E2027,
        "E1012",
        "use_after_move",
    );
    assert!(error.message.contains("'name'"), "{}", error.message);
    // Its last use moves it with nothing after, and a Copy value is copied.
    accepts_in(
        &format!(
            "{size}def once :: own string -> i64 = \\name -> size name\ndef copies :: i64 -> i64 = \\n -> size n + size n + n\n"
        ),
        Edition::E2027,
    );
    // A pipe moves it too.
    rejects_in(
        &format!("{size}def twice :: own string -> i64 = \\name -> (name |> size) + name.length\n"),
        Edition::E2027,
        "E1012",
        "use_after_move",
    );
}

#[test]
fn read_parameter_needs_owned_value() {
    let size = "def size :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n";
    let source = format!("{size}def forward :: string -> i64 = \\name -> size name\n");
    // A read parameter is a borrow (2027): its string cannot move into the union.
    let error = rejects_in(
        &source,
        Edition::E2027,
        "E1012",
        "move_out_of_reference_binding",
    );
    assert!(
        error.message.contains("read-only by default"),
        "{}",
        error.message
    );
    // The same program passes its own value in 2026, and with `own` or a copy in 2027.
    accepts_in(&source, Edition::E2026);
    accepts_in(
        &format!(
            "{size}def forward :: own string -> i64 = \\name -> size name\ndef copied :: string -> i64 = \\name -> size (clone_string name)\n"
        ),
        Edition::E2027,
    );
}

#[test]
fn references_pass_through() {
    let size = "def size :: (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n";
    // A read parameter of the union type is `ref (i64 | string)`, which a read parameter
    // borrows again, unwrapped and not wrapped twice.
    passes(
        &format!(
            "{size}def forward :: (i64 | string) -> i64 = \\v -> size v + (v |> size)\n\ntest \"references\" =\n    assert (forward \"abc\" == 6)\n    assert (forward 20 == 40)\n"
        ),
        Edition::E2027,
    );
    passes(
        "def size :: ref (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n\ntest \"explicit\" =\n    let v: (i64 | string) = \"abcd\"\n    let r = ref v\n    assert (size r + size (ref v) == 8)\n",
        Edition::E2026,
    );
}

#[test]
fn injects_through_select_checked_and_holes() {
    passes(
        &format!(
            "{MEASURE}test \"select and checked\" =\n    let jobs: Channel<i64> = Channel.make 1\n    let got: (i64 | string) = select\n        | recv job from jobs -> match job with | Maybe.Some v -> v | Maybe.None -> 0\n        | default -> \"idle\"\n    let checked: (i64 | string) = @checked (40 + 1)\n    assert (measure got + measure checked == 45)\n"
        ),
        Edition::E2026,
    );
    // `@checked` arithmetic still traps on overflow inside the union.
    traps(
        "test \"overflow\" =\n    let big = 9223372036854775807\n    let v: (i64 | string) = @checked (big + 1)\n    assert (match v with | :? i64 -> true | :? string -> false)\n",
        Edition::E2026,
    );
    // A hole reports the union, not an undetermined type.
    let error = rejects("let v: (string | i64) = ?\n0\n", "E1030", "typed_hole");
    assert!(
        error.message.contains("of type '(i64 | string)'"),
        "{}",
        error.message
    );
}

#[test]
fn collection_literals_pick_their_member() {
    passes(
        "test \"collections\" =\n    let empty: ([i64] | string) = []\n    let fixed: ([i64; 3] | string) = [1, 2, 3]\n    let list: ([|i64|] | bool) = [|1, 2|]\n    let none: ([|string|] | bool) = [||]\n    let heap: ([i64] | bool) = new [4, 5]\n    let tuple: ((i64 * [string]) | bool) = (1, [])\n    assert (match empty with | :? [i64] as a -> a.length == 0 | :? string -> false)\n    assert (match fixed with | :? [i64; 3] as a -> a[2] == 3 | :? string -> false)\n    assert (match list with | :? [|i64|] as l -> List.length l == 2 | :? bool -> false)\n    assert (match none with | :? [|string|] as l -> List.length l == 0 | :? bool -> false)\n    assert (match heap with | :? [i64] as a -> a[1] == 5 | :? bool -> false)\n    assert (match tuple with | :? (i64 * [string]) as t -> (match t with | (n, xs) -> n + xs.length == 1) | :? bool -> false)\n",
        Edition::E2026,
    );
    // Two array members: the elements decide.
    accepts("let v: ([i64] | [string]) = [\"a\"]\n0\n");
    rejects(
        "let v: ([i64] | [string]) = []\n0\n",
        "E1004",
        "empty_list_needs_annotation",
    );
    // A literal checked against its member reports the element that does not fit.
    let error = rejects(
        "let v: ([i64] | string) = [1, \"a\"]\n0\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected i64, found string");
}

#[test]
fn comparisons_and_exports_keep_their_rules() {
    // `1` is injected into the left operand's type, which has no `Eq` until A26.
    rejects(
        "let v: (i64 | string) = 1\nv == 1\n",
        "E1005",
        "no_instance",
    );
    let error = analyze("export def f :: (i64 | string) -> i64 = \\v -> 0\n").expect_err("export");
    assert_eq!(error.code, "E1008", "{}", error.message);
}

// Step 6: type test patterns `:? T [as x]`.

#[test]
fn type_test_binds_member_type() {
    passes(
        "def count :: own (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n + 1\n    | :? string as s -> String.length s\n    | :? bool as _ -> 100\n\ndef whole :: own (i64 | string) -> (i64 | string) = \\v ->\n    match v with\n    | (:? i64 as n) as all -> if n > 0 then all else 0\n    | :? string as s -> s\n\ndef either :: own (i32 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i32 | :? i64 -> 1\n    | :? string -> 2\n\ntest \"bindings\" =\n    assert (count 41 + count \"ab\" + count true == 144)\n    assert (match whole 5 with | :? i64 as n -> n == 5 | :? string -> false)\n    assert (match whole (-5) with | :? i64 as n -> n == 0 | :? string -> false)\n    assert (either 1i32 + either 1i64 + either \"x\" == 4)\n",
        Edition::E2026,
    );
    // The binder has the member's type, not the union's.
    let error = rejects(
        "def f :: own (i64 | string) -> bool = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string -> false\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected bool, found i64");
    // The two sides of an OR pattern bind one name with one type.
    rejects(
        "def f :: own (i32 | i64) -> i64 = \\v ->\n    match v with\n    | :? i64 as n | :? i32 as n -> 0\n",
        "E1003",
        "type_mismatch",
    );
    // Generic members test as the instance's types.
    accepts(
        "def f :: own (Maybe<'a> | i64) -> i64 = \\v ->\n    match v with\n    | :? Maybe<'a> -> 1\n    | :? i64 as n -> n\n",
    );
}

#[test]
fn type_test_views_through_references() {
    passes(
        "def size :: ref (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? string as s -> s.length\n    | :? i64 as n -> n\n\ntest \"views\" =\n    let items: [(i64 | string)] = [\"abc\", 4]\n    let mut total = 0\n    for item in items do\n        total = total + (match item with\n            | :? string as s -> s.length\n            | :? i64 as n -> n)\n    let v: (i64 | string) = \"xy\"\n    assert (total + size (ref v) == 9)\n    assert (match v with | :? string as s -> s.length == 2 | :? i64 -> false)\n",
        Edition::E2026,
    );
    // A payload seen through a reference is a view: it cannot move out.
    rejects(
        "def take :: ref (i64 | string) -> string = \\v ->\n    match v with\n    | :? string as s -> s\n    | :? i64 -> \"\"\n",
        "E1012",
        "move_out_of_reference_binding",
    );
}

#[test]
fn type_test_rejections() {
    // The matched type is unknown when the pattern is checked.
    let error = rejects(
        "let g = \\v ->\n    match v with\n    | :? i64 -> 0\n    | _ -> 1\n0\n",
        "E1041",
        "type_test_unknown_scrutinee",
    );
    assert_eq!(
        error.message,
        "the type of the matched value must be known to use ':?'"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("annotate the parameter or binding with its anonymous union type")
    );
    // The location is the `:?` itself.
    assert_eq!((error.span.start, error.span.end), (37, 39));
    // The matched type is no anonymous union: a number literal, or a named union.
    let error = rejects(
        "match 1 with\n| :? i64 -> 0\n| _ -> 1\n",
        "E1041",
        "type_test_not_anonymous_union",
    );
    assert_eq!(
        error.message,
        "':?' tests a member of an anonymous union, but the matched value has type i32"
    );
    assert_eq!(error.help, None);
    let error = rejects(
        "let m: Maybe<i64> = Some 1\nmatch m with\n| :? i64 -> 0\n| _ -> 1\n",
        "E1041",
        "type_test_not_anonymous_union",
    );
    assert_eq!(
        error.message,
        "':?' tests a member of an anonymous union, but the matched value has type Maybe<i64>"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("match the cases of a named union by name")
    );
    // The tested type is an anonymous union (A25 narrows to one).
    let error = rejects(
        "let v: (i64 | string | bool) = 1\nmatch v with\n| :? (string | i64) -> 0\n| _ -> 1\n",
        "E1041",
        "type_test_union",
    );
    assert_eq!(
        error.message,
        "':?' names one member of (i64 | bool | string); write one ':?' arm for each member"
    );
    // The tested type is no member.
    let error = rejects(
        "let v: (i64 | string) = 1\nmatch v with\n| :? bool -> 0\n| _ -> 1\n",
        "E1041",
        "type_test_not_member",
    );
    assert_eq!(error.message, "bool is not a member of (i64 | string)");
    assert_eq!(error.help.as_deref(), Some("the members are i64, string"));
    assert_eq!((error.span.start, error.span.end), (44, 48));
    // A type variable that the signature does not name stays an error of its own.
    rejects(
        "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? Maybe<'b> -> 0\n    | _ -> 1\n",
        "E1015",
        "type_variable_absent_from_signature",
    );
}

#[test]
fn type_test_in_function_guards() {
    passes(
        "def size :: own (i64 | string | (i64 * i64)) -> i64 = \\v ->\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n    | :? (i64 * i64) as pair -> (match pair with | (a, b) -> a + b)\n\ndef inner :: own (i64 | Maybe<i64>) -> i64 = \\v ->\n    | (:? Maybe<i64> as m) -> (match m with | Some n -> n | None -> 0)\n    | :? i64 as n -> n\n\ntest \"guards\" =\n    assert (size 1 + size \"ab\" + size (3, 4) == 10)\n    assert (inner (Some 30) + inner 12 == 42)\n",
        Edition::E2026,
    );
    // A read parameter (2027): the arms read their views.
    passes(
        "def size :: (i64 | string) -> i64 = \\v ->\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n\ntest \"read\" =\n    assert (size 40 + size \"ab\" == 42)\n",
        Edition::E2027,
    );
}

#[test]
fn type_test_in_lambda_parameter_traps_at_run_time() {
    let lambda = "let only: (i64 | string) -> i64 = \\(:? i64 as n) -> n\nlet bare: (i64 | string) -> i64 = \\:? i64 as n -> n\n";
    passes(
        &format!(
            "test \"matches\" =\n    {}\n    {}\n    let xs: [(i64 | string)] = [1, 2, 39]\n    let mut total = 0\n    for (:? i64 as n) in xs do\n        total = total + n\n    assert (only 1 + bare 1 + total == 44)\n",
            lambda.lines().next().unwrap(),
            lambda.lines().nth(1).unwrap()
        ),
        Edition::E2026,
    );
    traps(
        &format!(
            "test \"lambda\" =\n    {}\n    assert (only \"x\" == 0)\n\ntest \"bare lambda\" =\n    {}\n    assert (bare \"x\" == 0)\n\ntest \"for\" =\n    let xs: [(i64 | string)] = [1, \"x\"]\n    let mut total = 0\n    for (:? i64 as n) in xs do\n        total = total + n\n    assert (total == 1)\n",
            lambda.lines().next().unwrap(),
            lambda.lines().nth(1).unwrap()
        ),
        Edition::E2026,
    );
}

#[test]
fn read_parameter_type_test_moves_need_own() {
    let describe = "def describe :: (i64 | string | bool) -> string = \\value ->\n    match value with\n    | :? i64 as n -> $\"int {n}\"\n    | :? string as s -> s\n    | :? bool as b -> if b then \"yes\" else \"no\"\n";
    // A read parameter's payload is a view (2027): moving it out is an error, as with a named union.
    let error = rejects_in(
        describe,
        Edition::E2027,
        "E1012",
        "move_out_of_reference_binding",
    );
    assert!(
        error.message.contains("bound through a reference"),
        "{}",
        error.message
    );
    // `own` takes the value; the acceptance program of the ticket prints "int 42".
    let owned = describe.replace(":: (i64", ":: own (i64");
    passes(
        &format!(
            "{owned}test \"describe\" =\n    let first: (string | bool | i64) = 42\n    assert (describe first == \"int 42\")\n    assert (describe \"text\" == \"text\")\n    assert (describe false == \"no\")\n"
        ),
        Edition::E2027,
    );
    // Arms that only read the view need no `own`.
    accepts_in(
        "def size :: (i64 | string | bool) -> i64 = \\value ->\n    match value with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n    | :? bool as b -> if b then 1 else 0\n",
        Edition::E2027,
    );
}

// Step 7: the witness of a non-exhaustive match names the missing member (D7).

/// The `E1021` of `source`, whose message must name `witness`.
fn missing(source: &str, witness: &str) -> Diagnostic {
    let error = rejects(source, "E1021", "non_exhaustive_match");
    assert_eq!(
        error.message,
        format!("match is not exhaustive; missing: {witness}"),
        "{source}"
    );
    error
}

/// `source` with the edits of the first fix of its `E1021`, one arm per missing value.
fn fixed(source: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1021", "{source}\n{}", error.message);
    let edits: Vec<&Edit> = error.fixes[0].edits.iter().collect();
    apply_edits(source, &edits).unwrap()
}

#[test]
fn missing_member_is_named() {
    // The member's own type, as `Type::display` writes it: never the synthesized case.
    let error = missing(
        "def f :: own (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? bool -> 1\n",
        ":? string",
    );
    assert_eq!(
        error.help.as_deref(),
        Some("add an arm for ':? string', or end the match with a wildcard arm '| _ -> ...'")
    );
    assert!(!error.message.contains('$'), "{}", error.message);
    // The first missing member in canonical order, whatever order the type was written in.
    missing(
        "def f :: own (string | bool | i64) -> i64 = \\v ->\n    match v with\n    | :? bool -> 1\n",
        ":? i64",
    );
    // The example of the ticket without its `bool` arm.
    missing(
        "def describe :: own (i64 | string | bool) -> string = \\value ->\n    match value with\n    | :? i64 as n -> $\"int {n}\"\n    | :? string as s -> s\n",
        ":? bool",
    );
    // A function member is parenthesized: `:?` reads a `type_product`.
    missing(
        "def f :: own ((i64 -> i64) | string) -> i64 = \\v ->\n    match v with\n    | :? string -> 0\n",
        ":? (i64 -> i64)",
    );
    // Inside a tuple, through a reference, and with a generic member.
    missing(
        "def f :: own ((i64 | string) * bool) -> i64 = \\v ->\n    match v with\n    | (:? i64 as n, _) -> n\n    | (:? string, true) -> 0\n",
        "(:? string, false)",
    );
    missing(
        "def f :: ref (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n",
        ":? string",
    );
    missing(
        "def f :: own (Maybe<'a> | i64) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n",
        ":? Maybe<'a>",
    );
    // A function guard names it the same way.
    missing(
        "def f :: own (i64 | string) -> i64 = \\v ->\n    | :? i64 as n -> n\n",
        ":? string",
    );
}

#[test]
fn missing_member_inside_maybe() {
    missing(
        "def f :: own Maybe<(i64 | string)> -> i64 = \\v ->\n    match v with\n    | Some (:? i64 as n) -> n\n    | None -> 0\n",
        "Some (:? string)",
    );
    // A record field and a list element take the member types of their own declarations.
    missing(
        "record Cell { value: (i64 | string), count: i64 }\ndef f :: own Cell -> i64 = \\c ->\n    match c with\n    | { value = :? i64 as n } -> n\n",
        "{ value = :? string }",
    );
    missing(
        "def f :: own [|(i64 | bool)|] -> i64 = \\xs ->\n    match xs with\n    | [||] -> 0\n    | (:? i64 as n) :: _ -> n\n",
        "(:? bool) :: _",
    );
}

#[test]
fn missing_member_fix_is_written() {
    let source = "def f :: own (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n";
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1021");
    assert_eq!(error.fixes.len(), 1);
    // Every missing member, one arm each, in canonical order (`bool` before `string`).
    assert_eq!(
        error.fixes[0].title,
        "Add the arms '| :? bool -> unreachable () | :? string -> unreachable ()'"
    );
    let fixed_source = fixed(source);
    assert_eq!(
        fixed_source,
        source.replace(
            "-> n\n",
            "-> n | :? bool -> unreachable () | :? string -> unreachable ()\n"
        )
    );
    accepts(&fixed_source);
    // The fixes that name a member inside another pattern, a function member and a generic member
    // compile as written.
    for source in [
        "def f :: own Maybe<(i64 | string)> -> i64 = \\v ->\n    match v with\n    | Some (:? i64 as n) -> n\n    | None -> 0\n",
        "def f :: own ((i64 | string) * bool) -> i64 = \\v ->\n    match v with\n    | (:? i64 as n, _) -> n\n    | (:? string, true) -> 0\n",
        "def f :: own ((i64 -> i64) | string) -> i64 = \\v ->\n    match v with\n    | :? string -> 0\n",
        "def f :: own (Maybe<'a> | i64) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n",
        "def f :: own (i64 | [string] | (bool * char)) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n",
    ] {
        let fixed_source = fixed(source);
        assert_ne!(fixed_source, source);
        accepts(&fixed_source);
    }
    // A record or list witness is not written, as before.
    let error = analyze(
        "record Cell { value: (i64 | string), count: i64 }\ndef f :: own Cell -> i64 = \\c ->\n    match c with\n    | { value = :? i64 as n } -> n\n",
    )
    .unwrap_err();
    assert!(error.fixes.is_empty());
}

#[test]
fn duplicate_type_test_is_unreachable() {
    let source = "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string -> 0\n    | :? i64 -> 1\n    | _ -> 2\n";
    let module = analyze(source).unwrap();
    let warnings: Vec<(&str, &str)> = module
        .warnings
        .iter()
        .map(|warning| (warning.code, &source[warning.span.start..warning.span.end]))
        .collect();
    assert_eq!(warnings, [("W1003", ":? i64"), ("W1003", "_")]);
    // A guarded arm covers nothing, so the next test of the member is reachable.
    let guarded = "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n when n > 0 -> n\n    | :? i64 -> 0\n    | :? string -> 1\n";
    assert!(analyze(guarded).unwrap().warnings.is_empty());
}

#[test]
fn existing_missing_messages_unchanged() {
    // Named unions, tuples, booleans and lists keep their witnesses when the program also writes
    // an anonymous union (and so has the synthesized declarations).
    let union = "def g :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string -> 0\n";
    for (source, witness) in [
        (
            "union Shape = Empty | Circle of f64 | Rect of f64 * f64\ndef f :: Shape -> f64 = \\s ->\n    match s with\n    | Empty -> 0.0\n    | Circle r -> r\n",
            "Rect _",
        ),
        (
            "def f :: Maybe<bool> -> i64 = \\m ->\n    match m with\n    | Some true -> 1\n    | None -> 0\n",
            "Some false",
        ),
        (
            "def f :: (bool * bool) -> i64 = \\p ->\n    match p with\n    | (true, _) -> 1\n    | (false, true) -> 0\n",
            "(false, false)",
        ),
        (
            "def f :: [|i64|] -> i64 = \\xs ->\n    match xs with\n    | [||] -> 0\n",
            "_ :: _",
        ),
    ] {
        let without = analyze(source).expect_err(source);
        let with = analyze(&format!("{union}{source}")).expect_err(source);
        assert_eq!(
            without.message,
            format!("match is not exhaustive; missing: {witness}")
        );
        assert_eq!(with.message, without.message, "{source}");
        assert_eq!(with.help, without.help, "{source}");
        assert_eq!(
            with.fixes.iter().map(|fix| &fix.title).collect::<Vec<_>>(),
            without
                .fixes
                .iter()
                .map(|fix| &fix.title)
                .collect::<Vec<_>>(),
            "{source}"
        );
    }
}

// Step 8: no user instance for an anonymous union (D8), and the guards that keep the synthesized
// declaration out of other diagnostics.

#[test]
fn rejects_instances_for_anonymous_unions() {
    // Every class, built in or marker or user, gets the same error before its own checks: a
    // `Copy`, `Drop` or `Format` instance would otherwise report E1016 or its own rule first.
    for class in ["Display", "Eq", "Copy", "Send", "Drop", "Format", "Hash"] {
        let source = format!("instance {class}<(string | i64)> {{\n}}\n0\n");
        let error = rejects(&source, "E1041", "anonymous_union_instance");
        assert_eq!(
            error.message,
            "instances cannot be declared for the anonymous union (i64 | string); wrap it in a record, or declare a union with named cases"
        );
        // The location is the class name.
        assert_eq!(&source[error.span.start..error.span.end], class, "{source}");
    }
    // A class of the program, and an alias of the union.
    let error = rejects_modules(
        &[
            ("Shapes.tt", "class Shape<'a> { def area :: 'a -> f64 }\n"),
            (
                "Main.tz",
                "type Value = (i64 | f64)\ninstance Shape<Value> { fn area v = 0.0 }\n0\n",
            ),
        ],
        "E1041",
        "anonymous_union_instance",
    );
    assert!(error.message.contains("(i64 | f64)"), "{}", error.message);
    // An instance for a type that holds an anonymous union is not rejected by this rule.
    accepts_modules(&[
        ("Shapes.tt", "class Shape<'a> { def area :: 'a -> f64 }\n"),
        (
            "Main.tz",
            "record Holder { value: (i64 | f64) }\ninstance Shape<Holder> { fn area h = 1.0 }\ninstance Shape<Maybe<(i64 | f64)>> { fn area m = 2.0 }\n0\n",
        ),
    ]);
}

#[test]
fn member_function_constraint_has_no_module() {
    // A function constraint `#f` finds `f` in the module that declares the type. An anonymous union
    // has none, as a tuple has none: the message never names the pseudo-module.
    let error = rejects(
        "def measure :: 'T -> i64\n  @'T : #distance\nfn measure value = 'T.distance value\nlet v: (i64 | string) = 1\nmeasure v\n",
        "E1005",
        "function_constraint_unresolvable",
    );
    assert_eq!(
        error.message,
        "(i64 | string) has no declaring module for function constraint '#distance'"
    );
    let tuple = rejects(
        "def measure :: 'T -> i64\n  @'T : #distance\nfn measure value = 'T.distance value\nlet v: (i64 * string) = (1, \"a\")\nmeasure v\n",
        "E1005",
        "function_constraint_unresolvable",
    );
    assert_eq!(
        tuple.message,
        "(i64 * string) has no declaring module for function constraint '#distance'"
    );
}

#[test]
fn higher_kinded_unification_refuses_anonymous_unions() {
    let functor = "class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }\n";
    // `'f<'a>` does not take a part of an anonymous union as a constructor, which would build a
    // union out of canonical order or with a repeated member, and name the synthesized one.
    let error = rejects_modules(
        &[
            ("Traits.tt", functor),
            (
                "Main.tz",
                "def fmap :: Functor<'f> => ('a -> 'b) -> 'f<'a> -> 'f<'b>\nfn fmap transform value = Functor.map transform value\ndef go :: (i64 | string) -> i64 = \\v ->\n    let w = fmap (\\s -> 1) v\n    0\n0\n",
            ),
        ],
        "E1015",
        "constructor_kind_mismatch",
    );
    assert!(!error.message.contains('$'), "{}", error.message);
    // A named generic union still decomposes, as before: `'f` takes `P` with its last argument.
    let named = analyze_modules(&[
        ("Traits.tt", functor),
        (
            "Main.tz",
            "union P<'a, 'b> = L of 'a | R of 'b\ndef fmap :: Functor<'f> => ('a -> 'b) -> 'f<'a> -> 'f<'b>\nfn fmap transform value = Functor.map transform value\ndef go :: P<i64, string> -> i64 = \\v ->\n    let w = fmap (\\s -> 1) v\n    0\n0\n",
        ),
    ])
    .expect_err("no instance");
    assert_eq!(named.code, "E1005", "{}", named.message);
    assert!(
        named.message.contains("Functor<Main.P<string>>"),
        "{}",
        named.message
    );
}

// Step 9: the synthesized declarations, their cases and their pseudo-module never reach a user.

/// Fails when `text` names the synthesized declarations, their cases or their pseudo-module.
fn assert_hidden(text: &str) {
    for synthetic in ["$anonymous", "Union$", "$t0"] {
        assert!(!text.contains(synthetic), "{synthetic} in\n{text}");
    }
}

#[test]
fn typed_holes_do_not_offer_synthetic_cases() {
    // The hole reports the union, and its candidates are values of a member or of the union: no
    // synthesized case is a constructor that a program can write.
    let source = "def f :: own (i64 | string) -> i64 = \\v -> 0\nlet n: i64 = 4\nlet v: (string | i64) = ?\nf v\n";
    let errors = analyze_all(source).expect_err(source).diagnostics;
    assert_eq!(errors.len(), 1, "{errors:?}");
    let hole = &errors[0];
    assert_eq!(hole.code, "E1030");
    assert!(hole.message.contains("(i64 | string)"), "{}", hole.message);
    for candidate in &hole.detail.candidates {
        assert_hidden(&candidate.name);
        assert_hidden(&candidate.signature);
    }
    assert_hidden(&format!("{hole:?}"));
    // A hole of a generic type does not offer the generic synthesized cases either.
    let source = "def f :: own (i64 | string) -> i64 = \\v -> 0\nlet m: Maybe<i64> = ?\n0\n";
    let errors = analyze_all(source).expect_err(source).diagnostics;
    assert_hidden(&format!("{errors:?}"));
}

#[test]
fn suggestions_skip_synthetic_names() {
    let union = "def f :: own (i64 | string) -> i64 = \\v -> 0\n";
    for (typo, code) in [
        // A module whose name is one letter from the pseudo-module.
        ("Anonymous.f 1\n", "E1002"),
        ("anonymous 1\n", "E1002"),
        // A type and a case one letter from the synthesized ones.
        ("let v: Union2 = 1\nv\n", "E1004"),
        ("let v = Union2 1\nv\n", "E1002"),
        ("match 1 with\n| Union2_0 x -> x\n| _ -> 0\n", "E1020"),
    ] {
        let source = format!("{union}{typo}");
        let errors = analyze_all(&source).expect_err(&source).diagnostics;
        let error = &errors[0];
        assert_eq!(error.code, code, "{source}\n{}", error.message);
        assert_hidden(&format!("{error:?}"));
    }
}

#[test]
fn hover_shows_anonymous_union() {
    let source = "def f :: own (string | i64) -> (i64 | string) = \\v -> v\ndef g :: own Maybe<(bool | i64)> -> i64 = \\m -> 0\n0\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", source)]).unwrap();
    let hover = |needle: &str, offset: usize| {
        let at = source.find(needle).unwrap() + offset;
        index
            .at(0, at)
            .unwrap_or_else(|| panic!("no hover at {needle}"))
    };
    // The union reads in its canonical order wherever it is written, and has no definition to go
    // to; a member reads as itself.
    for (needle, detail) in [
        ("(string | i64) ->", "(i64 | string)"),
        ("(i64 | string) =", "(i64 | string)"),
        ("(bool | i64)", "(i64 | bool)"),
    ] {
        let entry = hover(needle, 0);
        assert_eq!(entry.detail, detail, "{needle}");
        assert_eq!(entry.target, None, "{needle}");
    }
    assert_eq!(hover("string | i64) ->", 0).detail, "string");
    assert_eq!(hover("Maybe<", 0).detail, "Maybe<(i64 | bool)>");
    for entry in &index.entries {
        assert_hidden(&entry.detail);
    }
}

#[test]
fn definition_of_anonymous_union_has_no_target() {
    let source =
        "union Shape = Dot | Line of i64\ndef f :: own (i64 | Shape) -> i64 = \\v -> 0\n0\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", source)]).unwrap();
    let union = index.at(0, source.find("(i64 | Shape)").unwrap()).unwrap();
    assert_eq!(union.target, None);
    // A named member still goes to its declaration.
    let shape = index.at(0, source.find("Shape) ->").unwrap()).unwrap();
    assert_eq!(shape.detail, "Main.Shape");
    assert!(shape.target.is_some());
}

/// The changes between two versions of the module `Main`, in the default (syntactic) or the
/// `--effective` mode, as `tests/api_diff.rs` reads them: `severity action kind name` and the old
/// and new texts.
fn api_changes(old: &str, new: &str, effective: bool) -> Vec<String> {
    use tsuzuri::api_diff;
    use tsuzuri::driver::Project;
    let directory = |label: &str, text: &str| {
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-anonymous-unions-api-{label}-{}-{}",
            std::process::id(),
            effective
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Main.tz"), text).unwrap();
        root
    };
    let api = |path: &std::path::Path| {
        let project = if effective {
            Project::load_for_docs(path)
        } else {
            Project::load_for_api(path)
        }
        .unwrap();
        if effective {
            api_diff::effective_api(&project)
        } else {
            api_diff::api(&project)
        }
        .unwrap_or_else(|error| panic!("{path:?}: {}", error.message))
    };
    let (before, after) = (directory("old", old), directory("new", new));
    let report = api_diff::compare(&api(&before), &api(&after));
    std::fs::remove_dir_all(before).unwrap();
    std::fs::remove_dir_all(after).unwrap();
    report
        .changes
        .iter()
        .map(|change| {
            format!(
                "{} {} {} {} | {} | {}",
                change.severity(),
                change.action.name(),
                change.kind,
                change.name,
                change.old.as_deref().unwrap_or(""),
                change.new.as_deref().unwrap_or("")
            )
        })
        .collect()
}

#[test]
fn api_diff_treats_member_order_as_equal() {
    for effective in [false, true] {
        // Reordered members, nested members, and members in another order inside other types.
        for (old, new) in [
            (
                "def f :: own (i64 | string) -> i64 = \\v -> 0\n",
                "def f :: own (string | i64) -> i64 = \\v -> 0\n",
            ),
            (
                "def f :: own (i64 | (string | bool)) -> i64 = \\v -> 0\n",
                "def f :: own (bool | string | i64) -> i64 = \\v -> 0\n",
            ),
            (
                "def f :: own Maybe<(i64 | [string])> -> [(bool | char)] = \\v -> []\n",
                "def f :: own Maybe<([string] | i64)> -> [(char | bool)] = \\v -> []\n",
            ),
            (
                "record Cell { value: (i64 | string) }\ndef g :: own (Maybe<'a> | i64) -> 'a -> 'a = \\v x -> x\n",
                "record Cell { value: (string | i64) }\ndef g :: own (i64 | Maybe<'b>) -> 'b -> 'b = \\v x -> x\n",
            ),
        ] {
            assert_eq!(
                api_changes(old, new, effective),
                Vec::<String>::new(),
                "effective {effective}\n{old}{new}"
            );
        }
        // A changed member is a change, shown as written and never with a synthesized name.
        let changes = api_changes(
            "def f :: own (i64 | string) -> i64 = \\v -> 0\n",
            "def f :: own (i64 | bool) -> i64 = \\v -> 0\n",
            effective,
        );
        assert_eq!(changes.len(), 1, "{changes:?}");
        assert!(
            changes[0].starts_with("breaking changed def Main.f"),
            "{changes:?}"
        );
        assert!(changes[0].contains("(i64 | string)"), "{changes:?}");
        assert!(changes[0].contains("(i64 | bool)"), "{changes:?}");
        assert_hidden(&changes[0]);
    }
    // The syntactic comparison does not expand aliases (G19); the effective one does.
    let alias = "type Meters = f64\ndef f :: own (Meters | string) -> i64 = \\v -> 0\n";
    let plain = "type Meters = f64\ndef f :: own (f64 | string) -> i64 = \\v -> 0\n";
    assert_eq!(api_changes(alias, plain, false).len(), 1);
    assert!(api_changes(alias, plain, true).is_empty());
}

#[test]
fn effective_api_hides_synthetic_names() {
    // Through an alias, a record field and a generic argument, the effective comparison reads the
    // checked types back as unions, never as the synthesized declaration.
    let old = "type Value = (i64 | string)\nrecord Cell { value: Value }\ndef f :: own Value -> Maybe<Value> = \\v -> None\n";
    let new = "type Value = (i64 | string | bool)\nrecord Cell { value: Value }\ndef f :: own Value -> Maybe<Value> = \\v -> None\n";
    let changes = api_changes(old, new, true);
    assert!(!changes.is_empty());
    for change in &changes {
        assert_hidden(change);
    }
    assert!(api_changes(old, &old.replace("(i64 | string)", "(string | i64)"), true).is_empty());
}

#[test]
fn kg_has_no_synthetic_declarations() {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-anonymous-unions-kg-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "record Cell { value: (i64 | string) }\ndef f :: own (string | i64) -> Maybe<(bool | i64)> = \\v -> None\ndef g :: own Cell -> i64 = \\c ->\n    match c.value with\n    | :? i64 as n -> n\n    | :? string -> 0\nf 1\n",
    )
    .unwrap();
    let kg = root.join("kg");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(&root)
        .args(["kg", "build", "Main.tz", "--kg-dir"])
        .arg(&kg)
        .arg("--force")
        .env_remove("TSUZURI_KG")
        .env_remove("TSUZURI_KG_DIR")
        .env("TSUZURI_CACHE_DIR", root.join("cache"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut pending = vec![kg.clone()];
    let mut read = 0;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                assert_hidden(&text);
                read += 1;
            }
        }
    }
    assert!(read > 0);
    // The signatures hold the unions as they are written, and the layout of a field the checked
    // type, in its canonical order (`kg/ids.rs`).
    let nodes = std::fs::read_to_string(kg.join("nodes.jsonl")).unwrap();
    assert!(
        nodes.contains("def f :: own (string | i64) -> Maybe<(bool | i64)>"),
        "{nodes}"
    );
    let layout = std::fs::read_to_string(kg.join("layout/Main.jsonl")).unwrap();
    assert!(
        layout.contains("{\"name\":\"value\",\"type\":\"(i64 | string)\""),
        "{layout}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn find_searches_anonymous_unions_in_any_order() {
    // `tsuzuri find` reads `(A | B)` in signatures and queries, whatever order the members are in.
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-anonymous-unions-find-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("Tsuzuri.toml"),
        "[package]\nname = \"finder\"\nversion = \"0.1.0\"\nedition = \"2026\"\nnamespace = \"Finder\"\n",
    )
    .unwrap();
    std::fs::write(
        project.join("Main.tz"),
        "def pick :: own (string | i64) -> i64 = \\v -> 0\ndef wrap :: i64 -> Maybe<(bool | i64)> = \\n -> None\npick 1\n",
    )
    .unwrap();
    let find = |query: &str| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .current_dir(&root)
            .args(["find", "--project", "project", "--limit", "100", query])
            .env("TSUZURI_CACHE_DIR", root.join("cache"))
            .env_remove("TSUZURI_KG")
            .env_remove("TSUZURI_KG_DIR")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    for query in ["(i64 | string) -> i64", "(string | i64) -> i64"] {
        let text = find(query);
        assert!(
            text.contains("exact   Finder::Main.pick :: own (string | i64) -> i64\n"),
            "{query}\n{text}"
        );
    }
    let text = find("i64 -> Maybe<(i64 | bool)>");
    assert!(
        text.contains("exact   Finder::Main.wrap :: i64 -> Maybe<(bool | i64)>\n"),
        "{text}"
    );
    // A union with another member, or a tuple, is another type.
    assert!(!find("(i64 | bool) -> i64").contains("Main.pick"));
    assert!(!find("(i64 * string) -> i64").contains("Main.pick"));
    std::fs::remove_dir_all(&root).unwrap();
}
