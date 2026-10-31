// D14: the Text class of std/Text.tt and its instances for string and utf8string (std/String.tz, std/Utf8String.tz).
// The values of the methods are tested by `node tests/features.mjs <compiler> text_class`, and the shape of the
// generated code by `node tests/text_class.mjs`; this file tests what type checking accepts and reports.
use tsuzuri::{analyze, analyze_modules, llvm};

fn checks(source: &str) {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
}

/// The program type-checks; the library IR of both targets is deterministic and has no duplicate declaration.
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
        assert!(llvm::duplicate_declarations(&ir).is_empty(), "{ir}");
        if !wasm {
            native = ir;
        }
    }
    native
}

fn rejects(source: &str, code: &str, message: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert!(
        error.message.contains(message),
        "{source}\n{}",
        error.message
    );
}

fn rejects_modules(sources: &[(&str, &str)], code: &str, message: &str) {
    let error = analyze_modules(sources).expect_err(&format!("{sources:?}"));
    assert_eq!(error.code, code, "{sources:?}\n{}", error.message);
    assert!(
        error.message.contains(message),
        "{sources:?}\n{}",
        error.message
    );
}

const LITERAL_LINES: &str = r#"let m01 = Text.length (ref "ab")
let m02 = Text.is_empty (ref "ab")
let m03 = Text.compare (ref "a") (ref "b")
let m04 = Text.clone (ref "ab")
let m05 = Text.find (ref "a") (ref "ab")
let m06 = Text.rfind (ref "a") (ref "ab")
let m07 = Text.contains (ref "a") (ref "ab")
let m08 = Text.starts_with (ref "a") (ref "ab")
let m09 = Text.ends_with (ref "b") (ref "ab")
let m10 = Text.slice 0 1 (ref "abc")
let m11 = Text.sub 0 1 (ref "abc")
let m12 = Text.join (ref ",") (ref ["a", "b"])
let m13 = Text.concat (ref ["a", "b"])
let m14 = Text.split (ref ",") (ref "a,b")
let m15 = Text.trim (ref " a ")
let m16 = Text.trim_start (ref " a ")
let m17 = Text.trim_end (ref " a ")
let m18 = Text.to_ascii_lower (ref "A")
let m19 = Text.to_ascii_upper (ref "a")
let m20 = Text.repeat 2 (ref "ab")
let m21 = Text.replace (ref "a") (ref "b") (ref "aa")
let d01 = String.compare (ref "a") (ref "b")
let d02 = String.repeat 2 (ref "ab")
let d03 = String.replace (ref "a") (ref "b") (ref "aa")
let d04 = String.find (ref "a") (ref "ab")
let d05 = String.slice 0 1 (ref "abc")
let d06 = Utf8String.compare (ref u8"a") (ref u8"b")
let m22 = Text.compare (ref u8"a") (ref u8"b")
let m23 = Text.repeat 2 (ref u8"ab")
let m24 = Text.find (ref u8"a") (ref u8"ab")
"#;

const AUTO_BORROWED_LINES: &str = r#"let m03 = Text.compare "a" "b"
let m05 = Text.find "a" "ab"
let m07 = Text.contains "a" "ab"
let m10 = Text.slice 0 1 "abc"
let m12 = Text.join "," ["a", "b"]
let m14 = Text.split "," "a,b"
let m20 = Text.repeat 2 "ab"
let m21 = Text.replace "a" "b" "aa"
let m22 = Text.repeat 2 u8"ab"
let m23 = Text.contains u8"a" u8"ab"
let m24 = Text.contains (ref "a") "ab"
let m25 = Text.contains "a" (ref "ab")
"#;

const USER_INSTANCE: &str = include_str!("fixtures/text_user_instance/Main.tz");

#[test]
fn every_method_type_checks_for_both_encodings() {
    // 20 methods and is_empty on a string and a utf8string, and generic functions over Text.
    accepts(include_str!("fixtures/text_class_calls/Main.tz"));
    accepts(include_str!("fixtures/text_class/Main.tz"));
}

#[test]
fn generic_code_compares_texts_through_the_ord_superclass() {
    accepts(
        "def earlier :: Text<'s> => ref 's -> ref 's -> bool\nfn earlier left right = deref left < deref right\n\n\
         export def a :: bool\nfn a = earlier \"apple\" \"banana\"\n\
         export def b :: bool\nfn b = earlier u8\"banana\" u8\"apple\"\n",
    );
}

