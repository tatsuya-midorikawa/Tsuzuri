//! Anonymous unions `(A | B)` and type test patterns `:? T` (A24), and the conversions between
//! anonymous unions (A25).

use tsuzuri::check::{CheckedModule, Type};
use tsuzuri::diagnostic::{Diagnostic, Edit, apply_edits};
use tsuzuri::driver::{Target, TestOptions, run_tests};
use tsuzuri::parser::{parse, parse_with_source};
use tsuzuri::syntax::{Edition, Program, TypeExpr, TypeExprKind};
use tsuzuri::{
    analyze, analyze_all, analyze_modules, analyze_modules_all, analyze_modules_with_semantics,
    analyze_with_edition, llvm,
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
/// `source: None } }, ` that a binder follows (the members of a tuple are followed by another type),
/// and the test's own extent follows the binder.
fn assert_type_test(source: &str, kind: &str, binder: Option<&str>) {
    assert_type_test_tree(source, &tree(source), kind, binder);
}

/// `assert_type_test` for `source` read in `edition`. The spans of the tree are those of
/// `parse`, which names no source.
fn assert_type_test_in(source: &str, edition: Edition, kind: &str, binder: Option<&str>) {
    let program = parse_with_source(source, 0, edition)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let tree = format!("{program:?}").replace("source: Some(0)", "source: None");
    assert_type_test_tree(source, &tree, kind, binder);
}

fn assert_type_test_tree(source: &str, tree: &str, kind: &str, binder: Option<&str>) {
    let start = format!("TypeTest(TypeExpr {{ kind: {kind}");
    let found = tree.match_indices(&start).any(|(at, _)| {
        let rest = &tree[at..];
        let named = rest.find("source: None } }, Some(Ident { text: \"");
        let none = rest.find("source: None } }, None, Span {");
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

/// Verification of Copilot review 2 of #102: the default comparison reads signatures that the
/// checker rejects (here `E1013`: a region-quantified function type inside a union), and members
/// that use the same region names in other places are still a set. By hand: in either written
/// order the members key as `({_0 _1} ref {_0} i64 -> ...)` and `({_0 _1} ref {_1} i64 -> ...)`,
/// so both versions print the same and nothing changed. A member whose regions really differ is
/// a change.
#[test]
fn api_diff_reorders_members_that_share_region_names() {
    let first = "({a b} ref {a} i64 -> ref {b} i64 -> i64)";
    let second = "({a b} ref {b} i64 -> ref {a} i64 -> i64)";
    let signature =
        |left: &str, right: &str| format!("def f :: own ({left} | {right}) -> unit = \\v -> ()\n");
    assert_eq!(
        api_changes(&signature(first, second), &signature(second, first), false),
        Vec::<String>::new()
    );
    let changes = api_changes(
        &signature(first, second),
        &signature(first, "({a b} ref {a} i64 -> ref {a} i64 -> i64)"),
        false,
    );
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert!(
        changes[0].starts_with("breaking changed def Main.f"),
        "{changes:?}"
    );
}

/// Copilot review 3 of #102: the default comparison reads signatures that the checker rejects
/// (`E1041`: members that differ only in their variables), and members whose blind keys tie are
/// still a set: the rest of the signature decides their order. By hand: in
/// `(Maybe<'a> | Maybe<'b>) -> 'a -> i64` the member whose variable is the later parameter comes
/// second in either written order, so both versions print `(Maybe<'t0> | Maybe<'t1>) -> 't1 ->
/// i64`; the chain and the ring of regions that review 2 left as a limit print one text for each
/// order too. Changing what the members share is still a change.
#[test]
fn api_diff_orders_tied_members_by_the_whole_signature() {
    let def = |signature: &str| format!("def f :: {signature} = \\v -> todo ()\n");
    for (old, new) in [
        (
            "(Maybe<'a> | Maybe<'b>) -> 'a -> i64",
            "(Maybe<'b> | Maybe<'a>) -> 'a -> i64",
        ),
        (
            "(Maybe<'a> | Maybe<'b>) -> 'a -> 'b -> i64",
            "(Maybe<'b> | Maybe<'a>) -> 'a -> 'b -> i64",
        ),
        (
            "((ref {c} string -> ref {a} i64 -> bool) | (ref {a} string -> ref {b} i64 -> bool)) -> unit",
            "((ref {a} string -> ref {b} i64 -> bool) | (ref {c} string -> ref {a} i64 -> bool)) -> unit",
        ),
        (
            "((ref {a} i64 * ref {b} i64 * ref {c} i64) | (ref {b} i64 * ref {c} i64 * ref {a} i64)) -> unit",
            "((ref {b} i64 * ref {c} i64 * ref {a} i64) | (ref {a} i64 * ref {b} i64 * ref {c} i64)) -> unit",
        ),
    ] {
        assert_eq!(
            api_changes(&def(old), &def(new), false),
            Vec::<String>::new(),
            "{old}\n{new}"
        );
    }
    for (old, new) in [
        (
            "(Maybe<'a> | Maybe<'b>) -> 'a -> i64",
            "(Maybe<'a> | Maybe<'a>) -> 'a -> i64",
        ),
        (
            "(Maybe<'a> | Maybe<'b>) -> 'a -> 'b -> i64",
            "(Maybe<'b> | Maybe<'a>) -> 'a -> 'a -> i64",
        ),
    ] {
        let changes = api_changes(&def(old), &def(new), false);
        assert_eq!(changes.len(), 1, "{old}\n{new}\n{changes:?}");
        assert!(
            changes[0].starts_with("breaking changed def Main.f"),
            "{changes:?}"
        );
    }
}

/// Verification of Copilot review 3 of #102: unions that share no names with the rest of the
/// declaration are searched one after the other, so their number no longer runs into the
/// search's limit; members with a union inside that hold all the occurrences of their names need
/// no search; and a class's methods are searched once per run of the class. By hand: thirteen
/// 3-rings `(('aK * 'bK * 'cK) | ('bK * 'cK * 'aK))` (`2^13` combinations) and the same rings of
/// regions print one text whichever member of the first ring is written first; 32 members
/// `[('vN | i64)]` print one text in either order; a class with four rings as superclass
/// constraints and a method with twelve rings of regions too. A ring whose second member swaps
/// two names instead of rotating them (`('bK * 'aK * 'cK)`) is no renaming of a ring, so it is a
/// change.
#[test]
fn api_diff_settles_independent_unions_one_by_one() {
    let ring = |k: usize, swapped: bool, region: bool, transposed: bool| {
        let name = |letter: &str| {
            if region {
                format!("ref {{{letter}{k}}} i64")
            } else {
                format!("'{letter}{k}")
            }
        };
        let (a, b, c) = (name("a"), name("b"), name("c"));
        let first = format!("({a} * {b} * {c})");
        let second = if transposed {
            format!("({b} * {a} * {c})")
        } else {
            format!("({b} * {c} * {a})")
        };
        if swapped {
            format!("({second} | {first})")
        } else {
            format!("({first} | {second})")
        }
    };
    let rings = |region: bool, swapped: bool, transposed: Option<usize>| {
        let parameters: Vec<String> = (0..13)
            .map(|k| ring(k, swapped && k == 0, region, transposed == Some(k)))
            .collect();
        format!(
            "def f :: {} -> unit = \\v -> todo ()\n",
            parameters.join(" -> ")
        )
    };
    let wide = |reversed: bool| {
        let mut members: Vec<String> = (0..32).map(|n| format!("[('v{n} | i64)]")).collect();
        if reversed {
            members.reverse();
        }
        format!(
            "def f :: ({}) -> unit = \\v -> todo ()\n",
            members.join(" | ")
        )
    };
    let class = |swapped: bool| {
        let constraints: Vec<String> = (0..4)
            .map(|k| format!("Show<{}>", ring(k, swapped && k == 0, false, false)))
            .collect();
        let rings: Vec<String> = (0..12)
            .map(|k| ring(k + 10, swapped && k == 0, true, false))
            .collect();
        format!(
            "class ({}) => C<'t> {{\n    def m :: 't -> {} -> unit\n}}\n",
            constraints.join(", "),
            rings.join(" -> ")
        )
    };
    for (old, new) in [
        (rings(false, false, None), rings(false, true, None)),
        (rings(true, false, None), rings(true, true, None)),
        (wide(false), wide(true)),
        (class(false), class(true)),
    ] {
        assert_eq!(
            api_changes(&old, &new, false),
            Vec::<String>::new(),
            "{old}\n{new}"
        );
    }
    for region in [false, true] {
        let (old, new) = (rings(region, false, None), rings(region, true, Some(12)));
        let changes = api_changes(&old, &new, false);
        assert_eq!(changes.len(), 1, "{old}\n{new}\n{changes:?}");
        assert!(
            changes[0].starts_with("breaking changed def Main.f"),
            "{changes:?}"
        );
    }
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

/// A project of `main` (the `Main.tz` of the namespace `Finder`) in a fresh directory under the
/// temporary directory, and `tsuzuri find --project ... --module Main ARGUMENTS` over it, which
/// must succeed; the caller removes the directory.
fn find_in_project(label: &str, main: &str) -> (std::path::PathBuf, impl Fn(&[&str]) -> String) {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-anonymous-unions-find-{label}-{}",
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
    std::fs::write(project.join("Main.tz"), main).unwrap();
    let directory = root.clone();
    let find = move |arguments: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .current_dir(&directory)
            .args(["find", "--project", "project", "--module", "Main"])
            .args(arguments)
            .env("TSUZURI_CACHE_DIR", directory.join("cache"))
            .env_remove("TSUZURI_KG")
            .env_remove("TSUZURI_KG_DIR")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    (root, find)
}

/// Copilot review 1: in a query, `*` binds more tightly than `->`, as in a signature: a member
/// `i64 -> i64 * i64` of an anonymous union returns a tuple, and `i64 * i64 -> i64` takes one.
#[test]
fn find_reads_products_tighter_than_arrows() {
    let (root, find) = find_in_project(
        "products",
        "def f :: (bool | i64 -> i64 * i64) -> unit = \\_u -> ()\n\
         def g :: (bool | (i64 -> i64) * i64) -> unit = \\_u -> ()\n\
         def h :: (i64 -> i64 * i64) -> unit = \\_u -> ()\n\
         def k :: (i64 * i64 -> i64 | string) -> unit = \\_u -> ()\n\
         def m :: i64 * i64 -> i64 = \\_p -> 0\n\
         def z :: (fn() -> i64 -> i64) -> unit = \\_u -> ()\n\
         0\n",
    );
    let exact = |text: &str, name: &str| text.contains(&format!("exact   Finder::Main.{name} ::"));
    for query in [
        "(bool | i64 -> i64 * i64) -> unit",
        "(i64 -> i64 * i64 | bool) -> unit",
        "(bool | i64 -> (i64 * i64)) -> unit",
    ] {
        let text = find(&[query]);
        assert!(exact(&text, "f"), "{query}\n{text}");
        assert!(!exact(&text, "g"), "{query}\n{text}");
    }
    for query in [
        "(bool | (i64 -> i64) * i64) -> unit",
        "((i64 -> i64) * i64 | bool) -> unit",
    ] {
        let text = find(&[query]);
        assert!(exact(&text, "g"), "{query}\n{text}");
        assert!(!exact(&text, "f"), "{query}\n{text}");
    }
    // The same precedence without a union, and for a function that takes a tuple.
    assert!(exact(&find(&["(i64 -> i64 * i64) -> unit"]), "h"));
    assert!(exact(&find(&["(string | i64 * i64 -> i64) -> unit"]), "k"));
    assert!(exact(&find(&["i64 * i64 -> i64"]), "m"));
    assert!(exact(
        &find(&["--takes", "i64 * i64", "--result", "i64"]),
        "m"
    ));
    // The result of `fn()` is a whole type, as in the signature.
    assert!(exact(&find(&["(fn() -> i64 -> i64) -> unit"]), "z"));
    std::fs::remove_dir_all(&root).unwrap();
}

/// Copilot review 1: a pairing of union members is not committed before the rest of the signature
/// matches. `'a` is `i64` by position, and the later `string` wants the other pairing; in every
/// tier and in `--takes`.
#[test]
fn find_reconsiders_member_pairings_for_later_parameters() {
    let (root, find) = find_in_project(
        "pairings",
        "def pick :: (Maybe<i64> | Maybe<string>) -> string -> i64 = \\_u _s -> 0\n\
         def peek :: ref (Maybe<i64> | Maybe<string>) -> ref string -> i64 = \\_u _s -> 0\n\
         def more :: (Maybe<i64> | Maybe<string>) -> string -> bool -> i64 = \\_u _s _b -> 0\n\
         0\n",
    );
    let text = find(&["(Maybe<'a> | Maybe<'b>) -> 'a -> i64"]);
    assert!(
        text.contains(
            "exact   Finder::Main.pick :: (Maybe<i64> | Maybe<string>) -> string -> i64\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "borrow  Finder::Main.peek :: ref (Maybe<i64> | Maybe<string>) -> ref string -> i64\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("partial Finder::Main.more :: (Maybe<i64> | Maybe<string>) -> string -> bool -> i64  (+1)\n"),
        "{text}"
    );
    // The pairing by position matches, as before.
    let text = find(&["(Maybe<'a> | Maybe<'b>) -> 'b -> i64"]);
    assert!(text.contains("exact   Finder::Main.pick ::"), "{text}");
    // No pairing gives `'a` both `string` (the parameter) and `i64` (the result).
    let text = find(&["(Maybe<'a> | Maybe<'b>) -> 'a -> 'a"]);
    assert!(!text.contains("Main.pick"), "{text}");
    let text = find(&[
        "--takes",
        "(Maybe<'a> | Maybe<'b>)",
        "--takes",
        "'a",
        "--result",
        "i64",
    ]);
    for line in [
        "exact   Finder::Main.pick ::",
        "borrow  Finder::Main.peek ::",
        "exact   Finder::Main.more ::",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    std::fs::remove_dir_all(&root).unwrap();
}

/// Copilot review 1, verification: the `partial` tier and `--takes` assign the parameters
/// greedily, as before, under each pairing of union members in turn. Under the pairing by position
/// of the result (`'a = i64`, `'b = string`), `'a` passes over `string` and takes `i64`, and the
/// query's `string` takes `string`. The fix of round 1 let `'a` take `string` (the other pairing
/// allows it) and found no parameter for the query's `string`.
#[test]
fn find_assigns_parameters_under_each_pairing() {
    let (root, find) = find_in_project(
        "assign",
        "def e :: string -> i64 -> (Maybe<i64> | Maybe<string>) = \\_s _n -> Some 0\n\
         def e3 :: string -> i64 -> bool -> (Maybe<i64> | Maybe<string>) = \\_s _n _b -> Some 0\n\
         def g :: string -> i64 -> unit = \\_s _n -> ()\n\
         0\n",
    );
    let text = find(&["'a -> string -> (Maybe<'a> | Maybe<'b>)"]);
    assert!(
        text.contains("partial Finder::Main.e :: string -> i64 -> (Maybe<i64> | Maybe<string>)\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "partial Finder::Main.e3 :: string -> i64 -> bool -> (Maybe<i64> | Maybe<string>)  (+1)\n"
        ),
        "{text}"
    );
    let text = find(&[
        "--takes",
        "'a",
        "--takes",
        "string",
        "--result",
        "(Maybe<'a> | Maybe<'b>)",
    ]);
    for line in [
        "exact   Finder::Main.e :: string -> i64 -> (Maybe<i64> | Maybe<string>)\n",
        "exact   Finder::Main.e3 :: string -> i64 -> bool -> (Maybe<i64> | Maybe<string>)\n",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    // Without a union nothing binds `'a` first: it takes `string`, and the query's `string` finds
    // no parameter left (the greedy assignment of `b3dd7f1c`, unchanged).
    assert_eq!(find(&["'a -> string -> unit"]), "no matches\n");
    std::fs::remove_dir_all(&root).unwrap();
}

/// Copilot review 1, verification: when the search across the comparison fails, a second pass
/// keeps the first pairing of every union, as the matcher did before, so that what it found is
/// still found. `n`: the nested union's variables occur in the result, so its 7! pairings stay
/// open in the first pass and use up the attempts. `m`: the first pass gives the first wanted
/// type the first parameter (`'x = string` by the other pairing), which the second needs; kept by
/// position (`'x = i64`), that parameter does not unify and the first wanted type takes the second
/// (`'x = bool`).
#[test]
fn find_keeps_the_matches_of_the_earlier_matcher() {
    let seven = "(i64 | string | bool | f64 | char | i8 | i32)";
    let n = format!(
        "(Result<{seven}, Maybe<i64>> | Result<{seven}, Result<{seven}, Maybe<i64>>>) -> {seven}"
    );
    let m =
        "((Maybe<i64> | Maybe<string>) * string) -> ((Maybe<bool> | Maybe<i64>) * bool) -> unit";
    let main = format!("def n :: {n} = \\_u -> true\ndef m :: {m} = \\_a _b -> ()\n0\n");
    let (root, find) = find_in_project("earlier", &main);
    let names = "('p | 'q | 'r | 's | 't | 'u | 'v)";
    let text = find(&[&format!("(Result<{names}, 'a> | 'a) -> {names}")]);
    assert!(
        text.contains(&format!("exact   Finder::Main.n :: {n}\n")),
        "{text}"
    );
    let text = find(&[
        "((Maybe<'x> | Maybe<'y>) * 'x) -> ((Maybe<i64> | Maybe<string>) * string) -> unit",
    ]);
    assert!(
        text.contains(&format!("partial Finder::Main.m :: {m}\n")),
        "{text}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// Copilot review 1, verification: a pairing whose variables nothing after it reads is kept once
/// found. The 7! pairings of the nested `(_ | ... | _)` bind only its own `_`s, so the failure of
/// `'a` (the second member of the outer union) goes straight back to the outer union, whose other
/// pairing (`'a = Result<(...), Maybe<i64>>`) matches. The fix of round 1 tried every pairing of
/// the nested union first and reached the attempt limit from 7 members on.
#[test]
fn find_keeps_pairings_that_nothing_else_reads() {
    let seven = "(i64 | string | bool | f64 | char | i8 | i32)";
    let six = "(i64 | string | bool | f64 | char | i8)";
    let signature = |inner: &str| {
        format!(
            "(Result<{inner}, Maybe<i64>> | Result<{inner}, Result<{inner}, Maybe<i64>>>) -> unit"
        )
    };
    let main = format!(
        "def w :: {} = \\_u -> ()\ndef w6 :: {} = \\_u -> ()\n0\n",
        signature(seven),
        signature(six)
    );
    let (root, find) = find_in_project("unseen", &main);
    let text = find(&["(Result<(_|_|_|_|_|_|_), 'a> | 'a) -> unit"]);
    assert!(
        text.contains(&format!("exact   Finder::Main.w :: {}\n", signature(seven))),
        "{text}"
    );
    let text = find(&["(Result<(_|_|_|_|_|_), 'a> | 'a) -> unit"]);
    assert!(
        text.contains(&format!("exact   Finder::Main.w6 :: {}\n", signature(six))),
        "{text}"
    );
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
fn widening_inside_a_lent_if_or_block_moves_array_members() {
    // An `if`, a block or a `match` lent to a read parameter whose branch widens an owned value:
    // the branch moves the payload into the borrowed temporary, as the widened value itself does
    // (review round 3). A value that the `if` holds already wide moves too. The window of `Alloc`
    // sees as many frees as allocations, on the branch that moves and on the one that does not.
    let measure = "def measure :: (i64 | [i64] | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? [i64] -> 100\n    | :? string as s -> s.length\n\n\
         def make :: i64 -> (i64 | [i64]) = \\n -> if n > 0 then [n, n] else n\n\n\
         def wide :: i64 -> (i64 | [i64] | string) = \\n -> if n > 0 then [n] else \"abc\"\n\n\
         def balanced :: (unit -> i64) -> i64 = \\f ->\n    let start = Alloc.snapshot ()\n    let value = f ()\n    let used = start |> Alloc.since\n    if used.frees == used.allocs then value else -1\n\n";
    let functions = [
        (
            "branch",
            "    let v = make n\n    measure (if n > 1 then v else 7)\n",
        ),
        (
            "nested",
            "    let v = make n\n    measure (if n > 1 then (if n > 5 then v else 8) else 7)\n",
        ),
        (
            "block",
            "    let v = make n\n    measure (if n > 1 then\n                 let k = n + 1\n                 if k > 3 then v else k\n             else 7)\n",
        ),
        (
            "matched",
            "    let v = make n\n    measure (if n > 1 then (match n with | 2 -> 9 | _ -> v) else 7)\n",
        ),
        (
            "held",
            "    let u = wide n\n    measure (if n > 1 then u else \"de\")\n",
        ),
    ];
    let mut source = measure.to_owned();
    for (name, body) in functions {
        source.push_str(&format!(
            "@no_copy\ndef {name} :: i64 -> i64 = \\n ->\n{body}\n"
        ));
    }
    let module = accepts_in(&source, Edition::E2027);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    for (name, _) in functions {
        let body = function_body(&ir, name);
        assert!(!body.contains("@tz.alloc"), "{name}:\n{body}");
    }
    // Expected values by hand: make 3 is [3, 3] (100), make 1 is [1, 1] and the `if` takes 7;
    // nested: 6 takes the array, 3 takes 8, 0 takes 7; block: 4 takes the array (k = 5), 2 takes k = 3,
    // 1 takes 7; matched: 2 takes 9, 4 takes the array, 0 takes 7; held: wide 3 is [3] (100), wide
    // 1 is [1] and the `if` takes "de" (2), wide -1 is "abc" and the `if` takes "de".
    source.push_str(
        "test \"values\" =\n    assert (balanced (\\() -> branch 3) == 100 && balanced (\\() -> branch 1) == 7)\n    assert (balanced (\\() -> nested 6) == 100 && balanced (\\() -> nested 3) == 8 && balanced (\\() -> nested 0) == 7)\n    assert (balanced (\\() -> block 4) == 100 && balanced (\\() -> block 2) == 3 && balanced (\\() -> block 1) == 7)\n    assert (balanced (\\() -> matched 2) == 9 && balanced (\\() -> matched 4) == 100 && balanced (\\() -> matched 0) == 7)\n    assert (balanced (\\() -> held 3) == 100 && balanced (\\() -> held 1) == 2 && balanced (\\() -> held (-1)) == 2)\n",
    );
    passes(&source, Edition::E2027);
    // A value used again after the `if` is copied for it and moved by the last use: 100 + 100
    // for make 3, 7 + 100 for make 1.
    let source = format!(
        "{measure}def again :: i64 -> i64 = \\n ->\n    let v = make n\n    measure (if n > 1 then v else 7) + measure v\n\n\
         test \"again\" =\n    assert (balanced (\\() -> again 3) == 200 && balanced (\\() -> again 1) == 107)\n"
    );
    let module = accepts_in(&source, Edition::E2027);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(
        function_body(&ir, "again").matches("@tz.alloc").count(),
        1,
        "{ir}"
    );
    passes(&source, Edition::E2027);
    // A closure that lends its capture to the `if` on each call copies the payload out of it
    // (round 2: the capture is borrowed), and a loop copies it on each pass but the last.
    // Expected values by hand: make 3 is [3, 3] (100): 101 + 102 + 7, and 7 + 100 + 100; make -2
    // is the i64 -2: -1 + 0 + 7, and 7 - 2 - 2.
    let source = format!(
        "{measure}def captured :: i64 -> i64 = \\n ->\n    let v = make n\n    let c = \\(k: i64) -> measure (if k > 0 then v else 7) + k\n    c 1 + c 2 + c 0\n\n\
         def looped :: i64 -> i64 = \\n ->\n    let v = make n\n    let mut sum = 0\n    for i in 0 ..< 3 do\n        sum = sum + measure (if i > 0 then v else 7)\n    sum\n\n\
         test \"captured\" =\n    assert (balanced (\\() -> captured 3) == 210 && balanced (\\() -> captured (-2)) == 6)\n    assert (balanced (\\() -> looped 3) == 207 && balanced (\\() -> looped (-2)) == 7 - 4)\n"
    );
    passes(&source, Edition::E2027);
}

/// The definitions of the instances `name.$mono.N` of the generic function `name` of module
/// `Main` in `ir`, in the order of the IR.
fn instance_bodies<'ir>(ir: &'ir str, name: &str) -> Vec<&'ir str> {
    let head = format!("tz.fn.Main.{name}.$mono.");
    ir.match_indices("\ndefine ")
        .map(|(at, _)| at + 1)
        .filter(|at| {
            let line = ir[*at..].lines().next().unwrap();
            line.find(&head).is_some_and(|start| {
                let rest = &line[start + head.len()..];
                let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
                rest.starts_with('(') || rest.starts_with("\"(")
            })
        })
        .map(|start| &ir[start..start + ir[start..].find("\n}\n").expect("the definition ends")])
        .collect()
}

#[test]
fn widening_beside_an_open_member_moves_array_members() {
    // `(Maybe<'a> | [i64])` widened into `(bool | Maybe<'a> | [i64])`: the retagging binds the
    // payload of each member, and `LastUses` keeps one decision for each binding span. The binding
    // of the open member `Maybe<'a>` has the span of the widened value, as the binding of the array
    // has, since an instance may make it an array. An instance that made it a type that no binding
    // moves (`Maybe<i64>`, Copy without a drop, or `Maybe<string>`, not Copy) took the move from
    // the binding of the array, so the only use of an owned value copied the array, and `@no_copy`
    // rejected it (Copilot review round 2). Every instance moves the array into the wider union.
    let weigh = "def weigh :: own (bool | Maybe<'a> | [i64]) -> i64 = \\v ->\n    match v with\n    | :? bool -> 1\n    | :? Maybe<'a> as m -> (match m with | Some _ -> 2 | None -> 3)\n    | :? [i64] as xs -> xs.length * 100\n\n";
    // `allocations f` is the number of allocations that `f` makes, -1 if it does not free each.
    let makers = "def ints :: i64 -> (Maybe<i64> | [i64]) = \\n -> if n > 0 then Array.init n (\\i -> i) else if n == 0 then None else Some n\n\n\
         def texts :: i64 -> (Maybe<string> | [i64]) = \\n -> if n > 0 then Array.init n (\\i -> i) else if n == 0 then None else Some $\"t{n}\"\n\n\
         def arrays :: i64 -> (Maybe<[i64]> | [i64]) = \\n -> if n > 0 then Array.init n (\\i -> i) else if n == 0 then None else Some [n]\n\n\
         def balanced :: (unit -> i64) -> i64 = \\f ->\n    let start = Alloc.snapshot ()\n    let value = f ()\n    let used = start |> Alloc.since\n    if used.frees == used.allocs then value else -1\n\n\
         def allocations :: (unit -> i64) -> i64 = \\f ->\n    let start = Alloc.snapshot ()\n    let _value = f ()\n    let used = start |> Alloc.since\n    if used.frees == used.allocs then used.allocs else -1\n\n";
    let functions = "@no_copy\ndef widen :: own (Maybe<'a> | [i64]) -> (bool | Maybe<'a> | [i64]) = \\v -> v\n\n\
         @no_copy\ndef argument :: own (Maybe<'a> | [i64]) -> i64 = \\v -> weigh v\n\n\
         @no_copy\ndef bound :: own (Maybe<'a> | [i64]) -> i64 = \\v ->\n    let w: (bool | Maybe<'a> | [i64]) = v\n    weigh w\n\n\
         def run :: i64 -> i64 = \\n ->\n    weigh (widen (ints n)) + weigh (widen (texts n)) + weigh (widen (arrays n)) + argument (ints n) + argument (texts n) + argument (arrays n) + bound (ints n) + bound (texts n) + bound (arrays n)\n\n";
    let source = [weigh, makers, functions].concat();
    let module = accepts_in(&source, Edition::E2026);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    for name in ["widen", "argument", "bound"] {
        let bodies = instance_bodies(&ir, name);
        assert_eq!(bodies.len(), 3, "{name}:\n{ir}");
        for body in bodies {
            assert!(!body.contains("@tz.alloc"), "{name}:\n{body}");
        }
    }
    // Expected values by hand: 100 for each element of an array, 2 for `Some`, 3 for `None`. The
    // only allocation is the array of `Array.init`.
    let tests = "test \"values\" =\n    assert (balanced (\\() -> weigh (widen (ints 3))) == 300 && balanced (\\() -> weigh (widen (ints 0))) == 3 && balanced (\\() -> weigh (widen (ints (-2)))) == 2)\n    assert (balanced (\\() -> weigh (widen (texts 2))) == 200 && balanced (\\() -> weigh (widen (texts 0))) == 3 && balanced (\\() -> weigh (widen (texts (-2)))) == 2)\n    assert (balanced (\\() -> weigh (widen (arrays 1))) == 100 && balanced (\\() -> weigh (widen (arrays 0))) == 3 && balanced (\\() -> weigh (widen (arrays (-2)))) == 2)\n    assert (balanced (\\() -> argument (ints 4)) == 400 && balanced (\\() -> argument (texts 5)) == 500 && balanced (\\() -> argument (arrays 6)) == 600)\n    assert (balanced (\\() -> argument (ints (-1))) == 2 && balanced (\\() -> argument (texts (-1))) == 2 && balanced (\\() -> argument (arrays 0)) == 3)\n    assert (balanced (\\() -> bound (ints 2)) == 200 && balanced (\\() -> bound (texts 3)) == 300 && balanced (\\() -> bound (arrays 4)) == 400)\n    assert (balanced (\\() -> bound (ints 0)) == 3 && balanced (\\() -> bound (texts (-3))) == 2 && balanced (\\() -> bound (arrays (-3))) == 2)\n\n\
         test \"allocations\" =\n    assert (allocations (\\() -> weigh (widen (ints 3))) == 1 && allocations (\\() -> weigh (widen (texts 3))) == 1 && allocations (\\() -> weigh (widen (arrays 3))) == 1)\n    assert (allocations (\\() -> argument (ints 3)) == 1 && allocations (\\() -> argument (texts 3)) == 1 && allocations (\\() -> argument (arrays 3)) == 1)\n    assert (allocations (\\() -> bound (ints 3)) == 1 && allocations (\\() -> bound (texts 3)) == 1 && allocations (\\() -> bound (arrays 3)) == 1)\n";
    passes(&[source.as_str(), tests].concat(), Edition::E2026);
    // A value used again is copied for the first widening and moved by the last, in each Copy
    // instance: the array of `Array.init` and one copy (`(Maybe<string> | [i64])` is not Copy, so
    // it cannot be used twice). By hand: 300 + 300 for ints 3, 2 + 2 for ints (-1), 200 + 200 for
    // arrays 2.
    let twice = "def twice :: own (Maybe<'a> | [i64]) -> i64 = \\v -> weigh v + weigh v\n\n\
         def again :: i64 -> i64 = \\n -> twice (ints n) + twice (arrays n)\n\n\
         test \"twice\" =\n    assert (balanced (\\() -> twice (ints 3)) == 600 && balanced (\\() -> twice (ints (-1))) == 4 && balanced (\\() -> twice (arrays 2)) == 400)\n    assert (allocations (\\() -> twice (ints 3)) == 2 && allocations (\\() -> twice (arrays 3)) == 2)\n";
    passes(&[weigh, makers, twice].concat(), Edition::E2026);
    // Edition 2027 lends the widened value to a read parameter (review round 1), which binds no
    // array, so that `@no_copy` sees only the caller. By hand: 100 for an array, 2 for `Some`.
    let read = "def measure :: (bool | Maybe<'a> | [i64]) -> i64 = \\v ->\n    match v with\n    | :? bool -> 1\n    | :? Maybe<'a> -> 2\n    | :? [i64] -> 100\n\n\
         @no_copy\ndef lent :: own (Maybe<'a> | [i64]) -> i64 = \\v -> measure v\n\n\
         def run :: i64 -> i64 = \\n -> lent (ints n) + lent (texts n) + lent (arrays n)\n\n\
         test \"read\" =\n    assert (balanced (\\() -> lent (ints 3)) == 100 && balanced (\\() -> lent (texts 2)) == 100 && balanced (\\() -> lent (arrays 1)) == 100)\n    assert (balanced (\\() -> lent (ints (-1))) == 2 && balanced (\\() -> lent (texts (-1))) == 2 && balanced (\\() -> lent (arrays (-1))) == 2)\n    assert (allocations (\\() -> lent (ints 3)) == 1 && allocations (\\() -> lent (texts 3)) == 1 && allocations (\\() -> lent (arrays 3)) == 1)\n";
    let source = [makers, read].concat();
    let module = accepts_in(&source, Edition::E2027);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let bodies = instance_bodies(&ir, "lent");
    assert_eq!(bodies.len(), 3, "{ir}");
    for body in bodies {
        assert!(!body.contains("@tz.alloc"), "{body}");
    }
    passes(&source, Edition::E2027);
}

/// A closure whose body widens or narrows a captured value, called once or twice, and the
/// continuation of a computation expression that widens a value it captures (review round 2).
/// The call of a closure that does not consume its captures lends them to a worker; the binding
/// of the conversion copies an array payload out of them instead of moving it, so the closure's
/// environment frees each array once. The window of `Alloc` sees as many frees as allocations.
/// `explicit` and `rewrapped` are the matches that A24 already accepted, which took the same
/// payload out of the environment.
const CAPTURED: &str = "def mk :: i64 -> ([i64] | i64) = \\n -> if n % 2 == 0 then n else Array.init n (\\i -> i + 1)\n\n\
def mk3 :: i64 -> ([i64] | i64 | f64) = \\n -> if n % 2 == 0 then n else Array.init n (\\i -> i + 1)\n\n\
def weigh :: own (f64 | [i64] | i64) -> i64 = \\v ->\n    match v with\n    | :? f64 -> 1\n    | :? i64 as k -> k\n    | :? [i64] as xs -> xs.length * 100\n\n\
def weigh_read :: (f64 | [i64] | i64) -> i64 = \\v ->\n    match v with\n    | :? f64 -> 1\n    | :? i64 as k -> k\n    | :? [i64] as xs -> xs.length * 100\n\n\
def size :: own ([i64] | i64) -> i64 = \\v ->\n    match v with\n    | :? i64 as k -> k\n    | :? [i64] as xs -> xs.length * 100\n\n\
def len_own :: own [i64] -> i64 = \\xs -> xs.length * 100\n\n\
def once :: i64 -> i64 = \\n ->\n    let w = mk n\n    let c = \\(k: i64) -> weigh w + k\n    c 1\n\n\
def twice :: i64 -> i64 = \\n ->\n    let w = mk n\n    let c = \\(k: i64) -> weigh w + k\n    c 1 + c 2\n\n\
def read :: i64 -> i64 = \\n ->\n    let w = mk n\n    let c = \\(k: i64) -> weigh_read w + k\n    c 1 + c 2\n\n\
def bound :: i64 -> i64 = \\n ->\n    let w = mk n\n    let c = \\(k: i64) ->\n        let u: (f64 | [i64] | i64) = w\n        weigh u + k\n    c 1 + c 2\n\n\
def narrowed :: i64 -> i64 = \\n ->\n    let w = mk3 n\n    let c = \\(k: i64) ->\n        match w with\n        | :? ([i64] | i64) as x -> size x + k\n        | :? f64 -> 0\n    c 1 + c 2\n\n\
def pick :: i64 -> Maybe<(f64 | i64 | [i64])> = \\n ->\n    let w = mk n\n    let! _a = Maybe.Some 1\n    return w\n\n\
def continued :: i64 -> i64 = \\n ->\n    match pick n with\n    | Some v -> weigh v\n    | None -> 0\n\n\
def explicit :: i64 -> i64 = \\n ->\n    let w = Array.init n (\\i -> i)\n    let c = \\(k: i64) ->\n        match w with\n        | ys -> len_own ys + k\n    c 1 + c 2\n\n\
def pick_rewrapped :: i64 -> Maybe<([i64] | i64)> = \\n ->\n    let w = mk n\n    let! _a = Maybe.Some 1\n    return (match w with | :? i64 as k -> k | :? [i64] as xs -> xs)\n\n\
def rewrapped :: i64 -> i64 = \\n ->\n    match pick_rewrapped n with\n    | Some v -> size v\n    | None -> 0\n\n\
def balanced :: (unit -> i64) -> i64 = \\f ->\n    let start = Alloc.snapshot ()\n    let value = f ()\n    let used = start |> Alloc.since\n    if used.frees == used.allocs then value else -1\n\n";

#[test]
fn widening_a_borrowed_capture_copies_its_payload() {
    // Expected values by hand: mk 3 is [1, 2, 3] (300), mk 4 is the i64 4. once: 300 + 1, 4 + 1;
    // twice, read, bound and narrowed: (300 + 1) + (300 + 2), (4 + 1) + (4 + 2); continued and
    // rewrapped: 300, 4; explicit: [0, 1, 2] is 300, so 301 + 302.
    let tests = "test \"captured\" =\n    assert (balanced (\\() -> once 3) == 301 && balanced (\\() -> once 4) == 5)\n    assert (balanced (\\() -> twice 3) == 603 && balanced (\\() -> twice 4) == 11)\n    assert (balanced (\\() -> read 3) == 603 && balanced (\\() -> read 4) == 11)\n    assert (balanced (\\() -> bound 3) == 603 && balanced (\\() -> bound 4) == 11)\n    assert (balanced (\\() -> narrowed 3) == 603 && balanced (\\() -> narrowed 4) == 11)\n    assert (balanced (\\() -> continued 3) == 300 && balanced (\\() -> continued 4) == 4)\n    assert (balanced (\\() -> continued 5) == 500 && balanced (\\() -> continued 3) == 300)\n    assert (balanced (\\() -> explicit 3) == 603)\n    assert (balanced (\\() -> rewrapped 3) == 300 && balanced (\\() -> rewrapped 4) == 4)\n";
    for edition in [Edition::E2026, Edition::E2027] {
        passes(&format!("{CAPTURED}{tests}"), edition);
    }
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

/// The functions and values of `operand_results_do_not_widen`: `flag 1` is true and `flag 0`
/// false; `size` reads a member back (`size 2 == 2`, `size "neg" == 3`); `wide` takes and
/// returns the wider union; `make n` is `n` for `n > 0` and `"neg"` otherwise; `ident` is generic.
const OPERANDS: &str = "def flag :: i64 -> bool = \\n -> n > 0\n\
     def size :: own (f64 | i64 | string) -> i64 = \\x ->\n    match x with\n    | :? i64 as n -> n\n    | :? f64 -> 100\n    | :? string as s -> s.length\n\n\
     def wide :: own (f64 | i64 | string) -> (f64 | i64 | string) = \\x -> x\n\
     def make :: i64 -> (i64 | string) = \\n -> if n > 0 then n else \"neg\"\n\
     def ident :: 'a -> 'a = \\x -> x\n\n";

/// A builder whose computations are their values (`Identity { return w }` is `w`).
const IDENTITY: &str = "def Return :: own 'a -> 'a\nfn Return value = value\n\ndef Bind :: own 'a -> own ('a -> 'b) -> 'b\nfn Bind value next = next value\n";

/// `OPERANDS`, a channel `a`, the values `v` and `x: (f64 | i64 | string)`, `w` and
/// `u: (i64 | string)`, and `let same = {operand}` with the markers `«` and `»` taken out, and the
/// offset of `«`.
fn operand_source(operand: &str) -> (String, usize) {
    let source = format!(
        "{OPERANDS}let a: Channel<i64> = Channel.make 1\nlet v: (f64 | i64 | string) = 1\nlet w: (i64 | string) = 2\nlet u: (i64 | string) = 3\nlet x: (f64 | i64 | string) = 4\nlet same = {operand}\n0\n"
    );
    let at = source.find('«').unwrap_or(0);
    (source.replace(['«', '»'], ""), at)
}

/// The errors of `sources`, which compare `v` with an operand whose value is the smaller union
/// `(i64 | string)`: `E1003` at the offset `at` (when given), and only `E1003` of that operand or
/// the `E1005` of the comparison of anonymous unions, which have no `Eq` and `Ord` until A26 (a
/// `match` keeps the union as its type when an arm's error is recovered).
fn operand_mismatch(sources: &[(&str, &str)], at: Option<usize>, label: &str) {
    let errors = analyze_modules_all(sources).expect_err(label).diagnostics;
    let mut mismatched = Vec::new();
    for error in errors.iter().filter(|error| error.code.starts_with('E')) {
        if error.code == "E1003" {
            assert_eq!(error.kind, Some("type_mismatch"), "{label}");
            assert_eq!(
                error.message, "expected (i64 | f64 | string), found (i64 | string)",
                "{label}"
            );
            mismatched.push(error.span.start);
        } else {
            assert_eq!(error.code, "E1005", "{label}: {}", error.message);
            assert!(
                ["Eq", "Ord"].iter().any(|class| error
                    .message
                    .starts_with(&format!("no instance for {class}<(i64 | f64 | string)>"))),
                "{label}: {}",
                error.message
            );
        }
    }
    assert!(!mismatched.is_empty(), "{label}: {errors:?}");
    if let Some(at) = at {
        assert!(
            mismatched.contains(&at),
            "{label}: {mismatched:?}, not {at}"
        );
    }
}

#[test]
fn operand_results_do_not_widen() {
    // Copilot review round 3 of A25: the value of an operand may come from the branches of an
    // `if`, the arms of a `match` or `select`, the result of a block, `@checked` or a computation
    // expression, which pass the sibling's type down. Each of them is the operand's value and is
    // not widened either (`E1003` at the branch that is the smaller union), with every comparison
    // operator. Before the fix, each branch was widened and the only error was the missing
    // `Eq`/`Ord` instance (`E1005`).
    let forms = [
        "(if flag 1 then «w» else u)",
        "(if flag 1 then v else «w»)",
        "(match 1 with | 0 -> «w» | _ -> u)",
        "{ let k = 1; «w» }",
        "(@checked «w»)",
        // A parenthesized expression spans its parentheses.
        "(if flag 1 then { let k = 1; «(w)» } else (match 1 with | 0 -> u | _ -> w))",
        "((if flag 0 then v else «((u))»))",
        "(select | recv _ from a -> «w» | default -> u)",
        // A call is a value of its own: its result is not widened in an operand.
        "(if flag 1 then «make 1» else w)",
    ];
    for operator in ["==", "!=", "<", "<=", ">", ">="] {
        for form in forms {
            let (source, at) = operand_source(&format!("v {operator} {form}"));
            operand_mismatch(
                &[("Main", &source)],
                Some(at),
                &format!("{operator} {form}"),
            );
        }
    }
    // A computation expression whose builder returns the value as it is.
    for body in ["Identity { return w }", "Identity { let! x = w; return x }"] {
        let (source, _) = operand_source(&format!("v == {body}"));
        operand_mismatch(
            &[("Main.tz", &source), ("Identity.tc", IDENTITY)],
            None,
            body,
        );
    }
    // A pipe and a call in an operand are checked by themselves (as before the fix).
    for form in ["«(w |> ident)", "«ident w", "«(w |> \\x -> x)"] {
        let (source, at) = operand_source(&format!("v == {form}"));
        let error = rejects(&source, "E1003", "type_mismatch");
        assert_eq!(
            error.message, "expected (i64 | f64 | string), found (i64 | string)",
            "{form}"
        );
        assert_eq!(error.span.start, at, "{form}");
    }
    // The left operand is checked first and gives its type to the right one, which is not
    // narrowed (as before the fix).
    let (source, at) = operand_source("(if flag 1 then w else u) == «v»");
    let error = rejects(&source, "E1003", "type_mismatch");
    assert_eq!(
        error.message,
        "expected (i64 | string), found (i64 | f64 | string)"
    );
    assert_eq!(error.span.start, at);
    // A call or an annotated binding inside an operand still widens: these operands are the
    // wider union, so the check reaches `Eq`, which anonymous unions do not have until A26.
    for form in [
        "(if flag 1 then wide w else v)",
        "(match 1 with | 0 -> wide w | _ -> v)",
        "{ let y: (f64 | i64 | string) = w; y }",
        "(w |> wide)",
    ] {
        let (source, _) = operand_source(&format!("v == {form}"));
        let error = rejects(&source, "E1005", "no_instance");
        assert!(
            error
                .message
                .starts_with("no instance for Eq<(i64 | f64 | string)>"),
            "{form}: {}",
            error.message
        );
    }
    // The same conversions in operands of type `i64` run; the values follow from `size` and
    // `make` by hand (`make 2` is 2, `make 0` is "neg" of length 3).
    passes(
        &format!(
            "{OPERANDS}test \"conversions inside operands\" =\n    \
             assert (size (if flag 1 then make 2 else make 0) == 2)\n    \
             assert (2 == size (if flag 0 then make 2 else make 0) - 1)\n    \
             assert ({{ let y: (f64 | i64 | string) = make 5; size y }} == 5)\n    \
             assert ((if flag 1 then size (make 0) else 0) == 3)\n    \
             assert (size (match 2 with | 0 -> make 7 | _ -> make 0) + 1 == 4)\n    \
             assert (6 > 1 + size ({{ let k = make 4; k }}))\n"
        ),
        Edition::E2026,
    );
}

#[test]
fn operand_checked_first_widens_by_itself() {
    // Verification of the Copilot review round 3 of A25: only the operand that takes its
    // sibling's type is not widened. The operand checked first (the left one, or the right one
    // when the left one is an unsuffixed number) has no sibling's type: it is checked by itself,
    // so an `if`, `match` or `select` there gives the type of its first branch, `v`'s
    // `(f64 | i64 | string)`, to the next one (D4), and `w` is widened as in
    // `let t = if c then v else w`. Both operands are then `(f64 | i64 | string)` (the number is
    // injected into its member), and the only error is the comparison's: anonymous unions have
    // no `Eq` and `Ord` until A26. When the operand checked first was recorded too, `w` was
    // `E1003` in each of these.
    let first = [
        "(if flag 1 then v else w)",
        "(match 1 with | 0 -> v | _ -> w)",
        "{ let k = 1; if flag k then v else w }",
        "(@checked (if flag 1 then v else w))",
        "(select | recv _ from a -> v | default -> w)",
        // The inner `if` takes the outer `then` branch's type: both of its branches are widened.
        "(if flag 1 then v else (if flag 0 then u else w))",
    ];
    for operator in ["==", "!=", "<", "<=", ">", ">="] {
        let class = if matches!(operator, "==" | "!=") {
            "Eq"
        } else {
            "Ord"
        };
        for form in first {
            for operands in [
                format!("{form} {operator} x"),
                format!("1 {operator} {form}"),
                format!("-1 {operator} {form}"),
                format!("1.5 {operator} {form}"),
                // The parentheses keep a block operand from being the lambda's body.
                format!("{{ let f = \\b -> ({form} {operator} b); f x }}"),
            ] {
                let (source, _) = operand_source(&operands);
                let errors = analyze_modules_all(&[("Main", &source)])
                    .expect_err(&operands)
                    .diagnostics;
                let errors: Vec<_> = errors
                    .iter()
                    .filter(|error| error.code.starts_with('E'))
                    .collect();
                assert_eq!(errors.len(), 1, "{operands}: {errors:?}");
                assert_eq!(errors[0].code, "E1005", "{operands}");
                assert!(
                    errors[0]
                        .message
                        .starts_with(&format!("no instance for {class}<(i64 | f64 | string)>")),
                    "{operands}: {}",
                    errors[0].message
                );
            }
        }
    }
    // The operand checked second takes its sibling's type, which the `if` passes down, and the
    // `else` branch takes the `then` branch's: `E1003` at `w`, whether the sibling's type is
    // written, inferred later (`b`), or already the wider union.
    for operands in [
        "x == (if flag 1 then v else «w»)",
        "{ let f = \\(b: (f64 | i64 | string)) -> b == (if flag 1 then v else «w»); f x }",
        "{ let f = \\b -> b == (if flag 1 then v else «w»); f x }",
        "{ let f = \\b -> b < (match 1 with | 0 -> v | _ -> «w»); f x }",
    ] {
        let (source, at) = operand_source(operands);
        operand_mismatch(&[("Main", &source)], Some(at), operands);
    }
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
fn a_parenthesized_type_test_pattern_spans_its_parentheses() {
    // The parentheses around a type test are part of the pattern, as around any other pattern:
    // an arm's pattern and an `as` or `|` around the test start at the `(` and end at the `)`
    // or after, as in A24 (review round 3). The test's own diagnostics stay at its `:?`
    // (`type_test_errors_point_at_the_marker_inside_parentheses`).
    let source = "def f :: own (i64 | f64 | string) -> i64 = \\v ->\n    match v with\n    | _ -> 0\n    | (:? i64 as k) as whole -> k\n    | (:? i64 as k) -> k\n    | ( :? f64 ) | (:? string) -> 1\n";
    let module = analyze(source).unwrap();
    let warnings: Vec<(&str, &str)> = module
        .warnings
        .iter()
        .filter(|warning| warning.code == "W1003")
        .map(|warning| (warning.code, &source[warning.span.start..warning.span.end]))
        .collect();
    assert_eq!(
        warnings,
        [
            ("W1003", "(:? i64 as k) as whole"),
            ("W1003", "(:? i64 as k)"),
            ("W1003", "( :? f64 ) | (:? string)"),
        ]
    );
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
fn a_second_widening_says_that_the_value_moved() {
    // A widening moves the payload, all that the value holds besides its tag: the note says that
    // the value moved and the help binds the wider value once, since `ref w` is never widened
    // (review round 2). Into an own parameter, an annotated binding and a read parameter.
    let make = "def mk :: i64 -> (i64 | string) = \\n -> if n > 0 then n else \"x\"\n\n\
         def take :: own (f64 | i64 | string) -> i64 = \\v -> 0\n\n\
         def look :: (f64 | i64 | string) -> i64 = \\v -> 0\n\n";
    for (body, marker) in [
        (
            "    let a = take w\n    let b = take w\n    a + b\n",
            "take w",
        ),
        (
            "    let a: (f64 | i64 | string) = w\n    let b: (f64 | i64 | string) = w\n    take a + take b\n",
            "= w",
        ),
        (
            "    let a = look w\n    let b = look w\n    a + b\n",
            "look w",
        ),
    ] {
        let source = format!("{make}def f :: i64 -> i64 = \\n ->\n    let w = mk n\n{body}");
        let error = rejects_in(&source, Edition::E2027, "E1012", "use_after_move");
        assert_eq!(
            error.message, "use of moved or partially moved value 'w'",
            "{body}"
        );
        assert_eq!(
            error.help.as_deref(),
            Some(
                "a widening moves 'w'; bind the wider value once with 'let wide: (i64 | f64 | string) = w' and borrow 'wide', or move it last"
            ),
            "{body}"
        );
        let first = source.find(marker).unwrap() + marker.len() - 1;
        let second = source.rfind(marker).unwrap() + marker.len() - 1;
        assert_eq!(error.span.start, second, "{body}");
        let related: Vec<_> = error
            .related
            .iter()
            .map(|related| (related.message.as_str(), related.span.start))
            .collect();
        assert_eq!(
            related,
            [("value moved here into a wider anonymous union", first)],
            "{body}"
        );
    }
    // The help that the bound wider value follows: it is borrowed twice.
    accepts_in(
        &format!(
            "{make}def f :: i64 -> i64 = \\n ->\n    let w = mk n\n    let wide: (f64 | i64 | string) = w\n    look wide + look wide\n"
        ),
        Edition::E2027,
    );
    // A move out of a case of a union that is not anonymous keeps its message.
    let error = rejects_in(
        "union V = A of string | B of i64\n\ndef eat :: own string -> i64 = \\s -> 0\n\n\
         def f :: own V -> i64 = \\v ->\n    let a = match v with\n        | A s -> eat s\n        | B _ -> 0\n    let b = match v with\n        | A s -> eat s\n        | B _ -> 0\n    a + b\n",
        Edition::E2027,
        "E1012",
        "use_after_move",
    );
    assert_eq!(
        error.help.as_deref(),
        Some("borrow the field with 'ref' instead of moving it out of 'v', or move it last")
    );
    assert_eq!(error.related[0].message, "part of the value moved here");
}

#[test]
fn a_widened_field_is_named_in_the_help() {
    // A widening of a record field moves the field's payload: the note says that part of the
    // value moved and the help binds the field, as the source spells it, not the record, whose
    // type is not the union (review round 3). The field used again, the record used whole, a
    // field of a field, a loop, and a read parameter.
    let make = "record R { u: (i64 | string), k: i64 }\n\nrecord S { r: R }\n\n\
         def take :: own (f64 | i64 | string) -> i64 = \\v ->\n    match v with\n    | :? i64 as k -> k\n    | _ -> 0\n\n\
         def look :: (f64 | i64 | string) -> i64 = \\v -> 0\n\n";
    let wider = "(i64 | f64 | string)";
    for (function, widened, used, path, root, looped) in [
        (
            "def f :: own R -> i64 = \\r -> take r.u + take r.u\n",
            "take r.u",
            "r.u\n",
            "r.u",
            "r",
            false,
        ),
        (
            "def f :: own R -> i64 = \\r ->\n    let a = take r.u\n    let b = r\n    a + b.k\n",
            "take r.u",
            "= r\n",
            "r.u",
            "r",
            false,
        ),
        (
            "def f :: own S -> i64 = \\s -> take s.r.u + take s.r.u\n",
            "take s.r.u",
            "s.r.u\n",
            "s.r.u",
            "s",
            false,
        ),
        (
            "def f :: own R -> i64 = \\r ->\n    let mut sum = 0\n    for i in 0 ..= 1 do\n        sum = sum + take r.u\n    sum\n",
            "take r.u",
            "take r.u",
            "r.u",
            "r",
            true,
        ),
        (
            "def f :: own R -> i64 = \\r -> look r.u + look r.u\n",
            "look r.u",
            "r.u\n",
            "r.u",
            "r",
            false,
        ),
    ] {
        let source = format!("{make}{function}");
        let error = rejects_in(&source, Edition::E2027, "E1012", "use_after_move");
        assert_eq!(
            error.message,
            format!("use of moved or partially moved value '{root}'"),
            "{function}"
        );
        assert_eq!(
            error.help,
            Some(format!(
                "a widening moves '{path}' out of '{root}'; bind the wider value once with 'let wide: {wider} = {path}' and borrow 'wide', or move it last"
            )),
            "{function}"
        );
        let first = source.find(widened).unwrap() + widened.len() - path.len();
        let at = if looped {
            first
        } else {
            source.rfind(used).unwrap() + used.find(root).unwrap()
        };
        assert_eq!(error.span.start, at, "{function}");
        let related: Vec<_> = error
            .related
            .iter()
            .map(|related| {
                (
                    related.message.as_str(),
                    related.span.start,
                    related.span.end,
                )
            })
            .collect();
        let message = if looped {
            "part of the value moved here into a wider anonymous union, in the previous iteration of the loop"
        } else {
            "part of the value moved here into a wider anonymous union"
        };
        assert_eq!(
            related,
            [(message, first, first + path.len())],
            "{function}"
        );
    }
    // A widened local in a loop: the value moved in the previous iteration (the nodes of a
    // widening share its span, which the flow alone did not tell from a use in the same pass).
    let source = format!(
        "{make}def g :: own (i64 | string) -> i64 = \\w ->\n    let mut sum = 0\n    for i in 0 ..= 1 do\n        sum = sum + take w\n    sum\n"
    );
    let error = rejects_in(&source, Edition::E2027, "E1012", "use_after_move");
    let at = source.find("take w").unwrap() + "take ".len();
    assert_eq!((error.span.start, error.span.end), (at, at + 1));
    assert_eq!(
        error.help.as_deref(),
        Some(
            "a widening moves 'w'; bind the wider value once with 'let wide: (i64 | f64 | string) = w' and borrow 'wide', or move it last"
        )
    );
    assert_eq!(
        error.related[0].message,
        "value moved here into a wider anonymous union, in the previous iteration of the loop"
    );
    // The help that the bound wider field follows: it is borrowed twice.
    passes(
        &format!(
            "{make}def f :: own R -> i64 = \\r ->\n    let wide: {wider} = r.u\n    look wide + look wide + take wide\n\n\
             test \"field\" =\n    assert (f (R {{ u: 4, k: 1 }}) == 4)\n    assert (f (R {{ u: \"x\", k: 1 }}) == 0)\n"
        ),
        Edition::E2027,
    );
}

#[test]
fn a_converted_local_borrowed_beyond_the_call_names_the_conversion() {
    // A read parameter of a partial application borrows its argument while the function value
    // lives. A local converted into the wider union is a temporary: the error says so and how
    // to bind the converted value, not to bind a temporary that the source does not write
    // (review round 2). The injection of A24 says the same.
    let make = "def mk :: i64 -> (i64 | [i64]) = \\n -> if n % 2 == 0 then n else Array.init n (\\i -> i)\n\n\
         def three :: (f64 | i64 | [i64]) -> i64 -> i64 = \\a b -> b\n\n";
    for (body, name, union) in [
        (
            "    let w = mk n\n    let g = three w\n    g 1\n",
            "w",
            "(i64 | f64 | [i64])",
        ),
        (
            "    let w = mk n\n    5 |> three w\n",
            "w",
            "(i64 | f64 | [i64])",
        ),
        (
            "    let k = n\n    let g = three k\n    g 1\n",
            "k",
            "(i64 | f64 | [i64])",
        ),
    ] {
        let source = format!("{make}def f :: i64 -> i64 = \\n ->\n{body}");
        let error = rejects_in(&source, Edition::E2027, "E1013", "borrow_of_temporary");
        assert_eq!(
            error.message,
            format!(
                "borrow requires a local place; '{name}' converted into {union} is a temporary value"
            ),
            "{body}"
        );
        assert_eq!(
            error.help,
            Some(format!(
                "bind the converted value first, as in 'let wide: {union} = {name}', and pass 'wide'; or take the parameter with 'own'"
            )),
            "{body}"
        );
        let at = source.find(&format!("three {name}")).unwrap() + "three ".len();
        assert_eq!((error.span.start, error.span.end), (at, at + 1), "{body}");
    }
    // Both ways out that the help names are accepted, and compute the same: (f64 | i64 | [i64])
    // ignored, b returned.
    let source = format!(
        "{make}def f :: i64 -> i64 = \\n ->\n    let w = mk n\n    let wide: (f64 | i64 | [i64]) = w\n    let g = three wide\n    g 1\n\n\
         def own_three :: own (f64 | i64 | [i64]) -> i64 -> i64 = \\a b -> b\n\n\
         def h :: i64 -> i64 = \\n ->\n    let w = mk n\n    let g = own_three w\n    g 2\n\n\
         test \"bound\" =\n    assert (f 3 + h 3 + f 4 + h 4 == 6)\n"
    );
    passes(&source, Edition::E2027);
    // A temporary that the source writes keeps its message.
    let error = rejects_in(
        &format!("{make}def f :: i64 -> i64 = \\n ->\n    let g = three (mk n)\n    g 1\n"),
        Edition::E2027,
        "E1013",
        "borrow_of_temporary",
    );
    assert_eq!(
        error.message,
        "borrow requires a local place; bind the temporary with 'let' first"
    );
}

/// `depth` nested matches of the same value whose arms narrow it with `arm` (`{x}` is the
/// binder of the level), each of them followed by an arm for the string.
fn nested_narrowings(depth: usize, arm: &str) -> String {
    let mut source = "def mk :: i64 -> (f64 | i64 | string | bool) = \\n -> if n > 0 then n else \"x\"\n\n\
         def num :: (i64 | f64 | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as k -> k\n    | :? f64 -> 1\n    | :? bool -> 2\n\n\
         export def t :: i64 -> i64 = \\n ->\n    let v = mk n\n"
        .to_owned();
    for level in 0..depth {
        let pad = "    ".repeat(level + 1);
        let arm = arm.replace("{x}", &format!("x{level}"));
        source.push_str(&format!("{pad}match v with\n{pad}| {arm} + (\n"));
    }
    source.push_str(&format!("{}0\n", "    ".repeat(depth + 1)));
    for level in (0..depth).rev() {
        let pad = "    ".repeat(level + 1);
        source.push_str(&format!("{pad}    )\n{pad}| :? string -> 1\n"));
    }
    source
}

#[test]
fn nested_narrowing_arms_check_each_body_once() {
    // Each `:? (A | B | C)` is three alternatives of one arm. They bind the same values, so the
    // ownership check visits the body of the arm once, not once for each alternative: a match
    // nested 30 deep in such arms was 3^30 visits of the innermost body (review round 2). The
    // same holds for a narrowing without a binder and for the OR of type tests that A24 wrote.
    for arm in [
        ":? (i64 | f64 | bool) as {x} -> num {x}",
        ":? (i64 | f64 | bool) -> 1",
        ":? i64 | :? f64 | :? bool -> 1",
    ] {
        let source = nested_narrowings(30, arm);
        accepts_in(&source, Edition::E2027);
    }
    // Expected value by hand: every level of `nested_narrowings 3` takes the i64 5 (5 each) for
    // t 5, and t (-1) is the string at the outer level (1).
    let source = format!(
        "{}\ntest \"nested\" =\n    assert (t 5 == 15 && t (-1) == 1)\n",
        nested_narrowings(3, ":? (i64 | f64 | bool) as {x} -> num {x}")
    );
    passes(&source, Edition::E2027);
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
fn narrowing_a_union_that_may_hold_loans_copies_the_payload() {
    // A function member may hold loans, and ownership and code generation view no value of a type
    // that may (`TypedMatchArm::views`). `:? (i64 | f64) as n` of such a union copies the selected
    // Copy payload in a step of the pattern, as the arms `:? i64 as n`, `:? f64 as n` do, instead of
    // taking the whole value into its hidden binding (Copilot review round 1): that was `E1012`
    // behind a reference and a move of an owned value, which could not be used again. By hand:
    // `num` is 2 k for the i64 k and 3 for an f64 above 1; `size` is k for the i64 k, 1 for an f64,
    // the length of a string and f 1 for a function f.
    const ANY: &str = "(i64 | f64 | string | (i64 -> i64))";
    let source = format!(
        "{NUM}def size :: ref {ANY} -> i64 = \\v ->\n    match v with\n    | :? i64 as k -> k\n    | :? f64 -> 1\n    | :? string as s -> s.length\n    | :? (i64 -> i64) as f -> f 1\n\n\
         def by_ref :: ref {ANY} -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n    | :? (i64 -> i64) -> 100\n\n\
         def owned :: own {ANY} -> i64 = \\v ->\n    let r = match v with\n        | :? (i64 | f64) as n -> num n\n        | :? string -> 0\n        | :? (i64 -> i64) -> 0\n    r * 1000 + size (ref v)\n\n\
         test \"loans\" =\n    \
         let a: {ANY} = 4\n    \
         let b: {ANY} = 2.5\n    \
         let c: {ANY} = \"abc\"\n    \
         let d: {ANY} = \\(k: i64) -> k + 41\n    \
         assert (by_ref (ref a) + by_ref (ref b) + by_ref (ref c) + by_ref (ref d) == 8 + 3 + 3 + 100)\n    \
         assert (by_ref (ref a) == 8)\n    \
         assert (owned a == 8004 && owned b == 3001 && owned c == 3 && owned d == 42)\n"
    );
    passes(&source, Edition::E2026);
    // A read parameter (2027) is such a view. By hand: 8 + 3 + 2 + 2 * 5.
    passes(
        &format!(
            "{NUM}def read :: {ANY} -> i64 = \\v ->\n    match v with\n    | :? (i64 | f64) as n -> num n\n    | :? string as s -> s.length\n    | :? (i64 -> i64) as f -> f 2\n\n\
             test \"read\" =\n    assert (read 4 + read 2.5 + read \"ab\" + read (\\(k: i64) -> k * 5) == 8 + 3 + 2 + 10)\n"
        ),
        Edition::E2027,
    );
}

#[test]
fn narrowing_a_copy_union_copies_its_array_once() {
    // A union whose members are all Copy, one of them an array: the hidden binding of
    // `:? (i64 | [i64]) as x` views the matched value, as it does a union that is not Copy, and the
    // retagging copies the array once, into the narrowed value (Copilot review round 1). Before, the
    // hidden binding copied the whole value first behind a reference, through a read parameter and
    // from an owned value that the function reads again: two copies. An owned value read once moved
    // into the hidden binding: one copy, as now. The arm may write over the matched variable: the
    // narrowed value was copied before the body runs.
    let arr = "def arr :: own ([i64] | i64) -> i64 = \\n ->\n    match n with\n    | :? i64 as k -> k\n    | :? [i64] as xs -> xs.length * 100\n\n\
         def balanced :: (unit -> i64) -> i64 = \\f ->\n    let start = Alloc.snapshot ()\n    let value = f ()\n    let used = start |> Alloc.since\n    if used.frees == used.allocs then value else -1\n\n";
    let functions = "def by_ref :: ref (i64 | f64 | [i64]) -> i64 = \\v ->\n    match v with\n    | :? (i64 | [i64]) as x -> arr x\n    | :? f64 -> 1\n\n\
         def again :: own (i64 | f64 | [i64]) -> i64 = \\v ->\n    let r = match v with\n        | :? (i64 | [i64]) as x -> arr x\n        | :? f64 -> 1\n    r * 1000 + by_ref (ref v)\n\n\
         def touched :: own (i64 | f64 | [i64]) -> i64 = \\v ->\n    match v with\n    | :? (i64 | [i64]) as x -> arr x * 1000 + by_ref (ref v)\n    | :? f64 -> 1\n\n\
         def replaced :: own (i64 | f64 | [i64]) -> i64 = \\start ->\n    let mut v = start\n    match v with\n    | :? (i64 | [i64]) as x ->\n        v = 2.5\n        arr x * 1000 + by_ref (ref v)\n    | :? f64 -> 1\n\n\
         def once :: own (i64 | f64 | [i64]) -> i64 = \\v ->\n    match v with\n    | :? (i64 | [i64]) as x -> arr x\n    | :? f64 -> 1\n\n";
    let source = format!("{arr}{functions}");
    let module = accepts_in(&source, Edition::E2026);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    for name in ["by_ref", "again", "touched", "replaced", "once"] {
        let body = function_body(&ir, name);
        assert_eq!(body.matches("@tz.alloc").count(), 1, "{name}:\n{body}");
    }
    // The copy is the narrowed value's, at the pattern, as for a union that is not Copy.
    let source_no_copy = format!("{arr}@no_copy\n{functions}");
    let error = rejects(&source_no_copy, "E1031", "effect_copy_in_no_copy");
    let pattern = ":? (i64 | [i64]) as x";
    let at = source_no_copy.find(pattern).unwrap();
    assert_eq!((error.span.start, error.span.end), (at, at + pattern.len()));
    // Expected values by hand: arr gives 100 for each number of an array and the i64 itself, the
    // f64 arm 1. again [1, 2]: 200 * 1000 + 200; touched as again; replaced [1, 2]: the narrowed
    // value keeps the array (200), v is then the f64 (1).
    let tests = "def via_ref :: own (i64 | f64 | [i64]) -> i64 = \\v -> by_ref (ref v)\n\n\
         test \"values\" =\n    \
         assert (balanced (\\() -> via_ref [1, 2, 3]) == 300 && balanced (\\() -> via_ref 7) == 7 && balanced (\\() -> via_ref 2.5) == 1)\n    \
         assert (balanced (\\() -> again [1, 2]) == 200200 && balanced (\\() -> again 5) == 5005 && balanced (\\() -> again 1.5) == 1001)\n    \
         assert (balanced (\\() -> touched [1, 2]) == 200200 && balanced (\\() -> touched 5) == 5005 && balanced (\\() -> touched 1.5) == 1)\n    \
         assert (balanced (\\() -> replaced [1, 2]) == 200001 && balanced (\\() -> replaced 5) == 5001 && balanced (\\() -> replaced 1.5) == 1)\n    \
         assert (balanced (\\() -> once [4]) == 100 && balanced (\\() -> once 9) == 9 && balanced (\\() -> once 0.5) == 1)\n";
    passes(&format!("{source}{tests}"), Edition::E2026);
    // A read parameter (2027) views the matched value too.
    let read = format!(
        "{arr}def read :: (i64 | f64 | [i64]) -> i64 = \\v ->\n    match v with\n    | :? (i64 | [i64]) as x -> arr x\n    | :? f64 -> 1\n\n\
         test \"read\" =\n    assert (balanced (\\() -> read [1, 2, 3]) == 300 && balanced (\\() -> read 7) == 7 && balanced (\\() -> read 2.5) == 1)\n"
    );
    let module = accepts_in(&read, Edition::E2027);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let body = function_body(&ir, "read");
    assert_eq!(body.matches("@tz.alloc").count(), 1, "read:\n{body}");
    passes(&read, Edition::E2027);
}

/// `arr` (100 for each number of an array, the i64 itself) and `balanced` (the value of `f ()`
/// when it frees what it allocates, else -1) for the narrowing tests below.
const ARR_BALANCED: &str = r#"def arr :: own ([i64] | i64) -> i64 = \n ->
    match n with
    | :? i64 as k -> k
    | :? [i64] as xs -> xs.length * 100

def balanced :: (unit -> i64) -> i64 = \f ->
    let start = Alloc.snapshot ()
    let value = f ()
    let used = start |> Alloc.since
    if used.frees == used.allocs then value else -1

"#;

#[test]
fn narrowing_a_generic_union_copies_the_payload_for_every_type_argument() {
    // A type variable may stand for a type that holds loans: `(i64 | f64 | Maybe<'a>)` with `'a` a
    // pair of a string and a function, or a record with such fields, is not Copy and holds loans,
    // and ownership and code generation decide views in each specialization
    // (`TypedMatchArm::views`). So `:? (i64 | f64) as n` of a union with a type variable copies the
    // payload in a step of the pattern for every type argument (Copilot review round 1,
    // verification). Before, the hidden binding viewed the generic union and took the whole value
    // in such a specialization: `E1012` behind a reference and a move of an owned value that is
    // used again, both reported inside the generic definition.
    // By hand: `num` is 2 k for the i64 k, 3 for an f64 above 1 and 0 for one below; `by_ref` 50 for
    // a `Maybe`; `owned` is 1000 times the arm plus `by_ref`; `touched` 1000 num n + by_ref, 7 for a
    // `Maybe`; `guarded` num n when it is above 4, 1 for another number, 2 for a `Maybe`; `beside`
    // 1000 times the length of the string plus num n; `either` num n of the side that is a number.
    // `sf base kind` is base, base + 0.5 or `Some` of a pair whose function captures `base`.
    let functions = r#"record Pair { text: string, f: (i64 -> i64) }

def by_ref :: ref (i64 | f64 | Maybe<'a>) -> i64 = \v ->
    match v with
    | :? (i64 | f64) as n -> num n
    | :? Maybe<'a> -> 50

def owned :: own (i64 | f64 | Maybe<'a>) -> i64 = \v ->
    let r = match v with
        | :? (i64 | f64) as n -> num n
        | :? Maybe<'a> -> 0
    r * 1000 + by_ref (ref v)

def touched :: own (i64 | f64 | Maybe<'a>) -> i64 = \v ->
    match v with
    | :? (i64 | f64) as n -> num n * 1000 + by_ref (ref v)
    | :? Maybe<'a> -> 7

def guarded :: ref (i64 | f64 | Maybe<'a>) -> i64 = \v ->
    match v with
    | :? (i64 | f64) as n when num n > 4 -> num n
    | :? (i64 | f64) -> 1
    | :? Maybe<'a> -> 2

def with_array :: ref (i64 | [i64] | Maybe<'a>) -> i64 = \v ->
    match v with
    | :? (i64 | [i64]) as x -> arr x
    | :? Maybe<'a> -> 9

def with_array_own :: own (i64 | [i64] | Maybe<'a>) -> i64 = \v ->
    let r = match v with
        | :? (i64 | [i64]) as x -> arr x
        | :? Maybe<'a> -> 9
    r * 1000 + with_array (ref v)

def beside :: own (string * (i64 | f64 | Maybe<'a>)) -> i64 = \t ->
    match t with
    | (s, :? (i64 | f64) as n) -> s.length * 1000 + num n
    | (s, _) -> s.length

def either :: ref ((i64 | f64 | Maybe<'a>) * (i64 | f64 | Maybe<'a>)) -> i64 = \pair ->
    match pair with
    | (:? (i64 | f64) as n, :? Maybe<'a>) | (:? Maybe<'a>, :? (i64 | f64) as n) -> num n
    | _ -> -1

def sf :: i64 -> i64 -> (i64 | f64 | Maybe<(string * (i64 -> i64))>) = \base kind ->
    let f = \(k: i64) -> k + base
    if kind == 0 then base else if kind == 1 then (base as f64) + 0.5 else Some ($"ab{base}", f)

def pr :: i64 -> i64 -> (i64 | f64 | Maybe<Pair>) = \base kind ->
    if kind == 0 then base else if kind == 1 then (base as f64) + 0.5 else Some (Pair { text: $"x{base}", f: \(k: i64) -> k * base })

"#;
    let tests = r#"test "pair of a string and a function" =
    let a = sf 3 0
    let b = sf 2 1
    let c = sf 3 2
    assert (by_ref (ref a) == 6 && by_ref (ref b) == 3 && by_ref (ref c) == 50)
    assert (guarded (ref a) == 6 && guarded (ref b) == 1 && guarded (ref c) == 2)
    assert (balanced (\() -> owned (sf 3 0)) == 6006 && balanced (\() -> owned (sf 2 1)) == 3003 && balanced (\() -> owned (sf 3 2)) == 50)
    assert (balanced (\() -> touched (sf 3 0)) == 6006 && balanced (\() -> touched (sf 2 1)) == 3003 && balanced (\() -> touched (sf 3 2)) == 7)
    let t: (string * (i64 | f64 | Maybe<(string * (i64 -> i64))>)) = ($"abc{a |> by_ref}", sf 3 0)
    assert (beside t == 4006)
    let e: (i64 | [i64] | Maybe<(string * (i64 -> i64))>) = [1, 2]
    assert (with_array (ref e) == 200 && balanced (\() -> with_array_own e) == 200200)
    let pair = (sf 4 0, sf 4 2)
    assert (either (ref pair) == 8)
    let other = (sf 4 2, sf 2 1)
    assert (either (ref other) == 3)

test "record with a string and a function" =
    let a = pr 7 0
    let c = pr 2 2
    assert (by_ref (ref a) == 14 && by_ref (ref c) == 50)
    assert (balanced (\() -> owned (pr 7 0)) == 14014 && balanced (\() -> touched (pr 7 0)) == 14014)
    assert (balanced (\() -> owned (pr 2 2)) == 50)

test "other type arguments" =
    let s: (i64 | f64 | Maybe<string>) = 3
    let i: (i64 | f64 | Maybe<i64>) = 2.5
    let xs: (i64 | f64 | Maybe<[i64]>) = 4
    let fs: (i64 | f64 | Maybe<(i64 -> i64)>) = 1
    assert (by_ref (ref s) + owned s == 6012)
    assert (by_ref (ref i) + owned i == 3006)
    assert (by_ref (ref xs) + owned xs == 8016)
    assert (by_ref (ref fs) + owned fs == 2004)
    let t: (i64 | [i64] | Maybe<string>) = [1, 2, 3]
    let u: (i64 | [i64] | Maybe<i64>) = [4]
    assert (balanced (\() -> with_array_own t) == 300300 && balanced (\() -> with_array_own u) == 100100)
"#;
    passes(
        &[NUM, ARR_BALANCED, functions, tests].concat(),
        Edition::E2026,
    );
    // A read parameter (2027) is a view too. By hand: 6 + 3 + 50.
    let read = r#"def read :: (i64 | f64 | Maybe<'a>) -> i64 = \v ->
    match v with
    | :? (i64 | f64) as n -> num n
    | :? Maybe<'a> -> 50

def sf :: i64 -> i64 -> (i64 | f64 | Maybe<(string * (i64 -> i64))>) = \base kind ->
    let f = \(k: i64) -> k + base
    if kind == 0 then base else if kind == 1 then (base as f64) + 0.5 else Some ($"ab{base}", f)

test "read" =
    assert (read (sf 3 0) + read (sf 2 1) + read (sf 3 2) == 6 + 3 + 50)
"#;
    passes(&[NUM, read].concat(), Edition::E2027);
}

#[test]
fn narrowing_a_copy_union_beside_a_moved_part_copies_its_array_once() {
    // The hidden binding of a narrowing into Copy members views a Copy union, and only a move of
    // the part that it views meets that view (Copilot review round 1, verification). Beside a
    // binding that moves another part (`s` takes `t.0`, the narrowing reads `t.1`), the hidden
    // binding takes the value as a binding does: it moves out of the owned subject, which is dead
    // afterwards, and the retagging copies the array once. Behind a reference it stays a view.
    // Stepping the narrowing, as for `(:? W as x) as whole`, copied the array twice: into the value
    // that the pattern makes and into the binding.
    let functions = r#"record R { u: (i64 | f64 | [i64]), s: string }

def size :: ref (i64 | f64 | [i64]) -> i64 = \v ->
    match v with
    | :? i64 as k -> k
    | :? f64 -> 1
    | :? [i64] as xs -> xs.length * 10

def tuple :: own (string * (i64 | f64 | [i64])) -> i64 = \t ->
    match t with
    | (s, :? (i64 | [i64]) as x) -> s.length * 1000000 + arr x
    | (s, _) -> s.length

def field :: own R -> i64 = \r ->
    match r with
    | R { u = :? (i64 | [i64]) as x, s = s } -> s.length * 1000000 + arr x
    | R { s = s } -> s.length

def through :: own (string * ref (i64 | f64 | [i64])) -> i64 = \t ->
    match t with
    | (s, :? (i64 | [i64]) as x) -> s.length * 1000000 + arr x
    | (s, _) -> s.length

def either :: own ((string * (i64 | f64 | [i64])) * (i64 | f64 | [i64])) -> i64 = \p ->
    match p with
    | ((s, :? (i64 | [i64]) as x), :? f64) -> s.length * 1000000 + arr x
    | ((s, :? f64), :? (i64 | [i64]) as x) -> s.length * 2000000 + arr x
    | _ -> 0

def ored :: own ((string * (i64 | f64 | [i64])) * (i64 | f64 | [i64])) -> i64 = \p ->
    match p with
    | ((s, :? (i64 | [i64]) as x), :? f64) | ((s, :? f64), :? (i64 | [i64]) as x) -> s.length * 1000000 + arr x
    | _ -> 0

def whole :: own (i64 | f64 | [i64]) -> i64 = \v ->
    match v with
    | (:? (i64 | [i64]) as x) as w -> arr x * 1000 + size (ref w)
    | :? f64 -> 1

"#;
    let source = [ARR_BALANCED, functions].concat();
    let module = accepts_in(&source, Edition::E2026);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    // One copy for each arm that narrows: the retagging copies the array of the narrowed value.
    for (name, copies) in [
        ("tuple", 1),
        ("field", 1),
        ("through", 1),
        ("either", 2),
        ("ored", 1),
    ] {
        let body = function_body(&ir, name);
        assert_eq!(body.matches("@tz.alloc").count(), copies, "{name}:\n{body}");
    }
    // `W1006` is at the pattern only, as for a union that is not Copy, never at the binder.
    let pattern = ":? (i64 | [i64]) as x";
    let warnings = tsuzuri::copies::warnings(&module);
    for name in ["tuple", "field", "through", "either"] {
        let start = source.find(&format!("def {name} ::")).unwrap();
        let end = start + source[start..].find("\n\n").unwrap();
        let spelled: Vec<&str> = warnings
            .iter()
            .filter(|warning| warning.code == "W1006")
            .filter(|warning| (start..end).contains(&warning.span.start))
            .map(|warning| &source[warning.span.start..warning.span.end])
            .collect();
        let narrowings = source[start..end].matches(pattern).count();
        assert_eq!(spelled, vec![pattern; narrowings], "{name}");
    }
    // Expected values by hand: the string's length times 1000000 (2000000 in the second arm of
    // `either`) plus arr x; `whole` 1000 arr x plus size, 10 for each number of an array.
    let tests = r#"def mk :: unit -> (i64 | f64 | [i64]) = \() -> [1, 2, 3]

def through_caller :: unit -> i64 = \() ->
    let v = mk ()
    let r = through ($"ab{1}", ref v)
    r + size (ref v)

test "values" =
    assert (balanced (\() -> tuple ($"ab{1}", mk ())) == 3000300 && balanced (\() -> tuple ("a", 7)) == 1000007 && balanced (\() -> tuple ("a", 0.5)) == 1)
    assert (balanced (\() -> field (R { u: mk (), s: $"ab{1}" })) == 3000300 && balanced (\() -> field (R { u: 2.5, s: "ab" })) == 2)
    assert (balanced through_caller == 3000330)
    assert (balanced (\() -> either (($"ab{1}", mk ()), 2.5)) == 3000300 && balanced (\() -> either (($"ab{1}", 2.5), mk ())) == 6000300)
    assert (balanced (\() -> either (("ab", 1.5), 1.5)) == 0)
    assert (balanced (\() -> ored (($"ab{1}", mk ()), 1.5)) == 3000300 && balanced (\() -> ored (("ab", 1.5), 4)) == 2000004)
    assert (balanced (\() -> whole (mk ())) == 300030 && balanced (\() -> whole 6) == 6006 && balanced (\() -> whole 0.5) == 1)
"#;
    passes(&[source.as_str(), tests].concat(), Edition::E2026);
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

/// Copilot review 2 of #102: `@old (x)` in an `@ensures` line is injected where an anonymous
/// union is expected, as `x` is: at an annotated binding, in the arms of an `if` and in the
/// elements of an array literal, as well as at an argument. The contracts are checked at run time,
/// so each line must also hold for the values computed by hand.
#[test]
fn old_values_are_injected() {
    let count = "def count :: own (i64 | bool) -> i64 = \\v ->\n    match v with\n    | :? i64 as n -> n\n    | :? bool as b -> if b then 1000 else 2000\n\ndef total :: own [(i64 | bool)] -> i64 = \\vs ->\n    let mut sum = 0\n    for v in vs do\n        sum = sum + count v\n    sum\n\n";
    // grow 41 is 42: count 41 + 1 is 42; r is not over 100, so w is true and count w is 1000;
    // total [41, true, false] is 41 + 1000 + 2000, which is 42 + 2999. grow 200 is 201: w is the
    // old 200, which is 201 - 1, and total [200, true, false] is 3200, which is 201 + 2999.
    passes(
        &format!(
            "{count}def grow :: i64 -> i64 = \\x ->\n    @ensures \\r -> {{ let v: (i64 | bool) = @old (x); count v + 1 == r }}\n    @ensures \\r -> {{ let w: (i64 | bool) = if r > 100 then @old (x) else true; count w == (if r > 100 then r - 1 else 1000) }}\n    @ensures \\r -> total [@old (x), true, false] == r + 2999\n    @ensures \\r -> count (@old (x)) + 1 == r\n    x + 1\n\ntest \"old values\" =\n    assert (grow 41 == 42)\n    assert (grow 200 == 201)\n"
        ),
        Edition::E2026,
    );
    // The injected value is the old one: count 41 is 41, not 42, so the line traps.
    traps(
        &format!(
            "{count}def wrong :: i64 -> i64 = \\x ->\n    @ensures \\r -> {{ let v: (i64 | bool) = @old (x); count v == r }}\n    x + 1\n\ntest \"wrong\" =\n    assert (wrong 41 == 42)\n"
        ),
        Edition::E2026,
    );
}

/// Copilot review 2 of #102: the arm's own `->` is outside every parenthesis. An arrow inside
/// one, here the function type of a pattern's annotation, does not end the scan for a type test,
/// so the function-guard arm is a pattern (its `<` read as a comparison made it a predicate).
#[test]
fn arrows_in_annotations_do_not_hide_type_tests() {
    // pick f 5 is 5 (f (Some 5) holds), pick f 7 is -7, pick f true is 100 and pick f false is 200.
    let source = "def pick :: (Maybe<i64> -> bool) -> own (i64 | bool) -> i64 = \\g v ->\n    | ((g: Maybe<i64> -> bool), :? i64 as n) -> if g (Some n) then n else -n\n    | (_, :? bool as b) -> if b then 100 else 200\n\ndef guarded :: (Maybe<i64> -> bool) -> own (i64 | bool) -> i64 = \\g v ->\n    | ((g: Maybe<i64> -> bool), :? i64 as n) when g (Some n) -> n\n    | _ -> 0\n\ndef matched :: (Maybe<i64> -> bool) -> own (i64 | bool) -> i64 = \\g v ->\n    match (g, v) with\n    | ((g: Maybe<i64> -> bool), :? i64 as n) -> if g (Some n) then n else -n\n    | _ -> 0\n\ntest \"pick\" =\n    let five = \\m -> Maybe.default_value 0 m == 5\n    assert (pick five 5 == 5)\n    assert (pick five 7 == -7)\n    assert (pick five true == 100)\n    assert (pick five false == 200)\n    assert (guarded five 5 == 5)\n    assert (guarded five 7 == 0)\n    assert (matched five 5 == 5)\n    assert (matched five 7 == -7)\n";
    let program = parsed(source);
    assert!(program.uses_anonymous_unions);
    passes(source, Edition::E2026);
    // A predicate with arrows inside parentheses and no type test of its own is still a
    // predicate: f 2 is 1 (1 < 2) and f 0 is 0.
    passes(
        "def holds :: (i64 -> bool) -> bool = \\h -> h 1\ndef f :: i64 -> i64 = \\n ->\n    | holds ((\\(h: i64 -> bool) -> h) (\\k -> k < n)) -> 1\n    | otherwise -> 0\n\ntest \"f\" =\n    assert (f 2 == 1)\n    assert (f 0 == 0)\n",
        Edition::E2026,
    );
}

/// Copilot review 3 of #102: the scan for a type test reads the type of an annotation as the
/// parser does (`type_expr` inside parentheses, `type_product` outside them), so nothing in that
/// type hides the type test after it: neither the `fn` of `fn() -> T` (which started an
/// expression of its own to the scan), nor an arrow outside the parentheses of the arm (inside
/// `<...>`, `[...]` or after `fn()`), nor a `<` (which the later scans read as a comparison).
/// Each form of type stands before a type test in a function-guard arm, with and without `when`,
/// in a `match` arm inside one and two case patterns, with and without `when`, after an
/// annotation outside the parentheses of the arm, and in a record pattern.
#[test]
fn annotation_types_do_not_hide_type_tests() {
    let forms = [
        "fn() -> i64",
        "fn(i64, bool) -> i64",
        "fn() -> i64 -> i64",
        "Maybe<fn() -> i64>",
        "Maybe<Maybe<Maybe<i64>>>",
        "Result<Maybe<i64>, Maybe<fn() -> Maybe<i64>>>",
        "Maybe<i64> -> bool",
        "i64 -> Maybe<i64> -> bool",
        "i64 * Maybe<i64>",
        "(bool | Maybe<i64>)",
        "(fn() -> i64 | Maybe<i64 -> bool>)",
        "ref Maybe<i64>",
        "ref mut Maybe<i64>",
        "&Maybe<i64>",
        "&&Maybe<i64>",
        "ref {a} Maybe<i64>",
        "{a} ref {a} Maybe<i64> -> i64",
        "[i64; 3]",
        "[Maybe<i64>; N]",
        "[Maybe<i64>]",
        "[|Maybe<i64>|]",
        "ref mut [Maybe<i64>..]",
        "(i64 * Maybe<i64>)",
        "Point",
        "'a",
        "'f<Maybe<i64>>",
        "own Maybe<i64>",
        "mut Maybe<i64>",
        "pure (i64 -> Maybe<i64>)",
        "dyn Show",
        "dyn (Show, Send)",
    ];
    for form in forms {
        let guard = "def f :: i64 -> own (i64 | bool) -> i64 = \\x v ->\n";
        let matched =
            "def f :: i64 -> own (i64 | bool) -> i64 = \\x v ->\n    match Some (x, v) with\n";
        let nested = "def f :: i64 -> own (i64 | bool) -> i64 = \\x v ->\n    match Some (Some (x, v)) with\n";
        let boxed = "def f :: Box -> own (i64 | bool) -> i64 = \\b v ->\n";
        for source in [
            format!("{guard}    | (x: {form}, :? i64 as n) -> n\n    | _ -> 0\n"),
            format!("{guard}    | (x: {form}, :? i64 as n) when n > 0 -> n\n    | _ -> 0\n"),
            format!("{matched}    | Some (x: {form}, :? i64 as n) -> n\n    | _ -> 0\n"),
            format!("{matched}    | Some (x: {form}, :? i64 as n) when n > 0 -> n\n    | _ -> 0\n"),
            format!("{nested}    | Some (Some (x: {form}, :? i64 as n)) -> n\n    | _ -> 0\n"),
            format!(
                "{nested}    | Some (Some (x: {form}, :? i64 as n)) when n > 0 -> n\n    | _ -> 0\n"
            ),
            format!(
                "def f :: own (i64 * (i64 | bool)) -> i64 = \\t ->\n    | t: Maybe<{form}> * (i64 | bool) & (_, :? i64 as n) -> n\n    | _ -> 0\n"
            ),
            format!("{boxed}    | ({{ value = (h: {form}) }}, :? i64 as n) -> n\n    | _ -> 0\n"),
            format!(
                "{boxed}    | ({{ value: (h: {form}) }}, :? i64 as n) when n > 0 -> n\n    | _ -> 0\n"
            ),
            format!(
                "{boxed}    match Some (b, v) with\n    | Some ({{ value: (h: {form}) }}, :? i64 as n) -> n\n    | _ -> 0\n"
            ),
        ] {
            // `&` is a spelling of 2026 only (A22); the scan is the same in both editions.
            for edition in [Edition::E2026, Edition::E2027] {
                if edition == Edition::E2026 || !form.contains('&') {
                    assert_type_test_in(&source, edition, "Named(\"i64\")", Some("n"));
                }
            }
        }
    }
    // The same forms run, with the values bound by the annotated patterns: by hand, `answer()` is
    // 40, `mul 3 4` is 12, `(3, Some 4)` gives 7, `Some 6` gives 6, `[1, 2, 3]` gives 1 + 3 for the
    // fixed array and 3 for its length, `(3, 4)` gives 12; a type test of 500 adds 1000, `true`
    // takes the last arm (0).
    let runs = [
        ("thunk", "(fn() -> i64)", "fn() -> i64", "answer", "x()", 40),
        (
            "generic",
            "Maybe<fn() -> i64>",
            "Maybe<fn() -> i64>",
            "(Some answer)",
            "(match x with | Some h -> h() | None -> 0)",
            40,
        ),
        (
            "nested",
            "Maybe<Maybe<Maybe<i64>>>",
            "Maybe<Maybe<Maybe<i64>>>",
            "(Some (Some (Some 2)))",
            "(match x with | Some (Some (Some k)) -> k | _ -> 0)",
            2,
        ),
        (
            "arrow",
            "(i64 -> i64 -> i64)",
            "i64 -> i64 -> i64",
            "mul",
            "x 3 4",
            12,
        ),
        (
            "product",
            "(i64 * Maybe<i64>)",
            "i64 * Maybe<i64>",
            "(3, Some 4)",
            "(match x with | (a, Some b) -> a + b | (a, None) -> a)",
            7,
        ),
        (
            "union",
            "own (bool | Maybe<i64>)",
            "(bool | Maybe<i64>)",
            "(Some 6)",
            "(match x with | :? Maybe<i64> as m -> Maybe.default_value 0 m | :? bool -> 1)",
            6,
        ),
        (
            "fixed",
            "[i64; 3]",
            "[i64; 3]",
            "[1, 2, 3]",
            "x[0] + x[2]",
            4,
        ),
        ("array", "own [i64]", "[i64]", "[1, 2, 3]", "x.length", 3),
        (
            "tuple",
            "(i64 * i64)",
            "(i64 * i64)",
            "(3, 4)",
            "(match x with | (a, b) -> a * b)",
            12,
        ),
        ("variable", "'a", "'a", "1.5", "0", 0),
    ];
    let mut source = "def answer :: i64 = 40\n\ndef mul :: i64 -> i64 -> i64 = \\a b -> a * b\n\nrecord Box { value: Maybe<fn() -> i64> }\n\n".to_owned();
    let mut tests = String::new();
    for (name, parameter, annotation, argument, value, expected) in runs {
        source.push_str(&format!(
            "def g_{name} :: {parameter} -> own (i64 | bool) -> i64 = \\x v ->\n    | (x: {annotation}, :? i64 as n) when n > 100 -> {value} + n + 1000\n    | (x: {annotation}, :? i64 as n) -> {value} + n\n    | _ -> 0\n\ndef m_{name} :: {parameter} -> own (i64 | bool) -> i64 = \\x v ->\n    match Some (Some (x, v)) with\n    | Some (Some (x: {annotation}, :? i64 as n)) when n > 100 -> {value} + n + 1000\n    | Some (Some (x: {annotation}, :? i64 as n)) -> {value} + n\n    | _ -> 0\n\n"
        ));
        for function in ["g", "m"] {
            tests.push_str(&format!(
                "    assert ({function}_{name} {argument} 5 == {})\n    assert ({function}_{name} {argument} 500 == {})\n    assert ({function}_{name} {argument} true == 0)\n",
                expected + 5,
                expected + 1500
            ));
        }
    }
    // Outside the parentheses of the arm, after `&`, and inside a record pattern.
    source.push_str("def g_and :: own (Maybe<fn() -> i64> * (i64 | bool)) -> i64 = \\t ->\n    | t: Maybe<fn() -> i64> * (i64 | bool) & (_, :? i64 as n) -> n\n    | _ -> 0\n\ndef g_box :: Box -> own (i64 | bool) -> i64 = \\b v ->\n    | ({ value = (h: Maybe<fn() -> i64>) }, :? i64 as n) -> (match h with | Some k -> k() + n | None -> n)\n    | _ -> 0\n\n");
    tests.push_str("    assert (g_and (Some answer, 5) == 5)\n    assert (g_and (None, true) == 0)\n    assert (g_box (Box { value: Some answer }) 2 == 42)\n    assert (g_box (Box { value: None }) 2 == 2)\n    assert (g_box (Box { value: Some answer }) false == 0)\n");
    // Predicates that hold such types stay predicates, before the last type test of the source
    // (so the scan reads them): by hand, `answer()` is 40, over 30 and over 10 but not over 50.
    source.push_str("def holds :: bool -> bool = \\b -> b\n\ndef value_of :: Box -> i64 = \\b ->\n    match b.value with\n    | Some h -> h()\n    | None -> 0\n\ndef p_record :: i64 -> i64 = \\n ->\n    | value_of (Box { value: Some answer }) > n -> 1\n    | otherwise -> 0\n\ndef p_block :: i64 -> i64 = \\n ->\n    | { let y: Maybe<fn() -> i64> = Some answer; (match y with | Some h -> h() | None -> 0) > n } -> 1\n    | otherwise -> 0\n\ndef p_lambda :: own (i64 | bool) -> i64 = \\v ->\n    | holds ((\\(h: fn() -> i64) -> h() > 30) answer) && (match v with | :? i64 as k -> k > 0 | :? bool -> false) -> 1\n    | otherwise -> 0\n\n");
    tests.push_str("    assert (p_lambda 5 == 1)\n    assert (p_lambda (-5) == 0)\n    assert (p_lambda true == 0)\n    assert (p_record 10 == 1)\n    assert (p_record 50 == 0)\n    assert (p_block 10 == 1)\n    assert (p_block 50 == 0)\n");
    source.push_str(&format!("test \"annotations\" =\n{tests}"));
    passes(&source, Edition::E2026);
}
