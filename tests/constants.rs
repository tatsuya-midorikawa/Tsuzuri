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