#[test]
fn every_method_takes_a_literal_argument_without_a_let() {
    for line in LITERAL_LINES.lines().chain(AUTO_BORROWED_LINES.lines()) {
        checks(&format!("{line}\n0\n"));
    }
}

#[test]
fn partial_application_and_loan_returning_methods_still_need_a_let() {
    let borrow = "borrow requires a local place";
    rejects("let f = Text.find \"a\"\n0\n", "E1013", borrow);
    // The same rule for a class of the user (a method that returns a reference may hold a loan on a temporary).
    let class = "class Pick<'a> {\n    def pick :: ref 'a -> ref 'a -> ref 'a\n    def both :: ref 'a -> ref 'a -> i64\n}\n";
    let main = "instance Pick<string> {\n    fn pick left right = left\n    fn both left right = left.length + right.length\n}\n\nlet total = Pick.both \"ab\" \"cde\"\ntotal\n";
    analyze_modules(&[("Main.tz", main), ("Traits.tt", class)]).unwrap();
    let kept = main.replace("total\n", "let kept = Pick.pick \"ab\" \"cde\"\ntotal\n");
    rejects_modules(&[("Main.tz", &kept), ("Traits.tt", class)], "E1013", borrow);
}

const STAGES_CLASS: &str = include_str!("fixtures/text_class_stages/Traits.tt");
const STAGES: &str = include_str!("fixtures/text_class_stages/Main.tz");

/// Review of PR #73: whether a method call applies every argument at once is a property of the implementation that
/// the instance chose, as it is of a function. An implementation that runs a first stage before it takes the rest
/// (`fn pick text = { ...; \n -> ... }`) must see that stage run before the later arguments are evaluated, so a
/// temporary borrowed by its first argument needs a `let`, as it does for such a function. The order that the
/// fixture prints at run time is checked by `node tests/text_class.mjs`.
#[test]
fn a_staged_instance_method_still_needs_a_let_for_a_temporary() {
    let borrow = "borrow requires a local place";
    analyze_modules(&[("Main.tz", STAGES), ("Traits.tt", STAGES_CLASS)]).unwrap();
    let staged = STAGES.replace(
        "let word = \"ab\"\nlet staged = Stage.pick word",
        "let staged = Stage.pick \"ab\"",
    );
    assert_ne!(staged, STAGES);
    rejects_modules(
        &[("Main.tz", &staged), ("Traits.tt", STAGES_CLASS)],
        "E1013",
        borrow,
    );
    // The same staged body in a function, for comparison.
    rejects(
        "def pick :: ref string -> i64 -> i64\nfn pick text = { let first = Debug.trace 1; \\n -> text.length + n + first }\n\nlet total = pick \"ab\" (Debug.trace 2)\ntotal\n",
        "E1013",
        borrow,
    );
    // In a generic body the instance is not known yet, so a temporary needs a `let` for a method of two arguments.
    let generic = "def has :: Text<'s> => ref 's -> ref 's -> bool\nfn has needle text = Text.contains (ref (Text.trim needle)) text\n\n0\n";
    rejects(generic, "E1013", borrow);
    checks(&generic.replace(
        "fn has needle text = Text.contains (ref (Text.trim needle)) text",
        "fn has needle text =\n    let trimmed = Text.trim needle\n    Text.contains trimmed text",
    ));
}

#[test]
fn a_user_type_can_be_a_text() {
    checks(USER_INSTANCE);
    // The superclass Ord (and its superclass Eq) is required.
    let no_ord = USER_INSTANCE.replace(
        "record Name { value: string } deriving (Eq, Ord)",
        "record Name { value: string }",
    );
    assert_ne!(no_ord, USER_INSTANCE);
    rejects(&no_ord, "E1027", "no instance for Eq<Main.Name>");
    // Every method except is_empty is required.
    let no_trim_end = USER_INSTANCE.replace(
        "\tfn trim_end text = Name { value: String.trim_end (ref text.value) }\n",
        "",
    );
    assert_ne!(no_trim_end, USER_INSTANCE);
    rejects(&no_trim_end, "E1016", "missing method 'Text.Text.trim_end'");
}

