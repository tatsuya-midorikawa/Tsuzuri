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
    // A pure result, the parameter count and a borrowed parameter make other types. Purity alone
    // does not (review round 1): see `members_that_differ_only_in_purity_are_rejected`.
    accepts("def f :: ((i64 -> pure (i64 -> i64)) | (i64 -> i64 -> i64)) -> i64 = \\v -> 0\n");
    accepts("def f :: ((i64 -> i64) | (i64 -> i64 -> i64)) -> i64 = \\v -> 0\n");
    accepts("def f :: ((ref i64 -> i64) | (i64 -> i64)) -> i64 = \\v -> 0\n");
    let error = rejects(
        "def f :: ((i64 -> i64) | pure (i64 -> string) | (fn() -> i64)) -> i64 = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(
        error.message,
        "expected i64, found ((fn() -> i64) | (i64 -> i64) | pure (i64 -> string))"
    );
}

/// Review round 1: the order of two function members holds for every type argument. A curried
/// function that is not pure is one flat type, so a result that is still a type variable can add
/// parameters; such a member is not ordered against a longer one that agrees with it so far.
#[test]
fn function_members_keep_their_order_under_substitution() {
    for source in [
        "def f :: own ((i64 -> 'r) | (i64 -> bool -> i64)) -> 'r -> i64 = \\v d -> 0\n",
        "def f :: 'r -> ((i64 -> 'r) | (i64 -> i64 -> i64)) = \\w -> unreachable ()\n",
        "record Box<'r> { v: ((i64 -> 'r) | (i64 -> i64 -> string)) }\n",
        "type Either<'r> = ((i64 -> i64 -> string) | (i64 -> 'r))\n",
        // Inside another member, and behind a pure function, which is also a value of the
        // function type that is not pure.
        "def f :: own (Maybe<(i64 -> 'r)> | Maybe<(i64 -> i64 -> i64)>) -> 'r -> i64 = \\v d -> 0\n",
        "def f :: own (pure (i64 -> 'r) | (i64 -> i64 -> i64)) -> 'r -> i64 = \\v d -> 0\n",
        "def f :: own (pure (i64 -> 'r) | pure (i64 -> i64 -> i64)) -> 'r -> i64 = \\v d -> 0\n",
        // The results decide, and a result is a type variable.
        "def f :: own ((i64 -> 'r) | (i64 -> i64)) -> 'r -> i64 = \\v d -> 0\n",
        "def f :: own ((i64 -> 'r) | (i64 -> 's)) -> 'r -> 's -> i64 = \\v d e -> 0\n",
    ] {
        rejects(source, "E1041", "anonymous_union_indistinct_members");
    }
    let error = rejects(
        "def f :: own ((i64 -> 'r) | (i64 -> bool -> i64)) -> 'r -> i64 = \\v d -> 0\n",
        "E1041",
        "anonymous_union_indistinct_members",
    );
    assert_eq!(
        error.message,
        "i64 -> 'r and i64 -> bool -> i64 may be the same type for some type arguments, so they cannot both be members of an anonymous union"
    );
    for source in [
        // A parameter decides before the result is read.
        "def f :: own ((i64 -> 'r) | (string -> i64 -> string)) -> 'r -> i64 = \\v d -> 0\n",
        "def f :: own ((i64 -> 'r) | (string -> 'r)) -> 'r -> i64 = \\v d -> 0\n",
        // The shorter chain cannot grow: its result is known.
        "def f :: own ((i64 -> string) | (i64 -> i64 -> 'r)) -> 'r -> i64 = \\v d -> 0\n",
        "def f :: own (pure (i64 -> 'r) | pure (string -> i64 -> i64)) -> 'r -> i64 = \\v d -> 0\n",
    ] {
        accepts(source);
    }
    // The parameters are compared before their number, so a chain that grows keeps its place:
    // `(i64 -> 'r)` is before `(string -> i64)` whatever `'r` is.
    let generic =
        "def mk :: 'r -> ((i64 -> 'r) | (string -> i64)) = \\witness ->\n    \\_x -> witness\n";
    passes(
        &format!(
            "{generic}def kind :: own ((string -> i64) | (i64 -> i64 -> i64)) -> i64 = \\v ->\n    match v with\n    | :? (i64 -> i64 -> i64) as f -> f 1 2\n    | :? (string -> i64) -> -1\n\ntest \"the result is a function\" =\n    let add: (i64 -> i64) = \\y -> y + 40\n    assert (kind (mk add) == 42)\n    let text: (string -> i64) = \\s -> s.length\n    assert (kind text == -1)\n"
        ),
        Edition::E2026,
    );
    // The instance reads as the same union written with its type arguments.
    for source in [
        format!(
            "{generic}let add: (i64 -> i64) = \\y -> y\nlet made = mk add\nlet v: i64 = made\n0\n"
        ),
        "def f :: ((string -> i64) | (i64 -> i64 -> i64)) -> i64 = \\v -> v\n".to_owned(),
    ] {
        let error = rejects(&source, "E1003", "type_mismatch");
        assert_eq!(
            error.message, "expected i64, found ((i64 -> i64 -> i64) | (string -> i64))",
            "{source}"
        );
    }
    // With equal parameters the shorter chain is first, then the results decide.
    let error = rejects(
        "def f :: ((i64 -> i64 -> string) | (i64 -> string) | (i64 -> i64 -> bool) | (bool -> i64 -> i64 -> i64)) -> i64 = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(
        error.message,
        "expected i64, found ((i64 -> string) | (i64 -> i64 -> bool) | (i64 -> i64 -> string) | (bool -> i64 -> i64 -> i64))"
    );
}

