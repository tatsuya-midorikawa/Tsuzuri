use tsuzuri::{analyze, analyze_modules, llvm, parser};

fn accepts(source: &str) -> String {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!(
            "{source}\n{}: {} at {:?}",
            error.code, error.message, error.span
        )
    });
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    ir
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn break_continue_parse_keywords() {
    for jump in ["break", "continue"] {
        parser::parse(&format!("while true do {jump}")).unwrap();
        parser::parse(&format!(
            "for index = 0 to 3 do {{ if index == 1 then {jump} else () }}"
        ))
        .unwrap();
        let error = parser::parse(&format!("let {jump} = 1\n{jump}")).unwrap_err();
        assert_eq!(error.code, "E0002");
        assert!(parser::parse(&format!("record Record {{ {jump}: i64 }}")).is_err());
    }
}

#[test]
fn break_continue_type_contexts() {
    for jump in ["break", "continue"] {
        for source in [
            format!("while false do {jump}"),
            format!("for index = 0 to 1 do {jump}"),
            format!("for index in [1, 2] do {jump}"),
            format!("Option {{ let value = {{ while false do {jump}; 42 }}; return value }}"),
            format!("let work = task {{ while false do {jump}; return 42 }}\nTask.run work"),
            format!("let run = value -> {{ while false do {jump}; value }}\nrun 42"),
        ] {
            analyze(&source)
                .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
        }
        for source in [
            jump.to_owned(),
            format!("while true do {{ let run = value -> {jump}; run 0 }}"),
            format!("while true do {{ let work = task {{ {jump} }}; () }}"),
            format!("while true do {{ let value = Option {{ return {jump} }}; () }}"),
            format!("Option {{ while true do {jump} }}"),
        ] {
            rejects(&source, "E1023");
        }
        rejects(
            &format!("while true do {{ if true then {jump} else 1; }}"),
            "E1003",
        );
    }
}

#[test]
fn break_continue_ir_shapes() {
    for source in [
        "while true do break",
        "for index = 0 to 3 do continue",
        "for index = 3 downto 0 do if index == 1 then break else continue",
        "for index in 9223372036854775806 .. 9223372036854775807 do continue",
        "for index in -9223372036854775807 .. -1 .. -9223372036854775808 do continue",
        "for index in 0 .. 3 .. 10 do continue",
        "for index in [1, 2, 3] do if index == 1 then continue else break",
        "for text in [|\"first\", \"second\"|] do if text.length == 5 then continue else break",
        "for code in \"text\" do continue",
        "for outer = 0 to 2 do { while true do break; continue }",
        "for outer = 0 to 2 do match outer with | 0 -> continue | _ -> break",
    ] {
        let ir = accepts(source);
        assert!(ir.contains("br label"));
    }
}

#[test]
fn break_continue_ownership() {
    let consume = "def consume :: string -> unit\nfn consume text = ()\n";
    for body in [
        "let text = \"owned\"\nwhile true do { consume text; break }",
        "let text = \"owned\"\nfor index = 0 to 3 do { consume text; break }",
        "let text = \"owned\"\nwhile true do { if true then { consume text; break } else (); let size = text.length; }",
        "let text = \"owned\"\nwhile true do { match true with | true -> { consume text; break } | false -> (); let size = text.length; }",
        "let text = \"owned\"\nwhile true do { while true do break; consume text; break }",
        "let mut text = \"owned\"\nfor index = 0 to 3 do { consume text; text = \"next\"; continue }",
    ] {
        accepts(&format!("{consume}{body}"));
    }
    for body in [
        "let text = \"owned\"\nwhile true do { consume text; continue }",
        "let mut text = \"owned\"\nwhile true do { consume text; continue; text = \"next\"; }",
        "let mut text = \"owned\"\nwhile true do { consume text; break; text = \"next\"; }\ntext.length",
        "let text = \"owned\"\nwhile true do { consume text; break }\ntext.length",
    ] {
        rejects(&format!("{consume}{body}"), "E1012");
    }
    for jump in ["break", "continue"] {
        rejects(
            &format!(
                "let seed = 0\nlet mut borrowed = ref seed\nwhile true do {{ let local = 1; borrowed = ref local; {jump} }}\nderef borrowed"
            ),
            "E1013",
        );
        rejects(
            &format!(
                "let mut values = [\"first\", \"second\"]\nfor value in values do {{ values = [\"next\"]; {jump} }}"
            ),
            "E1014",
        );
    }
}

