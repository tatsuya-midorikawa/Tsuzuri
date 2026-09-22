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
}

#[test]
fn active_recognizers_are_typed_calls_with_ordered_patterns() {
    for source in [
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\nmatch 42 with | Even -> 1 | _ -> 0",
        "def (|Divisible|_|) :: i64 -> i64 -> bool\nfn (|Divisible|_|) d n = n % d == 0\nmatch 42 with | Divisible (1 + 2) -> 1 | _ -> 0",
        "def (|Parts|) :: i64 -> (i64 * i64)\nfn (|Parts|) n = (n, n + 1)\nmatch 20 with | Parts (a, b) when a < b -> a + b | _ -> 0",
        "def (|Length|) :: &string -> i64\nfn (|Length|) s = s.length\nmatch \"abc\" with | Length 3 -> 42 | _ -> 0",
        "def (|Copy|) :: &string -> string\nfn (|Copy|) s = clone_string s\nmatch \"abc\" with | Copy s when s.length == 9 -> s | Copy s -> s",
        "def (|Length|) :: &string -> i64\nfn (|Length|) s = s.length\nlet f = fx (Length n) -> n\nf \"abc\"",
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\nlet f = fx Even -> 42\nf 2",
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
fn fx_preserves_currying_patterns_and_capture_rules() {
    for source in [
        "let f = fx x y -> x + y\nf 20 22",
        "let n = 20\nlet f = fx x -> x + n\nf 22",
        "let f = fx (x: i32) y -> x + y\nf 20 22",
        "let f = fx (g: i64 -> i64) x -> g x\nf (fx n -> n + 1) 41",
        "let text = \"owned\"\nlet f: &string -> i64 = fx (r: &string) -> r.length\nf (&text)",
        "let text = \"owned\"\nlet f: &string -> &string = fx R -> R\n(f (&text)).length",
        "let f = fx (x, y) -> x + y\nf (20, 22)",
        "let f = fx () -> 42\nf ()",
        "let f = fx [x; y] -> x + y\nf [20, 22]",
        "def apply :: ('a -> 'b) -> 'a -> 'b\nfn apply f x = f x\napply (fx x -> x + 1) 41",
    ] {
        accepts(source);
    }
    rejects("let f = fx x x -> x\nf 1 2", "E1001");
    rejects("let mut x = 0\nlet f = fx _ -> x = 1\nf ()", "E1014");
}

#[test]
fn recursion_is_explicit_including_mutual_and_function_values() {
    for source in [
        "def rec sum :: i64 -> i64 -> i64\nfn rec sum n acc = if n == 0 then acc else sum (n - 1) (acc + n)",
        "def rec even :: i64 -> bool\ndef and odd :: i64 -> bool\nfn rec even n = if n == 0 then true else odd (n - 1)\nand odd n = if n == 0 then false else even (n - 1)",
        "fn rec f(n: i64) -> i64 { if n == 0 { 0 } else { f(n - 1) } }",
        "def rec f :: i64 -> i64\nlet rec f = fx n -> if n == 0 then 0 else f (n - 1)",
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
        ("fx x -> (", ")"),
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
    accepts("let text = \"a\"\nmatch text[0] with | 97ubyte -> 42 | _ -> 0");
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
        "class C 'a { def f :: 'a -> i64 }\ninstance C i64 { fn f n = C.f n }",
        "E1019",
    );
    rejects(
        "record R { x: i64 }\ninstance Add R { fn add x y = x + y }",
        "E1019",
    );
    accepts(
        "class C 'a { def f :: 'a -> i64 }\ninstance C i64 { fn rec f n = if n == 0 then 0 else C.f (n - 1) }\nC.f 4",
    );
}
