use tsuzuri::{analyze, lexer, llvm, parser, syntax::TokenKind};

#[test]
fn character_literals_are_distinct_from_type_variables_and_encodings() {
    let tokens = lexer::lex("'a 'abc 'a' '\\n' '\\uD800' u8'\\u{1F600}'").unwrap();
    assert!(matches!(&tokens[0].kind, TokenKind::TypeVariable(name) if name == "a"));
    assert!(matches!(&tokens[1].kind, TokenKind::TypeVariable(name) if name == "abc"));
    assert!(matches!(tokens[2].kind, TokenKind::Char(97)));
    assert!(matches!(tokens[3].kind, TokenKind::Char(10)));
    assert!(matches!(tokens[4].kind, TokenKind::Char(0xd800)));
    assert!(matches!(tokens[5].kind, TokenKind::Utf8Char(0x1f600)));
    for source in [
        "''",
        "'ab'",
        "u8''",
        "u8'ab'",
        "'\\u{1F600}'",
        "u8'\\u{D800}'",
        "u8'\\u0041'",
        "'\\u{}'",
        "'\\u{110000}'",
        "'\\q'",
        "'\n'",
    ] {
        assert_eq!(lexer::lex(source).unwrap_err().code, "E0001", "{source}");
    }
    for source in [
        "identity 'a'",
        "match 'a' with | 'a' -> 1 | _ -> 0",
        "match u8'a' with | u8'a' -> 1 | _ -> 0",
        "def identity :: 'a -> 'a\nfn identity value = value",
    ] {
        parser::parse(source).unwrap();
    }
}

#[test]
fn character_types_compare_without_numeric_coercion() {
    for source in [
        "'A' < 'a' && '\\uD800' > 'a'",
        "u8'\\u{1F600}' > u8'a'",
        "def classify :: char -> i64\nfn classify value = match value with | 'a' -> 1 | '\\uD800' -> 2 | _ -> 0",
        "def classify :: utf8char -> i64\nfn classify value = match value with | u8'a' -> 1 | u8'\\u{1F600}' -> 2 | _ -> 0",
        "def identity :: 'a -> 'a\nfn identity value = value\nidentity 'a' == 'a'",
    ] {
        let module = analyze(source).unwrap();
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for (source, code) in [
        ("'a' == u8'a'", "E1003"),
        ("'a' as i16u", "E1005"),
        ("1i16u as char", "E1005"),
        ("'a' + 'b'", "E1005"),
        ("u8'a' + u8'b'", "E1005"),
        ("export def value :: char\nfn value = 'a'", "E1008"),
        ("let value: char = u8'a'\nvalue", "E1003"),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}

#[test]
fn character_modules_use_explicit_typed_conversions() {
    for source in [
        "Char.to_u16 (Char.to_ascii_upper 'a')",
        "Char.of_u16 55296i16u == '\\uD800'",
        "Utf8Char.to_u32 (Utf8Char.to_ascii_lower u8'A')",
        "Maybe.get (Utf8Char.of_u32 128512i32u) == u8'\\u{1F600}'",
        "Utf8Char.of_u32_unchecked 65i32u == u8'A'",
        "Char.is_ascii_digit '5' && Utf8Char.is_ascii_alphabetic u8'z'",
    ] {
        let module = analyze(source).unwrap();
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
}

#[test]
fn character_display_parse_and_console_lowering() {
    for source in [
        "let value = '\\uD800'\nlet text = to_string value\nlet parsed: Maybe<char> = Parse.parse (ref text)\nMaybe.get parsed == value",
        "let value = u8'\\u{1F600}'\nlet text = to_string value\nlet parsed: Maybe<utf8char> = Parse.parse (ref text)\nMaybe.get parsed == value",
    ] {
        let module = analyze(source).unwrap();
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for source in ["'a'", "u8'\\u{1F600}'", "'\\uD800'"] {
        let module = analyze(source).unwrap();
        let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
        assert_eq!(
            ir.matches("define internal i32 @tz.console.write").count(),
            1
        );
        assert!(ir.contains("@tz.character.utf8"));
    }
}