/// Review round 1: purity picks no member when a value is injected (a pure function is also a
/// value of the function type that is not pure), so two members that differ only in it are
/// rejected where the union is written.
#[test]
fn members_that_differ_only_in_purity_are_rejected() {
    let error = rejects(
        "def f :: own ((i64 -> i64) | pure (i64 -> i64) | string) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_indistinct_members",
    );
    assert_eq!(
        error.message,
        "i64 -> i64 and pure (i64 -> i64) differ only in the purity of a function type, and purity does not pick a member, so they cannot both be members of an anonymous union"
    );
    assert_eq!(
        error.help.as_deref(),
        Some(
            "a pure function is also a value of the function type that is not pure; keep one of the two members, or declare a union with named cases"
        )
    );
    for source in [
        "def f :: own (pure (i64 -> i64) | (i64 -> i64)) -> i64 = \\v -> 0\n",
        // Inside another member.
        "def f :: own ([pure (i64 -> i64)] | [(i64 -> i64)]) -> i64 = \\v -> 0\n",
        "def f :: own (((i64 -> i64) -> i64) | (pure (i64 -> i64) -> i64)) -> i64 = \\v -> 0\n",
        // A pure function whose result is a function is, without its purity, the curried one.
        "def f :: own (pure (i64 -> (i64 -> i64)) | (i64 -> i64 -> i64)) -> i64 = \\v -> 0\n",
    ] {
        rejects(source, "E1041", "anonymous_union_indistinct_members");
    }
    // Two pure function types that differ only in their currying read alike (`Type::display`
    // writes both `pure (i64 -> i64 -> i64)`), so a diagnostic could not name one of them.
    let error = rejects(
        "def f :: own (pure (i64 -> (i64 -> i64)) | pure (i64 -> i64 -> i64)) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_indistinct_members",
    );
    assert_eq!(
        error.message,
        "pure (i64 -> i64 -> i64) and pure (i64 -> i64 -> i64) differ only in how a pure function type is curried, which their text does not show, so they cannot both be members of an anonymous union"
    );
    rejects(
        "def f :: own (pure (i64 -> i64) | pure (i64 -> i64)) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_duplicate_member",
    );
    // A pure and an impure member that differ elsewhere are two members, and a value picks one.
    passes(
        "def apply :: own (pure (i64 -> i64) | (string -> i64) | bool) -> i64 = \\f ->\n    match f with\n    | :? pure (i64 -> i64) as g -> g 41\n    | :? (string -> i64) as h -> h \"abc\"\n    | :? bool -> 0\n\ntest \"purity beside another difference\" =\n    let inc: pure (i64 -> i64) = \\x -> x + 1\n    let length: (string -> i64) = \\s -> s.length\n    assert (apply inc == 42)\n    assert (apply length == 3)\n    assert (apply true == 0)\n",
        Edition::E2026,
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

/// Review round 1: an array literal is as much a `[T; N]` as a `[T]`, so no member is its
/// default: its length and its elements decide, and two members that fit are an error.
#[test]
fn array_literals_do_not_default_to_the_dynamic_array() {
    let both = "def f :: own ([i64; 3] | [i64]) -> i64 = \\v ->\n    match v with\n    | :? [i64; 3] -> 3\n    | :? [i64] as a -> 100 + a.length\n";
    for (source, start) in [
        ("let v: ([i64; 3] | [i64]) = [1, 2, 3]\n0\n".to_owned(), 28),
        (format!("{both}f [1, 2, 3]\n"), both.len() + 2),
        // A piped literal is the same literal.
        (format!("{both}[1, 2, 3] |> f\n"), both.len()),
    ] {
        let error = rejects(&source, "E1041", "anonymous_union_ambiguous_member");
        assert_eq!(
            error.message, "this array literal could be [i64] or [i64; 3] in ([i64] | [i64; 3])",
            "{source}"
        );
        assert_eq!(
            error.help.as_deref(),
            Some("annotate the value, for example with a typed let binding")
        );
        assert_eq!(
            (error.span.start, error.span.end),
            (start, start + 9),
            "{source}"
        );
    }
    passes(
        &format!(
            "{both}def kind :: own ([i64; 3] | [string]) -> i64 = \\u ->\n    match u with\n    | :? [i64; 3] as a -> a[0] + a[2]\n    | :? [string] as s -> 100 + s.length\n\ndef two :: own ([i64; 2] | [i64; 3]) -> i64 = \\u ->\n    match u with\n    | :? [i64; 2] as a -> a[0] + a[1]\n    | :? [i64; 3] as a -> a[0] + a[1] + a[2]\n\ntest \"length and elements decide\" =\n    assert (f [1, 2] == 102)\n    assert (f (new [1, 2, 3]) == 103)\n    let named = [1, 2, 3]\n    assert (f named == 103)\n    let typed: [i64; 3] = [1, 2, 3]\n    assert (f typed == 3)\n    assert (kind [5, 6, 7] == 12)\n    assert (kind [\"a\", \"b\", \"c\"] == 103)\n    assert (([4, 5, 6] |> kind) == 10)\n    assert (two [1, 2, 3] == 6)\n    assert (two [1, 2] == 3)\n    let w: ([i64; 3] | [string]) = [1, 2, 3]\n    assert (kind w == 4)\n"
        ),
        Edition::E2026,
    );
    // Neither member takes the literal.
    let error = rejects(
        "let v: ([i64; 2] | [string]) = [1, 2, 3]\n0\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected string, found i32");
}

/// Review round 1: the result of a function value that is still open may be a function of more
/// parameters, so such a value is not silently the member with fewer parameters.
#[test]
fn open_function_results_may_be_curried_members() {
    let pick = "def pick :: own ((string -> i64) | (string -> i64 -> i64)) -> i64 = \\f ->\n    match f with\n    | :? (string -> i64) as g -> g \"abc\"\n    | :? (string -> i64 -> i64) as g -> g \"abc\" 40\n\ndef later :: unit -> 'r = \\_ -> unreachable ()\n";
    let error = rejects(
        &format!("{pick}pick (\\s -> later ())\n"),
        "E1041",
        "anonymous_union_ambiguous_member",
    );
    assert!(
        error
            .message
            .ends_with("could be (string -> i64) or (string -> i64 -> i64) in ((string -> i64) | (string -> i64 -> i64))"),
        "{}",
        error.message
    );
    passes(
        &format!(
            "{pick}\ntest \"a known result decides\" =\n    assert (pick (\\(s: string) -> s.length) == 3)\n    let two: (string -> i64 -> i64) = \\s n -> s.length + n\n    assert (pick two == 43)\n    let curried = \\(s: string) -> \\(n: i64) -> s.length + n + 1\n    assert (pick curried == 44)\n"
        ),
        Edition::E2026,
    );
}

/// Review round 1: a number literal inside the value reads as the type that it takes when
/// nothing decides it, as a literal by itself does.
#[test]
fn diagnostics_name_nested_number_literals() {
    let error = rejects(
        "let v: (Maybe<(i64 | string)> | i64) = Some 4\n0\n",
        "E1003",
        "anonymous_union_not_member",
    );
    assert_eq!(
        error.message,
        "expected (i64 | Maybe<(i64 | string)>), found Maybe<i32>; Maybe<i32> is not a member of the anonymous union"
    );
    let error = rejects(
        "def e :: ((i64 * f64) | (i64u * f64)) -> i64 = \\v -> 0\ne (1, 2.5)\n",
        "E1041",
        "anonymous_union_ambiguous_member",
    );
    assert_eq!(
        error.message,
        "this value of type (i32 * f64) could be (i64u * f64) or (i64 * f64) in ((i64u * f64) | (i64 * f64))"
    );
}

/// Review round 1: an anonymous union takes no instance (D8), so `E1005` and `E1025` do not ask
/// for one; a named type keeps the usual advice. Since A26 the classes that an anonymous union
/// lacks are those without a structural instance, such as `Default`, `Add` and `Arbitrary`.
#[test]
fn missing_instances_do_not_ask_for_an_instance() {
    let advice = "instances cannot be declared for an anonymous union; match on its members, wrap it in a record, or declare a union with named cases";
    for (source, code, kind, class) in [
        (
            "let v: (i64 | string) = 1\nlet w: (i64 | string) = 2\nv + w\n",
            "E1005",
            "no_instance",
            "Add",
        ),
        (
            "let v: (i64 | string) = Default.default()\n0\n",
            "E1005",
            "no_instance",
            "Default",
        ),
        (
            "record R { v: (i64 | string) } deriving (Default)\n0\n",
            "E1025",
            "derived_instance_invalid",
            "Default",
        ),
    ] {
        let error = rejects(source, code, kind);
        assert_eq!(
            error.message,
            format!("no instance for {class}<(i64 | string)>; {advice}"),
            "{source}"
        );
    }
    let error = rejects(
        "record Point { x: i64 }\nlet p = Point { x: 1 }\nlet q = Point { x: 1 }\np == q\n",
        "E1005",
        "no_instance",
    );
    assert_eq!(
        error.message,
        "no instance for Eq<Main.Point>; define an instance or use a supported type"
    );
}

#[test]
fn comparisons_and_exports_keep_their_rules() {
    // `1` is injected into the left operand's type, which has `Eq` since A26
    // (`comparison_injects_right_operand` runs it).
    accepts("let v: (i64 | string) = 1\nv == 1\n");
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

/// Review round 1: a type test inside an expression of its own (a `match` or a lambda in a guard
/// predicate or in an active pattern's argument) does not make the arm or the group a pattern.
#[test]
fn nested_type_tests_stay_in_their_expressions() {
    passes(
        "def (|Holds|_|) :: (i64 -> bool) -> i64 -> bool = \\pred -> \\value ->\n    pred value\n\ndef mk :: i64 -> (i64 | string) = \\x -> if x > 0 then x else \"s\"\ndef small :: i64 -> bool = \\k -> k < 10\ndef same :: bool -> bool = \\b -> b\n\ndef check :: i64 -> i64 = \\v ->\n    match v with\n    | Holds (\\x -> match mk x with | :? i64 as n -> n > 0 | :? string -> false) -> 1\n    | _ -> 0\n\ndef classify :: i64 -> i64 = \\x ->\n    | same (match mk x with | :? i64 -> true | :? string -> false) -> 1\n    | _ -> 0\n\ndef sized :: own (i64 | string) -> i64 = \\v ->\n    | small (match v with | :? i64 as k -> k | :? string -> 0) -> 1\n    | otherwise -> 0\n\ndef both :: i64 -> own (i64 | string) -> i64 = \\n v ->\n    | n > 0 && (match v with | :? i64 -> true | :? string -> false) -> n\n    | otherwise -> 0\n\ndef tested :: own (Maybe<i64> | string) -> i64 = \\v ->\n    | (:? Maybe<i64> as m) -> (match m with | Some n -> n | None -> -1)\n    | (:? string) -> -2\n\ntest \"predicates and arguments\" =\n    assert (check 5 == 1)\n    assert (check (-5) == 0)\n    assert (classify 5 == 1)\n    assert (classify (-5) == 0)\n    assert (sized 3 == 1)\n    assert (sized 30 == 0)\n    assert (sized \"x\" == 1)\n    assert (both 4 7 == 4)\n    assert (both 4 \"s\" == 0)\n    assert (both (-4) 7 == 0)\n    assert (tested (Some 9) == 9)\n    assert (tested \"s\" == -2)\n",
        Edition::E2026,
    );
    // A lambda whose parameter is a type test, inside a predicate: the arm is still a predicate
    // (a pattern would be a parse error here).
    let program = parsed(
        "def same :: bool -> bool = \\b -> b\ndef positive :: i64 -> ((i64 | string) -> bool) -> i64 = \\x f ->\n    | same (f x) && same ((\\(g: (i64 | string) -> bool) -> g 1) (\\(:? i64 as n) -> n > 0)) -> 1\n    | otherwise -> 0\n",
    );
    assert!(program.uses_anonymous_unions);
    // A pattern that holds such an expression and a type test of its own is still a pattern.
    assert_type_test(
        "def (|Holds|_|) :: (i64 -> bool) -> i64 -> bool = \\pred -> \\value -> pred value\ndef f :: (i64 * (i64 | string)) -> i64 = \\v ->\n    match v with\n    | (Holds (\\x -> match x with | 0 -> true | _ -> false), :? string as s) -> 1\n    | _ -> 0\n",
        "Named(\"string\")",
        Some("s"),
    );
    // A hole after a field's colon is no type test: the arm stays a predicate, and the hole
    // reports its type.
    let hole = "record R { x: i64 }\ndef g :: R -> bool = \\r -> r.x > 0\ndef f :: (i64 | string) -> i64 = \\n ->\n    | g (R { x:? }) -> 1\n    | otherwise -> 0\n";
    parsed(hole);
    let error = analyze(hole).expect_err(hole);
    assert_eq!(error.code, "E1030", "{}", error.message);
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

/// Review round 1: members of the same last name pair in any order, qualified or not.
#[test]
fn find_pairs_members_of_the_same_name() {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-anonymous-unions-find-names-{}",
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
    std::fs::write(project.join("Shapes.tz"), "record Point { x: i64 }\n").unwrap();
    std::fs::write(project.join("Alpha.tz"), "record Point { y: i64 }\n").unwrap();
    std::fs::write(
        project.join("Main.tz"),
        "record Point { z: i64 }\ndef pick :: own (Shapes.Point | Point) -> i64 = \\v -> 0\ndef other :: own (Shapes.Point | Alpha.Point) -> i64 = \\v -> 0\n0\n",
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
    let exact = |text: &str, name: &str| text.contains(&format!("exact   Finder::Main.{name} ::"));
    for query in [
        "(Shapes.Point | Main.Point) -> i64",
        "(Main.Point | Shapes.Point) -> i64",
    ] {
        let text = find(query);
        assert!(exact(&text, "pick"), "{query}\n{text}");
        assert!(!exact(&text, "other"), "{query}\n{text}");
    }
    for query in [
        "(Shapes.Point | Alpha.Point) -> i64",
        "(Alpha.Point | Shapes.Point) -> i64",
    ] {
        let text = find(query);
        assert!(exact(&text, "other"), "{query}\n{text}");
        assert!(!exact(&text, "pick"), "{query}\n{text}");
    }
    // An unqualified name is any of the types of that name.
    for query in [
        "(Point | Shapes.Point) -> i64",
        "(Shapes.Point | Point) -> i64",
        "(Point | Point) -> i64",
    ] {
        let text = find(query);
        assert!(
            exact(&text, "pick") && exact(&text, "other"),
            "{query}\n{text}"
        );
    }
    assert!(!exact(&find("(Main.Point | Main.Point) -> i64"), "pick"));
    std::fs::remove_dir_all(&root).unwrap();
}

/// Review round 1: the type of a type test is a reference to its declaration, as an annotation
/// is: hover, go to definition, references and rename see it.
#[test]
fn type_tests_are_type_references() {
    let source = "union Tree = Leaf | Node of (i64 | Tree)\nrecord Point { x: i64 }\n\ndef rec depth :: Tree -> i64 = \\tree ->\n    match tree with\n    | Leaf -> 0\n    | Node (:? i64) -> 1\n    | Node (:? Tree as rest) -> 1 + depth rest\n\ndef size :: own (i64 | Point | Maybe<Point>) -> i64 = \\v ->\n    | :? Point as p -> p.x\n    | :? Maybe<Point> -> 0\n    | :? i64 as n -> n\n\ndef total :: [(i64 | Point)] -> i64 = \\values ->\n    let mut sum = 0\n    for :? Point as p in values do\n        sum = sum + p.x\n    sum\n0\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", source)]).unwrap();
    let declaration = |name: &str| {
        index
            .definitions
            .iter()
            .position(|definition| definition.name == name)
            .unwrap_or_else(|| panic!("no definition of {name}"))
    };
    let referenced = |needle: &str, offset: usize, name: &str| {
        let start = source.find(needle).unwrap() + offset;
        assert!(
            index
                .occurrences(declaration(name))
                .iter()
                .any(|occurrence| occurrence.span.start == start
                    && occurrence.span.end == start + name.len()),
            "no reference to {name} at {needle:?}"
        );
        let entry = index
            .at(0, start)
            .unwrap_or_else(|| panic!("no hover at {needle:?}"));
        assert_eq!(entry.detail, format!("Main.{name}"), "{needle}");
        assert!(entry.target.is_some(), "{needle}");
    };
    referenced(":? Tree as rest", 3, "Tree");
    referenced(":? Point as p -> p.x", 3, "Point");
    referenced(":? Maybe<Point>", 9, "Point");
    referenced(":? Point as p in values", 3, "Point");
    // A member that is no declaration reads as itself and goes nowhere.
    let builtin = index.at(0, source.find(":? i64)").unwrap() + 3).unwrap();
    assert_eq!(builtin.detail, "i64");
    assert_eq!(builtin.target, None);
}

// A26: the built-in instances `Eq`, `Ord`, `Hash` and `Display` of anonymous unions. The expected
// values are worked out by hand from the rules: the members compare in the canonical order
// (`displays_in_canonical_order`), then by payload; the hash is the stream of a derived union.

/// `Hash.hash` of `A 1` and of `B "one"` of `union R = A of i64 | B of string deriving (Hash)`:
/// FNV-1a over the canonical stream (tag `0x55`, the case, the hash of the payload), computed by
/// a Python script from the definition of the stream in `docs/language.md`, and equal to what the
/// compiler gave for the named union before A26.
const HASH_OF_1: &str = "15967365493956732001i64u";
const HASH_OF_ONE: &str = "17017872639125386659i64u";

#[test]
fn anonymous_unions_have_structural_classes() {
    // Operators, methods and the constraints of generic functions, in both editions.
    let source = "def same :: Eq<'a> => own 'a -> own 'a -> bool = \\x y -> x == y\ndef before :: Ord<'a> => own 'a -> own 'a -> bool = \\x y -> x < y\ndef hashed :: Hash<'a> => own 'a -> i64u = \\x -> Hash.hash x\ndef shown :: Display<'a> => own 'a -> string = \\x -> Display.display x\ndef open :: Eq<'a> => own (['a] | i64) -> own (['a] | i64) -> bool = \\x y -> x == y\ndef wide :: (Ord<'a>, Display<'a>) => own ([|'a|] | 'a * 'a | string) -> own ([|'a|] | 'a * 'a | string) -> string = \\x y ->\n    if x <= y then Display.display x else Display.display y\n\ntest \"constraints\" =\n    let a: (i64 | string) = 1\n    let b: (string | i64) = \"one\"\n    assert (not (same a b))\n    let c: (i64 | string) = 1\n    let d: (string | i64) = \"one\"\n    assert (before c d)\n    let e: (i64 | string) = 1\n    assert (hashed e == 15967365493956732001i64u)\n    let f: (i64 | string) = \"one\"\n    assert (shown f == \"one\")\n    assert (open [2] [2])\n    assert (not (open [2] 2))\n    let first: ([|i64|] | i64 * i64 | string) = (2, 1)\n    let second: ([|i64|] | i64 * i64 | string) = [|9|]\n    assert (wide first second == \"[|9|]\")\n\ntest \"methods\" =\n    let a: (i64 | string) = 1\n    let b: (i64 | string) = \"one\"\n    assert (Eq.ne a b)\n    assert (not (Eq.eq a b))\n    assert (Ord.lt a b)\n    assert (Ord.le a b)\n    assert (not (Ord.gt a b))\n    assert (not (Ord.ge a b))\n    assert (Ord.ge b a)\n";
    passes(source, Edition::E2026);
    passes(source, Edition::E2027);
    // The IR is deterministic, and its names are LLVM identifiers.
    accepts(
        "let a: (i64 | string) = 1\nlet b: (i64 | string) = \"one\"\nlet hash = if a == b || a < b then Hash.hash (ref a) else Hash.hash (ref b)\nlet shown = Display.display (ref (a, b))\nhash\n",
    );
    // A user instance is still rejected by the rule of A24, not as an override of a built-in one.
    rejects(
        "instance Eq<(i64 | string)> {\n}\n0\n",
        "E1041",
        "anonymous_union_instance",
    );
}

#[test]
fn missing_member_instance_names_the_member() {
    let point = "record Point { x: i64 }\n";
    for (source, code, kind, message) in [
        (
            format!("{point}let a: (i64 | Point) = 1\nlet b: (i64 | Point) = 1\na == b\n"),
            "E1005",
            "no_instance",
            "no instance for Eq<Main.Point>; define an instance or use a supported type",
        ),
        (
            format!("{point}let a: (i64 | Point) = 1\nlet s = Display.display (ref a)\n0\n"),
            "E1005",
            "no_instance",
            "no instance for Display<Main.Point>; define an instance or use a supported type",
        ),
        (
            format!("{point}let a: (i64 | Point) = 1\nlet h = Hash.hash (ref a)\n0\n"),
            "E1005",
            "no_instance",
            "no instance for Hash<Main.Point>; define an instance or use a supported type",
        ),
        // `bool` has `Eq` and no `Ord`; a function has neither.
        (
            "let a: (i64 | bool) = 1\nlet b: (i64 | bool) = 1\na < b\n".to_owned(),
            "E1005",
            "no_instance",
            "no instance for Ord<bool>; define an instance or use a supported type",
        ),
        (
            "let a: (i64 | (i64 -> i64)) = 1\nlet b: (i64 | (i64 -> i64)) = 1\na == b\n".to_owned(),
            "E1005",
            "no_instance",
            "no instance for Eq<i64 -> i64>; define an instance or use a supported type",
        ),
        (
            format!("{point}record R {{ v: (i64 | Point) }} deriving (Eq)\n0\n"),
            "E1025",
            "derived_instance_invalid",
            "no instance for Eq<Main.Point>; define an instance or use a supported type",
        ),
    ] {
        let error = rejects(&source, code, kind);
        assert_eq!(error.message, message, "{source}");
    }
    // `(i64 | bool)` has `Eq` all the same.
    accepts("let a: (i64 | bool) = 1\nlet b: (i64 | bool) = true\na == b\n");
}

#[test]
fn no_default_for_anonymous_unions() {
    // No member is the default one, and the other classes have no structural instance either.
    for (source, code, kind, class) in [
        (
            "let v: (i64 | string) = Default.default()\n0\n",
            "E1005",
            "no_instance",
            "Default",
        ),
        (
            "record R { v: (i64 | string) } deriving (Default)\n0\n",
            "E1025",
            "derived_instance_invalid",
            "Default",
        ),
        (
            "record R { v: (i64 | string) } deriving (Arbitrary)\n0\n",
            "E1025",
            "derived_instance_invalid",
            "Arbitrary",
        ),
        (
            "let v: (i64 | i32) = 1i64\nlet w: (i64 | i32) = 2i64\nv + w\n",
            "E1005",
            "no_instance",
            "Add",
        ),
    ] {
        let error = rejects(source, code, kind);
        assert!(
            error
                .message
                .starts_with(&format!("no instance for {class}<(")),
            "{source}\n{}",
            error.message
        );
    }
    // A tuple of defaults is still a default: only the union is refused.
    let error = rejects(
        "let v: (i64 * (i64 | string)) = Default.default()\n0\n",
        "E1005",
        "no_instance",
    );
    assert!(
        error
            .message
            .starts_with("no instance for Default<(i64 | string)>"),
        "{}",
        error.message
    );
}

#[test]
fn eq_compares_tag_then_payload() {
    passes(
        "test \"members\" =\n    let one: (i64 | string) = 1\n    let uno: (string | i64) = 1\n    let two: (i64 | string) = 2\n    let text: (i64 | string) = \"1\"\n    let same: (i64 | string) = \"1\"\n    assert (one == uno)\n    assert (not (one != uno))\n    assert (not (one == two))\n    assert (one != two)\n    assert (not (one == text))\n    assert (one != text)\n    assert (text == same)\n    assert (not (text != same))\n\ntest \"owned payloads\" =\n    let a: (string | [i64] | [|string|]) = [1, 2]\n    let b: (string | [i64] | [|string|]) = [1, 2]\n    let c: (string | [i64] | [|string|]) = [1, 3]\n    let d: (string | [i64] | [|string|]) = [|\"1\", \"2\"|]\n    let e: (string | [i64] | [|string|]) = [|\"1\", \"2\"|]\n    assert (a == b)\n    assert (a != c)\n    assert (a != d)\n    assert (d == e)\n    assert (a == b)\n\ntest \"one layout for every member\" =\n    let signed: (i64 | i64u) = 5i64\n    let unsigned: (i64 | i64u) = 5i64u\n    let again: (i64 | i64u) = 5i64u\n    assert (signed != unsigned)\n    assert (not (signed == unsigned))\n    assert (unsigned == again)\n\ntest \"inside other types\" =\n    let left: ((i64 | string) * [(i64 | string)]) = (1, [\"a\", 2])\n    let right: ((i64 | string) * [(i64 | string)]) = (1, [\"a\", 2])\n    let other: ((i64 | string) * [(i64 | string)]) = (1, [\"a\", \"2\"])\n    assert (left == right)\n    assert (left != other)\n",
        Edition::E2027,
    );
}

#[test]
fn ord_follows_member_order() {
    // The canonical order is i64 < f64 < bool < char < string < [i64] < (i64 * i64), however the
    // type is written. `bool` has no `Ord`, so it is left out here.
    passes(
        "test \"members then payloads\" =\n    let number: (string | i64) = 9\n    let text: (string | i64) = \"1\"\n    let small: (i64 | string) = -3\n    let later: (i64 | string) = \"a\"\n    assert (number < text)\n    assert (number <= text)\n    assert (not (number > text))\n    assert (not (number >= text))\n    assert (text > number)\n    assert (text >= number)\n    assert (not (text < number))\n    assert (not (text <= number))\n    assert (small < number)\n    assert (small <= number)\n    assert (not (small > number))\n    assert (number >= small)\n    assert (text < later)\n    assert (later > text)\n    assert (number <= number)\n    assert (number >= number)\n    assert (not (number < number))\n    assert (not (text > text))\n\ntest \"written order does not matter\" =\n    let pair: ((i64 * i64) | [i64] | string | char | f64 | i64) = (0, 0)\n    let array: ((i64 * i64) | [i64] | string | char | f64 | i64) = [9]\n    let text: ((i64 * i64) | [i64] | string | char | f64 | i64) = \"z\"\n    let letter: ((i64 * i64) | [i64] | string | char | f64 | i64) = 'z'\n    let float: ((i64 * i64) | [i64] | string | char | f64 | i64) = 1.0e300\n    let integer: ((i64 * i64) | [i64] | string | char | f64 | i64) = 9000000000000000000\n    assert (integer < float)\n    assert (float < letter)\n    assert (letter < text)\n    assert (text < array)\n    assert (array < pair)\n    assert (pair > integer)\n    assert (not (pair <= array))\n\ntest \"sorting\" =\n    let sorted: [(char | i64)] = Array.sort ['b', 3, 'a', 1]\n    assert (Display.display sorted == \"[1, 3, 'a', 'b']\")\n    let floats: [(f64 | i64)] = Array.sort [2.5, 3, 1.5, 1]\n    assert (Display.display floats == \"[1, 3, 1.5, 2.5]\")\n\ntest \"one layout for every member\" =\n    let signed: (i64 | i64u) = 0i64\n    let unsigned: (i64 | i64u) = 18446744073709551615i64u\n    assert (unsigned < signed)\n    assert (signed >= unsigned)\n",
        Edition::E2027,
    );
}

#[test]
fn hash_matches_named_union_stream() {
    // The named union with the same cases in the canonical order, and the two reference values.
    passes(
        &format!(
            "union R = A of i64 | B of string deriving (Hash)\n\ntest \"stream\" =\n    let one: (string | i64) = 1\n    let text: (i64 | string) = \"one\"\n    assert (Hash.hash one == {HASH_OF_1})\n    assert (Hash.hash text == {HASH_OF_ONE})\n    assert (Hash.hash (A 1) == {HASH_OF_1})\n    assert (Hash.hash (B \"one\") == {HASH_OF_ONE})\n    assert (Hash.hash one == Hash.hash (A 1))\n    assert (Hash.hash text == Hash.hash (B \"one\"))\n\ntest \"equal values hash equally\" =\n    let a: (string | [i64]) = [1, 2]\n    let b: ([i64] | string) = [1, 2]\n    let c: (string | [i64]) = [2, 1]\n    assert (a == b)\n    assert (Hash.hash a == Hash.hash b)\n    assert (Hash.hash a != Hash.hash c)\n    let zero: (f64 | string) = 0.0\n    let negative: (f64 | string) = -0.0\n    assert (zero == negative)\n    assert (Hash.hash zero == Hash.hash negative)\n\ntest \"the member is part of the hash\" =\n    let signed: (i64 | i64u) = 1i64\n    let unsigned: (i64 | i64u) = 1i64u\n    assert (Hash.hash signed != Hash.hash unsigned)\n    assert (Hash.hash (1, signed) != Hash.hash (1, unsigned))\n"
        ),
        Edition::E2027,
    );
}

#[test]
fn display_shows_the_payload() {
    passes(
        "test \"payload only\" =\n    let number: (i64 | string | char | [string]) = -7\n    let text: (i64 | string | char | [string]) = \"one\"\n    let letter: (i64 | string | char | [string]) = 'c'\n    let array: (i64 | string | char | [string]) = [\"a\", \"b\"]\n    assert (Display.display number == \"-7\")\n    assert (Display.display text == \"one\")\n    assert (Display.display letter == \"c\")\n    assert (Display.display array == \"[\\\"a\\\", \\\"b\\\"]\")\n    assert ($\"{number} {text} {letter} {array}\" == \"-7 one c [\\\"a\\\", \\\"b\\\"]\")\n    assert (to_string text == \"one\")\n",
        Edition::E2027,
    );
}

#[test]
fn display_quotes_inside_tuples() {
    // Inside a tuple, an array, a list, and a derived record or union, a string or character
    // member is quoted, as a string or character element is.
    passes(
        "record Cell { value: (i64 | string), tag: string } deriving (Display)\nunion Shape = Dot | Text of (string | char) deriving (Display)\n\ntest \"containers\" =\n    let a: (i64 | string) = 1\n    let b: (i64 | string) = \"one\"\n    assert (Display.display (a, b) == \"(1, \\\"one\\\")\")\n    let items: [(char | string | i64)] = ['c', \"s\", 3]\n    assert (Display.display items == \"['c', \\\"s\\\", 3]\")\n    let list: [|(i64 | string)|] = [|\"x\", 2|]\n    assert (Display.display list == \"[|\\\"x\\\", 2|]\")\n    let nested: ((i64 | string) * [(i64 | string)]) = (\"p\", [\"q\", 1])\n    assert (Display.display nested == \"(\\\"p\\\", [\\\"q\\\", 1])\")\n    assert ($\"{nested}\" == \"(\\\"p\\\", [\\\"q\\\", 1])\")\n\ntest \"derived instances\" =\n    assert (Display.display (Cell { value: \"one\", tag: \"a\" }) == \"Cell { value: \\\"one\\\", tag: \\\"a\\\" }\")\n    assert (Display.display (Cell { value: 1, tag: \"a\" }) == \"Cell { value: 1, tag: \\\"a\\\" }\")\n    assert (Display.display (Text 'c') == \"Text 'c'\")\n    assert (Display.display (Text \"s\") == \"Text \\\"s\\\"\")\n",
        Edition::E2027,
    );
}

#[test]
fn nan_members_follow_member_eq() {
    passes(
        "test \"nan\" =\n    let zero = 0.0\n    let nan: (f64 | string) = zero / zero\n    let other: (f64 | string) = zero / zero\n    let plus: (f64 | string) = 0.0\n    let minus: (f64 | string) = -0.0\n    let text: (f64 | string) = \"nan\"\n    assert (not (nan == other))\n    assert (nan != other)\n    assert (not (nan < other))\n    assert (not (nan <= other))\n    assert (not (nan > other))\n    assert (not (nan >= other))\n    assert (plus == minus)\n    assert (not (plus != minus))\n    assert (plus <= minus)\n    assert (not (plus < minus))\n    assert (nan < text)\n    assert (nan != text)\n",
        Edition::E2027,
    );
}

#[test]
fn recursive_types_compare_through_named_unions() {
    // The union goes through the heap node of `Tree`; the derived instances of `Tree` and the
    // built-in ones of `(i64 | Tree)` call each other.
    passes(
        "union Tree = Leaf | Node of (i64 | Tree) deriving (Eq, Ord, Hash, Display)\n\ntest \"recursive\" =\n    let one = Node (Node 1)\n    let same = Node (Node 1)\n    let two = Node (Node 2)\n    let deep = Node (Node (Node Leaf))\n    assert (one == same)\n    assert (one != two)\n    assert (one < two)\n    assert (Leaf < one)\n    assert (two < deep)\n    assert (Hash.hash one == Hash.hash same)\n    assert (Hash.hash one != Hash.hash two)\n    assert (Display.display one == \"Node Node 1\")\n    assert (Display.display deep == \"Node Node Node Leaf\")\n",
        Edition::E2027,
    );
}

#[test]
fn wide_unions_have_instances() {
    // 32 members (D9), every one with the four classes: the bodies test up to 31 tags in a row.
    let members = [
        "i8",
        "i8u",
        "i16",
        "i16u",
        "i32",
        "i32u",
        "i64",
        "i64u",
        "i128",
        "i128u",
        "f32",
        "f64",
        "char",
        "utf8char",
        "string",
        "utf8string",
        "[i64]",
        "[string]",
        "[char]",
        "[f64]",
        "[|i64|]",
        "[|string|]",
        "(i64 * i64)",
        "(i64 * string)",
        "(string * i64)",
        "(string * string)",
        "[[i64]]",
        "[[string]]",
        "(i64 * i64 * i64)",
        "(i64 * (i64 * i64))",
        "[i8]",
        "[i16]",
    ];
    assert_eq!(members.len(), 32);
    // `i8u` is the first member in the canonical order and the tuple of three the last one: a
    // tuple follows every array and list, and a longer tuple the shorter ones.
    passes(
        &format!(
            "type Wide = ({})\n\ntest \"wide\" =\n    let first: Wide = 1i8u\n    let last: Wide = (1, 2, 3)\n    let text: Wide = \"s\"\n    let again: Wide = (1, 2, 3)\n    assert (first < last)\n    assert (first < text)\n    assert (text < last)\n    assert (last > first)\n    assert (last == again)\n    assert (last != text)\n    assert (last >= again)\n    assert (Hash.hash last == Hash.hash again)\n    assert (Hash.hash last != Hash.hash text)\n    assert (Display.display last == \"(1, 2, 3)\")\n    assert (Display.display text == \"s\")\n    assert (Display.display (text, first) == \"(\\\"s\\\", 1)\")\n",
            members.join(" | ")
        ),
        Edition::E2027,
    );
}

#[test]
fn derived_record_with_anonymous_field() {
    passes(
        "record Cell { value: (i64 | string), tag: string } deriving (Eq, Ord, Hash, Display)\nunion Shape = Dot | Text of (string | char) deriving (Eq, Ord, Hash, Display)\nrecord Pair<'a> { left: (['a] | string), right: 'a } deriving (Eq, Ord, Hash, Display)\n\ntest \"record\" =\n    let number = Cell { value: 1, tag: \"z\" }\n    let again = Cell { value: 1, tag: \"z\" }\n    let text = Cell { value: \"one\", tag: \"a\" }\n    assert (number == again)\n    assert (number != text)\n    assert (number < text)\n    assert (text >= number)\n    assert (Hash.hash number == Hash.hash again)\n    assert (Hash.hash number != Hash.hash text)\n    assert (Display.display text == \"Cell { value: \\\"one\\\", tag: \\\"a\\\" }\")\n\ntest \"union\" =\n    assert (Text 's' == Text 's')\n    assert (Text 's' != Text \"s\")\n    assert (Text 'z' < Text \"a\")\n    assert (Dot < Text 'a')\n    assert (Hash.hash (Text 's') != Hash.hash (Text \"s\"))\n    assert (Display.display (Text \"s\") == \"Text \\\"s\\\"\")\n\ntest \"generic record\" =\n    let left = Pair { left: \"1\", right: 2 }\n    let right = Pair { left: [1], right: 2 }\n    assert (left != right)\n    assert (left < right)\n    assert (right == Pair { left: [1], right: 2 })\n    assert (Display.display left == \"Pair { left: \\\"1\\\", right: 2 }\")\n    assert (Display.display right == \"Pair { left: [1], right: 2 }\")\n",
        Edition::E2027,
    );
}

#[test]
fn hashset_and_map_keys() {
    passes(
        "def key :: own (i64 | string) -> (i64 | string) = \\v -> v\n\ndef found :: own Maybe<i64> -> i64 = \\m ->\n    match m with\n    | Some n -> n\n    | None -> -1\n\ntest \"hash keys\" =\n    let seen: HashSet<(i64 | string)> = HashSet.empty() |> HashSet.insert (key 1) |> HashSet.insert (key \"one\") |> HashSet.insert (key 1) |> HashSet.insert (key \"1\")\n    assert (HashSet.length seen == 3)\n    assert (HashSet.contains (key \"one\") seen)\n    assert (HashSet.contains (key 1) seen)\n    assert (not (HashSet.contains (key 2) seen))\n    let table: HashMap<(i64 | string), i64> = HashMap.empty() |> HashMap.insert (key \"one\") 10 |> HashMap.insert (key 1) 20 |> HashMap.insert (key \"one\") 30\n    assert (HashMap.length table == 2)\n    assert (found (HashMap.get (key \"one\") table) == 30)\n    assert (found (HashMap.get (key 1) table) == 20)\n    assert (found (HashMap.get (key \"1\") table) == -1)\n\ntest \"ordered keys\" =\n    let set: Set<(i64 | string)> = Set.empty() |> Set.insert (key \"one\") |> Set.insert (key 2) |> Set.insert (key 1) |> Set.insert (key \"one\")\n    assert (Set.length set == 3)\n    assert (Set.contains (key 2) set)\n    assert (not (Set.contains (key \"two\") set))\n    let map: Map<(i64 | string), i64> = Map.empty() |> Map.insert (key \"one\") 10 |> Map.insert (key 1) 20 |> Map.insert (key \"one\") 30\n    assert (Map.length map == 2)\n    assert (found (Map.get (key \"one\") map) == 30)\n    assert (found (Map.get (key 1) map) == 20)\n    assert (found (Map.get (key 2) map) == -1)\n    let mut keys = \"\"\n    for (k, _) in Map.iter map do\n        keys = keys + Display.display k + \";\"\n    assert (keys == \"1;one;\")\n",
        Edition::E2027,
    );
}

#[test]
fn comparison_injects_right_operand() {
    // The other operand is injected into the union: `v == 1` compares two values of the union.
    passes(
        "test \"right operand\" =\n    let v: (i64 | string) = 1\n    assert (v == 1)\n    assert (not (v == 2))\n    assert (v != \"1\")\n    assert (not (v == \"1\"))\n    assert (v < 2)\n    assert (v < \"\")\n    assert (v >= 1)\n\ntest \"left operand\" =\n    let v: (i64 | string) = 1\n    let t: (i64 | string) = \"one\"\n    assert (1 == v)\n    assert (not (2 == v))\n    assert (0 < v)\n    assert (1 != t)\n    assert (2 < t)\n",
        Edition::E2027,
    );
    // A value that is no member is still rejected, and a literal that fits two members too.
    rejects(
        "let v: (i64 | string) = 1\nv == true\n",
        "E1003",
        "anonymous_union_not_member",
    );
    rejects(
        "let v: (i32 | i64) = 1i64\nv == 1\n",
        "E1041",
        "anonymous_union_ambiguous_literal",
    );
}

#[test]
fn dyn_stores_anonymous_union() {
    // As a tuple is: the built-in instance fills the vtable, and the text is the payload.
    passes(
        "test \"dyn\" =\n    let text: (i64 | string) = \"one\"\n    let number: (i64 | string) = 1\n    let items: [dyn Display] = [Dyn.of text, Dyn.of number, Dyn.of (1, \"one\")]\n    assert (Display.display items[0] == \"one\")\n    assert (Display.display items[1] == \"1\")\n    assert (Display.display items[2] == \"(1, \\\"one\\\")\")\n",
        Edition::E2027,
    );
}

#[test]
fn const_definitions_refuse_the_operators_as_for_tuples() {
    // The message that a tuple and a named union get: the operator is a call of an instance.
    let message = "an operator on a record, tuple, array, or union is not allowed in a const def; apply it to their fields";
    for source in [
        "const def same :: bool =\n    let v: (i64 | string) = 1\n    let w: (i64 | string) = 1\n    v == w\nsame\n",
        "const def same :: bool =\n    let v: (i64 * string) = (1, \"a\")\n    let w: (i64 * string) = (1, \"a\")\n    v == w\nsame\n",
    ] {
        let error = rejects(source, "E1026", "const_aggregate_operator");
        assert_eq!(error.message, message, "{source}");
    }
}

// A26 Phase 2: `Encode` and `Decode` (D7). `Encode` writes the JSON of the payload and no tag, so
// `Decode` exists only when the kind of a JSON value selects one member. The expected texts are
// written by hand from the JSON that std's instances write for the payloads (compact, a record as
// an object in declaration order, a tuple as an array, `None` as `null`) and from the messages of
// `Json.Error` in `std/Json.tz`.

#[test]
fn encode_writes_payload_without_tag() {
    let source = r#"record Point { x: i64, y: string } deriving (Encode)
union Shape = Dot | Line of i64 deriving (Encode)

def text :: Encode<'a> => own 'a -> string = \value ->
    match Json.serialize value with
    | Ok bytes -> String.from_utf8 (ref bytes)
    | Error error -> Display.display error

def cell :: own (i64 | [i64]) -> (i64 | [i64]) = \value -> value

test "payload only" =
    let number: (i64 | string) = 1
    let word: (string | i64) = "one"
    assert (text number == "1")
    assert (text word == "\"one\"")
    let flag: (bool | f64 | [i64] | (i64 * string)) = true
    let ratio: (bool | f64 | [i64] | (i64 * string)) = 1.5
    let items: (bool | f64 | [i64] | (i64 * string)) = [1, 2]
    let pair: (bool | f64 | [i64] | (i64 * string)) = (3, "c")
    assert (text flag == "true")
    assert (text ratio == "1.5")
    assert (text items == "[1,2]")
    assert (text pair == "[3,\"c\"]")

test "members that Decode could not tell apart" =
    let whole: (i64 | f64) = 2
    let half: (i64 | f64) = 0.5
    assert (text whole == "2")
    assert (text half == "0.5")
    let four: Maybe<i64> = Some 4
    let none: Maybe<i64> = None
    let present: (Maybe<i64> | string) = four
    let absent: (Maybe<i64> | string) = none
    assert (text present == "4")
    assert (text absent == "null")
    let line: (Shape | i64) = Line 7
    let dot: (Shape | i64) = Dot
    assert (text line == "{\"Line\":7}")
    assert (text dot == "\"Dot\"")
    let point: (Point | i64) = Point { x: 1, y: "a" }
    assert (text point == "{\"x\":1,\"y\":\"a\"}")
    let json: (Json.Value | i64) = Json.Null
    assert (text json == "null")

test "the error of the member" =
    let zero = 0.0
    let nan: (f64 | string) = zero / zero
    assert (text nan == "json: non-finite number")

test "inside other values" =
    let items: [(i64 | string)] = [1, "two", 3]
    assert (Result.is_ok (Encode.encode (ref items)))
    assert (text items == "[1,\"two\",3]")
    let table: Map<string, (i64 | [i64])> = Map.empty() |> Map.insert "a" (cell 1) |> Map.insert "b" (cell [2, 3])
    assert (text table == "{\"a\":1,\"b\":[2,3]}")
    let nested: ((i64 | string) * Maybe<(bool | string)>) = (1, Some "s")
    assert (text nested == "[1,\"s\"]")
"#;
    passes(source, Edition::E2026);
    passes(source, Edition::E2027);
    // The bodies are functions of the specializer, emitted the same way twice.
    let ir = accepts(
        "let a: (i64 | string) = 1\nlet encoded = Encode.encode (ref a)\nlet decoded: Result<(i64 | string), Json.Error> = Json.deserialize u8\"1\"\nif Result.is_ok (ref encoded) && Result.is_ok (ref decoded) then 1 else 0\n",
    );
    assert!(ir.contains("$intrinsic.Encode.encode."), "{ir}");
    assert!(ir.contains("$intrinsic.Decode.decode."), "{ir}");
    // A member without `Encode` is named, as for the other classes.
    let error = rejects(
        "record Point { x: i64 }\nlet a: (i64 | Point) = 1\nlet t = Json.serialize a\n0\n",
        "E1005",
        "no_instance",
    );
    assert_eq!(
        error.message,
        "no instance for Encode<Main.Point>; define an instance or use a supported type"
    );
}

#[test]
fn dyn_encode_stores_anonymous_union() {
    // As a tuple is: the built-in instance fills the vtable of `dyn Encode`.
    passes(
        r#"test "dyn" =
    let word: (i64 | string) = "one"
    let number: (i64 | string) = 1
    let stored: [dyn Encode] = [Dyn.of word, Dyn.of number, Dyn.of (1, "one")]
    let mut all = ""
    for item in stored do
        all = all + (match Encode.encode item with
            | Ok json -> String.from_utf8 (ref (Json.to_utf8string (ref json)))
            | Error error -> Display.display error) + ";"
    assert (all == "\"one\";1;[1,\"one\"];")
"#,
        Edition::E2027,
    );
}

#[test]
fn decode_selects_member_by_kind() {
    let source = r#"record Point { x: i64, y: string } deriving (Decode)

type Doc = (i64 | bool | string | [i64] | Point)

def bytes :: own string -> utf8string = \text -> Utf8String.from_string (ref text)

def read :: own string -> string = \text ->
    let decoded: Result<Doc, Json.Error> = Json.deserialize (ref (bytes text))
    match decoded with
    | Error error -> "error " + Display.display error
    | Ok value ->
        match value with
        | :? i64 as n -> $"i64 {n}"
        | :? bool as b -> $"bool {b}"
        | :? string as s -> "string " + s
        | :? [i64] as items -> $"array {items}"
        | :? Point as p -> $"point {p.x} {p.y}"

test "one member for each kind" =
    assert (read "42" == "i64 42")
    assert (read "-0" == "i64 0")
    assert (read " true" == "bool true")
    assert (read "false" == "bool false")
    assert (read "\"x\"" == "string x")
    assert (read "\"\"" == "string ")
    assert (read "[1, 2]" == "array [1, 2]")
    assert (read "[]" == "array []")
    assert (read "{\"y\": \"b\", \"x\": 3}" == "point 3 b")

def float_or_text :: own string -> string = \text ->
    let decoded: Result<(string | f64), Json.Error> = Json.deserialize (ref (bytes text))
    match decoded with
    | Ok value ->
        match value with
        | :? f64 as f -> if f == 2.5 then "f64 2.5" elif f == 3.0 then "f64 3" else "f64"
        | :? string as s -> "string " + s
    | Error error -> Display.display error

def list_or_flag :: Decode<'a> => own string -> Result<(bool | ['a]), Json.Error> = \text ->
    Json.deserialize (ref (bytes text))

def collection :: own string -> string = \text ->
    let decoded: Result<(i64 | utf8string | Map<string, i64>), Json.Error> = Json.deserialize (ref (bytes text))
    match decoded with
    | Ok value ->
        match value with
        | :? i64 as n -> $"i64 {n}"
        | :? utf8string as s -> $"utf8string {s.length}"
        | :? Map<string, i64> as map -> $"map {Map.length map}"
    | Error error -> Display.display error

def sequence :: own string -> string = \text ->
    let decoded: Result<(Set<i64> | string | bool), Json.Error> = Json.deserialize (ref (bytes text))
    match decoded with
    | Ok value ->
        match value with
        | :? Set<i64> as set -> $"set {Set.length set}"
        | :? string as s -> "string " + s
        | :? bool as b -> $"bool {b}"
    | Error error -> Display.display error

def pair :: own string -> string = \text ->
    let decoded: Result<((i64 * string) | i64), Json.Error> = Json.deserialize (ref (bytes text))
    match decoded with
    | Ok value ->
        match value with
        | :? (i64 * string) as both ->
            match both with
            | (n, s) -> $"pair {n} {s}"
        | :? i64 as n -> $"i64 {n}"
    | Error error -> Display.display error

test "the other types of the table" =
    assert (float_or_text "2.5" == "f64 2.5")
    assert (float_or_text "3" == "f64 3")
    assert (float_or_text "\"2.5\"" == "string 2.5")
    assert (collection "7" == "i64 7")
    assert (collection "\"abc\"" == "utf8string 3")
    assert (collection "{\"a\": 1, \"b\": 2}" == "map 2")
    assert (collection "[[\"a\", 1]]" == "map 1")
    assert (sequence "[3, 1, 2]" == "set 3")
    assert (sequence "\"s\"" == "string s")
    assert (sequence "true" == "bool true")
    assert (pair "[1, \"a\"]" == "pair 1 a")
    assert (pair "5" == "i64 5")

test "generic functions and nested values" =
    let numbers: Result<(bool | [i64]), Json.Error> = list_or_flag "[1, 2, 3]"
    let found = match numbers with
        | Ok value ->
            match value with
            | :? [i64] as items -> items.length
            | :? bool -> -1
        | Error _ -> -2
    assert (found == 3)
    let texts: Result<(bool | [string]), Json.Error> = list_or_flag "true"
    let flag = match texts with
        | Ok value ->
            match value with
            | :? [string] -> 0
            | :? bool as b -> if b then 1 else 2
        | Error _ -> -2
    assert (flag == 1)
    let mixed: Result<[(i64 | string)], Json.Error> = Json.deserialize u8"[1, \"two\", 3]"
    let shown = match mixed with
        | Ok items -> Display.display items
        | Error error -> Display.display error
    assert (shown == "[1, \"two\", 3]")
    let optional: Result<Maybe<(i64 | string)>, Json.Error> = Json.deserialize u8"null"
    let absent = match optional with
        | Ok found -> Maybe.is_none (ref found)
        | Error _ -> false
    assert absent
    let method: Result<(i64 | string), Json.Error> = match Json.parse u8"\"m\"" with
        | Ok json -> Decode.decode (ref json)
        | Error error -> Error error
    let text = match method with
        | Ok value -> Display.display value
        | Error error -> Display.display error
    assert (text == "m")
"#;
    passes(source, Edition::E2026);
    passes(source, Edition::E2027);
}

