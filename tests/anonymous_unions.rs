//! Anonymous unions `(A | B)` and type test patterns `:? T` (A24), and the conversions between
//! anonymous unions (A25).

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
    // A union with more members is another type too. A25 widens a value of the smaller one to
    // it (`widens_subset_unions`); the other way stays an error.
    accepts("def f :: (i64 | string) -> (i64 | string | bool) = \\v -> v\n");
    rejects(
        "def f :: (i64 | string | bool) -> (i64 | string) = \\v -> v\n",
        "E1003",
        "anonymous_union_not_subset",
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
/// nothing decides it, as a literal by itself does. Review round 2: where the value could be two
/// members, its literals have no type yet and the message names none.
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
        "this value could be (i64u * f64) or (i64 * f64) in ((i64u * f64) | (i64 * f64))"
    );
    assert_eq!(
        error.help.as_deref(),
        Some(
            "its number literals have no type yet: add a suffix to them, or annotate the value, for example with a typed let binding"
        )
    );
}

/// Review round 1: an anonymous union takes no instance (D8), so `E1005` and `E1025` do not ask
/// for one; a named type keeps the usual advice.
#[test]
fn missing_instances_do_not_ask_for_an_instance() {
    let advice = "instances cannot be declared for an anonymous union; match on its members, wrap it in a record, or declare a union with named cases";
    for (source, code, kind, class) in [
        (
            "let v: (i64 | string) = 1\nv == 1\n",
            "E1005",
            "no_instance",
            "Eq",
        ),
        (
            "let v: (i64 | string) = 1\nlet s = to_string v\n0\n",
            "E1005",
            "no_instance",
            "Display",
        ),
        (
            "record R { v: (i64 | string) } deriving (Eq)\n0\n",
            "E1025",
            "derived_instance_invalid",
            "Eq",
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
    // The tested type is an anonymous union: A25 narrows to it (A24 rejected it as
    // `type_test_union`).
    accepts("let v: (i64 | string | bool) = 1\nmatch v with\n| :? (string | i64) -> 0\n| _ -> 1\n");
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tsuzuri::api_diff;
    use tsuzuri::driver::Project;
    // Tests of one process run on several threads: each call takes directories of its own.
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, Ordering::Relaxed);
    let directory = |label: &str, text: &str| {
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-anonymous-unions-api-{label}-{}-{call}-{}",
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

/// Review round 2: a hole after the colon of a field, named or followed by an operator, is no
/// type test. The arm stays a predicate, as it was before type tests existed, and the hole
/// reports its type.
#[test]
fn named_holes_after_field_colons_stay_predicates() {
    let record = "record R { x: i64 }\ndef get :: R -> i64 = \\r -> r.x\n";
    for (arm, column) in [
        ("    | get (R { x:?h }) > n -> 1\n", 18),
        ("    | get (R { x:?h + 1 }) > n -> 1\n", 18),
        ("    | get (R { x:? + 1 }) > n -> 1\n", 18),
    ] {
        let source =
            format!("{record}def f :: i64 -> i64 = \\n ->\n{arm}    | otherwise -> 0\nf 1\n");
        let program = parsed(&source);
        assert!(!program.uses_anonymous_unions, "{source}");
        let error = rejects(&source, "E1030", "typed_hole");
        assert!(error.message.contains("of type 'i64'"), "{}", error.message);
        let line_start = source.find(arm).unwrap();
        assert_eq!(error.span.start, line_start + column - 1, "{source}");
    }
    // The argument of an active pattern, between its parentheses.
    let source = format!(
        "{record}def (|Over|_|) :: i64 -> i64 -> bool = \\limit -> \\value -> value > limit\ndef f :: i64 -> i64 = \\n ->\n    match n with\n    | Over (get (R {{ x:?h }})) -> 1\n    | _ -> 0\nf 1\n"
    );
    let error = rejects(&source, "E1030", "typed_hole");
    assert!(error.message.starts_with("typed hole '?h' of type 'i64'"));
    // A type test still starts a pattern after a field's colon or `=`, where a space parts the
    // two colons, and a name before `:?` is no pattern.
    let fields = "record P { x: (i64 | string) }\ndef f :: own P -> i64 = \\p ->\n    match p with\n    | { x: :? i64 as n } -> n\n    | { x = :? string as s } -> s.length\n";
    assert!(parsed(fields).uses_anonymous_unions);
    accepts(fields);
    parse_error(
        "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | n:? i64 -> n\n    | _ -> 0\n",
    );
}

/// Review round 2: an array literal inside a value is as much a `[T; N]` as a `[T]` when the
/// value is checked with a member as its expected type, so it picks no member silently either.
#[test]
fn nested_array_literals_pick_no_member_silently() {
    let kinds = "def id :: 'a -> 'a = \\x -> x\ndef kind :: own ([[i64; 2]; 2] | [[i64]]) -> string = \\v ->\n    match v with\n    | :? [[i64; 2]; 2] -> \"fixed\"\n    | :? [[i64]] -> \"dynamic\"\n\ndef pair :: own (([i64; 2] * i64) | ([i64] * i64)) -> string = \\v ->\n    match v with\n    | :? ([i64; 2] * i64) -> \"fixed\"\n    | :? ([i64] * i64) -> \"dynamic\"\n\ndef rows :: own ([[i64; 2]] | [[i64]]) -> string = \\v ->\n    match v with\n    | :? [[i64; 2]] -> \"fixed\"\n    | :? [[i64]] -> \"dynamic\"\n\ndef maybe :: own (Maybe<[i64; 2]> | Maybe<[i64]>) -> string = \\v ->\n    match v with\n    | :? Maybe<[i64; 2]> -> \"fixed\"\n    | :? Maybe<[i64]> -> \"dynamic\"\n\ndef one :: own ([i64; 2] | [i64]) -> string = \\v ->\n    match v with\n    | :? [i64; 2] -> \"fixed\"\n    | :? [i64] -> \"dynamic\"\n\ndef three :: own ([[i64; 3]; 2] | [[i64]]) -> string = \\v ->\n    match v with\n    | :? [[i64; 3]; 2] -> \"fixed\"\n    | :? [[i64]] -> \"dynamic\"\n\ndef only :: own ([[i64; 2]; 2] | string) -> string = \\v ->\n    match v with\n    | :? [[i64; 2]; 2] -> \"fixed\"\n    | :? string -> \"string\"\n\ndef mixed :: own ([[i64]; 2] | [[i64; 2]]) -> string = \\v ->\n    match v with\n    | :? [[i64]; 2] -> \"outer\"\n    | :? [[i64; 2]] -> \"inner\"\n\n";
    for (call, message) in [
        (
            "kind [[1, 2], [3, 4]]",
            "this array literal could be [[i64]] or [[i64; 2]; 2] in ([[i64]] | [[i64; 2]; 2])",
        ),
        (
            "rows [[1, 2], [3, 4]]",
            "this array literal could be [[i64]] or [[i64; 2]] in ([[i64]] | [[i64; 2]])",
        ),
        // The outer literal alone would be the fixed-length member: the inner ones say otherwise.
        (
            "mixed [[1, 2], [3, 4]]",
            "this array literal could be [[i64; 2]] or [[i64]; 2] in ([[i64; 2]] | [[i64]; 2])",
        ),
        (
            "pair ([1, 2], 3)",
            "this value, which holds an array literal, could be ([i64] * i64) or ([i64; 2] * i64) in (([i64] * i64) | ([i64; 2] * i64))",
        ),
        (
            "maybe (Some [1, 2])",
            "this value, which holds an array literal, could be Maybe<[i64]> or Maybe<[i64; 2]> in (Maybe<[i64]> | Maybe<[i64; 2]>)",
        ),
        // The expected type reaches the literal through a call as well.
        (
            "one (id [1, 2])",
            "this value, which holds an array literal, could be [i64] or [i64; 2] in ([i64] | [i64; 2])",
        ),
        (
            "[[1, 2], [3, 4]] |> kind",
            "this array literal could be [[i64]] or [[i64; 2]; 2] in ([[i64]] | [[i64; 2]; 2])",
        ),
    ] {
        let source = format!("{kinds}{call}\n");
        let error = rejects(&source, "E1041", "anonymous_union_ambiguous_member");
        assert_eq!(error.message, message, "{call}");
        assert_eq!(
            error.help.as_deref(),
            Some("annotate the value, for example with a typed let binding"),
            "{call}"
        );
    }
    // An annotated value holds no literal and is the member of its type. A literal whose
    // lengths fit one member only, the only member with an array, and the array of `new`
    // are not ambiguous.
    passes(
        &format!(
            "{kinds}test \"a typed value decides\" =\n    let a: [[i64; 2]; 2] = [[1, 2], [3, 4]]\n    let c: [[i64]] = [[1, 2], [3, 4]]\n    assert (kind a == \"fixed\")\n    assert (kind c == \"dynamic\")\n    let p: ([i64; 2] * i64) = ([1, 2], 3)\n    let q: ([i64] * i64) = ([1, 2], 3)\n    assert (pair p == \"fixed\")\n    assert (pair q == \"dynamic\")\n    let m: Maybe<[i64; 2]> = Some [1, 2]\n    let n: Maybe<[i64]> = Some [1, 2]\n    assert (maybe m == \"fixed\")\n    assert (maybe n == \"dynamic\")\n    let d: [i64] = id [1, 2]\n    assert (one d == \"dynamic\")\n    assert (three [[1, 2], [3, 4]] == \"dynamic\")\n    assert (only [[1, 2], [3, 4]] == \"fixed\")\n    assert (one (new [1, 2]) == \"dynamic\")\n    assert (pair ([1, 2, 3], 3) == \"dynamic\")\n"
        ),
        Edition::E2026,
    );
}

/// Review round 2: only the purity of the value itself is adapted where it is injected. A
/// function whose result is a pure function is not the curried function that is not pure, so the
/// two are members of one union and a value of either type is its own member.
#[test]
fn function_results_keep_their_purity() {
    passes(
        "def f :: own ((i64 -> pure (i64 -> i64)) | (i64 -> i64 -> i64)) -> i64 = \\v ->\n    match v with\n    | :? (i64 -> pure (i64 -> i64)) as g -> 100 + (g 1) 2\n    | :? (i64 -> i64 -> i64) as h -> 200 + h 1 2\n\ndef add :: i64 -> i64 -> i64 = \\a b -> a + b\n\ndef lists :: own ([pure (i64 -> i64)] * i64 | [(i64 -> i64)] * string) -> i64 = \\v ->\n    match v with\n    | :? ([pure (i64 -> i64)] * i64) -> 1\n    | :? ([(i64 -> i64)] * string) -> 2\n\ntest \"a pure result is another member\" =\n    let inc: pure (i64 -> i64) = \\x -> x + 1\n    let padd: i64 -> pure (i64 -> i64) = \\_a -> inc\n    assert (f add == 203)\n    assert (f padd == 103)\n    let pures: [pure (i64 -> i64)] = [inc]\n    assert (lists (pures, 1) == 1)\n",
        Edition::E2026,
    );
    // A value that is a pure function is still a value of the member that is not pure.
    passes(
        "def call :: own ((i64 -> i64) | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 -> i64) as g -> g 41\n    | :? string as s -> s.length\n\ntest \"the value's own purity is adapted\" =\n    let inc: pure (i64 -> i64) = \\x -> x + 1\n    assert (call inc == 42)\n",
        Edition::E2026,
    );
}

/// Review round 2: a function that fits no member as it is reaches the member that differs in a
/// borrowed parameter through the adapter that a function type makes where it is expected
/// (edition 2027: a `def` reads its parameters).
#[test]
fn functions_reach_members_through_the_parameter_adapter() {
    let source = "def len :: string -> i64 = \\s -> s.length\ndef olen :: own string -> i64 = \\s -> s.length + 100\ndef twice :: own i64 -> i64 = \\n -> n * 2\n\ndef call :: own ((string -> i64) | i64) -> i64 = \\v ->\n    match v with\n    | :? (string -> i64) as f -> f \"abc\"\n    | :? i64 as n -> n\n\ndef rcall :: own ((ref i64 -> i64) | string) -> i64 = \\v ->\n    match v with\n    | :? (ref i64 -> i64) as f -> f 21\n    | :? string as s -> s.length\n\ndef both :: own ((ref string -> i64) | (string -> i64)) -> i64 = \\v ->\n    match v with\n    | :? (ref string -> i64) as f -> f \"ab\"\n    | :? (string -> i64) as f -> 1000 + f \"ab\"\n\n";
    passes(
        &format!(
            "{source}test \"adapted functions\" =\n    assert (call len == 3)\n    assert (call olen == 103)\n    assert (rcall twice == 42)\n    assert (call 7 == 7)\n    let u: ((string -> i64) | i64) = len\n    assert (call u == 3)\n    assert ((len |> call) == 3)\n\ntest \"a member of the value's own type comes first\" =\n    assert (both len == 2)\n    assert (both olen == 1102)\n"
        ),
        Edition::E2027,
    );
    // The same value at the member's type alone: the union accepts and rejects what it does.
    accepts_in(
        "def len :: string -> i64 = \\s -> s.length\nlet f: string -> i64 = len\nf \"abc\"\n",
        Edition::E2027,
    );
    for annotation in ["ref string -> i64", "((ref string -> i64) | i64)"] {
        rejects_in(
            &format!(
                "def olen :: own string -> i64 = \\s -> s.length\nlet h: {annotation} = olen\n0\n"
            ),
            Edition::E2027,
            "E1012",
            "move_out_of_reference_binding",
        );
    }
    // Edition 2026 has no such adapter, with or without the union.
    let error = rejects_in(
        "def twice :: i64 -> i64 = \\n -> n * 2\nlet h: ((ref i64 -> i64) | string) = twice\n0\n",
        Edition::E2026,
        "E1003",
        "anonymous_union_not_member",
    );
    assert_eq!(
        error.message,
        "expected (string | (ref i64 -> i64)), found i64 -> i64; i64 -> i64 is not a member of the anonymous union"
    );
    rejects_in(
        "def twice :: i64 -> i64 = \\n -> n * 2\nlet h: ref i64 -> i64 = twice\n0\n",
        Edition::E2026,
        "E1003",
        "type_mismatch",
    );
}

/// Review round 2: a difference in purity hides no difference that follows it, and purity is no
/// key of the order, so the members have one order however they are written.
#[test]
fn purity_hides_no_later_difference() {
    passes(
        "def f :: own ((pure (i64 -> i64) * i64) | ((i64 -> i64) * string)) -> i64 = \\v ->\n    match v with\n    | :? (pure (i64 -> i64) * i64) as p -> (match p with | (g, n) -> g n)\n    | :? ((i64 -> i64) * string) as q -> (match q with | (g, s) -> g s.length)\n\ntest \"the second component decides\" =\n    let inc: pure (i64 -> i64) = \\x -> x + 1\n    let double: i64 -> i64 = \\x -> x * 2\n    let a: (pure (i64 -> i64) * i64) = (inc, 41)\n    let b: ((i64 -> i64) * string) = (double, \"abc\")\n    assert (f a == 42)\n    assert (f b == 6)\n    assert (f (inc, 1) == 2)\n    assert (f (double, \"abcd\") == 8)\n",
        Edition::E2026,
    );
    // Members that differ only in purity stay rejected, wherever the purity is.
    let error = rejects(
        "def f :: own ((pure (i64 -> i64) * i64) | ((i64 -> i64) * i64)) -> i64 = \\v -> 0\n",
        "E1041",
        "anonymous_union_indistinct_members",
    );
    assert!(
        error
            .message
            .contains("differ only in the purity of a function type"),
        "{}",
        error.message
    );
    // Three members that an order with purity as its first key would turn in a circle: the
    // first components of the first and the third differ only in purity.
    let members = [
        "(pure (i64 -> i64) * i64)",
        "((string -> i64) * i64)",
        "((i64 -> i64) * string)",
    ];
    let expected = "expected i64, found (((i64 -> i64) * i64) | ((i64 -> i64) * string) | ((string -> i64) * i64))";
    let mut seen = 0;
    for first in 0..3 {
        for second in 0..3 {
            for third in 0..3 {
                if first == second || first == third || second == third {
                    continue;
                }
                let source = format!(
                    "def f :: ({} | {} | {}) -> i64 = \\v -> v\n",
                    members[first], members[second], members[third]
                );
                let error = rejects(&source, "E1003", "type_mismatch");
                assert_eq!(
                    error.message.replace("(pure (i64 -> i64))", "(i64 -> i64)"),
                    expected,
                    "{source}"
                );
                seen += 1;
            }
        }
    }
    assert_eq!(seen, 6);
    // The parameters and the results order two function members, not their purity.
    let error = rejects(
        "def f :: (pure (string -> i64) | (i64 -> i64) | pure (bool -> i64) | (string -> string)) -> i64 = \\v -> v\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(
        error.message,
        "expected i64, found ((i64 -> i64) | pure (bool -> i64) | pure (string -> i64) | (string -> string))"
    );
}

/// Review round 2: a number literal inside a value that was checked by itself is an integer or
/// a floating-point number already, so it reaches only the members that its class has, and a
/// diagnostic names no type that the literal has not taken.
#[test]
fn number_literals_inside_values_keep_their_class() {
    let note = "a number literal inside the value takes no type from the union, so add a suffix to it or annotate the value";
    for (source, message, help) in [
        (
            "let c: (f64 | string) = 1 + 2\n0\n",
            "expected (f64 | string), found i32; i32 is not a member of the anonymous union",
            "the members are f64, string",
        ),
        (
            "let e: ([f64] | [string]) = [1, 2]\n0\n",
            "expected ([f64] | [string]), found [i32]; [i32] is not a member of the anonymous union",
            "the members are [f64], [string]",
        ),
        (
            "let m: (Maybe<f64> | string) = Some 1\n0\n",
            "expected (string | Maybe<f64>), found Maybe<i32>; Maybe<i32> is not a member of the anonymous union",
            "the members are string, Maybe<f64>",
        ),
        // No instance of `Integer` for `BigInt`, and a negative literal is no unsigned integer.
        (
            "let b: (Maybe<BigInt> | string) = Some 1\n0\n",
            "expected (string | Maybe<BigInt>), found Maybe<i32>; Maybe<i32> is not a member of the anonymous union",
            "the members are string, Maybe<BigInt>",
        ),
        (
            "let n: (Maybe<i8u> | string) = Some (-1)\n0\n",
            "expected (string | Maybe<i8u>), found Maybe<i32>; Maybe<i32> is not a member of the anonymous union",
            "the members are string, Maybe<i8u>",
        ),
        (
            "let i: (Maybe<i64> | string) = Some 1.5\n0\n",
            "expected (string | Maybe<i64>), found Maybe<f64>; Maybe<f64> is not a member of the anonymous union",
            "the members are string, Maybe<i64>",
        ),
    ] {
        let error = rejects(source, "E1003", "anonymous_union_not_member");
        assert_eq!(error.message, message, "{source}");
        assert_eq!(
            error.help.as_deref(),
            Some(format!("{help}; {note}").as_str()),
            "{source}"
        );
    }
    // A value without literals keeps the plain help.
    let error = rejects(
        "let t = true\nlet c: (f64 | string) = t\n0\n",
        "E1003",
        "anonymous_union_not_member",
    );
    assert_eq!(error.help.as_deref(), Some("the members are f64, string"));
    // Two members of the literals' class: the value has no type to name yet.
    let error = rejects(
        "let t: ((i32 * i32) | (i64 * i64)) = (1, 2)\n0\n",
        "E1041",
        "anonymous_union_ambiguous_member",
    );
    assert_eq!(
        error.message,
        "this value could be (i32 * i32) or (i64 * i64) in ((i32 * i32) | (i64 * i64))"
    );
    assert_eq!(
        error.help.as_deref(),
        Some(
            "its number literals have no type yet: add a suffix to them, or annotate the value, for example with a typed let binding"
        )
    );
    // The class of the literal picks the member.
    passes(
        "def kind :: own (Maybe<i64> | Maybe<f64>) -> string = \\v ->\n    match v with\n    | :? Maybe<i64> -> \"integer\"\n    | :? Maybe<f64> -> \"float\"\n\ndef wide :: own (Maybe<i64> | Maybe<i8u> | string) -> i64 = \\v ->\n    match v with\n    | :? Maybe<i64> as m -> (match m with | Some n -> n | None -> 0)\n    | :? Maybe<i8u> -> 1000\n    | :? string -> 2000\n\ntest \"the literal's class picks the member\" =\n    assert (kind (Some 1.5) == \"float\")\n    assert (kind (Some 1) == \"integer\")\n    assert (kind (Some (1 + 2)) == \"integer\")\n    assert (wide (Some (-7)) == -7)\n",
        Edition::E2026,
    );
}

/// Review round 2: a type test adapts nothing, so a function type that differs from a member in
/// its purity is no member, and the error is the one of any other type that is no member.
#[test]
fn type_tests_do_not_adapt_function_types() {
    for (member, tested, shown, members) in [
        (
            "pure (i64 -> i64)",
            "(i64 -> i64)",
            "i64 -> i64",
            "string, pure (i64 -> i64)",
        ),
        (
            "(i64 -> i64)",
            "pure (i64 -> i64)",
            "pure (i64 -> i64)",
            "string, (i64 -> i64)",
        ),
        (
            "(i64 -> pure (i64 -> i64))",
            "(i64 -> i64 -> i64)",
            "i64 -> i64 -> i64",
            "string, (i64 -> pure (i64 -> i64))",
        ),
        (
            "(ref string -> i64)",
            "(string -> i64)",
            "string -> i64",
            "string, (ref string -> i64)",
        ),
    ] {
        let source = format!(
            "def a :: own ({member} | string) -> i64 = \\v ->\n    match v with\n    | :? {tested} -> 1\n    | :? string as s -> s.length\n"
        );
        let error = rejects(&source, "E1041", "type_test_not_member");
        assert!(
            error
                .message
                .starts_with(&format!("{shown} is not a member of ")),
            "{}",
            error.message
        );
        assert_eq!(
            error.help.as_deref(),
            Some(format!("the members are {members}").as_str()),
            "{source}"
        );
        let start = source.find(":? ").unwrap() + 3;
        assert_eq!(error.span.start, start, "{source}");
    }
}

// A25, step 2: a value of an anonymous union is widened where a union of more members is expected.

/// `weigh`: a number for every member of `(i32 | i64 | f64 | bool | string)`, to read a widened
/// value back. The members of a smaller union take other case numbers in it, so a value that
/// kept its old number would be read as another member.
const WEIGH: &str = "def weigh :: own (string | bool | f64 | i64 | i32) -> i64 = \\v ->\n    match v with\n    | :? i32 -> 10000\n    | :? i64 as n -> n\n    | :? f64 -> 20000\n    | :? bool as b -> if b then 30001 else 30000\n    | :? string as s -> 40000 + s.length\n";

#[test]
fn widens_subset_unions() {
    // Bindings, results and arguments. Expected values by hand: `weigh` of an i64 is the
    // number, of a string 40000 + its length, of `true` 30001, of an f64 20000.
    let program = format!(
        "{WEIGH}def narrow :: own i64 -> (i64 | string) = \\n -> if n > 0 then n else \"neg\"\n\
         def middle :: own (i64 | string) -> (bool | i64 | string) = \\v -> v\n\
         def wide :: own (string | bool) -> (i32 | i64 | f64 | bool | string) = \\v -> v\n\n\
         test \"bindings\" =\n    let a: (i64 | string) = 1\n    let b: (f64 | i64 | string) = a\n    let c: (string | bool | f64 | i64 | i32) = b\n    assert (weigh c == 1)\n    let s: (i64 | string) = \"abc\"\n    let t: (i32 | i64 | f64 | bool | string) = s\n    assert (weigh t == 40003)\n\n\
         test \"results and arguments\" =\n    assert (weigh (narrow 5) == 5)\n    assert (weigh (narrow (-1)) == 40003)\n    assert (weigh (middle (narrow 7)) == 7)\n    assert (weigh (middle \"four\") == 40004)\n    let flag: (string | bool) = true\n    assert (weigh (wide flag) == 30001)\n    let word: (string | bool) = \"ab\"\n    assert (weigh (wide word) == 40002)\n"
    );
    passes(&program, Edition::E2026);
    passes(&program, Edition::E2027);
    // Fields, elements, the arms of `if` and `match`, a block's result and an assignment.
    // `sum` reads the elements in place: 12 + 40004 + 30001 + 3 = 70020.
    passes(
        &format!(
            "{WEIGH}record Cell {{ value: (bool | i64 | string), id: i64 }}\n\n\
             def pick :: own i64 -> own (i64 | string) -> own (bool | string) -> (bool | i64 | f64 | string) = \\k a b ->\n    if k == 0 then a\n    elif k == 1 then b\n    else\n        match k with\n        | 2 -> 2.5\n        | _ ->\n            let kept = a\n            kept\n\n\
             def sum :: own [(bool | i64 | string)] -> i64 = \\items ->\n    let mut total = 0i64\n    for item in items do\n        total = total + (match item with\n            | :? bool as b -> if b then 30001 else 30000\n            | :? i64 as n -> n\n            | :? string as s -> 40000 + s.length)\n    total\n\n\
             test \"fields and elements\" =\n    let a: (i64 | string) = 12\n    let cell = Cell {{ value: a, id: 1 }}\n    let b: (i64 | string) = \"wxyz\"\n    let c: (bool | i64) = true\n    assert (sum [cell.value, b, c, 3] == 70020)\n\n\
             test \"arms and blocks\" =\n    assert (weigh (pick 0 \"abc\" true) == 40003)\n    assert (weigh (pick 1 4 true) == 30001)\n    assert (weigh (pick 1 4 \"hello\") == 40005)\n    assert (weigh (pick 2 4 false) == 20000)\n    assert (weigh (pick 3 9 false) == 9)\n\n\
             test \"assignment\" =\n    let mut slot: (bool | i64 | string) = false\n    let a: (i64 | string) = 8\n    slot = a\n    let b: (bool | string) = \"xy\"\n    let first = weigh slot\n    slot = b\n    assert (first + weigh slot == 40010)\n"
        ),
        Edition::E2027,
    );
    // The edition 2026 program of A24's `members_commute_and_flatten`, now a widening.
    passes(
        "def f :: (i64 | string) -> (i64 | string | bool) = \\v -> v\n\ntest \"owned parameter\" =\n    assert (match f 41 with | :? i64 as n -> n == 41 | _ -> false)\n    assert (match f \"abc\" with | :? string as s -> s.length == 3 | _ -> false)\n",
        Edition::E2026,
    );
}

#[test]
fn rejects_non_subset_unions() {
    // A member of the value's union that the expected union does not have.
    let error = rejects(
        "let a: (i64 | bool) = true\nlet b: (f64 | i64 | string) = a\n0\n",
        "E1003",
        "anonymous_union_not_subset",
    );
    assert_eq!(
        error.message,
        "expected (i64 | f64 | string), found (i64 | bool); bool is not a member of (i64 | f64 | string)"
    );
    assert_eq!(
        error.help.as_deref(),
        Some(
            "the members are i64, f64, string; match the value with ':?' and pass on the members that (i64 | f64 | string) has"
        )
    );
    assert_eq!(
        error.detail.expected.as_deref(),
        Some("(i64 | f64 | string)")
    );
    assert_eq!(error.detail.found.as_deref(), Some("(i64 | bool)"));
    // More members than the expected union has: no implicit narrowing.
    let error = rejects(
        "let a: (i64 | string | bool) = 1\nlet b: (i64 | string) = a\n0\n",
        "E1003",
        "anonymous_union_not_subset",
    );
    assert_eq!(
        error.message,
        "expected (i64 | string), found (i64 | bool | string); bool is not a member of (i64 | string)"
    );
    // As many members: the two types are unified as before, with the plain mismatch.
    let error = rejects(
        "let a: (i64 | bool) = true\nlet b: (i64 | string) = a\n0\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected (i64 | string), found (i64 | bool)");
    // The same for arguments, piped values and results.
    let take = "def take :: own (f64 | i64 | string) -> i64 = \\v -> 0\n";
    for body in ["take a", "a |> take"] {
        let error = rejects(
            &format!("{take}let a: (i64 | bool) = true\n{body}\n"),
            "E1003",
            "anonymous_union_not_subset",
        );
        assert!(
            error
                .message
                .ends_with("bool is not a member of (i64 | f64 | string)"),
            "{body}: {}",
            error.message
        );
    }
    rejects(
        "def f :: own (i64 | unit) -> (i64 | string | bool) = \\v -> v\n",
        "E1003",
        "anonymous_union_not_subset",
    );
    // Nothing is widened inside another type, or behind a reference.
    for source in [
        "def f :: own [(i64 | string)] -> [(f64 | i64 | string)] = \\xs -> xs\n",
        "def f :: own Maybe<(i64 | string)> -> Maybe<(f64 | i64 | string)> = \\m -> m\n",
        "def f :: own (i64 -> (i64 | string)) -> (i64 -> (f64 | i64 | string)) = \\g -> g\n",
        "let a: (i64 | string) = 1\nlet r: ref (f64 | i64 | string) = ref a\n0\n",
    ] {
        rejects(source, "E1003", "type_mismatch");
    }
    // A member that may be two members of the expected union picks neither.
    let error = rejects(
        "def absent :: unit -> (Maybe<'a> | i64) = \\_ -> None\nlet u: (Maybe<i64> | Maybe<string> | i64 | bool) = absent ()\n0\n",
        "E1041",
        "anonymous_union_ambiguous_member",
    );
    assert!(
        error.message.ends_with(
            "could be Maybe<i64> or Maybe<string> in (i64 | bool | Maybe<i64> | Maybe<string>)"
        ),
        "{}",
        error.message
    );
    assert_eq!(
        error.help.as_deref(),
        Some("annotate the value, for example with a typed let binding")
    );
}

/// `size`: `(f64 | i64 | string)` read through a parameter that is read in edition 2027.
const SIZE: &str = "def size :: (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? f64 -> 20000\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n";

#[test]
fn widens_pipes_and_read_parameters() {
    // A read parameter (2027) borrows the widened value, a temporary, from an argument as from
    // a pipe; an owned parameter takes it. 4 + 5 + 3 + 3 + 6 = 21.
    let program = format!(
        "{SIZE}def show :: own (f64 | i64 | string) -> i64 = \\v -> size v\n\
         def make :: own i64 -> (i64 | string) = \\n -> if n > 0 then n else \"neg\"\n\n\
         test \"pipes\" =\n    let a: (i64 | string) = \"abcd\"\n    let b: (i64 | string) = 5\n    let c: (i64 | string) = \"xyz\"\n    let total = (a |> show) + (b |> size) + size c + (make 3 |> size) + (make 6 |> show)\n    assert (total == 21)\n"
    );
    passes(&program, Edition::E2027);
    passes(&program, Edition::E2026);
    // A parameter that edition 2027 reads by default is a reference, which is not widened: the
    // value would move out of the borrow. The message names `own`.
    for source in [
        format!("{SIZE}def f :: (i64 | string) -> (f64 | i64 | string) = \\v -> v\n"),
        format!("{SIZE}def f :: (i64 | string) -> i64 = \\v -> size v\n"),
        format!("{SIZE}def f :: (i64 | string) -> i64 = \\v -> v |> size\n"),
    ] {
        let error = rejects_in(&source, Edition::E2027, "E1003", "type_mismatch");
        assert_eq!(
            error.message,
            "expected (i64 | f64 | string), found ref (i64 | string); parameter 'v' is read-only by default (edition 2027); write 'own (i64 | string)' in the signature to take ownership",
            "{source}"
        );
        assert_eq!(
            error.detail.expected.as_deref(),
            Some("(i64 | f64 | string)")
        );
        assert_eq!(error.detail.found.as_deref(), Some("ref (i64 | string)"));
        assert_eq!(error.help, None, "{source}");
        // The same body takes the value in edition 2026, and with `own` in 2027.
        accepts_in(&source, Edition::E2026);
        accepts_in(
            &source.replace("def f :: (i64 | string)", "def f :: own (i64 | string)"),
            Edition::E2027,
        );
    }
    // An explicit reference is not widened either; it has no parameter to name.
    let error = rejects(
        "let a: (i64 | string) = 1\nlet r = ref a\nlet b: (f64 | i64 | string) = r\n0\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(
        error.message,
        "expected (i64 | f64 | string), found ref (i64 | string)"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("an anonymous union is widened from an owned value")
    );
    // A union of Copy members that need no drop is a value in edition 2027 too, and widens.
    passes(
        "def wide :: (i64 | bool) -> (i64 | f64 | bool) = \\v -> v\ndef read :: (i64 | f64 | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? f64 -> 20000\n    | :? bool as b -> if b then 30001 else 30000\n\n\
         def forward :: (i64 | bool) -> i64 = \\v -> read v + (v |> read)\n\n\
         test \"copy members\" =\n    assert (read (wide 7) == 7)\n    assert (read (wide false) == 30000)\n    assert (forward 4 == 8)\n",
        Edition::E2027,
    );
}

#[test]
fn widening_moves_the_value() {
    let take = "def take :: own (f64 | i64 | string) -> i64 = \\v -> 0\n";
    // The payload moves into the wider union, so a union with a string member is used up.
    let error = rejects(
        &format!(
            "{take}let a: (i64 | string) = \"x\"\nlet b: (f64 | i64 | string) = a\nlet c: (f64 | i64 | string) = a\n0\n"
        ),
        "E1012",
        "use_after_move",
    );
    assert!(error.message.contains("'a'"), "{}", error.message);
    rejects(
        &format!("{take}let a: (i64 | string) = \"x\"\nlet n = take a + (a |> take)\nn\n"),
        "E1012",
        "use_after_move",
    );
    // The last use needs nothing; a union of Copy members is copied and stays usable.
    // 5 + 5 + 1 + 20000 = 20011 by hand.
    passes(
        "def read :: own (i64 | f64 | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? f64 -> 20000\n    | :? bool as b -> if b then 1 else 0\n\n\
         test \"copies\" =\n    let a: (i64 | bool) = 5\n    let b: (i64 | f64 | bool) = a\n    let c: (bool | f64) = 2.5\n    assert (read b + read a + (match a with | :? i64 -> 1 | :? bool -> 0) + read c == 20011)\n",
        Edition::E2027,
    );
    // The value is evaluated once: `next` counts its calls. 1 + 1 = 2 calls, values 1 and 2.
    passes(
        "def next :: ref mut i64 -> (i64 | string) = \\count ->\n    deref count = deref count + 1\n    deref count\n\n\
         def read :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? f64 -> 20000\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n\n\
         test \"once\" =\n    let mut count = 0i64\n    let first: (f64 | i64 | string) = next (ref mut count)\n    let second = read (next (ref mut count))\n    assert (read first == 1)\n    assert (second == 2)\n    assert (count == 2)\n",
        Edition::E2027,
    );
    // A payload behind a borrow cannot move out of it: an element of a borrowed array, and the
    // target of a reference. The errors are those of any move out of such a place.
    rejects_in(
        &format!(
            "{take}def all :: [(i64 | string)] -> i64 = \\items ->\n    let mut sum = 0i64\n    for item in items do\n        sum = sum + take item\n    sum\n"
        ),
        Edition::E2027,
        "E1012",
        "move_out_of_collection_element",
    );
    rejects_in(
        &format!("{take}def through :: ref (i64 | string) -> i64 = \\r -> take (deref r)\n"),
        Edition::E2027,
        "E1012",
        "move_out_of_reference_binding",
    );
    // Copy members are copied out of the borrow. 3 + 1 + 4 = 8.
    passes(
        "def read :: own (i64 | f64 | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? f64 -> 20000\n    | :? bool as b -> if b then 1 else 0\n\n\
         def all :: [(i64 | bool)] -> i64 = \\items ->\n    let mut sum = 0i64\n    for item in items do\n        sum = sum + read item\n    sum\n\n\
         def through :: ref (i64 | bool) -> i64 = \\r -> read (deref r)\n\n\
         test \"borrowed copies\" =\n    assert (all [3, true, 4] == 8)\n    let v: (i64 | bool) = 6\n    assert (through (ref v) + through (ref v) == 12)\n",
        Edition::E2027,
    );
}

#[test]
fn widening_maps_generic_members() {
    // `(Maybe<'a> | i64)` is `(i64 | Maybe<'a>)`, and `(Maybe<'a> | i64 | bool)` is
    // `(i64 | bool | Maybe<'a>)`: the Maybe member moves from case 1 to case 2 in every instance.
    // Expected values by hand: 100 + the payload (or the length) for `Some`, 200 for `None`.
    let program = "def keep :: own (Maybe<'a> | i64) -> (Maybe<'a> | i64 | bool) = \\v -> v\n\
         def absent :: unit -> (Maybe<'a> | i64) = \\_ -> None\n\
         def number :: own (Maybe<i64> | i64 | bool) -> i64 = \\v ->\n    match v with\n    | :? Maybe<i64> as m -> (match m with | Some x -> 100 + x | None -> 200)\n    | :? i64 as n -> n\n    | :? bool -> 300\n\n\
         def text :: own (Maybe<string> | i64 | bool) -> i64 = \\v ->\n    match v with\n    | :? Maybe<string> as m -> (match m with | Some s -> 100 + s.length | None -> 200)\n    | :? i64 as n -> n\n    | :? bool -> 300\n\n\
         test \"instances\" =\n    let five: (Maybe<i64> | i64) = Some 5\n    assert (number (keep five) == 105)\n    let seven: (Maybe<i64> | i64) = 7\n    assert (number (keep seven) == 7)\n    let word: (Maybe<string> | i64) = Some \"abc\"\n    assert (text (keep word) == 103)\n\n\
         test \"open member\" =\n    let none: (Maybe<string> | i64 | bool) = absent ()\n    assert (text none == 200)\n    let other: (bool | i64 | Maybe<i64>) = absent ()\n    assert (number other == 200)\n";
    passes(program, Edition::E2026);
    passes(program, Edition::E2027);
    let error = rejects(
        "def keep :: own (Maybe<'a> | i64) -> (Maybe<'b> | i64 | bool) = \\v -> v\n",
        "E1003",
        "anonymous_union_not_subset",
    );
    assert_eq!(
        error.message,
        "expected (i64 | bool | Maybe<'b>), found (i64 | Maybe<'a>); Maybe<'a> is not a member of (i64 | bool | Maybe<'b>)"
    );
}

#[test]
fn widening_a_generic_read_parameter_needs_copy() {
    // Edition 2027 reads a generic parameter, and its body takes the value out of the view only
    // when the instance is Copy, as `\x v -> v` does without a union (review round 1): the body
    // widens, an instance with an i64 runs, and an instance with a string is E1005 at the call.
    // `own` takes the value. By hand: the payload 2, the length 2 of "bc".
    let keep = "def keep :: 'a -> (Maybe<'a> | i64) -> (Maybe<'a> | i64 | bool) = \\_x v -> v\n\n";
    let number = "def number :: own (Maybe<i64> | i64 | bool) -> i64 = \\v ->\n    match v with\n    | :? Maybe<i64> as m -> (match m with | Some x -> x | None -> 0)\n    | _ -> -1\n\n";
    let text = "def text :: own (Maybe<string> | i64 | bool) -> i64 = \\v ->\n    match v with\n    | :? Maybe<string> as m -> (match m with | Some s -> s.length | None -> 0)\n    | _ -> -1\n\n";
    passes(
        &format!("{keep}{number}test \"copy\" =\n    assert (number (keep 1 (Some 2)) == 2)\n"),
        Edition::E2027,
    );
    let source =
        format!("{keep}{text}def f :: i64 -> i64 = \\_n -> text (keep \"a\" (Some \"bc\"))\n");
    let error = rejects_in(&source, Edition::E2027, "E1005", "no_instance");
    assert_eq!(
        error.message,
        "no instance for Copy<string>; define an instance or use a supported type"
    );
    let at = source.find("keep \"a\"").unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + "keep".len()));
    passes(
        &format!(
            "{}{text}test \"own\" =\n    assert (text (keep \"a\" (Some \"bc\")) == 2)\n",
            keep.replace("-> (Maybe<'a> | i64) ->", "-> own (Maybe<'a> | i64) ->")
        ),
        Edition::E2027,
    );
}

/// The lines of the definition of the function `name` of module `Main` in `ir`.
fn function_body<'ir>(ir: &'ir str, name: &str) -> &'ir str {
    let start = ir
        .match_indices("\ndefine ")
        .map(|(at, _)| at + 1)
        .find(|at| {
            let line = ir[*at..].lines().next().unwrap();
            line.contains(&format!("@tz.fn.Main.{name}("))
                || line.contains(&format!("@\"tz.fn.Main.{name}\"("))
        })
        .unwrap_or_else(|| panic!("no definition of {name} in\n{ir}"));
    let end = ir[start..].find("\n}\n").expect("the definition ends");
    &ir[start..start + end]
}

