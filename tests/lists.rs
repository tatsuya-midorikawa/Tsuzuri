use tsuzuri::{analyze, analyze_modules, lexer, llvm, syntax::TokenKind};

fn accepts(source: &str) {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn list_delimiters_do_not_conflict_with_operators() {
    let tokens = lexer::lex("[||] [|1 | 2, true || false, 1 |> f|]").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::LeftList);
    assert_eq!(tokens[1].kind, TokenKind::RightList);
    for kind in [TokenKind::Pipe, TokenKind::OrOr, TokenKind::PipeForward] {
        assert!(tokens.iter().any(|token| token.kind == kind));
    }
    accepts("let a = [|1 | 2, 4,|]\na[0] + a[1]");
    accepts("let a = [|true || false, false && true|]\na[0]");
    accepts("def f :: i64 -> i64\nfn f x = x + 1\nlet a = [|1 |> f|]\na[0]");
}

#[test]
fn lists_support_inference_nesting_and_construction() {
    for source in [
        "def f :: [|i32|]\nfn f = [|1, 2, 3|]",
        "def f :: bool -> [|i32|]\nfn f b = if b { [||] } else { [|1, 2|] }",
        "record R { xs: [|i32|] }\ndef f :: R\nfn f = R { xs: [|1|] }",
        "def f :: [|[|i32|]|]\nfn f = [|[||], [|1|], [|2, 3|]|]",
        "def f :: [[|i32|]]\nfn f = [[||], [|1|]]",
        "def f :: [|[i32]|]\nfn f = [|[], [1, 2]|]",
        "def f :: i64 -> [|i32|]\nfn f n = new [|i32|](n, i -> i as i32)",
        "def make :: i64 -> (i64 -> 'a) -> [|'a|]\nfn make n f = new [|'a|](n, f)",
        "def f :: i64 -> [|i64 -> i64|]\nfn f n = new [|i64 -> i64|](n, i -> x -> i + x)",
        "def f :: i64 -> [|string|]\nfn f n = { let s = \"owned\"; new [|string|](n, i -> s) }",
        "def f :: i64 -> [|[|i32|]|]\nfn f n = new [|[|i32|]|](n, i -> new [|i32|](i, j -> j as i32))",
        "def size :: [|i64|] -> i64\nfn size xs = xs.length\nsize [|1|] + size new [|i64|](3, i -> i)",
        "def first :: &[|'a|] -> 'a\nfn first xs = xs[0]\nlet a = [|42|]\nfirst (&a)",
        "def f :: i64\nfn f = { let mut a = [|1|]; let b = a; a = [|2, 3|]; b[0] + a.length }",
        "def f :: &[|i64|] -> i64\nfn f xs = xs[0] + xs.length",
        "def f :: [|&mut i64 -> unit|]\nfn f = [|x -> { *x = 1; }|]",
    ] {
        accepts(source);
    }
    let values = vec!["1"; 1025].join(", ");
    accepts(&format!("def f :: [|i64|]\nfn f = [|{values}|]"));
    let module = analyze("def f :: [|i32|]\nfn f = [|1|]").unwrap();
    assert_eq!(
        module.functions[0]
            .signature
            .result
            .display(&module.types()),
        "[|i32|]"
    );
}

#[test]
fn lists_are_distinct_from_arrays_and_reject_invalid_types() {
    for (source, code) in [
        ("[||]", "E1004"),
        ("def f :: [|i64|]\nfn f = [1, 2]", "E1003"),
        ("def f :: [i64]\nfn f = [|1, 2|]", "E1003"),
        ("[|1, true|]", "E1003"),
        ("[|1][0]", "E0002"),
        ("[1|]", "E0002"),
        ("def f :: [|i64; 4|]\nfn f = [|1|]", "E0002"),
        ("new [|i64|](3)", "E0002"),
        ("new [|i64|](3, i -> i, 4)", "E0002"),
        ("new [|i64|](3i32, i -> i)", "E1003"),
        ("new [|i32|](3, i -> i)", "E1003"),
        ("new [|i64|](3, true)", "E1003"),
        ("new [|Missing|](3, i -> i)", "E1004"),
        ("export def f :: [|i32|] -> i64\nfn f a = a.length", "E1008"),
        ("record R { xs: [|R|] }", "E1010"),
    ] {
        rejects(source, code);
    }
}

