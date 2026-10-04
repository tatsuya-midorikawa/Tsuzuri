//! The language changes of `_specs/`: literals, operators, `@checked`,
//! `try ... with ... finally`, IO direct style, and callback-first std functions.
//! Runtime results on native and WASM at -O0/-O3 are in `tests/features.mjs`
//! (suite `exceptions`).
use tsuzuri::check::{Type, TypedExprKind};
use tsuzuri::{analyze, llvm};

fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    ir
}

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.message
}

fn entry_type(source: &str) -> Type {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    module.functions[module.entry.unwrap()]
        .signature
        .result
        .clone()
}

#[test]
fn literal_suffixes_and_defaults_follow_the_spec() {
    for (source, ty) in [
        ("86y", Type::Integer(8, true)),
        ("86uy", Type::Integer(8, false)),
        ("86s", Type::Integer(16, true)),
        ("86us", Type::Integer(16, false)),
        ("86", Type::Integer(32, true)),
        ("86u", Type::Integer(32, false)),
        ("86l", Type::Integer(64, true)),
        ("86ul", Type::Integer(64, false)),
        ("86L", Type::Integer(128, true)),
        ("86UL", Type::Integer(128, false)),
        ("4.14hf", Type::Binary(16)),
        ("4.14f", Type::Binary(32)),
        ("4.14", Type::Binary(64)),
        ("4.14F", Type::Binary(128)),
        ("0.7833hm", Type::Decimal(32)),
        ("0.7833m", Type::Decimal(64)),
        ("0.7833M", Type::Decimal(128)),
        ("'a'B", Type::Integer(8, false)),
        ("let x: byte = 1\nx", Type::Integer(8, false)),
        ("let x: sbyte = 1\nx", Type::Integer(8, true)),
        // Literals take the type their uses require, like Rust.
        ("let x = 1\nlet y: i64 = x\ny", Type::I64),
        ("let r: f64 = 2\nr", Type::F64),
        ("let r: d64 = -2\nr", Type::Decimal(64)),
    ] {
        assert_eq!(entry_type(source), ty, "{source}");
    }
    // Top-level code shows these arrays through Display, so check them through annotations.
    accepts(
        "let bytes: [i8u] = \"ab\"B\nlet scalars: [utf8char] = u8\"é\"B\nbytes.length + scalars.length",
    );
    // A literal outside i32 needs a suffix or a wider use.
    rejects("let big = 3000000000\nbig", "E1009");
    entry_type("let big = 3000000000\nlet wide: i64 = big\nwide");
    for source in ["0x10hf", "0b1m", "1.5I", "'é'B", "\"é\"B", "$\"x\"B"] {
        assert_eq!(
            tsuzuri::parser::parse(source).unwrap_err().code,
            "E0001",
            "{source}"
        );
    }
    // An integer literal used where another type is required reports a mismatch.
    rejects("let xs = [1, true]\n0", "E1003");
}

#[test]
fn bigint_literals_and_operators_are_typed() {
    let module = analyze(
        "let a = 9999999999999999999999999999I\nlet b: bigint = 2\nlet c = -a * b ** 3I % 7I\nc < a && c != b",
    )
    .unwrap();
    assert_eq!(
        module.functions[module.entry.unwrap()].signature.result,
        Type::Bool
    );
    rejects("let a = 1I + 1\nlet b: i64 = a\nb", "E1003");
    // The representation is opaque, so every value keeps canonical digits.
    for source in [
        "let value = BigInt.BigInt { negative: true, limbs: [] }\n0",
        "let value = 5I\nvalue.limbs.length",
        "let value = 5I\nlet other = { value with negative = true }\n0",
        "match 5I with\n| BigInt.BigInt { negative = sign } -> 0",
    ] {
        rejects(source, "E1022");
    }
}