#[test]
fn widening_does_not_copy_owned_array_members() {
    // An array member of an owned value moves into the wider union: no allocation and no
    // implicit copy (`@no_copy` holds), and the arm is chosen by a switch on the tag.
    let total = "def total :: own (i64 | [i64] | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? [i64] as xs -> xs.length\n    | :? string as s -> s.length\n\n";
    let ir = accepts(&format!(
        "{total}@no_copy\ndef argument :: own (i64 | [i64]) -> i64 = \\v -> total v\n\n\
         @no_copy\ndef bound :: own ([i64] | string) -> i64 = \\v ->\n    let w: (i64 | [i64] | string) = v\n    total w\n\n\
         export def run :: i64 -> i64 = \\n ->\n    let a: (i64 | [i64]) = [n, n]\n    let b: ([i64] | string) = [n]\n    argument a + bound b\n"
    ));
    for name in ["argument", "bound"] {
        let body = function_body(&ir, name);
        assert!(!body.contains("@tz.alloc"), "{name}:\n{body}");
        assert!(body.contains("switch i32 "), "{name}:\n{body}");
    }
    // A value that is used again is copied, as `let w = v` copies an array; the copy is the
    // widened value's (the position that `@no_copy` reports).
    let source = format!(
        "{total}@no_copy\ndef twice :: own (i64 | [i64]) -> i64 = \\v ->\n    let w: (i64 | [i64] | string) = v\n    total w + (match v with | :? i64 as n -> n | :? [i64] as xs -> xs.length)\n"
    );
    let error = rejects(&source, "E1031", "effect_copy_in_no_copy");
    let at = source.find("= v\n").unwrap() + 2;
    assert_eq!((error.span.start, error.span.end), (at, at + 1));
    let ir = accepts(&source.replace("@no_copy\n", ""));
    assert!(function_body(&ir, "twice").contains("@tz.alloc"), "{ir}");
}