#[test]
fn lists_are_deeply_immutable_but_bindings_can_be_replaced() {
    for (source, code) in [
        ("let mut a = [|1|]\na[0] = 2", "E1012"),
        ("let mut a = [|1|]\na.length = 2", "E1012"),
        ("let mut a = [|1|]\n&mut a[0]", "E1014"),
        ("let mut a = [|[|1|]|]\na[0][0] = 2", "E1012"),
        ("let mut a = [|[1]|]\n&mut a[0]", "E1014"),
        (
            "record R { x: i64 }\nlet mut a = [|R { x: 1 }|]\na[0].x = 2",
            "E1012",
        ),
        (
            "record R { x: i64 }\nlet mut a = [|R { x: 1 }|]\n&mut a[0].x",
            "E1014",
        ),
        (
            "def f :: &mut [|i64|] -> unit\nfn f a = { a[0] = 1; }",
            "E1012",
        ),
        (
            "def f :: &mut [|i64|] -> unit\nfn f a = { let r = &mut a[0]; *r = 1; }",
            "E1014",
        ),
        ("let mut n = 0\nlet a = [|&mut n|]\n*a[0] = 1", "E1005"),
        (
            "let mut n = 0\nlet r = &mut n\nlet a = [|&r|]\n**a[0] = 1",
            "E1005",
        ),
        (
            "def wrap :: 'a -> [|'a|]\nfn wrap x = [|x|]\nlet mut n = 0\nlet a = wrap (&mut n)\n*a[0] = 1",
            "E1005",
        ),
        ("def f :: [|&mut i64|] -> unit\nfn f a = {}", "E1005"),
        ("def f :: [|&&mut i64|] -> unit\nfn f a = {}", "E1005"),
        ("def f :: [|& &mut i64|] -> unit\nfn f a = {}", "E1005"),
        ("new [|[&mut i64]|](0, i -> [])", "E1005"),
        ("new [[|&mut i64|]](0, i -> [||])", "E1005"),
    ] {
        rejects(source, code);
    }
    accepts(
        "def replace :: &mut [|string|] -> unit
         fn replace a = { *a = new [|string|](3, i -> \"x\"); }
         def f :: i64
         fn f = { let mut a = [|\"old\"|]; replace (&mut a); a.length }",
    );
}

#[test]
fn lists_preserve_ownership_and_borrow_lifetimes() {
    accepts(
        "def refs :: &string -> i64 -> [|&string|]
         fn refs s n = new [|&string|](n, i -> s)
         def f :: i64
         fn f = { let s = \"hello\"; let a = refs (&s) 2; a[0].length }",
    );
    for (source, code) in [
        ("let a = [|\"x\"|]\nlet b = a\na.length", "E1012"),
        ("let a = [|\"x\"|]\na[0]", "E1012"),
        ("[|\"x\"|][0]", "E1012"),
        (
            "record R { s: string }\nlet a = [|R { s: \"x\" }|]\na[0].s",
            "E1012",
        ),
        (
            "let a = [|\"x\"|]\nlet r = &a[0]\nlet b = a\nr.length",
            "E1014",
        ),
        (
            "let mut a = [|1|]\nlet r = &a[0]\nlet _ = a = [|2|]\n*r",
            "E1014",
        ),
        ("let mut a = [|1|]\na[{ a = [|2, 3|]; 0 }]", "E1014"),
        (
            "def f :: [|&string|]\nfn f = { let s = \"x\"; let r = &s; new [|&string|](2, i -> r) }",
            "E1013",
        ),
        (
            "def f :: i64\nfn f = { let mut s = \"x\"; let r = &s; let a = new [|&string|](2, i -> r); let b = a; s = \"changed\"; b[0].length }",
            "E1014",
        ),
        (
            "def f :: i64\nfn f = { let mut s = \"x\"; let r = &s; let a = new [|unit -> i64|](2, i -> u -> r.length); let b = a; s = \"changed\"; b[0] () }",
            "E1014",
        ),
        (
            "def f :: [|unit -> i64|]\nfn f = { let s = \"x\"; let r = &s; [|u -> r.length|] }",
            "E1013",
        ),
        (
            "let mut n = 0\nlet r = &mut n\nlet a: [|unit -> unit|] = [|u -> { *r = 1; }|]\na[0] ()",
            "E1005",
        ),
    ] {
        rejects(source, code);
    }
}

#[test]
fn lists_specialize_across_modules_and_type_classes() {
    let module = analyze_modules(&[
        (
            "Lists",
            "def identity :: [|'a|] -> [|'a|]\nfn identity a = a
                   def generate :: i64 -> (i64 -> 'a) -> [|'a|]\nfn generate n f = new [|'a|](n, f)
                   class Size 'a { def size :: 'a -> i64 }
                   instance Size [|i32|] { fn size a = a.length }",
        ),
        (
            "Main",
            "let a = Lists.identity [|1i32|]
                  let b = Lists.identity [|2i32, 3|]
                  let c = Lists.generate 5 (i -> i as i32)
                  Lists.Size.size a + b.length + c.length",
        ),
    ])
    .unwrap();
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|f| f.module == "Lists" && f.name.starts_with("identity"))
            .count(),
        1
    );
    llvm::emit(&module, llvm::Entry::Console).unwrap();
}

#[test]
fn runtime_fixture_uses_linked_nodes_and_entry_allocas() {
    let module = analyze(include_str!("fixtures/lists/Lists.tz")).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("%tz.list = type { ptr, i64 }"));
        assert!(ir.contains("getelementptr inbounds { ptr, i64 }"));
        assert!(ir.contains("phi ptr"));
        for body in ir.split("define ").skip(1) {
            let body = body.split("\n}").next().unwrap();
            if let Some((_, after_loop)) = body.split_once("\nloop:\n") {
                assert!(!after_loop.contains(" alloca "), "{body}");
            }
        }
    }
}
