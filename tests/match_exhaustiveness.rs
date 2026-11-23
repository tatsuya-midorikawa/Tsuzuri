use tsuzuri::check::CheckedModule;
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::{analyze, analyze_modules, llvm};

const MAYBE: &str = "union Maybe<'a> = None | Some of 'a\n";
const SHAPE: &str = "union Shape = Empty | Circle of f64 | Rect of f64 * f64\n";
const EVEN: &str = "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\n";

fn checked(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

/// Accepts an exhaustive program without warnings and lowers it.
fn accepts(source: &str) {
    let module = checked(source);
    assert!(
        module.warnings.is_empty(),
        "{source}\n{:?}",
        module.warnings
    );
    for wasm in [false, true] {
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    }
}

fn missing(source: &str, witness: &str) -> Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(
        (error.code, error.message.as_str()),
        (
            "E1021",
            format!("match is not exhaustive; missing: {witness}").as_str()
        ),
        "{source}"
    );
    error
}

/// The source text of each `W1003` warning.
fn unreachable(source: &str) -> Vec<&str> {
    checked(source)
        .warnings
        .iter()
        .map(|warning| {
            assert_eq!(warning.code, "W1003", "{source}");
            &source[warning.span.start..warning.span.end]
        })
        .collect()
}

#[test]
fn exhaustive_matches_are_accepted() {
    for source in [
        "match true with\n| true -> 1\n| false -> 0".to_string(),
        "match () with\n| () -> 42".into(),
        format!("{MAYBE}match Some 42 with\n| Some n -> n\n| None -> 0"),
        format!(
            "{SHAPE}match Rect (3.0, 4.0) with\n| Empty -> 0.0\n| Circle r -> r\n| Rect (w, h) -> w * h"
        ),
        "match (true, ()) with\n| (true, ()) -> 1\n| (false, ()) -> 0".into(),
        "record R { ok: bool, value: i64 }\nmatch R { ok: true, value: 42 } with\n| { ok = true } -> 1\n| { ok = false } -> 0".into(),
        "match [1, 2] with\n| [] -> 0\n| [x] -> x\n| _ -> 42".into(),
        "match [|1, 2|] with\n| [||] -> 0\n| _ :: _ -> 1".into(),
        "match [|1, 2|] with\n| [||] -> 0\n| [|x|] -> x\n| _ :: _ :: _ -> 2".into(),
        "match [|true|] with\n| true :: _ -> 1\n| [|false|] -> 2\n| false :: _ :: _ -> 3\n| [||] -> 0".into(),
        "match (true, false) with\n| (true, _) | (_, true) -> 1\n| (false, false) -> 0".into(),
        "match (true, false) with\n| (true, _) & (_, _b) -> 1\n| (false, _) -> 0".into(),
        "match true with\n| (true as _t) -> 1\n| (false: bool) -> 0".into(),
        format!("{MAYBE}match Some 42 with\n| Some n when n > 0 -> n\n| Some _ -> 0\n| None -> -1"),
        format!(
            "{MAYBE}def f :: Maybe<Maybe<bool>> -> i64\nfn f x =\n    match x with\n    | Some (Some true) -> 2\n    | Some (Some false) -> 3\n    | Some None -> 1\n    | None -> 0\nf None"
        ),
        "def (|Parts|) :: i64 -> (i64 * i64)\nfn (|Parts|) n = (n, n + 1)\nmatch 20 with\n| Parts (a, b) -> a + b".into(),
        "def (|Id|) :: bool -> bool\nfn (|Id|) b = b\nmatch true with\n| Id (true | false) -> 1".into(),
        format!("{EVEN}match 42 with\n| Even -> 1\n| _ -> 0"),
        "match 1 with\n| 0 -> 0\n| n -> n".into(),
        "match \"a\" with\n| \"a\" -> 1\n| _ -> 0".into(),
        "def classify :: bool -> i64\nfn classify flag\n    | true -> 1\n    | false -> 0\nclassify true".into(),
        "def f :: i64 -> i64\nfn f n\n    | n < 0 -> -1\n    | otherwise -> 1\nf 1".into(),
        "def f :: bool -> bool -> i64\nfn f a b\n    | (true, _) -> 1\n    | (_, x) when x -> 2\n    | (false, _) -> 0\nf true false".into(),
    ] {
        accepts(&source);
    }
}