#[test]
fn widening_into_a_read_parameter_moves_array_members() {
    // Edition 2027 reads the parameter of `measure`: the call borrows the widened value, a
    // temporary, and an owned array member moves into it as into an owned parameter (review
    // round 1). `measure` binds no array, so that `@no_copy` sees only the callers.
    let measure = "def measure :: (i64 | [i64] | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? [i64] -> 100\n    | :? string as s -> s.length\n\n\
         def make :: i64 -> (i64 | [i64]) = \\n -> if n > 0 then [n, n] else n\n\n";
    let source = format!(
        "{measure}@no_copy\ndef local :: own (i64 | [i64]) -> i64 = \\v -> measure v\n\n\
         @no_copy\ndef temporary :: i64 -> i64 = \\n -> measure (make n)\n\n\
         @no_copy\ndef bound :: i64 -> i64 = \\n ->\n    let v = make n\n    measure v\n\n\
         test \"values\" =\n    let a: (i64 | [i64]) = [1, 2, 3]\n    let b: (i64 | [i64]) = 6\n    assert (local a + local b == 106)\n    assert (temporary 5 + temporary (-4) == 96)\n    assert (bound 2 + bound (-7) == 93)\n"
    );
    let module = accepts_in(&source, Edition::E2027);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    for name in ["local", "temporary", "bound"] {
        let body = function_body(&ir, name);
        assert!(!body.contains("@tz.alloc"), "{name}:\n{body}");
        assert!(body.contains("switch i32 "), "{name}:\n{body}");
    }
    // Expected values by hand: 100 for an array, the i64 itself: 100 + 6, 100 - 4, 100 - 7.
    passes(&source, Edition::E2027);
    // A value that is read again is copied for the first call and moved by the last; the copy
    // is the widened value's. 100 + 100 for an array, 2 k for the i64 k.
    let source = format!(
        "{measure}@no_copy\ndef twice :: own (i64 | [i64]) -> i64 = \\v -> measure v + measure v\n"
    );
    let error = rejects_in(&source, Edition::E2027, "E1031", "effect_copy_in_no_copy");
    let at = source.find("measure v +").unwrap() + "measure ".len();
    assert_eq!((error.span.start, error.span.end), (at, at + 1));
    let source = format!(
        "{}\ntest \"twice\" =\n    let a: (i64 | [i64]) = [4]\n    let b: (i64 | [i64]) = 21\n    assert (twice a + twice b == 242)\n",
        source.replace("@no_copy\n", "")
    );
    let module = accepts_in(&source, Edition::E2027);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(
        function_body(&ir, "twice").matches("@tz.alloc").count(),
        1,
        "{ir}"
    );
    passes(&source, Edition::E2027);
}