#[test]
fn wrong_uses_report_the_existing_diagnostics() {
    rejects(
        "let n = Text.length (ref 5)\nn\n",
        "E1005",
        "no instance for Text.Text<i32>",
    );
    rejects(
        "let found = Text.find \"a\" u8\"abc\"\n0\n",
        "E1003",
        "expected string, found utf8string",
    );
    rejects(
        "let flipped = Text.reverse \"abc\"\n0\n",
        "E1002",
        "type class 'Text' has no method 'reverse'",
    );
    rejects(
        "def size :: dyn Text -> i64\nfn size text = 0\n\n0\n",
        "E1028",
        "dyn Text is not allowed",
    );
}

#[test]
fn text_is_a_reserved_module_and_namespace() {
    let reserved = "a name reserved for the standard library";
    rejects_modules(&[("Main.tz", "namespace Text\n\n0\n")], "E1011", reserved);
    rejects_modules(
        &[("Main.tz", "namespace Text::Tools\n\n0\n")],
        "E1011",
        reserved,
    );
}

#[test]
fn declarations_named_text_coexist_with_the_class() {
    // A record: the std class is still Text.f.
    checks(
        "record Text { value: string }\ndef length_of :: ref Text -> i64\nfn length_of text = text.value.length\nlet a = Text { value: \"abc\" }\nlet b = Text.length (ref \"abcd\")\nlength_of (ref a) + b\n",
    );
    // A union: Text.f names the union, so the std class is Text.Text.f.
    checks(
        "union Text = Plain | Rich\n\ndef label :: Text -> i64\nfn label text = match text with\n    | Plain -> 1\n    | Rich -> 2\n\nlabel Plain + Text.Text.length \"abc\"\n",
    );
    rejects(
        "union Text = Plain | Rich\nlet size = Text.length \"abc\"\n0\n",
        "E1002",
        "union 'Main.Text' has no case 'length'",
    );
    // A class: its methods are Text.size, and the std class is Text.Text.length.
    let class = "class Text<'a> {\n    def size :: ref 'a -> i64\n}\n";
    let main = "instance Text<string> {\n    fn size value = value.length\n}\n\ndef twice :: Text<'a> => ref 'a -> i64\nfn twice value = Text.size value * 2\n\ndef std_length :: Text.Text<'a> => ref 'a -> i64\nfn std_length value = Text.Text.length value\n\nlet word = \"abc\"\nlet other = u8\"abcd\"\ntwice (ref word) + std_length (ref word) + std_length (ref other)\n";
    analyze_modules(&[("Main.tz", main), ("Traits.tt", class)]).unwrap();
}

#[test]
fn unused_text_instances_never_reach_the_ir() {
    let ir = accepts(
        "export def answer :: i64\nfn answer = String.length \"abc\" + Utf8String.length u8\"abcd\"\n",
    );
    assert!(!ir.contains("@tz.fn.String.$instance"), "{ir}");
    assert!(!ir.contains("@tz.fn.Utf8String.$instance"), "{ir}");
}

#[test]
fn regex_text_functions_take_both_encodings() {
    accepts(include_str!("fixtures/regex_text/Main.tz"));
}

#[test]
fn regex_text_functions_reject_a_text_without_an_instance() {
    rejects(
        "let re = Result.get (Regex.compile (ref \"a\"))\nlet n = 5\nlet found = Regex.is_match_text (ref re) (ref n)\nfound\n",
        "E1005",
        "no instance for RegexText.RegexText<i32>",
    );
}

#[test]
fn the_regex_text_class_is_named_with_its_module_outside_std() {
    rejects(
        "def count :: RegexText<'s> => ref Regex -> ref 's -> i64\nfn count re text = (Regex.find_all_text re text).length\n\n0\n",
        "E1016",
        "unknown type class 'RegexText'",
    );
    rejects(
        "let re = Result.get (Regex.compile (ref \"a\"))\nlet found = RegexText.find_all (ref re) (ref \"a1\")\nfound.length\n",
        "E1002",
        "module 'RegexText' has no function or union case 'find_all'",
    );
    checks(
        "def count :: RegexText.RegexText<'s> => ref Regex -> ref 's -> i64\nfn count re text = (Regex.find_all_text re text).length\n\n0\n",
    );
    checks(
        "let re = Result.get (Regex.compile (ref \"a\"))\nlet found = RegexText.RegexText.find_all (ref re) (ref \"a1a\")\nfound.length\n",
    );
}