#[test]
fn decode_overlap_is_rejected() {
    let advice = "so the JSON value does not select one member; decode Json.Value and match on it, or use a union with named cases";
    let open = "is not limited to one kind of JSON value (a Maybe, a union with named cases, Json.Value, or a type with a hand-written Decode can read several)";
    let decode =
        |ty: &str| format!("let r: Result<{ty}, Json.Error> = Json.deserialize u8\"1\"\n0\n");
    let shape = "union Shape = Dot | Line of i64 deriving (Decode)\n";
    let point =
        "record Point { x: i64 } deriving (Decode)\nrecord Size { w: i64 } deriving (Decode)\n";
    let manual = "record Manual { x: i64 }\ninstance Decode<Manual> {\n    fn decode json = Result.Ok (Manual { x: 1 })\n}\n";
    for (source, union, reason) in [
        // Two members read the same kind.
        (
            decode("(i64 | f64)"),
            "(i64 | f64)",
            "the members i64 and f64 both read a JSON number".to_owned(),
        ),
        (
            decode("(Map<i64, string> | [i64])"),
            "(Map<i64, string> | [i64])",
            "the members Map<i64, string> and [i64] both read a JSON array".to_owned(),
        ),
        (
            decode("(utf8string | string | i64)"),
            "(i64 | string | utf8string)",
            "the members string and utf8string both read a JSON string".to_owned(),
        ),
        (
            decode("((i64 * i64) | Vec<i64> | bool)"),
            "(bool | Vec<i64> | (i64 * i64))",
            "the members Vec<i64> and (i64 * i64) both read a JSON array".to_owned(),
        ),
        (
            decode("(HashMap<string, i64> | HashSet<i64>)"),
            "(HashMap<string, i64> | HashSet<i64>)",
            "the members HashMap<string, i64> and HashSet<i64> both read a JSON array".to_owned(),
        ),
        (
            format!("{point}{}", decode("(Point | Size)")),
            "(Main.Point | Main.Size)",
            "the members Main.Point and Main.Size both read a JSON object".to_owned(),
        ),
        (
            format!("{point}{}", decode("(Point | Map<string, i64>)")),
            // Records are in the order of their qualified names: `Main.Point`, then `Map.Map`.
            "(Main.Point | Map<string, i64>)",
            "the members Main.Point and Map<string, i64> both read a JSON object".to_owned(),
        ),
        // A member that is not limited to kinds of its own.
        (
            decode("(Maybe<i64> | string)"),
            "(string | Maybe<i64>)",
            format!("the member Maybe<i64> {open}"),
        ),
        (
            decode("(Json.Value | i64)"),
            "(i64 | Json.Value)",
            format!("the member Json.Value {open}"),
        ),
        (
            format!("{shape}{}", decode("(Shape | i64)")),
            "(i64 | Main.Shape)",
            format!("the member Main.Shape {open}"),
        ),
        (
            format!("{manual}{}", decode("(Manual | i64)")),
            "(i64 | Main.Manual)",
            format!("the member Main.Manual {open}"),
        ),
        // The method, a generic function and derived instances reach the same rule.
        (
            "def f :: ref Json.Value -> Result<(i64 | f64), Json.Error> = \\json -> Decode.decode json\n0\n".to_owned(),
            "(i64 | f64)",
            "the members i64 and f64 both read a JSON number".to_owned(),
        ),
        (
            "def f :: Decode<'a> => ref Json.Value -> Result<(['a] | Vec<'a>), Json.Error> = \\json -> Decode.decode json\n0\n".to_owned(),
            "(['a] | Vec<'a>)",
            "the members ['a] and Vec<'a> both read a JSON array".to_owned(),
        ),
        (
            "record R { v: (i64 | f64) } deriving (Decode)\n0\n".to_owned(),
            "(i64 | f64)",
            "the members i64 and f64 both read a JSON number".to_owned(),
        ),
        (
            "union U = A | B of (Maybe<i64> | string) deriving (Encode, Decode)\n0\n".to_owned(),
            "(string | Maybe<i64>)",
            format!("the member Maybe<i64> {open}"),
        ),
    ] {
        let error = rejects(&source, "E1025", "anonymous_union_decode_overlap");
        assert_eq!(
            error.message,
            format!("no Decode instance for {union}: {reason}, {advice}"),
            "{source}"
        );
    }
    // A member without `Decode` is reported as that, before the kinds are compared.
    for (source, message) in [
        (
            decode("(char | i64 | f64)"),
            "no instance for Decode<char>; define an instance or use a supported type",
        ),
        (
            format!("record Plain {{ x: i64 }}\n{}", decode("(Plain | i64)")),
            "no instance for Decode<Main.Plain>; define an instance or use a supported type",
        ),
        (
            decode("((i64 * i64 * i64 * i64 * i64) | [i64])"),
            "no instance for Decode<(i64 * i64 * i64 * i64 * i64)>; define an instance or use a supported type",
        ),
    ] {
        let error = rejects(&source, "E1005", "no_instance");
        assert_eq!(error.message, message, "{source}");
    }
    // `Encode` has no such rule, and a record with a derived `Decode` reads an object only.
    accepts("record R { v: (i64 | f64 | Maybe<i64>) } deriving (Encode)\n0\n");
    accepts(&format!("{point}{}", decode("(Point | i64 | [Size])")));
    accepts(
        "def f :: Decode<'a> => ref Json.Value -> Result<(['a] | string), Json.Error> = \\json -> Decode.decode json\n0\n",
    );
}