#[test]
fn non_exhaustive_matches_report_a_missing_value() {
    missing("match true with\n| true -> 1", "false");
    missing("match () with\n| _ when true -> 1", "_");
    missing("match 2 with | 1 -> 0", "_");
    missing(
        &format!("{MAYBE}def f :: Maybe<i64> -> i64\nfn f x = match x with | Some n -> n\nf None"),
        "None",
    );
    missing(
        &format!(
            "{SHAPE}def f :: Shape -> i64\nfn f s = match s with | Empty -> 0 | Circle _ -> 1\nf Empty"
        ),
        "Rect _",
    );
    missing(
        &format!(
            "{MAYBE}def f :: Maybe<Maybe<bool>> -> i64\nfn f x =\n    match x with\n    | None -> 0\n    | Some None -> 1\n    | Some (Some true) -> 2\nf None"
        ),
        "Some (Some false)",
    );
    missing(
        &format!("{MAYBE}match Some 42 with\n| Some n when n > 0 -> n\n| None -> -1"),
        "Some _",
    );
    missing("match [1] with\n| [] -> 0\n| [x] -> x", "_");
    missing("match [|1|] with\n| [||] -> 0\n| [|x|] -> x", "_ :: _ :: _");
    missing(
        "match [|true|] with\n| [||] -> 0\n| true :: _ -> 1",
        "false :: _",
    );
    missing(
        "def f :: (bool * bool) -> i64\nfn f x =\n    match x with\n    | (true, _) -> 1\n    | (_, true) -> 2\nf (false, false)",
        "(false, false)",
    );
    missing(
        "record R { ok: bool, value: i64 }\nmatch R { ok: true, value: 42 } with\n| { ok = true } -> 1",
        "{ ok = false }",
    );
    missing(
        "match (true, false) with\n| (true, _) & (_, true) -> 1\n| (false, _) -> 0",
        "(true, false)",
    );
    // Integer, floating-point, and string domains are never enumerated.
    missing("match 0i8u with\n| 0i8u -> 0\n| 1i8u -> 1", "_");
    missing("match 0.0 with\n| 0.0 -> 0", "_");
    missing("match \"a\" with\n| \"a\" -> 1", "_");
    // Partial recognizers, and total recognizers with a refutable payload,
    // never cover values.
    missing(&format!("{EVEN}match 42 with\n| Even -> 1"), "_");
    missing(
        "def (|IsBig|) :: i64 -> bool\nfn (|IsBig|) n = n > 10\nmatch 20 with\n| IsBig true -> 1\n| IsBig false -> 0",
        "_",
    );
}

#[test]
fn function_guards_are_checked_like_matches() {
    missing(
        "def f :: bool -> i64\nfn f x\n    | true -> 1\nf true",
        "false",
    );
    missing(
        "def f :: i64 -> i64\nfn f n\n    | n < 0 -> -1\n    | n > 0 -> 1\nf 1",
        "_",
    );
    let error = missing(
        "def f :: bool -> bool -> i64\nfn f a b\n    | (true, true) -> 1\n    | (false, _) -> 0\nf true false",
        "(true, false)",
    );
    assert_eq!(
        error.span.start,
        "def f :: bool -> bool -> i64\nfn f a b\n    ".len()
    );
}

