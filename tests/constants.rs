use tsuzuri::{analyze, check::TypedExprKind, llvm};
use tsuzuri::{
    analyze_modules, formatter, parser,
    syntax::{SourceKind, Visibility},
};

#[test]
fn parses_typed_constants_before_functions_and_entry() {
    let program = parser::parse(
        "const Answer: i64 = 40 + 2;\nprivate const Text: string = \"hello\"\n\
         def answer :: i64\nfn answer = Answer\nanswer()",
    )
    .unwrap();
    assert_eq!(program.constants.len(), 2);
    assert_eq!(program.constants[0].name.text, "Answer");
    assert_eq!(program.constants[1].visibility, Visibility::Private);
    assert_eq!(program.functions.len(), 1);
    assert!(program.entry.is_some());
}

#[test]
fn constants_require_types_and_reserve_the_keyword() {
    for source in [
        "const Answer = 42",
        "let const = 1; const",
        "const const: i64 = 1",
    ] {
        assert_eq!(parser::parse(source).unwrap_err().code, "E0002");
    }
}

#[test]
fn constants_follow_source_kind_and_formatting_rules() {
    assert_eq!(
        analyze_modules(&[("Types.tt", "const Answer: i64 = 42")])
            .unwrap_err()
            .code,
        "E1018"
    );
    let source = "const Answer:i64=40+2\nprivate const Text:string=\"hello\"\n";
    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert_eq!(
        formatted,
        formatter::format_source("Main.tz", &formatted, SourceKind::Code)
            .unwrap()
            .formatted
    );
    assert_eq!(
        formatter::ast_fingerprint(parser::parse(source).unwrap()),
        formatter::ast_fingerprint(parser::parse(&formatted).unwrap())
    );
}

