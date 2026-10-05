use tsuzuri::diagnostic::Span;
use tsuzuri::formatter::format_source;
use tsuzuri::lexer::{lex, lex_all};
use tsuzuri::parser::parse;
use tsuzuri::syntax::{
    FormatAlign, FormatKind, FormatSpec, InterpolationPiece, SourceKind, StringLiteral, TokenKind,
};
use tsuzuri::{analyze, llvm};

fn accepts(source: &str) {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        for forbidden in ["@printf", "@memcmp", "@strtod", "@strtof", "@snprintf"] {
            assert!(!ir.contains(forbidden), "{source}: {forbidden}");
        }
        let declarations: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("declare "))
            .collect();
        let unique: std::collections::BTreeSet<_> = declarations.iter().collect();
        assert_eq!(unique.len(), declarations.len(), "{source}");
    }
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

fn kinds(source: &str) -> Vec<TokenKind> {
    lex(source)
        .unwrap_or_else(|error| panic!("{source}\n{}", error.message))
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

fn piece(text: &str, spec: Option<FormatSpec>) -> Box<InterpolationPiece> {
    Box::new(InterpolationPiece {
        text: StringLiteral::Utf16(text.encode_utf16().collect()),
        spec,
    })
}

#[test]
fn lexes_pieces_and_nested_holes() {
    assert_eq!(
        kinds("$\"a{x}b\""),
        [
            TokenKind::InterpolationStart(piece("a", None)),
            TokenKind::Ident("x".into()),
            TokenKind::InterpolationEnd(piece("b", None)),
            TokenKind::End,
        ]
    );
    assert_eq!(
        kinds("$\"a{{b}}\"")[0],
        TokenKind::String(StringLiteral::Utf16("a{b}".encode_utf16().collect()))
    );
    assert_eq!(
        kinds("$\"\"")[0],
        TokenKind::String(StringLiteral::Utf16(vec![]))
    );
    let tokens = kinds("u8$\"é{x}\"");
    assert_eq!(
        tokens[0],
        TokenKind::InterpolationStart(Box::new(InterpolationPiece {
            text: StringLiteral::Utf8("é".into()),
            spec: None,
        }))
    );
    assert_eq!(
        kinds("$\"{x}{y}\""),
        [
            TokenKind::InterpolationStart(piece("", None)),
            TokenKind::Ident("x".into()),
            TokenKind::InterpolationMiddle(piece("", None)),
            TokenKind::Ident("y".into()),
            TokenKind::InterpolationEnd(piece("", None)),
            TokenKind::End,
        ]
    );
    // A string literal, a nested literal and a record literal inside holes keep their braces.
    assert_eq!(
        kinds("$\"{f \"q}\"}\"").len(),
        5,
        "a quoted brace stays inside the string literal"
    );
    assert_eq!(kinds("$\"<{$\"[{x}]\"}>\"").len(), 6);
    let braces = kinds("$\"{ {a: 1}.a }\"");
    assert_eq!(braces[1], TokenKind::LeftBrace);
    assert_eq!(braces.last(), Some(&TokenKind::End));
    // Brackets and parentheses protect a colon from starting a spec.
    assert!(matches!(kinds("$\"{(x: 1)}\"")[3], TokenKind::Colon));
    assert!(matches!(kinds("\"{x} }{ {{\"")[0], TokenKind::String(_)));
    assert_eq!(lex("$ \"x\"").unwrap_err().code, "E0001");
}

#[test]
fn rejects_malformed_interpolation_text() {
    for source in [
        "$\"a}b\"",
        "$\"{x\"",
        "$\"{x",
        "$\"{x}",
        "$\"{\n}\"",
        "$\"{x // c\n}\"",
        "$\"{/* c */x}\"",
        "$\"\\q{x}\"",
        "$\"{x}\\q\"",
        "$\"{x}\n\"",
        "u8$\"{x}\\uD800\"",
    ] {
        assert_eq!(lex(source).unwrap_err().code, "E0001", "{source:?}");
    }
    let (tokens, errors) = lex_all("let a = $\"{x\nlet b = 1\n");
    assert_eq!(errors.len(), 1);
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == TokenKind::Integer("1".into()))
    );
    let (_, errors) = lex_all("$\"a}b\" $\"{\n\"c\"\n");
    assert!(!errors.is_empty());
}