#[test]
fn conditional_keywords_and_optional_else() {
    for source in [
        "if true then 1 else 2",
        "if false then 1 elif true then 2 else 3",
        "if true then ()",
        "if true {}",
        "let mut n = 0\nif true then n = 42\nn",
        "def f :: bool -> bool -> i64\nfn f a b = if a then (if b then 1 else 2) else 3",
        "def f :: bool -> bool -> unit\nfn f a b =\n    if a then\n        if b then ()\n    else ()",
    ] {
        accepts(source);
    }
    rejects("if true then 1", "E1003");
    rejects("if 1 then ()", "E1003");
    rejects("if true then 1 else false", "E1003");
    rejects("if true then\nelse ()", "E0002");
    rejects("def f :: unit\nfn f =\n", "E0002");
    rejects("match 1 with | null -> 0", "E1020");
}

#[test]
fn typed_loops_and_layout_sequences() {
    for source in [
        "let mut n = 0\nwhile n < 42 do n = n + 1\nn",
        "let mut n = 0\nfor i = 1 to 6 do n = n + i as i64\nn",
        "let mut n = 0\nfor i = 6 downto 1 do n = n + i as i64\nn",
        "let mut n = 0\nfor i in 1 .. 2 .. 9 do n = n + i\nn",
        "let mut n = 0\nfor i in (9 .. -2 .. 1) do n = n + i\nn",
        "let mut n = 0\nfor i in [1, 2, 3] do n = n + i\nn",
        "let mut n = 0\nfor i in [|1, 2, 3|] do n = n + i\nn",
        "let mut n = 0\nfor b in \"abc\" do n = n + b as i64\nn",
        "let mut n = 0\nfor (x, y) in [(1, 2), (3, 4)] do n = n + x + y\nn",
        "let mut n = 0\nfor s in [\"abc\", \"def\"] do n = n + s.length\nn",
        "def f :: i64\nfn f =\n    let mut total = 0\n    for i = 1 to 4 do\n        for j = 1 to i do\n            total = total + j as i64\n    total",
        "def f :: unit\nfn f = { for _ in [1, 2] do (); while false do (); }",
    ] {
        accepts(source);
    }
    for source in [
        "for i = 1i64 to 2i64 do ()",
        "for i = 1 to 2 do i",
        "for i in [1, 2] do i",
        "while 1 do ()",
        "while true do 1",
    ] {
        rejects(source, "E1003");
    }
    rejects("for i in 2 do ()", "E1005");
    rejects("for i in 1..3 do i = 2", "E1014");
    rejects("for i = 1 to 3 do ()\ni", "E1002");
}