#[test]
fn unreachable_arms_are_warnings_at_their_patterns() {
    assert_eq!(
        unreachable(&format!(
            "{MAYBE}match Some 1 with\n| _ -> 0\n| Some n -> n"
        )),
        ["Some n"]
    );
    assert_eq!(
        unreachable("match true with\n| true -> 1\n| true -> 2\n| false -> 0"),
        ["true"]
    );
    assert_eq!(
        unreachable(
            "let expensive = true\nmatch true with\n| true when expensive -> 1\n| true -> 2\n| false -> 0"
        ),
        [] as [&str; 0]
    );
    // A guarded arm is still reported when earlier arms cover its pattern.
    assert_eq!(
        unreachable("let ok = true\nmatch true with\n| _ -> 0\n| true when ok -> 1"),
        ["true"]
    );
    assert_eq!(
        unreachable(&format!("{EVEN}match 42 with\n| _ -> 0\n| Even -> 1")),
        ["Even"]
    );
    assert_eq!(
        unreachable(&format!(
            "{EVEN}match 42 with\n| Even -> 1\n| Even -> 2\n| _ -> 0"
        )),
        [] as [&str; 0]
    );
    assert_eq!(
        unreachable("match (true, false) with\n| (true, _) | (false, _) -> 1\n| (_, true) -> 2"),
        ["(_, true)"]
    );
    assert_eq!(
        unreachable("match [|1|] with\n| [||] -> 0\n| _ :: _ -> 1\n| [|x|] -> x"),
        ["[|x|]"]
    );
    assert_eq!(
        unreachable("match [1] with\n| [x] -> x\n| [_] -> 0\n| _ -> 1"),
        ["[_]"]
    );
    // Nested matches are checked too, and warnings follow source order.
    assert_eq!(
        unreachable(
            "match true with\n| _ ->\n    match false with\n    | _ -> 0\n    | false -> 1\n| true -> 2"
        ),
        ["false", "true"]
    );
    assert_eq!(
        unreachable(
            "match true with\n| _ -> 0\n| true ->\n    match false with\n    | _ -> 0\n    | false -> 1"
        ),
        ["true", "false"]
    );
}

#[test]
fn literal_keys_follow_runtime_equality() {
    for (source, expected) in [
        (
            "match 0.0 with\n| -0.0 -> 1\n| 0.0 -> 2\n| _ -> 3",
            vec!["0.0"],
        ),
        (
            "match 0.0f32 with\n| 0.0f32 -> 1\n| -0.0f32 -> 2\n| _ -> 3",
            vec!["-0.0f32"],
        ),
        (
            "match 1.5f16 with\n| 1.5f16 -> 1\n| 1.50f16 -> 2\n| -1.5f16 -> 3\n| _ -> 3",
            vec!["1.50f16"],
        ),
        (
            "match 0.0f128 with\n| -0.0f128 -> 1\n| 0.0f128 -> 2\n| _ -> 3",
            vec!["0.0f128"],
        ),
        (
            "match 0.1 with\n| 0.1 -> 1\n| 0.10000000000000001 -> 2\n| _ -> 3",
            vec!["0.10000000000000001"],
        ),
        (
            "match 0.1d128 + 0.2d128 with\n| 0.3d128 -> 1\n| 0.3000000000000000000000000000000000d128 -> 2\n| _ -> 0",
            vec!["0.3000000000000000000000000000000000d128"],
        ),
        (
            "match 0.0d64 with\n| -0.0d64 -> 1\n| 0.00d64 -> 2\n| _ -> 3",
            vec!["0.00d64"],
        ),
        (
            "match 1.0d32 with\n| 1.0d32 -> 1\n| 1.000d32 -> 2\n| -1.0d32 -> 3\n| _ -> 3",
            vec!["1.000d32"],
        ),
        (
            "match 7i8 with\n| 7i8 -> 1\n| -7i8 -> 2\n| 7i8 -> 3\n| _ -> 4",
            vec!["7i8"],
        ),
        (
            "match \"a\" with\n| \"a\" -> 1\n| \"b\" -> 2\n| \"a\" -> 3\n| _ -> 4",
            vec!["\"a\""],
        ),
        (
            "match 1i64u with\n| 1i64u -> 1\n| 2i64u -> 2\n| _ -> 3",
            vec![],
        ),
    ] {
        let warnings = unreachable(source);
        assert_eq!(warnings.len(), expected.len(), "{source}\n{warnings:?}");
        for (warning, expected) in warnings.iter().zip(&expected) {
            assert_eq!(warning, expected, "{source}");
        }
    }
    // Generic literals are keyed after their type is known.
    assert_eq!(
        unreachable("def f :: 'a -> i64\nfn f x = match x with | 0 -> 0 | -0 -> 1 | _ -> 2\nf 1"),
        ["-0"]
    );
}