#[test]
fn operators_have_fsharp_forms_and_precedence() {
    for source in [
        "let a = +1\nlet b = -a\na + b",
        "2 ** 3 ** 2 == 512 && 1 + 2 * 3 ** 2 == 19",
        "((0b1100 &&& 0b1010 ||| 1 <<< 4) ^^^ ~~~0) == ((8 | 16) ^ ~0)",
        "-16 >>> 2 == -4 && Bits.ushr (-16) 30 == 3",
        "let f = (\\x -> x + 1) >> (\\x -> x * 2)\nlet g = (\\x -> x + 1) << (\\x -> x * 2)\nf 1 + g 1",
        "not true || (false |> not)",
        "ignore \"dropped\"\n1",
        "let total = 1\n         |> (\\x -> x + 1)\n         >> (\\x -> x * 2)\ntotal",
        "2.0 ** 0.5 > 1.4 && 2.0f ** 2.0f == 4.0f",
    ] {
        accepts(source);
    }
    rejects("\"a\" ** \"b\"", "E1005");
    rejects("+\"text\"", "E1005");
    // `not` is a builtin name, like `assert`.
    rejects("def not :: bool -> bool = \\x -> x\n0", "E1001");
}

#[test]
fn power_lowers_without_generic_dispatch() {
    let ir = accepts("export def p :: i64 -> i64 -> i64 = \\a b -> a ** b");
    let body = ir.split("@tz.fn.Main.p(").nth(1).unwrap();
    assert!(body.contains("mul i64"), "{body}");
    let ir = accepts("export def p :: f64 -> f64 -> f64 = \\a b -> a ** b");
    assert!(ir.contains("call double @tz_math_pow_f64"), "{ir}");
    let ir = accepts("export def p :: i64 -> i64 -> i64 = \\a b -> @checked a ** b");
    assert!(ir.contains("@llvm.smul.with.overflow.i64"), "{ir}");
    // An integer literal is not a float unless a float type is expected.
    rejects("let x = 2 ** 0.5\nx", "E1005");
}

#[test]
fn try_with_finally_returns_result_and_checks_its_rules() {
    let module = analyze(
        "def f :: i32 -> Result<i32, Exception> = \\x ->\n    try\n        @checked x + 1\n    with\n    | e is OverflowException -> e\n    finally\n        ()\nf 1",
    )
    .unwrap();
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .unwrap();
    assert!(matches!(function.signature.result, Type::Union(..)));
    // The entry drops the `Result`, which has no Display instance.
    assert_eq!(
        module.functions[module.entry.unwrap()].signature.result,
        Type::Unit
    );
    // A result-only `'E` is the caught exception, with or without `@'E : Err`.
    for constraint in ["\n    @'E : Err", ""] {
        accepts(&format!(
            "def f :: i32 -> Result<i32, 'E>{constraint} = \\x ->\n    try\n        @checked x * x\n    with\n    | e -> e\nf 3"
        ));
    }
    // The handler's value must implement Err.
    rejects(
        "def f :: i32 -> i32 = \\x ->\n    match try x with | _ -> 0 with\n    | Ok v -> v\n    | Error _ -> 0\nf 1",
        "E1005",
    );
    // break and continue cannot leave a try whose finally must run first.
    rejects(
        "while true do\n    let r = try break with | e -> e finally ()\n    ()\n0",
        "E1023",
    );
    accepts("while true do\n    let r = try 1 with | e -> e\n    break\n0");
    // Handler arms need not be exhaustive: an unmatched exception propagates.
    accepts(
        "def f :: i64 -> Result<i64, Exception> = \\x ->\n    try @checked x * x with | e when x < 0 -> e\nf 2",
    );
    rejects("try 1 finally ()", "E0002");
}

#[test]
fn checked_arithmetic_raises_or_traps_by_scope() {
    let ir = accepts("export def f :: i64 -> i64 = \\x -> @checked x + 1");
    assert!(ir.contains("@llvm.sadd.with.overflow.i64"), "{ir}");
    assert!(ir.contains("call void @llvm.trap()"), "{ir}");
    let ir = accepts(
        "export def f :: i64 -> i64 = \\x ->\n    match try (@checked x - 1) with | e -> e with\n    | Ok v -> v\n    | Error _ -> 0",
    );
    let body = ir.split("@tz.fn.Main.f(").nth(1).unwrap();
    assert!(body.contains("@llvm.ssub.with.overflow.i64"), "{body}");
    // Unchecked arithmetic still wraps without overflow intrinsics.
    let ir = accepts("export def f :: i64 -> i64 = \\x -> x * 3");
    assert!(!ir.contains("with.overflow"), "{ir}");
    // Floats are unchanged by @checked.
    accepts("export def f :: f64 -> f64 = \\x -> @checked x * x");
}