#[test]
fn hover_on_a_widened_value_shows_the_value() {
    let source = "def make :: own i64 -> (i64 | string) = \\n -> n\n\
         def f :: own (i64 | [i64]) -> (f64 | i64 | [i64]) = \\items ->\n    let wide: (f64 | i64 | [i64]) = items\n    wide\n\
         def g :: own i64 -> (f64 | i64 | string) = \\n -> make n\n0\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", source)]).unwrap();
    // The widened variable reads as itself and goes to its own definition, not to a local of
    // the conversion.
    let at = source.find("= items").unwrap() + 2;
    let entry = index.at(0, at).unwrap();
    assert_eq!(entry.detail, "items: (i64 | [i64])");
    let definition = source.find("\\items").unwrap() + 1;
    assert_eq!(
        entry.target.map(|span| (span.start, span.end)),
        Some((definition, definition + "items".len()))
    );
    // A widened call reads as the value that the position takes, as an injected value does.
    let entry = index.at(0, source.find("make n").unwrap() + 5).unwrap();
    assert_eq!(entry.detail, "n: i64");
    let entry = index.at(0, source.find("make n").unwrap() + 4).unwrap();
    assert_eq!(entry.detail, "(i64 | f64 | string)");
    // No entry spells a generated local, and none names a synthesized declaration.
    for entry in &index.entries {
        assert_hidden(&entry.detail);
        assert!(
            !entry.detail.contains("$match") && !entry.detail.contains("$retag"),
            "{}",
            entry.detail
        );
    }
}