#[test]
fn missing_cases_are_qualified_only_when_ambiguous() {
    let error = analyze_modules(&[
        ("Shapes.tz", "union Shape = Dot | Line"),
        ("Main.tz", "match Shapes.Dot with | Shapes.Dot -> 1"),
    ])
    .unwrap_err();
    assert_eq!(error.message, "match is not exhaustive; missing: Line");
    let error = analyze_modules(&[
        ("Shapes.tz", "union Shape = Dot | Line"),
        ("Other.tz", "union Other = Line | Point"),
        ("Main.tz", "match Shapes.Dot with | Shapes.Dot -> 1"),
    ])
    .unwrap_err();
    assert_eq!(
        error.message,
        "match is not exhaustive; missing: Shapes.Line"
    );
}

#[test]
fn oversized_matches_are_rejected() {
    let tuple = ["0"; 11].join(", ");
    let pattern = ["(0 | 1)"; 11].join(", ");
    let error = analyze(&format!(
        "match ({tuple}) with\n| ({pattern}) -> 1\n| _ -> 0"
    ))
    .unwrap_err();
    assert_eq!(error.code, "E1017");
    // Each row fixes one of the first columns and the last one, so the search
    // must split every first column before any row is decided.
    let columns = 25;
    let value = vec!["true"; columns].join(", ");
    let mut arms = String::new();
    for column in 0..columns - 1 {
        for constructor in ["true", "false"] {
            let mut row = vec!["_"; columns];
            row[column] = constructor;
            row[columns - 1] = "true";
            arms += &format!("| ({}) -> 0\n", row.join(", "));
        }
    }
    let mut row = vec!["_"; columns];
    row[columns - 1] = "false";
    arms += &format!("| ({}) -> 1\n", row.join(", "));
    let error = analyze(&format!("match ({value}) with\n{arms}")).unwrap_err();
    assert_eq!(
        (error.code, error.message.as_str()),
        (
            "E1017",
            "the match is too large to check for exhaustiveness; split the match"
        )
    );
}

#[test]
fn refutable_lambda_and_for_patterns_are_rejected_in_2027_and_warn_in_2026() {
    use tsuzuri::syntax::Edition;
    for (source, place, missing) in [
        (
            "let xs = [Maybe.Some 1, Maybe.None]\nArray.map (\\(Maybe.Some x) -> x) (ref xs)\n",
            "a lambda parameter",
            "None",
        ),
        (
            "let f: [i64] -> i64 = \\[a, b] -> a + b\nf [1, 2]\n",
            "a lambda parameter",
            "_",
        ),
        ("let f = \\0 -> 1\nf 0\n", "a lambda parameter", "_"),
        (
            "let f = \\x when x > 0 -> x\nf 1\n",
            "a lambda parameter",
            "_",
        ),
        (
            "for Maybe.Some _x in [Maybe.Some 1] do ()\n0\n",
            "a 'for' binding",
            "None",
        ),
        (
            "Maybe {\n    let! (Maybe.Some x) = Maybe.Some (Maybe.Some 1)\n    return x\n}\n",
            "a computation binding",
            "None",
        ),
        (
            "Maybe {\n    for Maybe.Some _x in [Maybe.Some 1] do ()\n    return 0\n}\n",
            "a computation binding",
            "None",
        ),
    ] {
        let error = tsuzuri::analyze_with_edition(source, Edition::E2027).unwrap_err();
        assert_eq!(error.code, "E1021", "{source}: {}", error.message);
        assert_eq!(error.kind, Some("refutable_binding_pattern"), "{source}");
        assert_eq!(
            error.message,
            format!("pattern in {place} is refutable; missing: {missing}"),
            "{source}"
        );

        let module = tsuzuri::analyze_with_edition(source, Edition::E2026).expect(source);
        assert_eq!(module.warnings.len(), 1, "{source}: {:?}", module.warnings);
        assert_eq!(module.warnings[0].code, "W1012", "{source}");
        assert_eq!(
            module.warnings[0].kind,
            Some("refutable_binding_pattern_warning"),
            "{source}"
        );
        assert_eq!(
            module.warnings[0].message,
            format!("pattern in {place} is refutable; missing: {missing}"),
            "{source}"
        );
    }

    // Irrefutable destructuring produces no error or warning in either edition.
    for source in [
        "record Point { x: i64, y: i64 }\nunion Boxed = Wrap of i64\n\
         let add = \\(x, y) -> x + y\n\
         let px = \\(Point { x: x }) -> x\n\
         let uid = \\(Wrap n) -> n\n\
         let mut total = 0\n\
         for (k, v) in [(1, 2), (3, 4)] do total = total + k + v\n\
         add (1, 2) + px (Point { x: 5, y: 6 }) + uid (Wrap 7) + total\n",
        "Maybe {\n    let! (a, b) = Maybe.Some (20, 22)\n    return a + b\n}\n",
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let module = tsuzuri::analyze_with_edition(source, edition).expect(source);
            assert!(
                module.warnings.is_empty(),
                "{edition}: {:?}",
                module.warnings
            );
        }
    }
}

