use tsuzuri::{analyze, analyze_modules, llvm};

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
fn array_types_do_not_depend_on_length() {
    for source in [
        "def choose :: bool -> [i32]\nfn choose b = if b { [] } else { [1, 2, 3] }",
        "def size :: [i64] -> i64\nfn size xs = xs.length\nsize [1] + size [2, 3]",
        "record R { xs: [i32] }\ndef f :: bool -> R\nfn f b = R { xs: if b { [1] } else { [2, 3] } }",
        "def f :: [[i32]]\nfn f = [[], [1], [2, 3]]",
        "def f :: i64 -> [i32]\nfn f n = new [i32](n, i -> i as i32)",
        "def make :: i64 -> (i64 -> 'a) -> ['a]\nfn make n f = new ['a](n, f)",
        "def f :: i64 -> [i64 -> i64]\nfn f n = new [i64 -> i64](n, i -> x -> i + x)",
        "def f :: i64 -> [string]\nfn f n = { let s = \"owned\"; new [string](n, i -> s) }",
        "def f :: i64 -> [[i32]]\nfn f n = new [[i32]](n, i -> new [i32](i, j -> j as i32))",
        "def size :: [i64] -> i64\nfn size xs = xs.length\nsize new [i64](3, i -> i)",
        "def f :: i64\nfn f = { let mut a = [1]; let b = a; a = [2, 3]; b[0] + a.length }",
        "def f :: &[i64] -> i64\nfn f xs = xs[0] + xs.length",
    ] {
        accepts(source);
    }
    let values = vec!["1"; 1025].join(", ");
    accepts(&format!("def f :: [i64]\nfn f = [{values}]"));
}

#[test]
fn array_constructors_require_a_length_and_typed_initializer() {
    for (source, code) in [
        ("def f :: [i64; 4]\nfn f = [1, 2, 3, 4]", "E0002"),
        ("record R { xs: [i64; 0] }", "E0002"),
        ("let xs: [i64; 1] = [1]", "E0002"),
        ("new i64(3, i -> i)", "E0002"),
        ("new [i64](3)", "E0002"),
        ("new [i64](3, i -> i, 4)", "E0002"),
        ("new [i64](3i32, i -> i)", "E1003"),
        ("new [i64](true, i -> i)", "E1003"),
        ("new [i32](3, i -> i)", "E1003"),
        ("new [i64](3, 0)", "E1003"),
        ("new [i64](3, i -> true)", "E1003"),
        ("new [Missing](3, i -> i)", "E1004"),
        ("def f :: [i32]\nfn f = new [i64](3, i -> i)", "E1003"),
        ("export def f :: [i32] -> i64\nfn f a = a.length", "E1008"),
    ] {
        rejects(source, code);
    }
}