#[test]
fn evaluates_integer_constants_and_inlines_forward_references() {
    for (source, expected) in [
        ("const Answer: i64 = Later + 2\nconst Later: i64 = 40", 42),
        ("const Answer: i8 = 127 + 1", 128),
        ("const Answer: i8 = -1 >>> 9", 255),
        ("const Answer: i8u = 255 + 1", 0),
        (
            "const Answer: i128 = (1i8 <<< 7) as i128",
            (-128i128) as u128,
        ),
        ("const Answer: i64 = if true then 42 else 1 / 0", 42),
    ] {
        let source = format!("{source}\nAnswer");
        let module = analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        let body = &module.functions[module.entry.unwrap()].body;
        let value = match &body.kind {
            TypedExprKind::Block { bindings, result } if bindings.is_empty() => &result.kind,
            kind => kind,
        };
        assert!(
            matches!(value, TypedExprKind::Int(value) if *value == expected),
            "{source}\n{body:?}"
        );
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
    analyze("const Answer: bool = true || (1 / 0 == 0)\nAnswer").unwrap();
    analyze("const Answer: bool = false && (1 / 0 == 0)\nAnswer").unwrap();
}

#[test]
fn validates_unused_constants_cycles_and_value_names() {
    for (source, code) in [
        ("const Bad: i64 = 1 / 0", "E1026"),
        ("const Bad: i8 = -128 / -1", "E1026"),
        ("const Bad: i64 = Bad", "E1026"),
        (
            "const First: i64 = Second\nconst Second: i64 = First",
            "E1026",
        ),
        ("const Bad: i64 = Missing", "E1002"),
        ("const Bad: i64 = true", "E1003"),
        ("const Bad: d128 = 0.1d128 + 0.2d128", "E1026"),
        ("const Bad: i64 = identity 1", "E1026"),
        ("const Bad: 'a = 1", "E1026"),
        ("const value: i64 = 1\nfn value() -> i64 { 2 }", "E1001"),
        ("const Value: i64 = 1\nconst Value: i64 = 2", "E1001"),
    ] {
        let error = analyze(source).unwrap_err();
        assert_eq!(error.code, code, "{source}\n{error:?}");
    }
    analyze_modules(&[
        ("Values.tz", "const Answer: i64 = 42"),
        ("Main.tz", "Values.Answer"),
    ])
    .unwrap();
    let error = analyze_modules(&[
        ("Values.tz", "private const Answer: i64 = 42"),
        ("Main.tz", "Values.Answer"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1022");
    analyze("const Text: string = \"x\"\nText + Text").unwrap();
    analyze("record Data { text: string, values: [i64] }\nconst Value: Data = Data { text: \"x\", values: [1, 2] }\nlet first = Value\nlet second = Value\nfirst.values[0] + second.values[1]").unwrap();
}

#[test]
fn evaluates_binary_float_rounding_special_values_and_casts() {
    for (ty, expression, expected) in [
        ("f16", "1.0 + 0.00048828125", "15360".to_owned()),
        ("f16", "1.0009765625 + 0.00048828125", "15362".to_owned()),
        ("f16", "-0.0 * 1.0", "32768".to_owned()),
        ("f32", "16777216.0 + 1.0", "0x4170000000000000".to_owned()),
        (
            "f64",
            "9007199254740992.0 + 1.0",
            "0x4340000000000000".to_owned(),
        ),
        (
            "f128",
            "1.0000000000000000000000000000000002 + 0.0",
            ((16383u128 << 112) + 1).to_string(),
        ),
        (
            "f128",
            "340282366920938463463374607431768211455i128u as f128",
            (16511u128 << 112).to_string(),
        ),
        (
            "f32",
            "(1.0000000000000000000000000000000002f128 as f32)",
            "0x3FF0000000000000".to_owned(),
        ),
    ] {
        let source = format!("const Value: {ty} = {expression}\ndef read :: {ty}\nfn read = Value");
        let module = analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        let body = &module
            .functions
            .iter()
            .find(|function| function.name == "read")
            .unwrap()
            .body;
        assert!(
            matches!(&body.kind, TypedExprKind::Float(value) if *value == expected),
            "{source}\n{body:?}"
        );
    }
    for (expression, expected) in [
        ("(1.0f16 / 0.0f16) as i8", 127),
        ("(-1.0f128 / 0.0f128) as i8", 128),
        ("(0.0f64 / 0.0f64) as i8", 0),
        ("3.9f32 as i8", 3),
        ("-3.9f128 as i8", 253),
    ] {
        let source = format!("const Value: i8 = {expression}\ndef read :: i8\nfn read = Value");
        let module = analyze(&source).unwrap();
        let body = &module
            .functions
            .iter()
            .find(|function| function.name == "read")
            .unwrap()
            .body;
        assert!(
            matches!(body.kind, TypedExprKind::Int(value) if value == expected),
            "{source}\n{body:?}"
        );
    }
    analyze("const Nan: f128 = 0.0 / 0.0\nconst Unequal: bool = Nan != Nan\nconst Zero: bool = -0.0f16 == 0.0f16\nUnequal && Zero").unwrap();
}

#[test]
fn temporary_constant_borrows_do_not_escape() {
    for expression in [
        "String.length (&Text)",
        "String.length ref Text",
        "String.length Text",
        "Array.length (&Values)",
    ] {
        let source =
            format!("const Text: string = \"hello\"\nconst Values: [i64] = [1, 2]\n{expression}");
        let module = analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for source in [
        "const Text: string = \"x\"\ndef bad :: &string\nfn bad = &Text",
        "const Text: string = \"x\"\ndef identity :: &string -> &string\nfn identity value = value\nidentity (&Text)",
        "const Text: string = \"x\"\nlet value = &Text\nvalue.length",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1013", "{source}");
    }
}

#[test]
fn bounds_constant_expansion() {
    let mut source = String::new();
    for index in 0..140 {
        source.push_str(&format!("const Value{index}: i64 = Value{}\n", index + 1));
    }
    source.push_str("const Value140: i64 = 42");
    let error = analyze(&source).unwrap_err();
    assert_eq!(error.code, "E1026");
    assert!(error.message.contains("compiler limit"));
}

fn const_functions(program: &tsuzuri::syntax::Program) -> Vec<(&str, bool)> {
    program
        .functions
        .iter()
        .map(|function| (function.name.text.as_str(), function.constant))
        .collect()
}

#[test]
fn parses_const_def_signatures_and_formatting() {
    let source = "const def twice :: i64 -> i64\nfn twice x = x * 2\n\
        private const def hidden :: i64 -> i64\nfn hidden x = x\n\
        export const def shown :: i64 -> i64\nfn shown x = x\n\
        const def inline :: i64 -> i64 = \\x -> x + 1\n\
        const def rec even :: i64 -> bool\nfn rec even n = if n == 0 then true else odd (n - 1)\n\
        and odd :: i64 -> bool = \\n -> if n == 0 then false else even (n - 1)\n\
        def plain :: i64 -> i64\nfn plain x = x\n\
        const Answer: i64 = 42\nplain 1";
    let program = parser::parse(source).unwrap();
    let mut functions = const_functions(&program);
    functions.sort();
    assert_eq!(
        functions,
        [
            ("even", true),
            ("hidden", true),
            ("inline", true),
            ("odd", true),
            ("plain", false),
            ("shown", true),
            ("twice", true),
        ]
    );
    let hidden = program
        .functions
        .iter()
        .find(|function| function.name.text == "hidden")
        .unwrap();
    assert_eq!(hidden.visibility, Visibility::Private);
    let shown = program
        .functions
        .iter()
        .find(|function| function.name.text == "shown")
        .unwrap();
    assert!(shown.exported);
    assert_eq!(program.constants.len(), 1);
    for (source, expected) in [
        ("const fn f x = x", "put 'const' on the 'def' signature"),
        (
            "const let f = \\x -> x",
            "put 'const' on the 'def' signature",
        ),
        (
            "def rec f :: i64 -> i64\nfn rec f x = x\nconst and g x = x",
            "put 'const' on the 'def' signature",
        ),
        (
            "const export def f :: i64",
            "write 'export const def' to export a const function",
        ),
        (
            "export const fn f x = x",
            "put 'const' on the 'def' signature",
        ),
        ("const def f x = x", "'::'"),
        (
            "@cpu [\"avx2\"]\nconst def f :: i64 -> i64\nfn f x = x",
            "'@cpu' cannot apply to a 'const def'",
        ),
        (
            "@cpu [\"avx2\"]\nexport const def f :: i64 -> i64\nfn f x = x",
            "'@cpu' cannot apply to a 'const def'",
        ),
        (
            "@literal\nconst def f :: i64 -> i64\nfn f x = x",
            "expected 'def' after '@literal'",
        ),
    ] {
        let error = parser::parse(source).unwrap_err();
        assert_eq!(error.code, "E0002", "{source}");
        assert!(error.message.contains(expected), "{source}\n{error:?}");
    }
    assert_eq!(
        parser::parse("private export const def f :: i64\nfn f = 1")
            .unwrap_err()
            .code,
        "E1022"
    );
    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert!(formatted.contains("export const def shown"));
    assert!(formatted.contains("private const def hidden"));
    assert_eq!(
        formatted,
        formatter::format_source("Main.tz", &formatted, SourceKind::Code)
            .unwrap()
            .formatted
    );
    assert_eq!(
        formatter::ast_fingerprint(parser::parse(source).unwrap()),
        formatter::ast_fingerprint(parser::parse(&formatted).unwrap())
    );
    let loose = "const   def   spaced  ::  i64->i64\nfn spaced x=x\n";
    assert_eq!(
        formatter::format_source("Main.tz", loose, SourceKind::Code)
            .unwrap()
            .formatted,
        "const def spaced :: i64 -> i64\nfn spaced x = x\n"
    );
}

/// The typed entry body of `source`, which ends with the name of a constant, without its block.
fn entry_kind(source: &str) -> TypedExprKind {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let body = &module.functions[module.entry.unwrap()].body;
    match &body.kind {
        TypedExprKind::Block { bindings, result } if bindings.is_empty() => result.kind.clone(),
        kind => kind.clone(),
    }
}

fn integer_of(source: &str) -> u128 {
    match entry_kind(source) {
        TypedExprKind::Int(value) => value,
        kind => panic!("{source}\n{kind:?}"),
    }
}

fn rejected(source: &str) -> tsuzuri::diagnostic::Diagnostic {
    analyze(source).expect_err(source)
}

#[test]
fn const_def_signatures_must_be_concrete_values() {
    for (source, expected) in [
        (
            "const def id :: 'a -> 'a\nfn id x = x",
            "concrete signature",
        ),
        (
            "const def apply :: (i64 -> i64) -> i64\nfn apply f = f 1",
            "plain values",
        ),
        (
            "const def first :: ref [i64] -> i64\nfn first xs = xs[0]",
            "plain values",
        ),
        (
            "def other :: i64 -> i64 -> i64\nfn other a b = a + b\nconst def add :: i64 -> i64 -> i64\nfn add x = other x",
            "plain values",
        ),
        (
            "const def make :: i64 -> [|i64|]\nfn make n = [|n|]",
            "plain values",
        ),
        (
            "const def count :: Vec<i64> -> i64\nfn count v = 0",
            "plain values",
        ),
        (
            "const def spawn :: i64 -> Task<i64>\nfn spawn n = task { return n }",
            "plain values",
        ),
    ] {
        let error = rejected(&format!("{source}\n0"));
        assert_eq!(error.code, "E1026", "{source}");
        assert!(error.message.contains(expected), "{source}\n{error:?}");
    }
    for source in [
        "const def pair :: i64 -> i64 * string\nfn pair n = (n, \"x\")",
        "record Box { value: i64 }\nconst def open :: Box -> i64\nfn open box = box.value",
        "const def echo :: string -> string\nfn echo text = text",
        "const def numbers :: [i64] -> [i64]\nfn numbers xs = xs",
        "const def maybe :: Maybe<i64> -> i64\nfn maybe m = match m with | Some v -> v | None -> 0",
        "const def fixed :: [i64; 2] -> i64\nfn fixed xs = xs[0] + xs[1]",
        "const def text :: utf8string -> utf8string\nfn text value = value",
        "const def wide :: i128 -> f16 -> d64 -> char -> utf8char -> bool -> unit -> i8u\nfn wide a b c d e f g = 1",
    ] {
        analyze(&format!("{source}\n0")).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
}

#[test]
fn const_def_bodies_reject_unsupported_operations() {
    // Each const def is unused, so only the check of its body can reject it.
    for (body, expected) in [
        ("fn f n = Array.sum [n]", "'sum' is not a const def"),
        (
            "fn f n = plain n",
            "'plain' is not a const def; mark it 'const def'",
        ),
        ("fn f n =\n    let g = \\x -> x\n    g n", "function value"),
        ("fn f n =\n    let xs = [|n|]\n    n", "not allowed"),
        (
            "fn f n =\n    let mut v = n\n    let r = &mut v\n    n",
            "not allowed",
        ),
        (
            "fn f n =\n    let r = ref n\n    n",
            "borrow is not allowed",
        ),
        ("fn f n = @checked n + 1", "@checked"),
        (
            "fn f n = if (n, n) == (n, n) then n else 0",
            "operator on a record, tuple, array, or union",
        ),
        ("fn f n = to_string n |> String.length", "pipe operator"),
        ("fn f n = host n", "'host' is not a const def"),
        (
            "fn f n = (\\x -> x + 1) n",
            "only a const def can be called",
        ),
        (
            "fn f n = String.length (Display.display n)",
            "not a const def",
        ),
    ] {
        let source = format!(
            "extern def host :: i64 -> i64\ndef plain :: i64 -> i64\nfn plain x = x\nconst def f :: i64 -> i64\n{body}\n0"
        );
        let error = rejected(&source);
        assert_eq!(error.code, "E1026", "{body}\n{error:?}");
        assert!(error.message.contains(expected), "{body}\n{error:?}");
    }
    // A for loop enumerates arrays and strings; other sources and the assignment of a field are out.
    let error = rejected(
        "const def f :: i64 -> i64\nfn f n =\n    for x in [|n|] do\n        ()\n    n\n0",
    );
    assert_eq!(error.code, "E1026");
    // The first rejected expression in source order is the one reported.
    let error = rejected(
        "def plain :: i64 -> i64\nfn plain x = x\nconst def f :: i64 -> i64\nfn f n = plain n + Array.sum [n]\n0",
    );
    assert!(error.message.contains("'plain'"), "{error:?}");
    // A call of a runtime function from a constant keeps its message and code.
    let error = rejected("def plain :: i64 -> i64\nfn plain x = x\nconst C: i64 = plain 1\n0");
    assert_eq!(error.code, "E1026");
    assert!(error.message.contains("phase-1 const"), "{error:?}");
}

/// The CRC-32 table entry of `index`, as the standard algorithm defines it.
fn crc_entry(index: u32) -> u32 {
    (0..8).fold(index, |value, _| {
        if value & 1 == 1 {
            (value >> 1) ^ 0xEDB8_8320
        } else {
            value >> 1
        }
    })
}

#[test]
fn const_def_calls_evaluate_like_runtime() {
    const COLLATZ: &str = "const def collatz :: i64 -> i64\nfn collatz start =\n    let mut n = start\n    let mut steps = 0\n    while n != 1 do\n        n = if n % 2 == 0 then n / 2 else 3 * n + 1\n        steps = steps + 1\n    steps";
    let collatz = |start: u64| {
        let (mut n, mut steps) = (start, 0u128);
        while n != 1 {
            n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
            steps += 1;
        }
        steps
    };
    for (definitions, expression, ty, expected) in [
        (
            "const def twice :: i64 -> i64\nfn twice x = x * 2",
            "twice 21",
            "i64",
            42,
        ),
        (
            "const def rec fib :: i64 -> i64\nfn rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)",
            "fib 20",
            "i64",
            6765,
        ),
        (
            "const def total :: i64 -> i64\nfn total n =\n    let mut sum = 0\n    for i in 1 .. n do\n        sum = sum + i\n    sum",
            "total 100",
            "i64",
            (1..=100).sum(),
        ),
        (COLLATZ, "collatz 27", "i64", collatz(27)),
        (COLLATZ, "collatz 97", "i64", collatz(97)),
        (
            "const def classify :: i64 -> i64\nfn classify n = match n with\n    | 0 -> 10\n    | 1 -> 20\n    | _ -> n * 2",
            "classify 0 + classify 1 + classify 7",
            "i64",
            10 + 20 + 14,
        ),
        (
            "const def bump :: i8 -> i8\nfn bump x = x + 1",
            "bump 127",
            "i8",
            128,
        ),
        (
            "const def crc_entry :: i32u -> i32u\nfn crc_entry index =\n    let mut value = index\n    for _ in 1 .. 8 do\n        value = if (value & 1) == 1 then (value >>> 1) ^ 0xEDB88320 else value >>> 1\n    value\nconst def crc_table :: [i32u]\nfn crc_table = new [i32u](256, \\i -> crc_entry (i as i32u))\nconst Table: [i32u] = crc_table()\nconst def pick :: [i32u] -> i64 -> i32u\nfn pick table at = table[at]",
            "pick Table 1",
            "i32u",
            u128::from(crc_entry(1)),
        ),
    ] {
        let source = format!("{definitions}\nconst Value: {ty} = {expression}\nValue");
        assert_eq!(integer_of(&source), expected, "{source}");
        let module = analyze(&source).unwrap();
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
    // The table itself: all 256 entries are literals, in order.
    let source = "const def crc_entry :: i32u -> i32u\nfn crc_entry index =\n    let mut value = index\n    for _ in 1 .. 8 do\n        value = if (value & 1) == 1 then (value >>> 1) ^ 0xEDB88320 else value >>> 1\n    value\nconst def crc_table :: [i32u]\nfn crc_table = new [i32u](256, \\i -> crc_entry (i as i32u))\nconst Table: [i32u] = crc_table()\ndef read :: [i32u]\nfn read = Table\n0";
    let module = analyze(source).unwrap();
    let read = module
        .functions
        .iter()
        .find(|function| function.name == "read")
        .unwrap();
    let TypedExprKind::Array(entries) = &read.body.kind else {
        panic!("{:?}", read.body);
    };
    assert_eq!(entries.len(), 256);
    for (index, entry) in entries.iter().enumerate() {
        assert!(
            matches!(&entry.kind, TypedExprKind::Int(value) if *value == u128::from(crc_entry(index as u32))),
            "entry {index}: {entry:?}"
        );
    }
}

#[test]
fn const_def_ranges_follow_the_loop_semantics() {
    // Each program sums the values that the loop variable takes; the references are computed here.
    let sum = |start: i128, step: i128, finish: i128, bits: u32, signed: bool| -> u128 {
        let (low, high) = if signed {
            (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
        } else {
            (0, (1i128 << bits) - 1)
        };
        let mut total = 0i128;
        let mut current = start;
        loop {
            let within = if step > 0 {
                current <= finish
            } else {
                current >= finish
            };
            if !within {
                break;
            }
            total += current;
            current += step;
            if current < low || current > high {
                break;
            }
        }
        total as u128
    };
    for (ty, start, step, finish, bits, signed) in [
        ("i8", 125, 2, 127, 8, true),
        ("i8", 120, 3, 127, 8, true),
        ("i8", -120, -3, -128, 8, true),
        ("i8", 127, -50, -128, 8, true),
        ("i8u", 250, 3, 255, 8, false),
        ("i8u", 0, 100, 255, 8, false),
        ("i32", -5, 2, 6, 32, true),
        ("i32", 5, -2, -6, 32, true),
        ("i64", 7, 1, 7, 64, true),
        ("i64", 8, 1, 7, 64, true),
        ("i16u", 65534, 1, 65535, 16, false),
    ] {
        let source = format!(
            "const def total :: {ty} -> {ty} -> {ty} -> i128\nfn total start step finish =\n    let mut sum = 0i128\n    for i in start .. step .. finish do\n        sum = sum + (i as i128)\n    sum\nconst Value: i128 = total ({start}{ty}) ({step}{ty}) ({finish}{ty})\nValue"
        );
        let expected = sum(start, step, finish, bits, signed);
        assert_eq!(integer_of(&source), expected, "{source}");
    }
}

#[test]
fn const_def_data_follows_runtime_semantics() {
    let cases: &[(&str, &str, u128)] = &[
        // Strings are read by code unit, and by byte for utf8string.
        (
            "const def units :: string -> i64\nfn units text =\n    let mut total = 0\n    for unit in text do\n        total = total + (unit as i64)\n    total + text.length * 1000 + (text[1] as i64)",
            "units \"日本語ab\"",
            (0x65E5 + 0x672C + 0x8A9E + 97 + 98) + 5 * 1000 + 0x672C,
        ),
        (
            "const def bytes :: utf8string -> i64\nfn bytes text =\n    let mut total = 0\n    for byte in text do\n        total = total + (byte as i64)\n    total + text.length * 1000",
            "bytes u8\"é\"",
            (0xC3 + 0xA9) + 2 * 1000,
        ),
        // Records, tuples, unions, and their updates.
        (
            "record Point { x: i64, y: i64 }\nconst def moved :: Point -> i64\nfn moved point =\n    let next = { point with y = point.y + 10 }\n    next.x * 100 + next.y",
            "moved (Point { x: 3, y: 4 })",
            314,
        ),
        (
            "union Shape = Dot | Square of i64 | Rect of (i64 * i64)\nconst def area :: Shape -> i64\nfn area shape = match shape with\n    | Dot -> 0\n    | Square side -> side * side\n    | Rect (width, height) -> width * height",
            "area (Square 7) * 100 + area (Rect (3, 5)) + area Dot",
            4915,
        ),
        (
            "const def swap :: (i64 * i64) -> i64\nfn swap pair = match pair with\n    | (a, b) when a > b -> a - b\n    | (a, b) -> b - a",
            "swap (3, 10) * 100 + swap (10, 3)",
            707,
        ),
        // Arrays: elements, length, a loop that exits early, and one that skips.
        (
            "const def pick :: [i64] -> i64\nfn pick xs =\n    let mut found = 0 - 1\n    let mut index = 0\n    while index < xs.length do\n        if xs[index] > 4 then\n            found = index\n            break\n        index = index + 1\n    found",
            "pick [1, 5, 9] * 10 + pick [1, 2]",
            (10 - 1) as u128 & 0xFFFF_FFFF_FFFF_FFFF,
        ),
        (
            "const def odd :: i64 -> i64\nfn odd n =\n    let mut total = 0\n    for i in 1 .. n do\n        if i % 2 == 0 then continue\n        total = total + i\n    total",
            "odd 10",
            25,
        ),
        // Integer widths, casts, shifts, and the wrapping of arithmetic.
        (
            "const def mix :: i32 -> i32u\nfn mix x = ((x as i32u) <<< 3) ^^^ ((x as i32u) >>> 1)",
            "mix (0 - 7)",
            u128::from(((-7i32 as u32) << 3) ^ ((-7i32 as u32) >> 1)),
        ),
        (
            "const def wide :: i128 -> i128\nfn wide x = x * x * x",
            "wide 1000000000000",
            1_000_000_000_000u128.pow(3),
        ),
        (
            "const def narrow :: i64 -> i8u\nfn narrow x = (x * 3) as i8u",
            "narrow 100",
            u128::from((300u32 & 0xFF) as u8),
        ),
    ];
    for (definitions, expression, expected) in cases {
        let ty = if definitions.contains("-> i32u\nfn mix") {
            "i32u"
        } else if definitions.contains("-> i128\nfn wide") {
            "i128"
        } else if definitions.contains("-> i8u\nfn narrow") {
            "i8u"
        } else {
            "i64"
        };
        let source = format!("{definitions}\nconst Value: {ty} = {expression}\nValue");
        assert_eq!(integer_of(&source), *expected, "{source}");
    }
}

#[test]
fn const_def_float_and_decimal_follow_the_constant_rules() {
    let source = "const def poly :: f64 -> f64\nfn poly x = x * x * 0.1 + x / 3.0\nconst Value: f64 = poly 1.7\nValue";
    let expected = 1.7f64 * 1.7 * 0.1 + 1.7 / 3.0;
    assert!(
        matches!(entry_kind(source), TypedExprKind::Float(bits)
            if u64::from_str_radix(&bits[2..], 16).unwrap() == expected.to_bits()),
        "{source}"
    );
    let source =
        "const def half :: f16 -> f16\nfn half x = x / 2.0\nconst Value: f16 = half 3.0\nValue";
    assert!(matches!(entry_kind(source), TypedExprKind::Float(bits) if bits == "15872"));
    // A decimal passes through a call, but arithmetic on it is still a constant error.
    analyze("const def same :: d64 -> d64\nfn same x = x\nconst Value: d64 = same 1.5d64\nValue")
        .unwrap();
    let error = rejected(
        "const def add :: d64 -> d64\nfn add x = x + x\nconst Value: d64 = add 1.5d64\nValue",
    );
    assert_eq!(error.code, "E1026");
    assert!(error.message.contains("decimal"), "{error:?}");
}

#[test]
fn const_dependencies_flow_through_const_defs() {
    assert_eq!(
        integer_of("const A: i64 = f()\nconst def f :: i64\nfn f = B * 2\nconst B: i64 = 21\nA"),
        42
    );
    // The dependency order does not depend on the order of the declarations.
    assert_eq!(
        integer_of(
            "const def f :: i64\nfn f = B * 2\nconst B: i64 = g () + 1\nconst def g :: unit -> i64\nfn g _ = 20\nconst A: i64 = f()\nA"
        ),
        42
    );
    for source in [
        "const A: i64 = f()\nconst def f :: i64\nfn f = A + 1\nA",
        "const A: i64 = f()\nconst def f :: i64\nfn f = B + 1\nconst B: i64 = g()\nconst def g :: i64\nfn g = A\nA",
    ] {
        let error = rejected(source);
        assert_eq!(error.code, "E1026", "{source}");
        assert!(error.message.contains("cyclic"), "{source}\n{error:?}");
    }
    // A constant that no initializer reaches through a call stays a plain function at run time.
    let module = analyze(
        "const def f :: i64 -> i64\nfn f x = x + 1\ndef g :: i64 -> i64\nfn g x = f x\ng 1",
    )
    .unwrap();
    assert!(module.functions.iter().any(|function| function.name == "f"));
    // Union cases build the arguments of a call, and a constant can hold a union.
    assert_eq!(
        integer_of(
            "union Pick = Pair of (i64 * i64)\nconst def sum :: Pick -> i64\nfn sum p = match p with | Pair (a, b) -> a + b\nconst A: i64 = sum (Pair (40, 2))\nA"
        ),
        42
    );
    // Calls of other functions from an initializer keep the old error.
    for source in [
        "const A: i64 = missing 1\nA",
        "const def f :: i64 -> i64\nfn f x = x\nconst A: i64 = f 1 |> f\nA",
    ] {
        assert_eq!(rejected(source).code, "E1026", "{source}");
    }
}

#[test]
fn const_def_traps_report_the_trap_kind() {
    for (definitions, expression, ty, expected) in [
        (
            "const def divide :: i64 -> i64 -> i64\nfn divide a b = a / b",
            "divide 1 0",
            "i64",
            "const division trapped on zero",
        ),
        (
            "const def remainder :: i8 -> i8 -> i8\nfn remainder a b = a % b",
            "remainder (0 - 127 - 1) (0 - 1)",
            "i8",
            "const division trapped",
        ),
        (
            "const def pick :: [i64] -> i64\nfn pick xs = xs[3]",
            "pick [1, 2]",
            "i64",
            "index out of bounds",
        ),
        (
            "const def pick :: [i64] -> i64\nfn pick xs = xs[0 - 1]",
            "pick [1, 2]",
            "i64",
            "index out of bounds",
        ),
        (
            "const def pick :: string -> i64\nfn pick text = text[9] as i64",
            "pick \"ab\"",
            "i64",
            "index out of bounds",
        ),
        (
            "const def steps :: i64 -> i64\nfn steps step =\n    let mut total = 0\n    for i in 0 .. step .. 3 do\n        total = total + i\n    total",
            "steps 0",
            "i64",
            "range step is zero",
        ),
        (
            "const def make :: i64 -> [i64]\nfn make n = new [i64](n, \\i -> i)",
            "make (0 - 1)",
            "[i64]",
            "negative array length",
        ),
        (
            "const def odd_ones :: [(i64 * i64)] -> i64\nfn odd_ones pairs =\n    let mut total = 0\n    for (1, second) in pairs do\n        total = total + second\n    total",
            "odd_ones [(1, 10), (2, 20)]",
            "i64",
            "no pattern matched",
        ),
        (
            "const def power :: i64 -> i64\nfn power n = 2 ** n",
            "power (0 - 1)",
            "i64",
            "negative exponent",
        ),
    ] {
        let source = format!("{definitions}\nconst Value: {ty} = {expression}\nValue");
        let error = rejected(&source);
        assert_eq!(error.code, "E1026", "{source}\n{error:?}");
        assert!(error.message.contains(expected), "{source}\n{error:?}");
    }
}

#[test]
fn bounds_const_def_calls_steps_and_values() {
    let down = |depth: u32| {
        format!(
            "const def rec down :: i64 -> i64\nfn rec down n = if n == 0 then 0 else down (n - 1)\nconst Value: i64 = down {depth}\nValue"
        )
    };
    // The 128th nested call is the last one that runs.
    assert_eq!(integer_of(&down(127)), 0);
    let error = rejected(&down(128));
    assert!(error.message.contains("128 nested"), "{error:?}");
    let started = std::time::Instant::now();
    for (source, expected) in [
        (
            "const def spin :: i64 -> i64\nfn spin n =\n    let mut i = 0\n    while true do\n        i = i + 1\n    i\nconst Value: i64 = spin 1\nValue",
            "compiler limit",
        ),
        (
            "const def big :: i64 -> [i64]\nfn big n = new [i64](n, \\i -> i)\nconst Value: [i64] = big 2000000\nValue",
            "compiler limit",
        ),
        (
            "union Chain = End | Link of Chain\nconst def build :: i64 -> Chain\nfn build n =\n    let mut chain = End\n    for _ in 1 .. n do\n        chain = Link chain\n    chain\nconst Value: Chain = build 200\nValue",
            "compiler limit",
        ),
    ] {
        let error = rejected(source);
        assert_eq!(error.code, "E1026", "{source}\n{error:?}");
        assert!(error.message.contains(expected), "{source}\n{error:?}");
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "the limits stop the evaluation quickly"
    );
    // Ten thousand loop rounds fit the budget, and a chain that stays shallow is accepted.
    assert_eq!(
        integer_of(
            "const def count :: i64 -> i64\nfn count n =\n    let mut total = 0\n    for i in 1 .. n do\n        total = total + i\n    total\nconst Value: i64 = count 10000\nValue"
        ),
        50_005_000
    );
}

const PLAIN: &str = "const Table: [i64] = [10, 20, 30, 40]\n\
    const Grid: [i64; 4] = [1, 2, 3, 4]\n\
    const Answer: i64 = 42\n\
    record Point { x: i32, y: i64 }\n\
    const Corner: Point = Point { x: 7, y: 9 }\n\
    union Reading = Missing | Value of i64 | Backup of i64\n\
    const Favorite: Reading = Backup 5\n\
    const Text: string = \"hello\"\n\
    const Names: [string] = [\"a\", \"b\"]\n";

/// A program that defines the constants of `PLAIN`, then the function `value`, whose body is
/// `body` (lines separated by a newline and indented), and calls it.
fn plain_program(body: &str) -> String {
    format!(
        "{PLAIN}\ndef value :: unit -> i64\nfn value _ =\n    {}\nvalue ()",
        body.replace('\n', "\n    ")
    )
}

/// The `@tz.static.` lines of the IR of `source` for native and WASM, which must agree.
fn static_data(source: &str) -> Vec<String> {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let mut data = Vec::new();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let lines: Vec<String> = ir
            .lines()
            .filter(|line| line.starts_with("@tz.static."))
            .map(str::to_owned)
            .collect();
        for line in &lines {
            assert!(
                line.contains(" = private unnamed_addr constant "),
                "{source}\n{line}"
            );
        }
        data.push(lines);
    }
    assert_eq!(data[0], data[1], "{source}");
    data.swap_remove(0)
}

#[test]
fn plain_constants_read_in_place_are_static_data() {
    for body in [
        "Table[1]",
        "Table.length",
        "Array.length Table",
        "Array.length (ref Table)",
        "Array.length &Table",
        "Array.sum Table",
        "Array.sum (ref Table[1..3])",
        "Grid[2]",
        "Grid.length",
        "Array.sum Grid",
        "Array.sum (ref Grid[1..3])",
        "(Corner.x as i64) + Corner.y",
        "(ref Corner).y",
        "String.length $\"{Answer}\"",
        "let mut total = 0\nfor x in Table do\n    total = total + x\ntotal",
    ] {
        let data = static_data(&plain_program(body));
        assert!(!data.is_empty(), "{body}");
        assert!(
            data.iter().all(|line| !line.contains("hello")),
            "strings are not static data: {body}"
        );
    }
}

#[test]
fn static_data_is_defined_once_per_constant_and_only_when_used() {
    let data = static_data(&plain_program(
        "Table[0] + Table[1] + Array.length (ref Table) + Grid[0] + Table.length",
    ));
    assert_eq!(data.len(), 2, "{data:?}");
    assert!(data[0].starts_with("@tz.static."), "{data:?}");
    assert!(
        data.iter()
            .any(|line| line.contains("[4 x i64] [i64 10, i64 20, i64 30, i64 40]"))
    );
    assert!(
        data.iter()
            .any(|line| line.contains("[4 x i64] [i64 1, i64 2, i64 3, i64 4]"))
    );
    assert!(static_data(&plain_program("Answer + 1")).is_empty());
}

#[test]
fn values_and_unused_constants_are_not_static_data() {
    for body in [
        "let t = Table\nt[0] + Array.sum (ref t)",
        "let t = Table\nt.length",
        "let g = Grid\ng[1]",
        "Answer + 1",
        "match Favorite with\n| Missing -> 0\n| Value value -> value\n| Backup value -> value * 10",
        "String.length (&Text) + String.length ref Text + Names.length",
        "let names = Names\nnames.length",
    ] {
        let data = static_data(&plain_program(body));
        assert!(data.is_empty(), "{body}\n{data:?}");
    }
    assert_eq!(
        static_data("const Table: [i64] = [1]\nTable.length + 1").len(),
        1
    );
    assert!(static_data("const Table: [i64] = [1]\nlet t = Table\nt.length").is_empty());
}

#[test]
fn plain_constant_borrows_escape_without_a_copy() {
    for source in [
        "const Table: [i64] = [10, 20]\ndef table :: unit -> ref [i64]\nfn table _ = ref Table\n(table ())[1]",
        "const Table: [i64] = [10, 20, 30]\ndef window :: unit -> ref [i64]\nfn window _ = ref Table[1..3]\n(window ()).length",
        "const Table: [i64] = [10, 20]\nrecord View { data: ref [i64] }\ndef view :: unit -> View\nfn view _ = View { data: ref Table }\n(view ()).data[1]",
        "const Table: [i64] = [10, 20]\ndef pass :: ref [i64] -> ref [i64]\nfn pass xs = xs\n(pass (ref Table))[0]",
        "const Table: [i64] = [10, 20]\nlet r = ref Table\nlet read = \\i -> r[i]\nread 1",
        "const Table: [i64] = [10, 20]\nlet read = \\i -> Table[i]\nread 1",
        "const Answer: i64 = 42\ndef answer :: unit -> ref i64\nfn answer _ = ref Answer\nderef (answer ())",
        "record Point { x: i32, y: i64 }\nconst Corner: Point = Point { x: 7, y: 9 }\ndef corner :: unit -> ref Point\nfn corner _ = ref Corner\n(corner ()).y",
        "const Grid: [i64; 4] = [1, 2, 3, 4]\ndef grid :: unit -> ref [i64; 4]\nfn grid _ = ref Grid\n(grid ())[3]",
        "union Shade = Dark | Light\nconst Chosen: Shade = Light\ndef shade :: unit -> ref Shade\nfn shade _ = ref Chosen\nmatch deref (shade ()) with\n| Dark -> 0\n| Light -> 1",
        "const Table: [i64] = [10, 20]\ndef first :: ref [i64] -> i64\nfn first xs = xs[0]\nfirst (ref Table) + first (ref Table[1..2])",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert!(ir.contains("@tz.static."), "{source}");
            assert_static_data_is_only_read(&ir, source);
        }
    }
}

/// The data of a static array is named only by the headers that lend it, and no instruction
/// writes through the address of any static: so a borrow never stores into, frees, or reallocates it.
fn assert_static_data_is_only_read(ir: &str, source: &str) {
    let uses: String = ir
        .lines()
        .filter(|line| !line.starts_with("@tz.static."))
        .collect::<Vec<_>>()
        .join("\n");
    for (index, _) in uses.match_indices("@tz.static.") {
        let rest = &uses[index..];
        let end = rest.find([',', ')', ' ']).unwrap_or(rest.len());
        if rest[..end].ends_with(".data") {
            assert!(
                uses[..index].ends_with("{ ptr "),
                "{source}\n{}",
                &uses[index.saturating_sub(40)..(index + 40).min(uses.len())]
            );
        }
    }
    for line in uses.lines() {
        for pointer in ["store ", "atomicrmw ", "cmpxchg "] {
            if line.trim_start().starts_with(pointer) {
                assert!(
                    !line.contains(", ptr @tz.static."),
                    "a static is never written: {source}\n{line}"
                );
            }
        }
    }
}

#[test]
fn string_constants_keep_their_borrow_rules() {
    for source in [
        "const Text: string = \"x\"\ndef bad :: unit -> &string\nfn bad _ = &Text",
        "const Names: [string] = [\"x\"]\ndef bad :: unit -> ref [string]\nfn bad _ = ref Names\nbad ()",
        "record Data { name: string }\nconst Stored: Data = Data { name: \"x\" }\ndef bad :: unit -> ref Data\nfn bad _ = ref Stored\nbad ()",
        "const Text: string = \"x\"\nlet value = &Text\nvalue.length",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1013", "{source}");
    }
}

#[test]
fn static_data_is_never_written_moved_or_exclusively_borrowed() {
    for source in [
        "const Table: [i64] = [1, 2]\ndef replace :: ref mut [i64] -> unit\nfn replace xs = *xs = [1]\nreplace (ref mut Table)",
        "const Table: [i64] = [1, 2]\nlet r = ref mut Table\nr.length",
        "const Table: [i64] = [1, 2]\ndef sort :: unit -> unit\nfn sort _ = Array.sort_in_place (ref mut Table[0..2])\nsort ()",
        "const Table: [i64] = [1, 2]\nTable[0] = 5\nTable[0]",
    ] {
        let error = analyze(source).unwrap_err();
        assert!(
            matches!(error.code, "E1012" | "E1013" | "E1014"),
            "{source}\n{error:?}"
        );
    }
}

#[test]
fn static_borrows_do_not_hide_a_dangling_reference() {
    for source in [
        "def make :: unit -> i64\nfn make _ =\n    let local = 5\n    let f = \\_ -> ref local\n    let r = f ()\n    deref r\nmake ()",
        "def pass :: i64 -> ref i64\nfn pass x = ref x\ndef make :: unit -> i64\nfn make _ =\n    let r = pass 5\n    deref r\nmake ()",
        "def make :: unit -> ref i64\nfn make _ =\n    let local = 5\n    ref local\nmake ()",
        "const Table: [i64] = [1, 2]\ndef make :: unit -> ref i64\nfn make _ =\n    let local = Table[0]\n    ref local\nmake ()",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1013", "{source}");
    }
}

#[test]
fn tasks_keep_their_rules_for_static_borrows() {
    for source in [
        "const Table: [i64] = [1, 2]\ndef run :: unit -> i64\nfn run _ =\n    let r = ref Table\n    Task.run task {\n        r[0]\n    }\nrun ()",
        "const Table: [i64] = [1, 2]\ndef run :: unit -> i64\nfn run _ =\n    let r = Task.run task {\n        ref Table\n    }\n    r[0]\nrun ()",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1013", "{source}");
    }
    // A task reads the constant by name, without a capture.
    analyze(
        "const Table: [i64] = [1, 2]\ndef run :: unit -> i64\nfn run _ =\n    Task.run task {\n        let! a = task { Table[0] }\n        return a + Table[1]\n    }\nrun ()",
    )
    .unwrap();
}

#[test]
fn large_static_data_is_one_global() {
    // The evaluation budget (1,048,576 steps) bounds one constant to about 200,000 elements, below
    // the 4 MiB of static data (the unit test of `const_static` checks that bound itself).
    let source = "const def big :: [i128u]\nfn big = new [i128u](50000, \\i -> i as i128u)\nconst Big: [i128u] = big()\ndef value :: unit -> i128u\nfn value _ = Big[49999]\nvalue ()";
    let data = static_data(source);
    assert_eq!(data.len(), 1, "{}", data.len());
    assert!(data[0].starts_with("@tz.static."), "{}", &data[0][..40]);
    assert!(data[0].contains("[50000 x i128]"));
    assert!(data[0].ends_with(", align 16"));
    // Read by value, the same constant is a copy of 800,000 bytes and no static data.
    let copied = "const def big :: [i128u]\nfn big = new [i128u](50000, \\i -> i as i128u)\nconst Big: [i128u] = big()\ndef value :: unit -> i64\nfn value _ =\n    let copy = Big\n    copy.length\nvalue ()";
    assert!(static_data(copied).is_empty());
}

#[test]
fn static_data_has_the_bits_of_its_constants() {
    let source = "const Halves: [f16] = [1.5, -2.5]\n\
        const Singles: [f32] = [-0.0, 0.0 / 0.0]\n\
        const Flags: [bool] = [true, false]\n\
        const Bytes: [ubyte] = [1, 255]\n\
        const Letters: [char] = ['a', 'z']\n\
        const Wide: [i128u] = [340282366920938463463374607431768211455]\n\
        const Signed: [i8] = [-1, 127]\n\
        record Point { x: i32, y: i64 }\n\
        const Points: [Point] = [Point { x: -3, y: 4 }]\n\
        union Reading = Missing | Value of i64 | Backup of i64\n\
        const Readings: [Reading] = [Missing, Value 7]\n\
        union Shade = Dark | Light\n\
        const Shades: [Shade] = [Light, Dark]\n\
        const Pairs: [i64 * bool] = [(5, true)]\n\
        const Empty: [i64] = []\n\
        def total :: unit -> i64\n\
        fn total _ = Halves.length + Singles.length + Flags.length + Bytes.length + Letters.length + Wide.length + Signed.length + Points.length + Readings.length + Shades.length + Pairs.length + Empty.length\n\
        total ()";
    let data = static_data(source).join("\n");
    for expected in [
        "[2 x i16] [i16 15872, i16 49408]",
        "[2 x float] [float 0x8000000000000000, float 0x7FF8000000000000]",
        "[2 x i1] [i1 1, i1 0]",
        "[2 x i8] c\"\\01\\FF\"",
        "[2 x i16] [i16 97, i16 122]",
        "[1 x i128] [i128 340282366920938463463374607431768211455]",
        "[2 x i8] c\"\\FF\\7F\"",
        "[1 x %tz.record.Main.Point] [%tz.record.Main.Point { i32 4294967293, i64 4 }]",
        "{ i8 0, i64 zeroinitializer }, %\"tz.union.Main.Reading\" { i8 1, i64 7 }",
        "[%\"tz.union.Main.Shade\" 1, %\"tz.union.Main.Shade\" 0]",
        "[1 x { i64, i1 }] [{ i64, i1 } { i64 5, i1 1 }]",
    ] {
        assert!(data.contains(expected), "{expected}\n{data}");
    }
    // An empty array has a null header and no data.
    assert!(!data.contains("[0 x"), "{data}");
}

#[test]
fn static_data_ir_is_deterministic() {
    let source = plain_program(
        "Table[0] + Grid[1] + Array.sum (ref Table) + (Corner.x as i64) + String.length $\"{Answer}\"",
    );
    let module = analyze(&source).unwrap();
    for wasm in [false, true] {
        let first = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let second = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(first, second);
        let names: Vec<_> = first
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(name, _)| name))
            .filter(|name| name.starts_with("@tz.static."))
            .collect();
        let unique: std::collections::BTreeSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "each static is defined once");
    }
}

/// The integers of the first array below the entry of `source`, which shows its last value.
fn integers_of(source: &str) -> Vec<u128> {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let mut pending = vec![&module.functions[module.entry.unwrap()].body];
    while let Some(expression) = pending.pop() {
        if let TypedExprKind::Array(items) = &expression.kind {
            return items
                .iter()
                .map(|item| match item.kind {
                    TypedExprKind::Int(value) => value,
                    _ => panic!("{source}\n{item:?}"),
                })
                .collect();
        }
        pending.extend(expression.children());
    }
    panic!("{source}");
}

#[test]
fn const_defs_passed_by_name_bring_their_constants_before_use() {
    // `add_k` reads `K`, which is declared after the constant that evaluates `table`.
    let source = "const T: [i64] = table ()\nconst K: i64 = 7\n\
        const def add_k :: i64 -> i64\nfn add_k i = i + K\n\
        const def table :: unit -> [i64]\nfn table _ = new [i64](4, add_k)\nT";
    assert_eq!(integers_of(source), [7, 8, 9, 10]);
    // The same through a lambda, and with the constant first.
    assert_eq!(
        integers_of(&source.replace("new [i64](4, add_k)", "new [i64](4, \\i -> add_k i)")),
        [7, 8, 9, 10]
    );
    let cyclic = "const T: [i64] = table ()\n\
        const def add_k :: i64 -> i64\nfn add_k i = i + (T[0])\n\
        const def table :: unit -> [i64]\nfn table _ = new [i64](4, add_k)\nT";
    assert_eq!(rejected(cyclic).code, "E1026");
}

#[test]
fn const_defs_index_and_measure_fixed_length_arrays() {
    assert_eq!(
        integer_of(
            "const def total :: [i64; 3] -> i64\nfn total xs = xs[0] + xs[2] + xs.length\nconst Value: i64 = total [10, 20, 30]\nValue"
        ),
        43
    );
    assert_eq!(
        integer_of(
            "const Grid: [i64; 4] = [1, 2, 3, 4]\nconst def pick :: i64 -> i64\nfn pick i = Grid[i] + Grid.length\nconst Value: i64 = pick 2\nValue"
        ),
        7
    );
    let error = rejected(
        "const Grid: [i64; 4] = [1, 2, 3, 4]\nconst def pick :: i64 -> i64\nfn pick i = Grid[i]\nconst Value: i64 = pick 4\nValue",
    );
    assert_eq!(error.code, "E1026");
    assert!(error.message.contains("index out of bounds"), "{error:?}");
}

#[test]
fn a_private_const_def_that_a_constant_calls_is_used() {
    for source in [
        "private const def twice :: i64 -> i64\nfn twice x = x * 2\nconst Value: i64 = twice 4\nValue",
        "private const def twice :: i64 -> i64\nfn twice x = x * 2\nprivate const def four :: i64\nfn four = twice 2\nconst Value: i64 = four()\nValue",
        "private const def add :: i64 -> i64\nfn add i = i + 1\nprivate const def table :: [i64]\nfn table = new [i64](2, add)\nconst Values: [i64] = table()\nValues.length",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        assert!(
            module
                .warnings
                .iter()
                .all(|warning| warning.code != "W1002"),
            "{source}\n{:?}",
            module.warnings
        );
    }
    let module = analyze("private const def unused :: i64 -> i64\nfn unused x = x\n1").unwrap();
    assert!(
        module
            .warnings
            .iter()
            .any(|warning| warning.code == "W1002"),
        "{:?}",
        module.warnings
    );
}