#[test]
fn comparison_does_not_widen() {
    // The operands of an operator are no place where a value is widened: it would move the
    // variable, and only on one side. The A24 rule for a member (`v == 1` is injected, then
    // needs `Eq`) is unchanged.
    for operand in ["w", "(w)"] {
        for operator in ["==", "!=", "<"] {
            let error = rejects(
                &format!(
                    "let v: (f64 | i64 | string) = 1\nlet w: (i64 | string) = 2\nlet same = v {operator} {operand}\n0\n"
                ),
                "E1003",
                "type_mismatch",
            );
            assert_eq!(
                error.message, "expected (i64 | f64 | string), found (i64 | string)",
                "{operator} {operand}"
            );
        }
    }
    rejects(
        "let v: (f64 | i64 | string) = 1\nlet w: (i64 | string) = 2\nlet same = w == v\n0\n",
        "E1003",
        "type_mismatch",
    );
}

// A25, step 3: `:? (A | B) [as x]` narrows to an anonymous union of some members.

/// `num`: a number for each member of `(i64 | f64)`, to read a narrowed value back
/// (`num 4 == 8`, `num 1.5 == 3`, `num 0.5 == 0`).
const NUM: &str = "def num :: (i64 | f64) -> i64 = \\n ->\n    match n with\n    | :? i64 as k -> k * 2\n    | :? f64 as x -> if x > 1.0 then 3 else 0\n\n";

/// `text`: a number for each member of `(i64 | string)` (`text 7 == 7`, `text \"hello\" == 5`).
const TEXT: &str = "def text :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n\n";

#[test]
fn narrows_to_sub_unions() {
    // The expected values follow from `num` and `text` by hand; each assert spells its sum.
    passes(
        &format!(
            "{NUM}{TEXT}\
             def size :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n\n\
             def keep :: own (i64 | string | [i64]) -> (i64 | string) = \\v ->\n    match v with\n    | :? (i64 | string) as x -> x\n    | :? [i64] as xs -> xs.length\n\n\
             def either :: own (f64 | i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? (i64 | bool) -> 1\n    | :? (string | f64) -> 2\n\n\
             def inside :: own Maybe<(f64 | i64 | string)> -> i64 = \\m ->\n    match m with\n    | Some (:? (i64 | f64) as n) -> num n\n    | Some (:? string as s) -> s.length\n    | None -> -1\n\n\
             def guards :: own (f64 | i64 | string) -> i64 = \\v ->\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n\n\
             def only :: own (f64 | i64 | string) -> i64 = \\(:? (i64 | f64) as n) -> num n\n\n\
             def generic :: own (Maybe<'a> | i64 | string) -> (Maybe<'a> | i64) = \\v ->\n    match v with\n    | :? (Maybe<'a> | i64) as m -> m\n    | :? string as s -> s.length\n\n\
             def count :: own (Maybe<string> | i64) -> i64 = \\v ->\n    match v with\n    | :? Maybe<string> as m -> (match m with | Some s -> s.length | None -> -1)\n    | :? i64 as n -> n\n\n\
             def make :: i64 -> (f64 | i64 | string) = \\n -> if n > 0 then n else \"neg\"\n\n\
             def made :: i64 -> i64 = \\k ->\n    match make k with\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n\n\
             test \"narrow\" =\n    \
             assert (size 4 == 8)\n    \
             assert (size 1.5 == 3)\n    \
             assert (size 0.5 == 0)\n    \
             assert (size \"abc\" == 3)\n    \
             assert (text (keep 7) == 7)\n    \
             assert (text (keep \"hello\") == 5)\n    \
             assert (text (keep [1, 2, 3]) == 3)\n    \
             assert (either 1 + either true + either \"s\" + either 2.0 == 1 + 1 + 2 + 2)\n    \
             assert (inside (Some 4) + inside (Some \"abc\") + inside None == 8 + 3 - 1)\n    \
             assert (guards 1.5 + guards \"abc\" == 3 + 3)\n    \
             assert (only 4 == 8)\n    \
             assert (count (generic (Some \"abcd\")) == 4)\n    \
             assert (count (generic 7) == 7)\n    \
             assert (count (generic \"xy\") == 2)\n    \
             assert (made 4 + made 0 == 8 + 3)\n"
        ),
        Edition::E2026,
    );
    // The binder has the narrowed union's type, in its canonical order.
    let error = rejects(
        "def f :: own (f64 | i64 | string) -> bool = \\v ->\n    match v with\n    | :? (f64 | i64) as n -> n\n    | :? string -> false\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected bool, found (i64 | f64)");
    // A lambda parameter traps for a value of another member (a `for` pattern: see
    // `narrowing_in_loops_records_arrays_and_lists`).
    traps(
        &format!(
            "{NUM}def only :: own (f64 | i64 | string) -> i64 = \\(:? (i64 | f64) as n) -> num n\n\ntest \"other member\" = assert (only \"s\" == 0)\n"
        ),
        Edition::E2026,
    );
    // An alias names the narrowed union as well.
    accepts(
        "type Number = (i64 | f64)\ndef f :: own (f64 | i64 | string) -> Number = \\v ->\n    match v with\n    | :? Number as n -> n\n    | :? string -> 0\n",
    );
}

#[test]
fn narrowing_in_loops_records_arrays_and_lists() {
    // A narrowing is a pattern like any other (review round 1): a `for` pattern, a record
    // field, array elements and the head of a list. By hand from `num` and `text`.
    let program = format!(
        "{NUM}{TEXT}record Cell {{ value: (f64 | i64 | string), count: i64 }}\n\n\
         def looped :: own [(f64 | i64 | string)] -> i64 = \\items ->\n    let mut total = 0i64\n    for (:? (i64 | f64) as n) in items do\n        total = total + num n\n    total\n\n\
         def cell :: own Cell -> i64 = \\c ->\n    match c with\n    | {{ value = :? (i64 | f64) as n, count = k }} -> num n * 100 + k\n    | {{ value = :? string as s, count = k }} -> s.length * 100 + k\n\n\
         def owner :: own Cell -> i64 = \\c ->\n    match c with\n    | {{ value = :? (i64 | string) as x }} -> text x\n    | {{ value = :? f64 }} -> -1\n\n\
         def pair :: own [(f64 | i64 | string)] -> i64 = \\xs ->\n    match xs with\n    | [:? (i64 | f64) as n, :? (i64 | f64) as m] -> num n * 100 + num m\n    | _ -> -1\n\n\
         def head :: own [|(f64 | i64 | string)|] -> i64 = \\list ->\n    match list with\n    | (:? (i64 | f64) as n) :: _ -> num n\n    | (:? string as s) :: _ -> s.length * 10\n    | [||] -> -1\n\n\
         test \"patterns\" =\n    \
         assert (looped [1, 2.5, 3] == 2 + 3 + 6)\n    \
         assert (looped [] == 0)\n    \
         assert (cell (Cell {{ value: 4, count: 7 }}) == 807)\n    \
         assert (cell (Cell {{ value: 0.5, count: 2 }}) == 2)\n    \
         assert (cell (Cell {{ value: \"ab\", count: 3 }}) == 203)\n    \
         assert (owner (Cell {{ value: \"abc\", count: 1 }}) + owner (Cell {{ value: 5, count: 1 }}) == 3 + 5)\n    \
         assert (owner (Cell {{ value: 1.5, count: 1 }}) == -1)\n    \
         assert (pair [4, 1.5] == 803)\n    \
         assert (pair [4, \"x\"] + pair [1] == -2)\n    \
         assert (head [|2.5, 9|] + head [|\"xyz\"|] + head [||] == 3 + 30 - 1)\n"
    );
    passes(&program, Edition::E2027);
    // A `for` pattern traps for an element of another member.
    traps(
        &format!(
            "{NUM}def looped :: own [(f64 | i64 | string)] -> i64 = \\items ->\n    let mut total = 0i64\n    for (:? (i64 | f64) as n) in items do\n        total = total + num n\n    total\n\ntest \"other member\" = assert (looped [1, \"s\"] == 0)\n"
        ),
        Edition::E2027,
    );
    // Eleven narrowings of two members in one pattern make 2^11 = 2048 alternatives, over the
    // limit of 1024.
    let tests = [":? (i64 | f64)"; 11].join(", ");
    let wildcards = ["_"; 11].join(", ");
    let values = ["(f64 | i64 | string)"; 11].join(" * ");
    rejects(
        &format!(
            "def f :: own ({values}) -> i64 = \\t ->\n    match t with\n    | ({tests}) -> 1\n    | ({wildcards}) -> 0\n"
        ),
        "E1017",
        "pattern_alternatives_limit",
    );
}