#[test]
fn entry_values_print_through_display_or_are_dropped() {
    assert_eq!(entry_type("[1, 2]"), Type::String);
    assert_eq!(entry_type("record R { x: i64 }\nR { x: 1 }"), Type::Unit);
    assert_eq!(entry_type("Some 1"), Type::Unit);
    // Top-level IO binds followed by a plain value run directly.
    assert_eq!(
        entry_type("do! IO.writeln \"hi\" |> ignore\n42"),
        Type::Integer(32, true)
    );
    // Ending in `do!` still builds the IO action that the entry runs.
    assert!(matches!(
        entry_type("do! IO.writeln \"hi\""),
        Type::Record(..)
    ));
}

#[test]
fn functions_accept_spec_signatures_guards_and_constraints() {
    for source in [
        "def add : i32 -> i32 -> i32 = \\x y -> x + y\nadd 1 2",
        "@literal\ndef PI : f64 = 3.14\nlet r: f64 = 2\nPI * (r ** 2)",
        "let positive = \\x when x > 0 -> x * 2\npositive 3",
        "def run :: i32 =\n    do! IO.writeln \"Hello\"\n    |> ignore\n    0\nrun",
    ] {
        accepts(source);
    }
    let module = analyze(
        "record Foo { num: i32 }\ndef value :: Foo -> i32 = \\x -> x.num\ndef add :: 'T -> 'T -> 'U\n    @'T : (#value: 'T -> 'U) = \\x -> \\y ->\n        ('T.value x) + ('T.value y)\nadd (Foo { num: 10 }) (Foo { num: 20 })",
    )
    .unwrap();
    assert_eq!(
        module.functions[module.entry.unwrap()].signature.result,
        Type::Integer(32, true)
    );
    // The annotation must match the module function.
    rejects(
        "record Foo { num: i32 }\ndef value :: Foo -> i32 = \\x -> x.num\ndef get :: 'T -> string\n    @'T : (#value: 'T -> string) = \\x -> 'T.value x\nget (Foo { num: 1 })",
        "E1003",
    );
}

#[test]
fn std_higher_order_functions_take_the_callback_first() {
    for source in [
        "let values = [3, 5, 8]\nvalues |> Array.map (\\x -> x * 2) |> Array.sum",
        "let values = [3, 5, 8]\nvalues |> (Array.map (\\x -> x * 2) >> Array.sum)",
        "let texts = [\"a\", \"bb\"]\ntexts |> Array.map_ref (\\t -> t.length) |> Array.sum",
        "let texts = [\"a\", \"bb\"]\nArray.fold_ref (\\total t -> total + t.length) 0 (ref texts)",
        "let xs = [1, 2, 3]\nArray.fold_back (\\x state -> state * 10 + x) (ref xs) 0",
        "let xs = [|1, 2, 3|]\nList.fold (\\s x -> s + x) 0 (ref xs)",
        "Seq.unfold (\\n -> if n < 3 then Option.Some (n, n + 1) else Option.None) 0 |> Seq.map (\\n -> n * 2) |> Seq.to_array",
        "let set = Set.singleton \"x\"\nSet.fold (\\total key -> total + key.length) 0 (ref set)",
    ] {
        accepts(source);
    }
    // A temporary passed where a shared reference is expected lives until the call returns.
    let ir = accepts("Array.sum (Array.map (\\x -> x + 1) (ref [1l, 2]))");
    assert!(ir.contains("@tz.fn.Array.sum"), "{ir}");
    rejects(
        "let values = [1, 2]\nlet first = Array.find (\\x -> deref x > 0) (Array.copy (ref values))\n0",
        "E1013",
    );
}

#[test]
fn not_and_full_applications_lower_to_the_operator() {
    let module = analyze("export def f :: bool -> bool = \\x -> not x").unwrap();
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .unwrap();
    assert!(matches!(function.body.kind, TypedExprKind::Unary(..)));
}