#[test]
fn matches_patterns_and_guards() {
    for source in [
        "match 2 with | 0 | 1 -> 10 | n when n > 0 -> n | otherwise -> -1",
        "match true with | true -> 1 | false -> 0",
        "match () with | () -> 42",
        "match -2i8 with | -2i8 -> 42 | _ -> 0",
        "match -0.0 with | 0.0 -> 42 | _ -> 0",
        "match \"abc\" with | \"xyz\" -> 0 | s when s.length == 3 -> s.length | _ -> 1",
        "match (1, 2) with | (x, y) & (_, 2) -> x + y | _ -> 0",
        "match (1, 2) with | ((x, _) | (_, x)) when x > 0 -> x | _ -> 0",
        "match (1, 2) with | (x, y) as pair -> x + y",
        "match [1, 2] with | [x; y] -> x + y | [] -> 0 | _ -> 1",
        "match [|1, 2, 3|] with | x :: y :: tail -> x + y + tail.length | _ -> 0",
        "record R { x: i64, y: string }\nmatch R { x: 1, y: \"text\" } with | { x = 1; y = s } -> s | _ -> \"none\"",
        "def first :: ('a * 'b) -> 'a\nfn first pair = match pair with | (x, _) -> x\nfirst (\"owned\", 2)",
        "def classify :: i64 -> i64\nfn classify n\n    | 0 -> 0\n    | x when x > 10 -> 2\n    | otherwise -> 1\nclassify 42",
        "def classify :: i64 -> i64\nfn classify n\n    | n < 0 -> -1\n    | n > 0 -> 1\n    | otherwise -> 0\nclassify 42",
        "def max :: i64 -> i64 -> i64\nfn max x y\n    | x > y -> x\n    | otherwise -> y\nmax 20 22",
        "def sum :: i64 -> i64 -> i64\nfn sum x y\n    | (a, b) when a > b -> a\n    | (a, b) -> b\nsum 20 22",
        "def f :: i64 -> i64\nfn f n =\n    match n with\n    | 0 ->\n        let x = 40\n        x + 2\n    | n -> n",
    ] {
        accepts(source);
    }
    rejects("match 1 with | true -> 0 | _ -> 1", "E1003");
    rejects("match 1 with | n when n -> 0 | _ -> 1", "E1003");
    rejects("match 1 with | 0 -> 1 | _ -> false", "E1003");
    rejects("match (1, 2) with | (x, x) -> x", "E1001");
    rejects("match (1, 2) with | (x, _) | (_, y) -> 0", "E1020");
    rejects("match (1, true) with | (x, _) | (_, x) -> x", "E1003");
    rejects("match 2 with | 1 -> 0", "E1021");
    rejects("match true with | true -> 1", "E1021");
    rejects(
        "def f :: bool -> i64\nfn f x\n    | true -> 1\nf true",
        "E1021",
    );
}

#[test]
fn match_origin_destructuring() {
    // Destructuring keeps its runtime trap instead of the exhaustiveness check.
    for source in [
        "for [_x] in [[1], [2, 3]] do ()",
        "(\\[x] -> x) [1, 2]",
        "union Maybe<'a> = None | Some of 'a\nlet f = \\(Some n) -> n\nf (Some 1)",
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\nfor Even in [2, 3] do ()",
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\n(\\Even -> 42) 3",
    ] {
        let ir = accepts(source);
        assert!(ir.contains("@llvm.trap"), "{source}");
        assert!(analyze(source).unwrap().warnings.is_empty(), "{source}");
    }
    let module = analyze_modules(&[
        (
            "Arrays.tc",
            "def For :: [[i64]] -> ([i64] -> i64) -> i64\nfn For values f = f values[0]\ndef Yield :: i64 -> i64\nfn Yield n = n",
        ),
        ("Main.tz", "Arrays { for [x] in [[42]] do { yield x } }"),
    ])
    .unwrap();
    assert!(module.warnings.is_empty());
    // A written match is checked even when it only destructures.
    rejects("match [1] with | [x] -> x", "E1021");
}

#[test]
fn active_option_payloads_use_fresh_types_and_existing_ownership() {
    for source in [
        "def (|Parsed|_|) :: ref string -> Option<i64>\nfn (|Parsed|_|) text = Parse.parse text\nmatch \"42\" with | Parsed number -> number | _ -> 0",
        "def (|Parts|_|) :: i64 -> Option<i64 * i64>\nfn (|Parts|_|) number = if number > 0 then Some (number, number + 1) else None\nmatch 20 with | Parts (left, right) -> left + right | _ -> 0",
        "def (|Divisible|_|) :: i64 -> i64 -> Option<unit>\nfn (|Divisible|_|) divisor number = if number % divisor == 0 then Some () else None\nmatch 42 with | Divisible (1 + 2) -> 1 | _ -> 0",
        "def (|Present|_|) :: 'a -> Option<'a>\nfn (|Present|_|) value = Some value\nlet first = match 1 with | Present number -> number | _ -> 0\nlet second = match true with | Present flag -> flag | _ -> false\nif second then first else 0",
    ] {
        accepts(source);
    }
    rejects(
        "def (|Parsed|_|) :: i64 -> Option<i64>\nfn (|Parsed|_|) value = Some value\nmatch 1 with | Parsed -> 1 | _ -> 0",
        "E1006",
    );
    rejects(
        "def (|Owned|_|) :: string -> Option<string>\nfn (|Owned|_|) value = Some value\nmatch \"text\" with | Owned value -> value.length | _ -> 0",
        "E1014",
    );
}