#[test]
fn narrowing_to_every_member_binds_the_value() {
    // `W == U`: the value itself, also behind a reference, where a member that is not Copy
    // forbids a narrowing to fewer members. Nothing is tested, so one arm is exhaustive.
    passes(
        &format!(
            "{TEXT}def all :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (string | i64) as w -> text w\n\n\
             def seen :: ref (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as w -> (match w with | :? i64 as n -> n | :? string as s -> s.length)\n\n\
             test \"whole\" =\n    assert (all \"abcd\" + all 3 == 4 + 3)\n    let v: (i64 | string) = \"hello\"\n    assert (seen (ref v) + seen (ref v) == 5 + 5)\n"
        ),
        Edition::E2026,
    );
    let source = "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) -> 0\n    | _ -> 1\n";
    let module = analyze(source).unwrap();
    let warnings: Vec<(&str, &str)> = module
        .warnings
        .iter()
        .map(|warning| (warning.code, &source[warning.span.start..warning.span.end]))
        .collect();
    assert_eq!(warnings, [("W1003", "_")]);
    // The view cannot move out, as any binding of a borrowed value.
    rejects(
        "def take :: ref (i64 | string) -> (i64 | string) = \\v ->\n    match v with\n    | :? (i64 | string) as w -> w\n",
        "E1012",
        "move_out_of_reference_binding",
    );
}

#[test]
fn narrowing_is_exhaustive_as_or() {
    // Each member of the narrowed union counts as its own `:?` arm.
    for binder in ["", " as n"] {
        let error = missing(
            &format!(
                "def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64){binder} -> 0\n"
            ),
            ":? string",
        );
        assert_eq!(
            error.help.as_deref(),
            Some("add an arm for ':? string', or end the match with a wildcard arm '| _ -> ...'")
        );
    }
    missing(
        "def f :: own (f64 | i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) -> 0\n    | :? (i64 | string) -> 1\n",
        ":? bool",
    );
    accepts(
        "def f :: own (f64 | i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n -> 0\n    | :? (bool | string) as rest -> 1\n",
    );
    // An arm that the narrowing covers is unreachable, and so is a narrowing whose members
    // are all covered.
    let warnings = |source: &str| -> Vec<(String, String)> {
        analyze(source)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
            .warnings
            .iter()
            .map(|warning| {
                (
                    warning.code.to_string(),
                    source[warning.span.start..warning.span.end].to_string(),
                )
            })
            .collect()
    };
    let unreachable = |text: &str| vec![("W1003".to_string(), text.to_string())];
    assert_eq!(
        warnings(
            "def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) -> 0\n    | :? i64 -> 1\n    | :? string -> 2\n"
        ),
        unreachable(":? i64")
    );
    assert_eq!(
        warnings(
            "def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 -> 0\n    | :? f64 -> 1\n    | :? (i64 | f64) -> 2\n    | :? string -> 3\n"
        ),
        unreachable(":? (i64 | f64)")
    );
    // A narrowing with one member left is reachable.
    assert!(
        warnings(
            "def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 -> 0\n    | :? (i64 | f64) -> 1\n    | :? string -> 2\n"
        )
        .is_empty()
    );
    // A guarded arm covers nothing.
    assert!(
        warnings(&format!(
            "{NUM}def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n when num n > 5 -> 0\n    | :? (i64 | f64) -> 1\n    | :? string -> 2\n"
        ))
        .is_empty()
    );
    missing(
        &format!(
            "{NUM}def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n when num n > 5 -> 0\n    | :? string -> 2\n"
        ),
        ":? i64",
    );
}

#[test]
fn narrowing_copies_copy_members_from_views() {
    // Behind a reference, in a collection and under a guard the narrowed value is a copy, so
    // the matched value stays usable. `arr [1, 2, 3] == 300`, `arr 7 == 7`.
    let arr = "def arr :: own ([i64] | i64) -> i64 = \\n ->\n    match n with\n    | :? i64 as k -> k\n    | :? [i64] as xs -> xs.length * 100\n\n";
    passes(
        &format!(
            "{NUM}{arr}\
             def by_ref :: ref (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n\n\
             def arr_ref :: ref ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? ([i64] | i64) as n -> arr n\n    | :? string as s -> s.length\n\n\
             def guarded :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n when num n > 5 -> 1000 + num n\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n\n\
             def guarded_arr :: own ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? ([i64] | i64) as n when arr n > 150 -> 1\n    | :? ([i64] | i64) as n -> arr n\n    | :? string as s -> s.length\n\n\
             def guarded_ref :: ref (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n when num n > 5 -> 1000 + num n\n    | _ -> 0\n\n\
             def pair :: own ((f64 | i64 | string) * (f64 | i64 | string)) -> i64 = \\v ->\n    match v with\n    | (:? (i64 | f64) as n, :? string) | (:? string, :? (i64 | f64) as n) -> num n\n    | _ -> -1\n\n\
             def twice :: own (f64 | i64 | string) -> i64 = \\v ->\n    let first = match v with\n        | :? (i64 | f64) as n -> num n\n        | :? string -> 0\n    first + by_ref (ref v)\n\n\
             test \"views\" =\n    \
             let a: (f64 | i64 | string) = 4\n    \
             let b: (f64 | i64 | string) = 1.5\n    \
             let c: (f64 | i64 | string) = \"abc\"\n    \
             assert (by_ref (ref a) + by_ref (ref b) + by_ref (ref c) == 8 + 3 + 3)\n    \
             assert (by_ref (ref a) == 8)\n    \
             let d: ([i64] | i64 | string) = [1, 2, 3]\n    \
             let e: ([i64] | i64 | string) = 7\n    \
             assert (arr_ref (ref d) + arr_ref (ref d) + arr_ref (ref e) == 300 + 300 + 7)\n    \
             assert (guarded 4 == 1008)\n    \
             assert (guarded 2 + guarded 1.5 + guarded \"ab\" == 4 + 3 + 2)\n    \
             assert (guarded_arr [1, 2] == 1)\n    \
             assert (guarded_arr [1] + guarded_arr 5 + guarded_arr \"abc\" == 100 + 5 + 3)\n    \
             assert (guarded_ref (ref a) + guarded_ref (ref b) + guarded_ref (ref c) == 1008)\n    \
             assert (pair (4, \"x\") + pair (\"x\", 1.5) + pair (\"x\", \"y\") + pair (1, 2) == 8 + 3 - 1 - 1)\n    \
             assert (twice 4 + twice 1.5 + twice \"abc\" == 16 + 6 + 3)\n    \
             let items: [(f64 | i64 | string)] = [1, \"ab\", 2.5]\n    \
             let mut total = 0\n    \
             for item in items do\n        \
             total = total + (match item with\n            \
             | :? (f64 | i64) as n -> num n\n            \
             | :? string as s -> s.length)\n    \
             assert (total == 2 + 2 + 3)\n    \
             assert (items.length == 3)\n"
        ),
        Edition::E2026,
    );
    // A read parameter (2027) is such a view.
    passes(
        &format!(
            "{NUM}def size :: (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n\n\
             test \"read\" =\n    assert (size 4 + size 1.5 + size \"abc\" == 8 + 3 + 3)\n"
        ),
        Edition::E2027,
    );
    // The copy of an array member is an implicit copy, reported at the pattern.
    let source = format!(
        "{arr}@no_copy\ndef arr_ref :: ref ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? ([i64] | i64) as n -> arr n\n    | :? string as s -> s.length\n"
    );
    let error = rejects(&source, "E1031", "effect_copy_in_no_copy");
    let pattern = ":? ([i64] | i64) as n";
    let at = source.find(pattern).unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + pattern.len()));
}

#[test]
fn rejects_narrowing_borrowed_non_copy() {
    let message = "cannot narrow a borrowed value into (i64 | string) because string is not Copy";
    let help = "match each member with its own ':?' arm, or match an owned value";
    // Through a reference: the location is the `:?`.
    let source = "def f :: ref ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as x -> 1\n    | :? [i64] -> 2\n";
    let error = rejects(source, "E1041", "type_test_narrow_borrowed");
    assert_eq!(error.message, message);
    assert_eq!(error.help.as_deref(), Some(help));
    let at = source.find(":?").unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + 2));
    // A borrowed local, a read parameter (2027), an element of a collection and a payload
    // below a reference.
    let error = rejects(
        "let v: ([i64] | i64 | string) = 1\nlet r = ref v\nmatch r with\n| :? (i64 | string) as x -> 1\n| :? [i64] -> 2\n",
        "E1041",
        "type_test_narrow_borrowed",
    );
    assert_eq!(error.message, message);
    let error = rejects_in(
        "def f :: ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as x -> 1\n    | :? [i64] -> 2\n",
        Edition::E2027,
        "E1041",
        "type_test_narrow_borrowed",
    );
    assert_eq!(error.message, message);
    assert_eq!(error.help.as_deref(), Some(help));
    rejects(
        "def f :: own [([i64] | i64 | string)] -> i64 = \\items ->\n    let mut total = 0\n    for item in items do\n        total = total + (match item with\n            | :? (i64 | string) as x -> 1\n            | :? [i64] -> 2)\n    total\n",
        "E1041",
        "type_test_narrow_borrowed",
    );
    rejects(
        "def f :: ref Maybe<([i64] | i64 | string)> -> i64 = \\m ->\n    match m with\n    | Some (:? (i64 | string) as x) -> 1\n    | _ -> 2\n",
        "E1041",
        "type_test_narrow_borrowed",
    );
    // A member with a type variable is not known to be Copy.
    let error = rejects(
        "def f :: ref (Maybe<'a> | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (Maybe<'a> | i64) as x -> 1\n    | :? string -> 2\n",
        "E1041",
        "type_test_narrow_borrowed",
    );
    assert_eq!(
        error.message,
        "cannot narrow a borrowed value into (i64 | Maybe<'a>) because Maybe<'a> is not Copy"
    );
    // Without a binder nothing is made, so the same tests are accepted.
    for binder in ["", " as _"] {
        accepts(&format!(
            "def f :: ref ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string){binder} -> 1\n    | :? [i64] -> 2\n"
        ));
    }
    // A guard reads the binder before the arm is chosen, when nothing can move yet.
    let source = "def f :: own ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as x when true -> 1\n    | _ -> 2\n";
    let error = rejects(source, "E1041", "type_test_narrow_borrowed");
    assert_eq!(
        error.message,
        "cannot narrow a value into (i64 | string) in an arm with a guard because string is not Copy"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("match each member with its own ':?' arm, or test the condition in the arm's body")
    );
    let at = source.find(":?").unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + 2));
    accepts(
        "def f :: own ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) when true -> 1\n    | _ -> 2\n",
    );
    // A guarded arm of a borrowed value reports the borrow: no guard is to blame.
    let error = rejects(
        "def f :: ref ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as x when true -> 1\n    | _ -> 2\n",
        "E1041",
        "type_test_narrow_borrowed",
    );
    assert_eq!(error.message, message);
}