#[test]
fn arrays_remain_immutable_and_owned() {
    for (source, code) in [
        ("let mut a = new [i64](3, i -> i)\na[0] = 1", "E1012"),
        ("let mut a = new [i64](3, i -> i)\na.length = 1", "E1012"),
        ("let mut a = new [i64](3, i -> i)\n&mut a[0]", "E1014"),
        (
            "def f :: &mut [i64] -> unit\nfn f a = { a[0] = 1; }",
            "E1012",
        ),
        (
            "let a = new [string](3, i -> \"x\")\nlet b = a\na.length",
            "E1012",
        ),
        ("let a = new [string](3, i -> \"x\")\na[0]", "E1012"),
        ("new [string](3, i -> \"x\")[0]", "E1012"),
        (
            "let a = new [string](3, i -> \"x\")\nlet r = &a[0]\nlet b = a\nr.length",
            "E1014",
        ),
        (
            "def f :: i64\nfn f = { let mut a = [1]; let r = &a[0]; a = [2, 3]; *r }",
            "E1014",
        ),
        ("let mut a = [1]\na[{ a = [2, 3]; 0 }]", "E1014"),
    ] {
        rejects(source, code);
    }
    accepts(
        "def replace :: &mut [string] -> unit
         fn replace a = { *a = new [string](3, i -> \"x\"); }
         def f :: i64
         fn f = { let mut a = [\"old\"]; replace (&mut a); a.length }",
    );
}

#[test]
fn generated_arrays_preserve_borrow_lifetimes() {
    accepts(
        "def refs :: &string -> i64 -> [&string]
         fn refs s n = new [&string](n, i -> s)
         def f :: i64
         fn f = { let s = \"hello\"; let a = refs (&s) 2; a[0].length }",
    );
    for (source, code) in [
        (
            "def f :: [&string]\nfn f = { let s = \"x\"; let r = &s; new [&string](2, i -> r) }",
            "E1013",
        ),
        (
            "def f :: i64\nfn f = {
                let mut s = \"x\"; let r = &s;
                let a = new [&string](2, i -> r); let b = a;
                s = \"changed\"; b[0].length
             }",
            "E1014",
        ),
        (
            "def f :: i64\nfn f = {
                let mut s = \"x\"; let r = &s;
                let a = new [unit -> i64](2, i -> u -> r.length);
                let b = a; s = \"changed\"; b[0] ()
             }",
            "E1014",
        ),
        (
            "def f :: i64\nfn f = { let mut n = 0; let r = &mut n; let a = new [&mut i64](2, i -> r); a.length }",
            "E1005",
        ),
    ] {
        rejects(source, code);
    }
}

#[test]
fn arrays_cannot_store_mutable_references() {
    for source in [
        "let mut n = 0\nlet a = [&mut n]\n*a[0] = 1",
        "let mut n = 0\nlet r = &mut n\nlet a = [&r]\n**a[0] = 1",
        "def wrap :: 'a -> ['a]\nfn wrap x = [x]\nlet mut n = 0\nlet a = wrap (&mut n)\n*a[0] = 1",
        "def f :: [&mut i64] -> unit\nfn f a = { *a[0] = 1; }",
        "def f :: [& &mut i64] -> unit\nfn f a = {}",
        "def f :: [[&mut i64]] -> unit\nfn f a = {}",
        "def f :: i64 -> [&mut i64]\nfn f n = new [&mut i64](n, i -> { let mut x = i; &mut x })",
    ] {
        rejects(source, "E1005");
    }
    accepts("def f :: [&mut i64 -> unit]\nfn f = [x -> { *x = 1; }]");
    for source in [
        "let mut a = [[1]]\na[0][0] = 2",
        "record R { x: i64 }\nlet mut a = [R { x: 1 }]\na[0].x = 2",
    ] {
        rejects(source, "E1012");
    }
}

#[test]
fn dynamic_arrays_cross_modules_and_specialize_only_on_element_types() {
    let module = analyze_modules(&[
        (
            "Arrays",
            "def identity :: ['a] -> ['a]\nfn identity a = a
             def generate :: i64 -> (i64 -> 'a) -> ['a]\nfn generate n f = new ['a](n, f)",
        ),
        (
            "Main",
            "let a = Arrays.identity [1i32]
             let b = Arrays.identity [2i32, 3]
             let c = Arrays.identity (Arrays.generate 5 (i -> i as i32))
             a.length + b.length + c.length",
        ),
    ])
    .unwrap();
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|function| function.module == "Arrays" && function.name.starts_with("identity"))
            .count(),
        1
    );
    llvm::emit(&module, llvm::Entry::Console).unwrap();
}

#[test]
fn runtime_fixture_lowers_for_both_targets() {
    let module = analyze(include_str!("fixtures/arrays/Arrays.tz")).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("%tz.array = type { ptr, i64 }"));
        for body in ir.split("define ").skip(1) {
            let body = body.split("\n}").next().unwrap();
            if let Some((_, after_loop)) = body.split_once("\nloop:\n") {
                assert!(!after_loop.contains(" alloca "), "{body}");
            }
        }
    }
}