#[test]
fn active_multiple_cases_validate_backing_unions_and_payloads() {
    rejects(
        "record A { value: i64 }\nunion View = First | Second\ndef (|A|B|) :: i64 -> View\nfn (|A|B|) value = First\nmatch 1 with | A -> 1 | _ -> 0",
        "E1001",
    );
    accepts(
        "union Parity = IsEven | IsOdd\ndef (|Even|Odd|) :: i64 -> Parity\nfn (|Even|Odd|) number = if number % 2 == 0 then IsEven else IsOdd\nmatch 41 with | Even -> 0 | Odd -> 1 | _ -> -1",
    );
    accepts(
        "union View<'a> = First of 'a | Second of 'a\ndef (|Small|Large|) :: 'a -> View<'a>\nfn (|Small|Large|) value = First value\nlet first = match 2 with | Small number -> number | Large number -> number | _ -> 0\nlet second = match true with | Small flag -> flag | Large flag -> flag | _ -> false\nif second then first else 0",
    );
    for (source, code) in [
        (
            "def (|A|B|_|) :: i64 -> bool\nfn (|A|B|_|) value = true",
            "E1020",
        ),
        (
            "def (|a|B|) :: i64 -> bool\nfn (|a|B|) value = true",
            "E1020",
        ),
        (
            "def (|A|A|) :: i64 -> bool\nfn (|A|A|) value = true",
            "E1020",
        ),
        (
            "def (|A|B|) :: i64 -> bool\nfn (|A|B|) value = true",
            "E1003",
        ),
        (
            "union View = Only\ndef (|A|B|) :: i64 -> View\nfn (|A|B|) value = Only",
            "E1020",
        ),
        (
            "union View = Only\ndef (|A|B|) :: i64 -> View\nfn (|A|B|) value = Only\nmatch 1 with | B -> 1 | _ -> 0",
            "E1020",
        ),
        (
            "union View = A | B\ndef (|A|B|) :: i64 -> View\nfn (|A|B|) value = A",
            "E1001",
        ),
    ] {
        rejects(source, code);
    }
    rejects(
        "union View = First | Second\ndef (|A|B|) :: i64 -> View\nfn (|A|B|) value = First\nmatch 1 with | A -> 1 | B -> 2",
        "E1021",
    );
}