#[test]
fn type_test_errors_point_at_the_marker_inside_parentheses() {
    // The parentheses around a type test (required in a payload) group it; the errors about the
    // test are at its `:?` all the same (review round 1), with or without spaces inside them.
    for (source, kind) in [
        (
            "def f :: ref Maybe<([i64] | i64 | string)> -> i64 = \\m ->\n    match m with\n    | Some (:? (i64 | string) as x) -> 1\n    | _ -> 2\n",
            "type_test_narrow_borrowed",
        ),
        (
            "def f :: ref ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | (  :? (i64 | string) as x  ) -> 1\n    | _ -> 2\n",
            "type_test_narrow_borrowed",
        ),
        (
            "def f :: own Maybe<([i64] | i64 | string)> -> i64 = \\m ->\n    match m with\n    | Some ((:? (i64 | string) as x)) when true -> 1\n    | _ -> 2\n",
            "type_test_narrow_borrowed",
        ),
        (
            "def f :: own Maybe<i64> -> i64 = \\m ->\n    match m with\n    | Some (:? i64 as x) -> x\n    | _ -> 2\n",
            "type_test_not_anonymous_union",
        ),
        (
            "def f :: own (i64 * i64) -> i64 = \\pair ->\n    match pair with\n    | ( :? i64 as x, _) -> x\n",
            "type_test_not_anonymous_union",
        ),
    ] {
        let error = rejects(source, "E1041", kind);
        let at = source.find(":?").unwrap();
        assert_eq!((error.span.start, error.span.end), (at, at + 2), "{source}");
    }
    // The type test's own extent is the pattern that a narrowing copy is reported at.
    let source = "def arr :: own ([i64] | i64) -> i64 = \\n ->\n    match n with\n    | :? i64 as k -> k\n    | :? [i64] as xs -> xs.length\n\n\
         @no_copy\ndef arrays :: ref Maybe<([i64] | i64 | string)> -> i64 = \\m ->\n    match m with\n    | Some (:? ([i64] | i64) as n) -> arr n\n    | _ -> 0\n";
    let error = rejects(source, "E1031", "effect_copy_in_no_copy");
    let pattern = ":? ([i64] | i64) as n";
    let at = source.find(pattern).unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + pattern.len()));
}

#[test]
fn rejects_narrowing_non_members() {
    let source = "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | bool) -> 0\n    | _ -> 1\n";
    let error = rejects(source, "E1041", "type_test_not_member");
    assert_eq!(error.message, "bool is not a member of (i64 | string)");
    assert_eq!(error.help.as_deref(), Some("the members are i64, string"));
    // The location is the type after `:?`.
    let tested = "(i64 | bool)";
    let at = source.find(tested).unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + tested.len()));
    // The first member that the matched union lacks is named, in the canonical order (numbers
    // come before `bool`); more members than it has is the same error.
    let error = rejects(
        "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (bool | i64 | f64) as x -> 0\n    | _ -> 1\n",
        "E1041",
        "type_test_not_member",
    );
    assert_eq!(error.message, "f64 is not a member of (i64 | string)");
    let error = rejects(
        "def f :: own (i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string | bool) -> 0\n",
        "E1041",
        "type_test_not_member",
    );
    assert_eq!(error.message, "bool is not a member of (i64 | string)");
}

#[test]
fn conversions_do_not_adapt_function_purity() {
    // A retagging moves the payload as it is, so a function member is only the member that has
    // the same purity (review round 1; A24's type tests do the same): the conversions report the
    // documented kinds, not a mismatch between two member types.
    let call = "def call :: own (i64 | bool | (i64 -> i64)) -> i64 = \\v ->\n    match v with\n    | :? (i64 -> i64) as f -> f 4\n    | :? i64 as k -> k\n    | :? bool -> 7\n\npure def twice :: i64 -> i64 = \\x -> x * 2\n\n";
    let error = rejects(
        &format!("{call}def run :: own (i64 | pure (i64 -> i64)) -> i64 = \\small -> call small\n"),
        "E1003",
        "anonymous_union_not_subset",
    );
    assert_eq!(
        error.message,
        "expected (i64 | bool | (i64 -> i64)), found (i64 | pure (i64 -> i64)); pure (i64 -> i64) is not a member of (i64 | bool | (i64 -> i64))"
    );
    let error = rejects(
        "def call :: own (i64 | bool | pure (i64 -> i64)) -> i64 = \\v -> 0\n\ndef run :: own (i64 | (i64 -> i64)) -> i64 = \\small -> call small\n",
        "E1003",
        "anonymous_union_not_subset",
    );
    assert!(
        error
            .message
            .ends_with("; (i64 -> i64) is not a member of (i64 | bool | pure (i64 -> i64))"),
        "{}",
        error.message
    );
    // Narrowing names the member of `W` that the matched union lacks.
    let source = "def f :: own (i64 | bool | (i64 -> i64)) -> i64 = \\v ->\n    match v with\n    | :? (i64 | pure (i64 -> i64)) as x -> 1\n    | _ -> 7\n";
    let error = rejects(source, "E1041", "type_test_not_member");
    assert_eq!(
        error.message,
        "pure (i64 -> i64) is not a member of (i64 | bool | (i64 -> i64))"
    );
    assert_eq!(
        error.help.as_deref(),
        Some("the members are i64, bool, (i64 -> i64)")
    );
    let tested = "(i64 | pure (i64 -> i64))";
    let at = source.find(tested).unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + tested.len()));
    // The pure function itself is adapted where it is injected, member by member. 2 * 4 + 7 = 15.
    passes(
        &format!(
            "{call}test \"members\" =\n    assert (call twice + call 7 == 15)\n    let wide: (i64 | bool | (i64 -> i64)) = twice\n    assert (call wide == 8)\n"
        ),
        Edition::E2027,
    );
}

#[test]
fn narrowing_moves_owned_values() {
    // A member that is not Copy moves into the narrowed value: the matched value is gone.
    let error = rejects(
        &format!(
            "{TEXT}def f :: own (i64 | string | bool) -> i64 = \\v ->\n    let first = match v with\n        | :? (i64 | string) as x -> text x\n        | :? bool -> 2\n    match v with\n    | :? i64 -> first\n    | _ -> 0\n"
        ),
        "E1012",
        "use_after_move",
    );
    assert!(error.message.contains("'v'"), "{}", error.message);
    // The narrowed value is an owned value of its own: it moves on, once.
    accepts(&format!(
        "{TEXT}def f :: own (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as x -> text x\n    | :? bool -> 2\n"
    ));
    let error = rejects(
        &format!(
            "{TEXT}def f :: own (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as x -> text x + text x\n    | :? bool -> 2\n"
        ),
        "E1012",
        "use_after_move",
    );
    assert!(error.message.contains("'x'"), "{}", error.message);
    // When every member of the narrowed union is Copy, the value stays whole, as it does
    // after the arms `:? i64 as n` and `:? f64 as n`.
    accepts(&format!(
        "{NUM}{TEXT}def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    let first = match v with\n        | :? (i64 | f64) as n -> num n + num n\n        | :? string -> 0\n    match v with\n    | :? string as s -> first + s.length\n    | _ -> first\n"
    ));
}

#[test]
fn narrowing_beside_other_bindings() {
    // `whole` moves the value that `n` is narrowed from, so `n` is a copy that the pattern
    // makes; two narrowings in one pattern; the payload of a case; a binder that shadows the
    // matched name. By hand: `size` is 1 for an f64, the i64 itself, the length of a string.
    passes(
        &format!(
            "{NUM}{TEXT}\
             def size :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? f64 -> 1\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n\n\
             def both :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | (:? (i64 | f64) as n) as whole -> num n + size whole\n    | :? string as s -> s.length\n\n\
             def two :: own ((f64 | i64 | string) * (f64 | i64 | string)) -> i64 = \\pair ->\n    match pair with\n    | (:? (i64 | f64) as a, :? (i64 | f64) as b) -> num a * 100 + num b\n    | (:? (i64 | string) as a, :? (i64 | string) as b) -> text a * 10 + text b\n    | _ -> -1\n\n\
             def beside :: own ((f64 | i64 | string) * string) -> i64 = \\pair ->\n    match pair with\n    | (:? (i64 | f64) as n, s) -> num n + s.length\n    | (:? string as t, s) -> t.length * s.length\n\n\
             def nested :: own Maybe<(i64 | string | [i64])> -> i64 = \\m ->\n    match m with\n    | Some (:? (i64 | string) as x) -> text x\n    | Some (:? [i64] as xs) -> xs.length * 10\n    | None -> -1\n\n\
             def shadow :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as v -> num v\n    | :? string as v -> v.length\n\n\
             test \"beside\" =\n    \
             assert (both 4 == 8 + 4)\n    \
             assert (both 1.5 == 3 + 1)\n    \
             assert (both \"abc\" == 3)\n    \
             assert (two (4, 1.5) == 800 + 3)\n    \
             assert (two (\"ab\", 7) == 20 + 7)\n    \
             assert (two (\"ab\", \"xyz\") == 20 + 3)\n    \
             assert (two (1.5, \"x\") == -1)\n    \
             assert (beside (4, \"ab\") == 8 + 2)\n    \
             assert (beside (\"abc\", \"ab\") == 6)\n    \
             assert (nested (Some \"hello\") == 5)\n    \
             assert (nested (Some 9) == 9)\n    \
             assert (nested (Some [1, 2]) == 20)\n    \
             assert (nested None == -1)\n    \
             assert (shadow 4 + shadow \"abc\" == 8 + 3)\n"
        ),
        Edition::E2026,
    );
    // A member that is not Copy moves with the narrowing, so a second binding of the value
    // is a second move, as it is beside `:? string as s`.
    rejects(
        &format!(
            "{TEXT}def f :: own (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | (:? (i64 | string) as x) as whole -> text x\n    | :? bool -> 2\n"
        ),
        "E1012",
        "use_after_move",
    );
}