#[test]
fn parses_holes_and_bounds_nesting() {
    for source in ["$\"{}\"", "$\"a{ }b\"", "$\"{x}{}\""] {
        rejects(source, "E0002");
    }
    let many = |count: usize| format!("let x = 1\n$\"{}\"", "{x}".repeat(count));
    accepts(&many(1024));
    rejects(&many(1025), "E0002");
    let nested = |depth: usize| {
        format!(
            "let x = 1\n{}x{}",
            "$\"{".repeat(depth),
            "}\"".repeat(depth)
        )
    };
    // Deep checking recurses through the type checker, so only the parser is run at the limit.
    accepts(&nested(12));
    assert!(parse(&nested(100)).is_ok());
    assert_eq!(parse(&nested(129)).unwrap_err().code, "E0002");
    for (source, expected) in [
        (
            "let x = 1\n$\"a{ x + 1 }b\"\n",
            "let x = 1\n$\"a{x + 1}b\"\n",
        ),
        ("let x = 1\n$\"a{{b}}\"\n", "let x = 1\n$\"a{{b}}\"\n"),
        (
            "let x = 1\nlet y = f $\"{ x }\" 2\n",
            "let x = 1\nlet y = f $\"{x}\" 2\n",
        ),
        (
            "let x = 1\n$\"{x:>4}{ x }\".length\n",
            "let x = 1\n$\"{x:>4}{x}\".length\n",
        ),
    ] {
        let formatted = format_source("Main.tz", source, SourceKind::Code)
            .unwrap_or_else(|error| panic!("{source}\n{}", error.message))
            .formatted;
        assert_eq!(formatted, expected, "{source}");
        let again = format_source("Main.tz", &formatted, SourceKind::Code).unwrap();
        assert!(!again.changed, "{formatted}");
    }
}

#[test]
fn holes_borrow_evaluate_once_and_keep_owners() {
    for source in [
        "let name = \"Tsuzuri\"\nlet count = 3\nlet text = $\"hello {name}, {count + 1} times {{ok}}\"\nname.length + text.length",
        "record Point { x: i64, label: string }\nlet p = Point { x: 1, label: \"xy\" }\nlet text = $\"{p.label}-{p.x}-{p.label}\"\np.label.length + text.length",
        "def show :: &string -> string\nfn show name = $\"{name}!\"\nlet text = \"ab\"\n(show (&text)).length + text.length",
        "def show :: &mut string -> string\nfn show name = $\"{name}!\"\nlet mut text = \"ab\"\n(show (&mut text)).length",
        "def f :: i64 -> string\nfn f x = $\"{x + 1}|{\"abc\"}|{42}\"\n(f 1).length",
        "def zero :: unit -> i64\nfn zero _unit = 0\ndef f :: i64 -> string\nfn f x = $\"{zero ()}{x}\"\n(f 1).length",
        "def show :: Display<'a> => &'a -> string\nfn show value = $\"<{value}>\"\nlet one = 1\n(show (&one)).length",
        "let a = $\"{1}\"\nlet b = $\"[{$\"({a})\"}]\"\nb.length",
        "let s8 = u8\"é\"\nlet s16 = \"ß\"\nlet c = 'x'\nlet text = u8$\"é{s8}😀{s16}{c}\"\nlet back = $\"{s8}{s16}\"\ntext.length + back.length",
        "record Label { text: string }\ninstance Display<Label> {\n    fn display value = $\"<{value.text}>\"\n}\nlet l = Label { text: \"a\" }\n$\"{l}{l}\".length",
        "let values = [1, 2, 3]\nlet text = $\"{values}|{values[1]}|{(1, \"a\")}\"\ntext.length",
        "let flag = true\nlet text = $\"{flag}{()}{1.5}{'a'}{u8'b'}\"\ntext.length",
    ] {
        accepts(source);
    }
}