#[test]
fn let_destructuring_accepts_irrefutable_patterns_and_rejects_refutable_ones() {
    use tsuzuri::syntax::{Edition, SourceKind};
    for edition in [Edition::E2026, Edition::E2027] {
        for source in [
            "let (a, b) = (20, 22)\na + b\n",
            "let (a, _): (i64 * string) = (42, \"ok\")\na\n",
            "record Point { x: i64, y: string }\nlet p = Point { x: 10, y: \"hello\" }\nlet Point { x: x, y: y } = p\nx + String.length (ref y)\n",
            "union Boxed = Wrap of i64\nlet Wrap n = Wrap 42\nn\n",
            "const def sum_pair :: (i64 * i64) -> i64\nfn sum_pair pair =\n    let (a, b) = pair\n    a + b\nconst TOTAL: i64 = sum_pair (20, 22)\nTOTAL\n",
            "def main :: unit -> i32 = \\() ->\n    let (a, b) = (20i32, 22i32)\n    do! IO.writeln (to_string (a + b))\n    0\n",
        ] {
            let module = tsuzuri::analyze_with_edition(source, edition).expect(source);
            let _ = tsuzuri::llvm::emit(&module, tsuzuri::llvm::Entry::Console).expect(source);
        }

        // Refutable `let` patterns are E1021 in every edition.
        for (source, missing) in [
            ("let Maybe.Some x = Maybe.Some 1\nx\n", "None"),
            ("let [a, b] = [1, 2]\na + b\n", "_"),
            ("let 0 = 0\n1\n", "_"),
        ] {
            let error = tsuzuri::analyze_with_edition(source, edition).unwrap_err();
            assert_eq!(error.code, "E1021", "{edition}: {source}");
            assert_eq!(error.kind, Some("refutable_binding_pattern"));
            assert_eq!(
                error.message,
                format!("pattern in a 'let' binding is refutable; missing: {missing}")
            );
        }

        // `let mut` stays restricted to a single identifier.
        let mut_err =
            tsuzuri::analyze_with_edition("let mut (a, b) = (1, 2)\na\n", edition).unwrap_err();
        assert_eq!(mut_err.code, "E0002");
    }

    // Partial move out of a non-Copy field works just like `match`.
    let partial_move = "record Pair { first: string, second: string }\n\
                        let p = Pair { first: \"a\", second: \"b\" }\n\
                        let Pair { first: first } = p\n\
                        String.length (ref p.first)\n";
    assert_eq!(
        tsuzuri::analyze_with_edition(partial_move, Edition::E2027)
            .unwrap_err()
            .code,
        "E1012"
    );

    // Formatting preserves `let <pattern> [: Type] = <expr>` without rewriting to `match`.
    let formatted_source = "let (a, b): (i64 * i64) = (20, 22)\na + b\n";
    let formatted =
        tsuzuri::formatter::format_source("Main.tz", formatted_source, SourceKind::Code).unwrap();
    assert_eq!(formatted.formatted, formatted_source);
    assert!(!formatted.changed);
}