#[test]
fn double_moves_out_of_a_matched_temporary_name_no_generated_local() {
    // Two bindings of one pattern that take the same part of a matched temporary: the error
    // names no local that the compiler made (review round 1), and the earlier move is at the
    // binder that made it.
    let make = "def make :: i64 -> (f64 | i64 | string) = \\n -> if n > 0 then n else \"neg\"\n\n\
         def take :: own (f64 | i64 | string) -> i64 = \\v -> 0\n\n\
         union V = A of string | B of i64\n\ndef mk :: i64 -> V = \\n -> if n > 0 then A \"a\" else B n\n\n\
         def eat :: own V -> i64 = \\v -> 0\n\n";
    for (body, binder) in [
        (
            "    match make n with\n    | (:? (i64 | string) as x) as whole -> take whole\n    | _ -> 0\n",
            "x)",
        ),
        (
            "    match make n with\n    | (:? string as s) as whole -> s.length + take whole\n    | _ -> 0\n",
            "s)",
        ),
        (
            "    match mk n with\n    | (A owner) as whole -> owner.length + eat whole\n    | _ -> 0\n",
            "owner)",
        ),
    ] {
        let source = format!("{make}def f :: i64 -> i64 = \\n ->\n{body}");
        let error = rejects(&source, "E1012", "use_after_move");
        assert_eq!(
            error.message,
            "use of moved or partially moved matched value: two bindings of the pattern take the same part of it",
            "{body}"
        );
        assert_eq!(
            error.help.as_deref(),
            Some(
                "bind the whole value with 'as' or bind its parts, not both, and match the bound value again in the arm"
            )
        );
        let at = source.find(binder).unwrap();
        let related: Vec<_> = error
            .related
            .iter()
            .map(|related| related.span.start)
            .collect();
        assert_eq!(related, [at], "{body}");
    }
    // A matched variable is named as before.
    let error = rejects(
        &format!(
            "{make}def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | (:? (i64 | string) as x) as whole -> take whole\n    | _ -> 0\n"
        ),
        "E1012",
        "use_after_move",
    );
    assert_eq!(error.message, "use of moved or partially moved value 'v'");
}

#[test]
fn narrowing_arms_may_move_or_replace_the_matched_value() {
    // A narrowing of Copy members copies the payload and leaves the matched value to the arm,
    // as the arms `:? i64 as n`, `:? f64 as n` do, also when the match is not the result of
    // its function (review round 1): the arm may move the matched variable, assign to it or
    // write through the reference that it was read from, and the narrowed value keeps the old
    // payload. By hand: `num` is 2 k for the i64 k and 3 for an f64 above 1, `size` 20000 for
    // an f64, the i64 itself and the length of a string.
    let program = format!(
        "{NUM}def size :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? f64 -> 20000\n    | :? i64 as n -> n\n    | :? string as s -> s.length\n\n\
         def moved :: own (f64 | i64 | string) -> i64 = \\v ->\n    let r = match v with\n        | :? (i64 | f64) as n -> num n + size v\n        | :? string -> 1\n    r\n\n\
         def replaced :: own (f64 | i64 | string) -> i64 = \\start ->\n    let mut v = start\n    let r = match v with\n        | :? (i64 | f64) as n ->\n            v = \"replaced\"\n            num n\n        | :? string -> 0\n    r * 100 + size v\n\n\
         def through :: ref mut (f64 | i64 | string) -> i64 = \\cell ->\n    let r = match deref cell with\n        | :? (i64 | f64) as n ->\n            deref cell = \"through\"\n            num n\n        | :? string -> 0\n    r\n\n\
         test \"arms\" =\n    \
         assert (moved 4 == 8 + 4)\n    \
         assert (moved 2.5 == 3 + 20000)\n    \
         assert (moved \"abc\" == 1)\n    \
         assert (replaced 6 == 1200 + 8)\n    \
         assert (replaced 0.5 == 0 + 8)\n    \
         assert (replaced \"ab\" == 0 + 2)\n    \
         let mut cell: (f64 | i64 | string) = 9\n    \
         assert (through (ref mut cell) == 18)\n    \
         assert (through (ref mut cell) == 0)\n    \
         assert (size cell == 7)\n"
    );
    passes(&program, Edition::E2027);
    // An arm that does not reach the matched variable keeps the switch on the tag.
    let ir = accepts(&format!(
        "{NUM}def kept :: own (f64 | i64 | string) -> i64 = \\v ->\n    let r = match v with\n        | :? (i64 | f64) as n -> num n\n        | :? string as s -> s.length\n    r\n"
    ));
    assert!(
        function_body(&ir, "kept").matches("switch i32 ").count() >= 2,
        "{ir}"
    );
}

#[test]
fn narrowing_in_or_patterns() {
    // Both sides narrow the same union with the same test (see `pair` in
    // `narrowing_copies_copy_members_from_views`). Anything else has no single value to bind.
    let unlike = "both sides of an OR pattern must narrow the same anonymous union with the same ':?' test to bind one name";
    for pattern in [
        // Two unions with the members of the narrowed one.
        "(:? (i64 | f64) as n, _) | (_, :? (i64 | f64) as n)",
        // A narrowing and a plain binding of the narrowed type, in both orders.
        "(:? (i64 | f64) as n, _) | (_, n)",
        "(_, n) | (:? (i64 | f64) as n, _)",
    ] {
        let source = format!(
            "def f :: own ((f64 | i64 | string) * (f64 | i64)) -> i64 = \\v ->\n    match v with\n    | {pattern} -> 1\n    | _ -> 2\n"
        );
        let error = rejects(&source, "E1020", "or_pattern_binding_mismatch");
        assert_eq!(error.message, unlike, "{pattern}");
        // The location is the second binder.
        let at = source.rfind(" n").unwrap() + 1;
        assert_eq!(
            (error.span.start, error.span.end),
            (at, at + 1),
            "{pattern}"
        );
    }
    // Two narrowings to different unions give the name two types.
    let error = rejects(
        "def f :: own (f64 | i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n | :? (i64 | bool) as n -> 1\n    | _ -> 2\n",
        "E1003",
        "type_mismatch",
    );
    assert_eq!(error.message, "expected (i64 | f64), found (i64 | bool)");
    // A name that only one side binds, and a name bound twice on one side.
    let error = rejects(
        "def f :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n | :? string -> 1\n",
        "E1020",
        "or_pattern_binding_mismatch",
    );
    assert_eq!(
        error.message,
        "both sides of an OR pattern must bind the same names"
    );
    let error = rejects(
        "def f :: own ((f64 | i64 | string) * (f64 | i64 | string)) -> i64 = \\v ->\n    match v with\n    | (:? (i64 | f64) as n, :? (i64 | f64) as n) -> 1\n    | _ -> 2\n",
        "E1001",
        "duplicate_name",
    );
    assert_eq!(error.message, "duplicate or reserved name 'n'");
    rejects(
        "def f :: own ((f64 | i64 | string) * i64) -> i64 = \\v ->\n    match v with\n    | (:? (i64 | f64) as n, n) -> 1\n    | _ -> 2\n",
        "E1001",
        "duplicate_name",
    );
}

#[test]
fn narrowing_retags_without_allocating() {
    // A narrowing of a union that is not recursive switches on the tag and moves or copies the
    // payload: no allocation and no implicit copy (`@no_copy` holds) for numbers and strings.
    let ir = accepts(&format!(
        "{NUM}{TEXT}@no_copy\ndef numbers :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n\n\
         @no_copy\ndef owners :: own (i64 | string | bool) -> i64 = \\v ->\n    match v with\n    | :? (i64 | string) as x -> text x\n    | :? bool -> 2\n\n\
         export def run :: i64 -> i64 = \\n ->\n    let a: (f64 | i64 | string) = n\n    let b: (i64 | string | bool) = n\n    numbers a + owners b\n"
    ));
    for name in ["numbers", "owners"] {
        let body = function_body(&ir, name);
        assert!(!body.contains("@tz.alloc"), "{name}:\n{body}");
        // The match of the function and the retagging match of the narrowing.
        assert!(body.matches("switch i32 ").count() >= 2, "{name}:\n{body}");
    }
    // An array member is copied once into the narrowed value, as a nested match that wraps it
    // again copies it; the copy is reported at the pattern.
    let source = "def arr :: own ([i64] | i64) -> i64 = \\n ->\n    match n with\n    | :? i64 as k -> k\n    | :? [i64] as xs -> xs.length\n\n\
         @no_copy\ndef arrays :: own ([i64] | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? ([i64] | i64) as n -> arr n\n    | :? string as s -> s.length\n";
    let error = rejects(source, "E1031", "effect_copy_in_no_copy");
    let pattern = ":? ([i64] | i64) as n";
    let at = source.find(pattern).unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + pattern.len()));
    let ir = accepts(&source.replace("@no_copy\n", ""));
    assert_eq!(
        function_body(&ir, "arrays").matches("@tz.alloc").count(),
        1,
        "{ir}"
    );
}

#[test]
fn hover_on_a_narrowed_binder_shows_the_narrowed_union() {
    let source = "def f :: own (f64 | i64 | string) -> (i64 | f64) = \\value ->\n    match value with\n    | :? (i64 | f64) as number -> number\n    | :? string -> 0\n0\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", source)]).unwrap();
    let binder = source.find("as number").unwrap() + 3;
    let entry = index.at(0, binder).unwrap();
    assert_eq!(entry.detail, "number: (i64 | f64)");
    // The use in the body goes to the binder.
    let used = source.find("-> number").unwrap() + 3;
    let entry = index.at(0, used).unwrap();
    assert_eq!(entry.detail, "number: (i64 | f64)");
    assert_eq!(
        entry.target.map(|span| (span.start, span.end)),
        Some((binder, binder + "number".len()))
    );
    // The `:?` reads as the narrowed value; no entry spells a generated local.
    let entry = index.at(0, source.find(":?").unwrap()).unwrap();
    assert_eq!(entry.detail, "(i64 | f64)");
    for entry in &index.entries {
        assert_hidden(&entry.detail);
        for hidden in ["$match", "$retag", "$narrow"] {
            assert!(!entry.detail.contains(hidden), "{}", entry.detail);
        }
    }
    // An unused binder is reported at its own name, with the usual fix.
    let source = "def f :: own (f64 | i64 | string) -> i64 = \\value ->\n    match value with\n    | :? (i64 | f64) as number -> 1\n    | :? string -> 0\n";
    let module = analyze(source).unwrap();
    let warnings: Vec<(&str, &str)> = module
        .warnings
        .iter()
        .map(|warning| (warning.code, &source[warning.span.start..warning.span.end]))
        .collect();
    assert_eq!(warnings, [("W1001", "number")]);
    assert_eq!(module.warnings[0].fixes.len(), 1);
}