#[test]
fn decode_mismatch_names_member_kinds() {
    passes(
        r#"record Point { x: i64 } deriving (Decode)

def bytes :: own string -> utf8string = \text -> Utf8String.from_string (ref text)

def refusal :: own Result<'a, Json.Error> -> string = \decoded ->
    match decoded with
    | Ok _ -> "ok"
    | Error error -> Display.display error

def number_or_text :: own string -> string = \text ->
    let decoded: Result<(string | i64), Json.Error> = Json.deserialize (ref (bytes text))
    refusal decoded

def flag_items_point :: own string -> string = \text ->
    let decoded: Result<([i64] | Point | bool), Json.Error> = Json.deserialize (ref (bytes text))
    refusal decoded

def number_or_map :: own string -> string = \text ->
    let decoded: Result<(Map<string, i64> | i64), Json.Error> = Json.deserialize (ref (bytes text))
    refusal decoded

test "no member reads the kind" =
    assert (number_or_text "true" == "json: expected number or string")
    assert (number_or_text "null" == "json: expected number or string")
    assert (number_or_text "[1]" == "json: expected number or string")
    assert (number_or_text "{}" == "json: expected number or string")
    assert (flag_items_point "1" == "json: expected boolean or object or array")
    assert (flag_items_point "\"s\"" == "json: expected boolean or object or array")
    assert (flag_items_point "null" == "json: expected boolean or object or array")
    assert (number_or_map "\"s\"" == "json: expected number or object or array")
    assert (number_or_map "false" == "json: expected number or object or array")

test "the error is ExpectedType outside parsing" =
    let decoded: Result<(i64 | string), Json.Error> = Json.deserialize u8"true"
    match decoded with
    | Ok _ -> assert false
    | Error error ->
        assert (error.offset == -1)
        assert (error.kind == Json.ExpectedType "number or string")

test "the selected member gives its own error" =
    assert (number_or_text "1.5" == "json: expected integer")
    assert (number_or_text "1e2" == "json: expected integer")
    assert (number_or_text "99999999999999999999" == "json: number out of range")
    assert (flag_items_point "[1, true]" == "json: expected integer")
    assert (flag_items_point "{}" == "json: missing field \"x\"")
    assert (flag_items_point "{\"x\": \"one\"}" == "json: expected integer")
    assert (number_or_map "[1]" == "json: expected array of 2")
    assert (number_or_map "{\"a\": true}" == "json: expected integer")
    assert (number_or_text "tru" == "json: unexpected end of input at byte 3")
    assert (number_or_text "7" == "ok")
"#,
        Edition::E2027,
    );
}