#[test]
fn active_aliases_follow_module_origin_and_report_ambiguity() {
    let recognizer = "def (|Identity|) :: i64 -> i64\nfn (|Identity|) value = value";
    let source = "match 42 with | Identity value -> value";
    analyze_modules(&[("Main.tz", source), ("First.tz", recognizer)]).unwrap();
    let error = analyze_modules(&[
        ("Main.tz", source),
        ("First.tz", recognizer),
        ("Second.tz", recognizer),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1004");
    analyze_modules(&[
        ("Main.tz", "match 42 with | First.Identity value -> value"),
        ("First.tz", recognizer),
        ("Second.tz", recognizer),
    ])
    .unwrap();
    tsuzuri::analyze_modules_with_std(
        &[("Main.tz", source), ("First.tz", recognizer)],
        &[("std/Option.tz", recognizer)],
    )
    .unwrap();
    let error = tsuzuri::analyze_modules_with_std(
        &[("Main.tz", "0"), ("First.tz", recognizer)],
        &[(
            "std/Option.tz",
            "def invoke :: i64\nfn invoke = match 42 with | Identity value -> value",
        )],
    )
    .unwrap_err();
    assert_eq!(error.code, "E1020");
}

#[test]
fn active_recognizers_are_typed_calls_with_ordered_patterns() {
    for source in [
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\nmatch 42 with | Even -> 1 | _ -> 0",
        "def (|Divisible|_|) :: i64 -> i64 -> bool\nfn (|Divisible|_|) d n = n % d == 0\nmatch 42 with | Divisible (1 + 2) -> 1 | _ -> 0",
        "def (|Parts|) :: i64 -> (i64 * i64)\nfn (|Parts|) n = (n, n + 1)\nmatch 20 with | Parts (a, b) when a < b -> a + b | _ -> 0",
        "def (|Length|) :: &string -> i64\nfn (|Length|) s = s.length\nmatch \"abc\" with | Length 3 -> 42 | _ -> 0",
        "def (|Copy|) :: &string -> string\nfn (|Copy|) s = clone_string s\nmatch \"abc\" with | Copy s when s.length == 9 -> s | Copy s -> s",
        "def (|Length|) :: &string -> i64\nfn (|Length|) s = s.length\nlet f = \\(Length n) -> n\nf \"abc\"",
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\nlet f = \\Even -> 42\nf 2",
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\nfor Even in [2, 4] do ()",
        "def (|Length|) :: &string -> i64\nfn (|Length|) s = s.length\nlet mut n = 0\nfor Length size in [\"a\", \"bc\"] do n = n + size\nn",
        "def positive :: i64 -> bool\nfn positive n = n > 0\ndef f :: i64 -> i64\nfn f n\n    | positive n -> 42\n    | otherwise -> 0\nf 1",
    ] {
        accepts(source);
    }
    rejects("def (|Even|_|) :: i64 -> i64\nfn (|Even|_|) n = n", "E1003");
    rejects(
        "def (|Bad|_|) :: &mut i64 -> bool\nfn (|Bad|_|) p = true\nlet mut n = 0\nmatch n with | Bad -> 1 | _ -> 0",
        "E1014",
    );
    rejects(
        "def (|Bad|_|) :: string -> bool\nfn (|Bad|_|) s = true\nmatch \"s\" with | Bad -> 1 | _ -> 0",
        "E1014",
    );
    rejects("match 1 with | Missing n -> n", "E1020");
    rejects(
        "def (|Copy|) :: &string -> string\nfn (|Copy|) s = clone_string s\nmatch \"x\" with | Copy s -> &s",
        "E1013",
    );
    analyze_modules(&[
        (
            "Checks.tz",
            "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0",
        ),
        ("Main.tz", "match 42 with | Checks.Even -> 42 | _ -> 0"),
    ])
    .unwrap();
}

#[test]
fn builder_control_keywords_and_patterns_use_normal_checks() {
    analyze_modules(&[
        ("Flow.tc", include_str!("fixtures/computations/Flow.tc")),
        (
            "Main.tz",
            "Flow { for (x, y) in [(20, 22)] do { yield x + y } }",
        ),
    ])
    .unwrap_err();
    analyze_modules(&[
        ("Tuple.tc", "def For :: [i64 * i64] -> ((i64 * i64) -> i64) -> i64\nfn For values f = f values[0]\ndef Yield :: i64 -> i64\nfn Yield n = n"),
        ("Main.tz", "Tuple { for (x, y) in [(20, 22)] do { yield x + y } }"),
    ]).unwrap();
    for source in [
        "Flow { for x in [20, 22] do yield x }",
        "Flow {\n    for x in [20, 22] do\n        if x > 0 then\n            yield x\n        else yield 0\n}",
        "Flow { if true then yield 42 else yield 0 }",
        "Flow { if false then yield 0 elif true then yield 42 else yield 0 }",
        "Flow { while false do yield 42 }",
    ] {
        analyze_modules(&[
            ("Flow.tc", include_str!("fixtures/computations/Flow.tc")),
            ("Main.tz", source),
        ])
        .unwrap_or_else(|error| panic!("{source}: {}", error.message));
    }
}

#[test]
fn backslash_lambdas_preserve_currying_patterns_and_capture_rules() {
    for source in [
        "let increment = \\value -> value + 1\nincrement 41",
        "let f = \\x -> \\y -> \\z -> x + y + z\nf 2 3 4",
        "let f = \\x y -> x + y\nf 20 22",
        "let n = 20\nlet f = \\x -> x + n\nf 22",
        "let f = \\(x: i32) y -> x + y\nf 20 22",
        "let f = \\(g: i64 -> i64) x -> g x\nf (\\n -> n + 1) 41",
        "let text = \"owned\"\nlet f: &string -> i64 = \\(r: &string) -> r.length\nf (&text)",
        "let text = \"owned\"\nlet f: &string -> &string = \\R -> R\n(f (&text)).length",
        "let f = \\(x, y) -> x + y\nf (20, 22)",
        "let f = \\() -> 42\nf ()",
        "let f = \\[x; y] -> x + y\nf [20, 22]",
        "def apply :: ('a -> 'b) -> 'a -> 'b\nfn apply f x = f x\napply (\\x -> x + 1) 41",
        "let increment = \\value ->\n    let step = 1\n    value + step\nincrement 41",
        "let increment = \\mut value -> { value = value + 1; value }\nincrement 41",
        "def apply :: (i64 -> i64) -> i64\nfn apply transform = transform 41\napply \\value -> value + 1",
    ] {
        accepts(source);
    }
    rejects("let f = \\x x -> x\nf 1 2", "E1001");
    rejects("let mut x = 0\nlet f = \\_ -> x = 1\nf ()", "E1014");
}

#[test]
fn rejects_malformed_backslash_lambdas() {
    for source in [
        "\\",
        "\\ -> 42",
        "\\value",
        "\\value ->",
        "\\value -> \\ -> value",
    ] {
        assert_eq!(parser::parse(source).unwrap_err().code, "E0002", "{source}");
    }
}

#[test]
fn fx_is_an_identifier_not_a_lambda_keyword() {
    accepts("let fx = \\value -> value + 1\nfx 41");
    assert!(tsuzuri::analyze("let increment = fx value -> value + 1\nincrement 41").is_err());
}

#[test]
fn recursion_is_explicit_including_mutual_and_function_values() {
    for source in [
        "def rec sum :: i64 -> i64 -> i64\nfn rec sum n acc = if n == 0 then acc else sum (n - 1) (acc + n)",
        "def rec even :: i64 -> bool\ndef and odd :: i64 -> bool\nfn rec even n = if n == 0 then true else odd (n - 1)\nand odd n = if n == 0 then false else even (n - 1)",
        "fn rec f(n: i64) -> i64 { if n == 0 { 0 } else { f(n - 1) } }",
        "def rec f :: i64 -> i64\nlet rec f = \\n -> if n == 0 then 0 else f (n - 1)",
        "def f :: i64 -> i64\nfn f x = g x\ndef g :: i64 -> i64\nfn g x = x",
    ] {
        accepts(source);
    }
    for source in [
        "def f :: i64 -> i64\nfn f x = f x",
        "fn f(x: i64) -> i64 { f(x) }",
        "def f :: i64 -> i64\nfn f x = { let self = f; self x }",
        "def f :: i64 -> i64\ndef g :: i64 -> i64\nfn f x = g x\nfn g x = f x",
        "def rec f :: i64 -> i64\nfn f x = x",
        "def f :: i64 -> i64\nfn rec f x = x",
        "def and f :: i64 -> i64\nand f x = x",
    ] {
        rejects(source, "E1019");
    }
    analyze_modules(&[
        (
            "A.tz",
            "def rec f :: i64 -> i64\nfn rec f n = if n == 0 then 0 else B.g (n - 1)",
        ),
        ("B.tz", "def rec g :: i64 -> i64\nfn rec g n = A.f n"),
    ])
    .unwrap();
}

#[test]
fn loops_and_guards_preserve_moves_loans_and_lifetimes() {
    accepts(
        "def consume :: string -> unit\nfn consume s = ()\nlet mut n = 0\nlet mut s = \"old\"\nwhile n < 3 do { consume s; s = \"new\"; n = n + 1; }\ns",
    );
    accepts("let mut n = 0\nwhile n < 3 do { let r = &n; let v = *r; n = v + 1; }\nn");
    accepts("let s = \"abc\"\nmatch s with | t when t.length == 3 -> t | _ -> \"none\"");
    rejects(
        "def consume :: string -> unit\nfn consume s = ()\nlet s = \"owned\"\nwhile true do consume s",
        "E1012",
    );
    rejects("let mut xs = [1, 2]\nfor x in xs do xs = [x]", "E1014");
    rejects(
        "def consume :: string -> bool\nfn consume s = true\nmatch \"owned\" with | s when consume s -> 1 | _ -> 0",
        "E1014",
    );
    rejects(
        "let mut n = 0\nlet mut r = &n\nfor i = 1 to 3 do r = &i\n*r",
        "E1003",
    );
    rejects(
        "let mut n = 0i32\nlet mut r = &n\nfor i = 1 to 3 do r = &i\n*r",
        "E1013",
    );
    rejects("let r = match (1, 2) with | (x, _) -> &x\n*r", "E1013");
    rejects(
        "let xs = [\"a\"]\nmatch xs with | [x] -> x | _ -> \"b\"",
        "E1012",
    );
}

#[test]
fn control_lowering_is_direct_and_tail_calls_stay_loops() {
    let ir =
        accepts("def rec f :: i64 -> i64\nfn rec f n = match n with | 0 -> 42 | _ -> f (n - 1)");
    assert!(ir.contains("switch i64"));
    let body = ir
        .split("define internal i64 @tz.fn.Main.f(")
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap();
    assert!(!body.contains("call i64 @tz.fn.Main.f("));
    let ir =
        accepts("def f :: i64 -> i64\nfn f n = { let mut x = 0; while x < n do x = x + 1; x }");
    assert!(!ir.contains("@tz.alloc"));
    assert!(!ir.contains("call i64 %"));
    let ir = accepts(
        "def f :: &[|i64|] -> i64\nfn f xs = { let mut x = 0; for n in xs do x = x + n; x }",
    );
    assert!(!ir.contains("@tz.alloc"));
    assert!(ir.contains("phi ptr"));
    for add in ["total + i", "Add.add total i"] {
        let ir = accepts(&format!(
            "def f :: i64 -> i64\nfn f n = {{ let mut total = 0; for i in 0..n do total = {add}; total }}"
        ));
        assert!(ir.contains("llvm.loop.unroll.enable"), "{add}");
    }
    let ir = accepts(
        "def f :: i64 -> i64\nfn f n = { let mut i = 0; let mut state = 1; while i < n do { state = (state ^ (state >>> 13)) * 17; i = i + 1; }; state }",
    );
    assert!(!ir.contains("llvm.loop.unroll.enable"));
}

#[test]
fn control_syntax_has_bounded_depth() {
    for (prefix, suffix) in [
        ("while true do (", ")"),
        ("for x in 1..2 do (", ")"),
        ("if true then (", ") else ()"),
        ("\\x -> (", ")"),
        ("match 1 with | _ -> (", ")"),
    ] {
        assert!(parser::parse(&format!("{}(){}", prefix.repeat(200), suffix.repeat(200))).is_err());
    }

    assert!(
        parser::parse(&format!(
            "match 1 with | {}x{} -> 0",
            "(".repeat(200),
            ")".repeat(200)
        ))
        .is_err()
    );
}

#[test]
fn match_place_evaluation_is_not_duplicated_by_ownership() {
    accepts("let xs = [42]\nlet text = \"owned\"\nmatch xs[{ let moved = text; 0 }] with | n -> n");
    accepts("let text = \"a\"\nmatch text[0] with | 97i16u -> 42 | _ -> 0");
    accepts("let text = u8\"a\"\nmatch text[0] with | 97ubyte -> 42 | _ -> 0");
    accepts("let text = \"owned\"\nlet r = match &text with | r -> r\nr.length");
    accepts(
        "def f :: string -> i64 -> string\nfn f text n\n    | n > 0 -> text\n    | otherwise -> text\nf \"owned\" 1",
    );
    accepts("def id :: i64 -> i64\nfn id x = x\nmatch 1 with | _ -> id(1 | 2)");
    accepts("match 1 with | _ -> { let xs = [1 | 2]; xs[0 | 0] }");
    accepts(
        "def apply :: (i64 -> bool) -> bool\nfn apply f = f 1\nmatch 1 with | _ when apply(x -> x > 0) -> 42 | _ -> 0",
    );
}

#[test]
fn recursion_checks_include_class_methods_and_operators() {
    rejects(
        "class C<'a> { def f :: 'a -> i64 }\ninstance C<i64> { fn f n = C.f n }",
        "E1019",
    );
    rejects(
        "record R { x: i64 }\ninstance Add<R> { fn add x y = x + y }",
        "E1019",
    );
    accepts(
        "class C<'a> { def f :: 'a -> i64 }\ninstance C<i64> { fn rec f n = if n == 0 then 0 else C.f (n - 1) }\nC.f 4",
    );
}
