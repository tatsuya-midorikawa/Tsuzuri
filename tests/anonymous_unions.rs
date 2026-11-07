//! Anonymous unions `(A | B)` and type test patterns `:? T` (A24).

use tsuzuri::check::Type;
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::parser::{parse, parse_with_source};
use tsuzuri::syntax::{Edition, Program, TypeExpr, TypeExprKind};
use tsuzuri::{analyze, analyze_modules, llvm};

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