#[test]
fn derived_record_with_anonymous_field_round_trips() {
    let source = r#"record Cell { name: string, value: (i64 | string | [i64]) } deriving (Eq, Encode, Decode)
union Shape = Dot | Label of (string | i64) deriving (Eq, Encode, Decode)
record Pair<'a> { left: (['a] | string), right: 'a } deriving (Eq, Encode, Decode)

def text :: Encode<'a> => own 'a -> string = \value ->
    match Json.serialize value with
    | Ok bytes -> String.from_utf8 (ref bytes)
    | Error error -> Display.display error

def cell :: own string -> Result<Cell, Json.Error> = \text ->
    Json.deserialize (ref (Utf8String.from_string (ref text)))

def shape :: own string -> Result<Shape, Json.Error> = \text ->
    Json.deserialize (ref (Utf8String.from_string (ref text)))

def refusal :: own Result<'a, Json.Error> -> string = \decoded ->
    match decoded with
    | Ok _ -> "ok"
    | Error error -> Display.display error

def gives :: Eq<'a> => own Result<'a, Json.Error> -> own 'a -> bool = \decoded expected ->
    match decoded with
    | Ok value -> value == expected
    | Error _ -> false

test "record" =
    assert (text (Cell { name: "a", value: [1, 2] }) == "{\"name\":\"a\",\"value\":[1,2]}")
    assert (text (Cell { name: "b", value: "two" }) == "{\"name\":\"b\",\"value\":\"two\"}")
    assert (text (Cell { name: "c", value: 3 }) == "{\"name\":\"c\",\"value\":3}")
    assert (gives (cell "{\"name\":\"a\",\"value\":[1,2]}") (Cell { name: "a", value: [1, 2] }))
    assert (gives (cell "{\"value\": \"two\", \"name\": \"b\"}") (Cell { name: "b", value: "two" }))
    assert (gives (cell "{\"name\":\"c\",\"value\":3}") (Cell { name: "c", value: 3 }))
    assert (not (gives (cell "{\"name\":\"c\",\"value\":3}") (Cell { name: "c", value: "3" })))
    assert (refusal (cell "{\"name\":\"a\"}") == "json: missing field \"value\"")
    assert (refusal (cell "{\"name\":\"a\",\"value\":null}") == "json: expected number or string or array")
    assert (refusal (cell "{\"name\":\"a\",\"value\":true}") == "json: expected number or string or array")
    assert (refusal (cell "{\"name\":\"a\",\"value\":[\"x\"]}") == "json: expected integer")

