use tsuzuri::{analyze, lexer, llvm};

fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let mut native = String::new();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        let declarations: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("declare "))
            .collect();
        let unique: std::collections::BTreeSet<_> = declarations.iter().collect();
        assert_eq!(declarations.len(), unique.len());
        if !wasm {
            native = ir;
        }
    }
    native
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn module_length_functions_borrow_and_reuse_property_lowering() {
    accepts(
        r#"
def measure :: (ref 'a -> i64) -> ref 'a -> i64
fn measure get_length text = get_length text
let text = "😀\uD800\0"
let bytes = u8"😀\0"
let text_length: ref string -> i64 = String.length
let byte_length: ref utf8string -> i64 = Utf8String.length
assert (String.length ref text == text.length)
assert (Utf8String.length ref bytes == bytes.length)
assert (text_length ref text == 4)
assert (byte_length ref bytes == 5)
assert ((ref text |> String.length) == 4)
assert ((ref bytes |> Utf8String.length) == 5)
assert (measure String.length (ref text) == 4)
assert (measure Utf8String.length (ref bytes) == 5)
text.length + bytes.length
"#,
    );
    for (namespace, ty, literal) in [
        ("String", "string", r#""owned""#),
        ("Utf8String", "utf8string", r#"u8"owned""#),
    ] {
        let ir = accepts(&format!(
            "def by_module :: ref {ty} -> i64\n\
             fn by_module text = {namespace}.length text\n\
             def by_property :: ref {ty} -> i64\n\
             fn by_property text = text.length"
        ));
        let body = |name: &str| {
            ir.split(&format!("define internal i64 @tz.fn.{name}("))
                .nth(1)
                .unwrap()
                .split_once("{\n")
                .unwrap()
                .1
                .split("\n}\n")
                .next()
                .unwrap()
        };
        let length = body(&format!("{namespace}.length"));
        assert_eq!(length, body("Main.by_property"));
        assert!(!length.contains("call "), "{length}");
        rejects(
            &format!("let text = {literal}\n{namespace}.length text"),
            "E1003",
        );
        rejects(
            &format!("let text = {literal}\nlet moved = text\n{namespace}.length ref text"),
            "E1012",
        );
    }
    rejects(r#"let text = u8"x"; String.length ref text"#, "E1003");
    rejects(r#"let text = "x"; Utf8String.length ref text"#, "E1003");
}

#[test]
fn ordinary_literals_are_always_utf16_and_u8_literals_are_always_utf8() {
    accepts(
        r#"
def unit_at :: ref string -> i64 -> i16u
fn unit_at text index = text[index]
def byte_at :: ref utf8string -> i64 -> ubyte
fn byte_at text index = text[index]
let text: string = "日本語😀"
let bytes: utf8string = u8"日本語😀"
let mut total = 0
for unit in text do
    let typed: i16u = unit
    total = total + typed as i64
for byte in bytes do
    let typed: ubyte = byte
    total = total + typed as i64
total + unit_at (ref text) 0 as i64 + byte_at (ref bytes) 0 as i64
"#,
    );
    for source in [
        r#"let value: utf8string = "no implicit encoding""#,
        r#"let value: string = u8"no implicit decoding""#,
        r#"def take :: utf8string -> i64
           fn take text = text.length
           take "ordinary literal""#,
        r#"let texts: [utf8string] = ["ordinary literal"]"#,
        r#"let unit: ubyte = "a"[0]"#,
        r#"let byte: i16u = u8"a"[0]"#,
        r#""a" + u8"b""#,
        r#""a" == u8"a""#,
        r#"let bytes = u8"42"
           let parsed: Option<i64> = Parse.parse ref bytes
           0"#,
    ] {
        rejects(source, "E1003");
    }
}

#[test]
fn utf16_escapes_preserve_code_units_but_utf8_keeps_scalar_only_escapes() {
    for literal in [
        r#""\u0000\u007f\u0080\u07FF\u0800\uFFFF""#,
        r#""\uD800\uDC00\uDFFF\uDBFF""#,
        r#""\u{D800}\u{DFFF}\u{10000}\u{10FFFF}""#,
        r#""😀\uD83D\uDE00\u{1f600}""#,
        r#""\u{000000000000}\u{00000000001f600}\u{00000000D800}\u{00000010FFFF}""#,
        r#"u8"\u{0}\u{e9}\u{1f600}\u{10FFFF}\0\n\r\t\\\"""#,
        r#"u8"\u{000061}""#,
    ] {
        accepts(&format!("let text = {literal}\ntext.length"));
    }
    for literal in [
        r#""\u12""#,
        r#""\uZZZZ""#,
        r#""\u{}""#,
        r#""\u{110000}""#,
        r#""\u{0000000000110000}""#,
        r#""\u{100000000000000000000000}""#,
        r#""\q""#,
        r#"u8"\u0061""#,
        r#"u8"\uD800""#,
        r#"u8"\uD83D\uDE00""#,
        r#"u8"\u{D800}""#,
        r#"u8"\u{DFFF}""#,
        r#"u8"\u{110000}""#,
        r#"u8"\u{0000000}""#,
        r#"u8"\u{00000001f600}""#,
        r#"u8"\q""#,
    ] {
        rejects(literal, "E0001");
    }
}

#[test]
fn invalid_utf8_escapes_recover_without_skipping_following_tokens() {
    for literal in [
        r#"u8"\q""#,
        r#"u8"é\q""#,
        r#"u8"\u0041""#,
        r#"u8"\u{D800}""#,
        r#"u8"\u{110000}""#,
        r#"u8"\u{0000000}""#,
        r#"u8"\u{é}""#,
        r#"u8"\u{bad""#,
        "u8\"unterminated",
    ] {
        let source = format!("{literal}\n42\n?");
        let (tokens, diagnostics) = lexer::lex_all(&source);
        assert_eq!(diagnostics.len(), 2, "{source}\n{diagnostics:?}");
        assert!(diagnostics.iter().all(|error| error.code == "E0001"));
        assert_eq!(lexer::lex(&source).unwrap_err(), diagnostics[0]);
        assert_eq!(diagnostics[1].span.start, source.len() - 1);
        assert!(
            tokens
                .iter()
                .any(|token| &source[token.span.start..token.span.end] == "42"),
            "{source}\n{tokens:?}"
        );
        assert_eq!(tokens.last().unwrap().kind, tsuzuri::syntax::TokenKind::End);
        for diagnostic in diagnostics {
            assert!(source.is_char_boundary(diagnostic.span.start));
            assert!(source.is_char_boundary(diagnostic.span.end));
        }
    }
}

#[test]
fn conversions_and_repair_are_explicit_borrowing_functions_and_first_class_values() {
    accepts(
        r#"
let decode: ref utf8string -> string = String.from_utf8
let encode: ref string -> utf8string = Utf8String.from_string
let clone_bytes: ref utf8string -> utf8string = Utf8String.clone
let valid: ref string -> bool = String.is_well_formed
let repair: ref string -> string = String.to_well_formed
let bytes = u8"😀"
let text = decode ref bytes
let copied = clone_string ref text
let encoded = encode ref copied
let again = clone_bytes ref encoded
let repaired = repair ref text
assert (valid ref repaired)
assert (text == copied && bytes == again)
let shown: string = Display.display ref bytes
let consumed: string = to_string again
text.length + bytes.length + shown.length + consumed.length
"#,
    );
    for source in [
        r#"let text = "x"
           Utf8String.clone ref text"#,
        r#"let bytes = u8"x"
           clone_string ref bytes"#,
        r#"let text = "x"
           String.from_utf8 ref text"#,
        r#"let bytes = u8"x"
           Utf8String.from_string ref bytes"#,
        r#"let bytes = u8"x"
           String.is_well_formed ref bytes"#,
        r#"let bytes = u8"x"
           String.to_well_formed ref bytes"#,
    ] {
        rejects(source, "E1003");
    }
}

#[test]
fn strings_support_ord_and_both_encodings_support_add_eq_and_display() {
    accepts(
        r#"
def less :: Ord<'a> => 'a -> 'a -> bool
fn less left right = left < right
def join :: Add<'a> => 'a -> 'a -> 'a
fn join left right = Add.add left right
def show :: Display<'a> => ref 'a -> string
fn show value = Display.display value
let text = join "😀" "\uD800"
let bytes = join u8"😀" u8"é"
let output = show ref bytes
let compare: string -> string -> bool = Ord.lt
assert (less "\uD800" "\uE000")
assert (compare "a" "b")
assert (Eq.eq (clone_string ref text) (clone_string ref text))
assert (Eq.eq (Utf8String.clone ref bytes) (Utf8String.clone ref bytes))
text == text && bytes == bytes && output.length == 3
"#,
    );
    for source in [
        r#"u8"a" < u8"b""#,
        r#"u8"a" <= u8"b""#,
        r#"u8"a" > u8"b""#,
        r#"u8"a" >= u8"b""#,
        r#"Ord.lt u8"a" u8"b""#,
        r#"def less :: Ord<'a> => 'a -> 'a -> bool
           fn less left right = left < right
           less u8"a" u8"b""#,
    ] {
        rejects(source, "E1005");
    }
}

#[test]
fn both_encodings_are_noncopy_and_keep_comparison_and_index_borrows() {
    for (ty, literal) in [("string", r#""x""#), ("utf8string", r#"u8"x""#)] {
        for source in [
            format!("let first = {literal}\nlet second = first\nfirst.length"),
            format!("let first = {literal}\nlet second = to_string first\nfirst.length"),
            format!("let text = {literal}\nlet rendered = text |> to_string\ntext.length"),
            format!(
                "def consume :: {ty} -> i64\nfn consume text = text.length\n\
                 let text = {literal}\nlet length = text |> consume\ntext.length"
            ),
            format!("let text = {literal}\ntext + text"),
            format!("def f :: ref {ty} -> {ty}\nfn f value = deref value"),
            format!("let values = [{literal}]\nlet moved = values[0]\n0"),
        ] {
            rejects(&source, "E1012");
        }
        rejects(
            &format!(
                "def twice :: Copy<'a> => 'a -> ('a * 'a)\nfn twice value = (value, value)\ntwice {literal}"
            ),
            "E1005",
        );
        rejects(
            &format!("let text = {literal}\ntext == {{ let moved = text; moved }}"),
            "E1014",
        );
        rejects(
            &format!("let text = {literal}\ntext[{{ let moved = text; 0 }}]"),
            "E1014",
        );
    }
    rejects(
        r#"let text = "x"
           text < { let moved = text; moved }"#,
        "E1014",
    );
    let ir = accepts("def transfer :: string -> string\nfn transfer text = to_string text");
    let transfer = ir
        .split(" @tz.fn.Main.transfer(")
        .nth(1)
        .unwrap()
        .split("\n}\n")
        .next()
        .unwrap();
    assert!(!transfer.contains("call %tz.string @tz.string.new"));
}

#[test]
fn string_patterns_use_exact_units_without_normalization() {
    let source = r#"
def classify :: string -> i64
fn classify text =
    match text with
    | "😀" -> 1
    | "\uD83D\uDE00" -> 2
    | "é" -> 3
    | "e\u0301" -> 4
    | "\uD800" -> 5
    | "\uD801" -> 6
    | "\uDC00" -> 7
    | "\uDC01" -> 8
    | _ -> 0
classify "\u{1f600}"
"#;
    let module = analyze(source).unwrap();
    assert_eq!(module.warnings.len(), 1);
    assert_eq!(module.warnings[0].code, "W1003");
    let span = module.warnings[0].span;
    assert_eq!(&source[span.start..span.end], r#""\uD83D\uDE00""#);
    accepts(source);
    for (first, second) in [
        (r#""😀""#, r#""\u{00000000001f600}""#),
        (r#""\uD800""#, r#""\u{00000000D800}""#),
    ] {
        let source = format!("match {first} with | {first} -> 1 | {second} -> 2 | _ -> 0");
        let module = analyze(&source).unwrap();
        assert_eq!(module.warnings.len(), 1, "{source}");
        assert_eq!(module.warnings[0].code, "W1003");
    }
    accepts(r#"match u8"😀" with | u8"\u{1f600}" -> 1 | _ -> 0"#);
    rejects(r#"match u8"x" with | "x" -> 1 | _ -> 0"#, "E1003");
    rejects(r#"match "x" with | u8"x" -> 1 | _ -> 0"#, "E1003");
    rejects(r#"match "\uD800" with | "\uD800" -> 1"#, "E1021");
}

#[test]
fn neither_encoding_is_exportable_through_the_scalar_c_abi() {
    for (ty, literal) in [("string", r#""x""#), ("utf8string", r#"u8"x""#)] {
        rejects(
            &format!("export def f :: {ty} -> i64\nfn f text = text.length"),
            "E1008",
        );
        rejects(&format!("export def f :: {ty}\nfn f = {literal}"), "E1008");
    }
}

#[test]
fn string_reference_fixture_lowers_deterministically_for_native_and_wasm() {
    accepts(include_str!("fixtures/strings/Main.tz"));
}

#[test]
fn programs_without_strings_still_lower_array_and_closure_allocation() {
    accepts(include_str!("fixtures/strings/no_text/Main.tz"));
}