#[test]
fn rejects_holes_without_display_or_with_conflicts() {
    rejects("record Plain { x: i64 }\n$\"{Plain { x: 1 }}\"", "E1005");
    rejects(
        "def bump :: &mut i64 -> i64\nfn bump x = 1\nlet mut x = 1\n$\"{x}{bump (&mut x)}\"",
        "E1014",
    );
    rejects("let s = \"a\"\nlet t = to_string s\n$\"{s}\"", "E1012");
    rejects(
        "record Plain { x: i64 }\ndef f :: 'a -> string\nfn f value = $\"{value}\"\nf (Plain { x: 1 })",
        "E1005",
    );
    rejects("$\"{x}\"", "E1002");
}

#[test]
fn emits_one_allocation_and_deterministic_ir() {
    let source = "def f :: &string -> i64 -> string\nfn f s n = $\"a{s}b{n}c\"\nlet text = \"q\"\n(f (&text) 1).length";
    let module = analyze(source).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let start = ir
            .find("define internal %tz.string @tz.fn.Main.f(")
            .expect("f is emitted");
        let body = &ir[start..start + ir[start..].find("\n}\n").unwrap()];
        assert_eq!(body.matches("@tz.string.allocate(").count(), 1, "{body}");
        assert!(!body.contains("@tz.string.concat"), "{body}");
        assert!(!body.contains("@tz.string.new"), "{body}");
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

#[test]
fn plain_literals_keep_their_ir() {
    for (plain, interpolated) in [
        ("\"abc\"", "$\"abc\""),
        ("\"a{b}\"", "$\"a{{b}}\""),
        ("u8\"é\"", "u8$\"é\""),
        ("\"\"", "$\"\""),
    ] {
        let one = analyze(&format!("{plain}.length")).unwrap();
        let two = analyze(&format!("{interpolated}.length")).unwrap();
        for wasm in [false, true] {
            assert_eq!(
                llvm::emit_target(&one, llvm::Entry::Library, wasm).unwrap(),
                llvm::emit_target(&two, llvm::Entry::Library, wasm).unwrap(),
                "{interpolated}"
            );
        }
    }
}

fn spec(source: &str) -> Option<FormatSpec> {
    let tokens = lex(&format!("$\"{{x:{source}}}\""))
        .unwrap_or_else(|error| panic!("{source}\n{}", error.message));
    match &tokens[2].kind {
        TokenKind::InterpolationEnd(piece) => piece.spec,
        other => panic!("{other:?}"),
    }
}

#[test]
fn lexes_format_specs() {
    let strip = |spec: Option<FormatSpec>| {
        spec.map(|spec| FormatSpec {
            span: Span::default(),
            ..spec
        })
    };
    let base = FormatSpec {
        fill: ' ',
        align: None,
        plus: false,
        width: 0,
        precision: None,
        kind: None,
        span: Span::default(),
    };
    assert_eq!(strip(spec("")), None);
    assert_eq!(
        strip(spec("*^+10.3e")),
        Some(FormatSpec {
            fill: '*',
            align: Some(FormatAlign::Center),
            plus: true,
            width: 10,
            precision: Some(3),
            kind: Some(FormatKind::Exponent),
            ..base
        })
    );
    assert_eq!(
        strip(spec(">4")),
        Some(FormatSpec {
            align: Some(FormatAlign::Right),
            width: 4,
            ..base
        })
    );
    assert_eq!(
        strip(spec("0>4")),
        Some(FormatSpec {
            fill: '0',
            align: Some(FormatAlign::Right),
            width: 4,
            ..base
        })
    );
    assert_eq!(
        strip(spec("<<")),
        Some(FormatSpec {
            fill: '<',
            align: Some(FormatAlign::Left),
            ..base
        })
    );
    assert_eq!(
        strip(spec(".2")),
        Some(FormatSpec {
            precision: Some(2),
            ..base
        })
    );
    assert_eq!(
        strip(spec("X")),
        Some(FormatSpec {
            kind: Some(FormatKind::UpperHex),
            ..base
        })
    );
    assert_eq!(
        strip(spec("😀>3")),
        Some(FormatSpec {
            fill: '😀',
            align: Some(FormatAlign::Right),
            width: 3,
            ..base
        })
    );
    assert_eq!(
        strip(spec(".0f")),
        Some(FormatSpec {
            precision: Some(0),
            kind: Some(FormatKind::Fixed),
            ..base
        })
    );
    for text in [
        ".2x",
        "08",
        "e",
        "f",
        "4097",
        ".4097",
        "z",
        "+-",
        ".05",
        ".",
        ".b",
        "5.",
        ">>>",
        " ",
        "\"<4",
        "4096.4097",
    ] {
        let error = lex(&format!("$\"{{x:{text}}}\"")).expect_err(text);
        assert_eq!(error.code, "E0001", "{text}: {}", error.message);
    }
    assert!(lex("$\"{x:4096}\"").is_ok());
    assert!(lex("$\"{x:.4096f}\"").is_ok());
    // The spec stays in the token that closes its hole.
    assert!(matches!(
        &kinds("$\"{x:>4}{y:x}z\"")[2],
        TokenKind::InterpolationMiddle(piece) if piece.spec.is_some()
    ));
}

#[test]
fn checks_specs_against_types() {
    for source in [
        "$\"{\"s\":>4}\"",
        "$\"{\"s\":*^9}\"",
        "record Label { text: string }\ninstance Display<Label> {\n    fn display value = $\"{value.text}\"\n}\nlet r = Label { text: \"ab\" }\n$\"{r:>8}\"",
        "$\"{1.5:+.2e}\"",
        "$\"{7:+}\"",
        "$\"{42:x}{-255:X}{5:b}{8:o}\"",
        "$\"{0.125:.2}{2.5:.0}{1.5f32:.3}{0.1f16:.3}{0.1f128:.20}{0.125d32:.3}\"",
        "let n: i64u = 7i64u\n$\"{n:x}\"",
        "u8$\"{7:>4}{1.25:.1}{\"é\":^5}\"",
    ] {
        accepts(source);
    }
    for source in [
        "$\"{\"s\":x}\"",
        "$\"{1.5:x}\"",
        "$\"{\"s\":+}\"",
        "$\"{7:.2}\"",
        "$\"{true:.2}\"",
        "$\"{'a':+}\"",
        "def f :: Display<'a> => &'a -> string\nfn f value = $\"{value:x}\"",
        "def g :: Display<'a> => &'a -> string\nfn g value = $\"{value:+}\"",
    ] {
        rejects(source, "E1003");
    }
}

const POINT_FORMAT: &str = "record Point { x: i64, y: i64 }\ninstance Format<Point> {\n    fn format point spec =\n        match Format.parse spec with\n        | Option.Some parsed -> Format.pad (&parsed) $\"{point.x},{point.y}\"\n        | Option.None -> \"\"\n}\nlet p = Point { x: 1, y: 2 }\n";

#[test]
fn routes_specs_to_format_instances() {
    for tail in [
        "$\"{p:>8}\"",
        "$\"{p:+}{p:.2}{p:*^9.1f}{p:x}\"",
        "u8$\"{p:>8}{p:.1e}\"",
        "$\"{Point { x: 3, y: 4 }:+}\"",
        "let spec = \"+\"\nFormat.format (&p) (&spec)",
        "let again = $\"{p:+}\"\np.x + again.length",
    ] {
        accepts(&format!("{POINT_FORMAT}{tail}"));
    }
    for source in [
        "union Shade = Light | Dark\ninstance Format<Shade> {\n    fn format shade spec =\n        match shade with\n        | Light -> $\"L{spec}\"\n        | Dark -> $\"D{spec}\"\n}\n$\"{Light:x}{Dark:>3}\"",
        "record Box<'a> { value: 'a }\ninstance Format<'a> => Format<Box<'a>> {\n    fn format boxed spec = $\"box({Format.format (&boxed.value) spec})\"\n}\nrecord Tag { id: i64 }\ninstance Format<Tag> {\n    fn format tag spec = $\"{tag.id}{spec}\"\n}\nlet boxed = Box { value: Tag { id: 7 } }\n$\"{boxed:+}\"",
        "record Tagged { id: i64 } deriving (Display)\nlet t = Tagged { id: 7 }\n$\"{t:>20}{t}\"",
        "union Tone = Quiet | Loud deriving (Display)\n$\"{Quiet}{Loud}\"",
        "let text = \"*>+8.2f\"\nmatch Format.parse (&text) with\n| Option.Some spec -> Format.pad (&spec) \"x\"\n| Option.None -> \"\"",
    ] {
        accepts(source);
    }
    for (source, code) in [
        (
            "record Point { x: i64 } deriving (Display)\nlet p = Point { x: 1 }\n$\"{p:+}\"",
            "E1005",
        ),
        (
            "record Point { x: i64 } deriving (Display)\nlet p = Point { x: 1 }\n$\"{p:.2}\"",
            "E1005",
        ),
        (
            "union Shade = Light | Dark deriving (Display)\n$\"{Light:x}\"",
            "E1005",
        ),
        (
            "record Point { x: i64 }\ninstance Format<Point> {\n    fn format _point _spec = \"\"\n}\nlet p = Point { x: 1 }\n$\"{p}\"",
            "E1005",
        ),
        (
            "record Other { x: i64 }\nlet q = Other { x: 1 }\n$\"{q:>4}\"",
            "E1005",
        ),
        (
            "def f :: Format<'a> => &'a -> string\nfn f value = $\"{value:.2}\"",
            "E1003",
        ),
        ("record Format { x: i64 }", "E1001"),
        ("union Format = One | Two", "E1001"),
        // Only a record or union declared in the program can implement Format; a hole never reaches any other.
        (
            "instance Format<i64> {\n    fn format _value _spec = \"\"\n}",
            "E1016",
        ),
        (
            "instance Format<string> {\n    fn format _value _spec = \"\"\n}",
            "E1016",
        ),
        (
            "instance Format<Option<i64>> {\n    fn format _value _spec = \"\"\n}",
            "E1016",
        ),
        // An unknown name next to an instance used to panic in instance matching.
        (
            "record Point { x: i64 }\ninstance Display<Point> {\n    fn display point = to_string point.x\n}\nDisplay.display (&missing)",
            "E1002",
        ),
        (
            "record Point { x: i64 }\ninstance Format<Point> {\n    fn format _point _spec = \"\"\n}\nlet spec = \"x\"\nFormat.format (&missing) (&spec)",
            "E1002",
        ),
    ] {
        rejects(source, code);
    }
    let error =
        tsuzuri::analyze_modules(&[("Main.tz", "0"), ("Format.tz", "def f :: i64\nfn f = 1")])
            .unwrap_err();
    assert_eq!(error.code, "E1011", "{}", error.message);
    let error =
        analyze("instance Format<i64> {\n    fn format _value _spec = \"\"\n}").unwrap_err();
    assert_eq!(
        error.message,
        "only records and unions declared in this program can implement Format"
    );
}

#[test]
fn format_holes_pass_the_spec_text_and_leave_padding_to_the_instance() {
    let module = analyze(&format!("{POINT_FORMAT}$\"{{p:>8.2}}\"")).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        // ">8.2" as UTF-16 units, borrowed by the instance and never copied.
        assert!(
            ir.contains("[4 x i16] [i16 62, i16 56, i16 46, i16 50]"),
            "{ir}"
        );
        // The runtime defines these helpers; the hole must not call them.
        assert!(!ir.contains("call i32 @tz_soft_format_spec("), "{ir}");
        assert!(!ir.contains("call i64 @tz.format.fill."), "{ir}");
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
    // A type with only `Display` is still padded by the compiler.
    let padded = analyze(
        "record Tagged { id: i64 } deriving (Display)\nlet t = Tagged { id: 7 }\n$\"{t:>20}\"",
    )
    .unwrap();
    let ir = llvm::emit_target(&padded, llvm::Entry::Library, false).unwrap();
    assert!(ir.contains("call i64 @tz.format.fill."), "{ir}");
}