test "union" =
    assert (text (Label "x") == "{\"Label\":\"x\"}")
    assert (text (Label 3) == "{\"Label\":3}")
    assert (text Dot == "\"Dot\"")
    assert (gives (shape "{\"Label\":\"x\"}") (Label "x"))
    assert (gives (shape "{\"Label\": 3}") (Label 3))
    assert (gives (shape "\"Dot\"") Dot)
    assert (refusal (shape "{\"Label\": [3]}") == "json: expected number or string")

test "generic record" =
    let numbers = Pair { left: [1, 2], right: 3 }
    let words = Pair { left: "w", right: "x" }
    assert (text (Pair { left: [1, 2], right: 3 }) == "{\"left\":[1,2],\"right\":3}")
    assert (text (Pair { left: "w", right: "x" }) == "{\"left\":\"w\",\"right\":\"x\"}")
    let first: Result<Pair<i64>, Json.Error> = Json.deserialize u8"{\"left\":[1,2],\"right\":3}"
    let second: Result<Pair<string>, Json.Error> = Json.deserialize u8"{\"left\":\"w\",\"right\":\"x\"}"
    assert (gives first numbers)
    assert (gives second words)

test "round trips" =
    let cells = [Cell { name: "a", value: [1, 2] }, Cell { name: "b", value: "two" }, Cell { name: "c", value: -3 }]
    let back: Result<[Cell], Json.Error> = match Json.serialize (ref cells) with
        | Ok bytes -> Json.deserialize (ref bytes)
        | Error error -> Error error
    assert (gives back cells)
"#;
    passes(source, Edition::E2026);
    passes(source, Edition::E2027);
}
