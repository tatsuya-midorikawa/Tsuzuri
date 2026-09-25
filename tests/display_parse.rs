use tsuzuri::{analyze, analyze_modules_with_std, llvm};

fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        for external in ["@printf", "@memcmp", "@strtod", "@strtof", "@snprintf"] {
            assert!(!ir.contains(external), "{external}");
        }
        let declarations: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("declare "))
            .collect();
        let unique: std::collections::BTreeSet<_> = declarations.iter().collect();
        assert_eq!(declarations.len(), unique.len());
    }
    llvm::emit(&module, llvm::Entry::Library).unwrap()
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn provides_display_for_primitives_and_parse_for_numbers_and_bool() {
    for ty in [
        "i8", "i16", "i32", "i64", "i128", "i8u", "i16u", "i32u", "i64u", "i128u", "f16", "f32",
        "f64", "f128", "d32", "d64", "d128", "bool",
    ] {
        accepts(&format!(
            "def show :: {ty} -> string\nfn show value = to_string value
             def display :: &{ty} -> string\nfn display value = Display.display value
             def parse :: &string -> Option<{ty}>\nfn parse text = Parse.parse text"
        ));
    }
    accepts("to_string ()");
    accepts("let text = \"abc\"\nDisplay.display (&text)");
    let ir = accepts("to_string 42");
    assert!(ir.contains("@tz_soft_format"));
    assert!(
        accepts("def parse :: &string -> Option<i64>\nfn parse s = Parse.parse s")
            .contains("@tz_soft_parse")
    );
}

#[test]
fn method_values_and_generic_constraints_use_normal_specialization() {
    accepts(
        "def render :: Display<'a> => 'a -> string
         fn render value = to_string value
         let show = Display.display
         let value = 42
         show (&value) + render true",
    );
    accepts(
        "def parse :: Parse<'a> => &string -> Option<'a>
         fn parse text = Parse.parse text
         let parser: &string -> Option<i64> = Parse.parse
         let text = \"42\"
         let first = parser (&text)
         let second: Option<i64> = parse (&text)
         Option.get first + Option.get second",
    );
}

#[test]
fn user_instances_are_used_by_to_string_and_parse() {
    let ir = accepts(
        "record Label { text: string }
         instance Display<Label> {
             fn display value = clone_string (&value.text)
         }
         instance Parse<Label> {
             fn parse text = Some (Label { text: clone_string text })
         }
         let text = \"owned\"
         let parsed: Option<Label> = Parse.parse (&text)
         to_string (Option.get parsed)",
    );
    assert!(ir.contains("$instance."));
    accepts(
        "union Flag = On | Off
         instance Display<Flag> {
             fn display flag = match flag with | On -> \"on\" | Off -> \"off\"
         }
         to_string On",
    );
    rejects(
        "record Label { text: string }
         instance Display<Label> {
             fn display value = to_string (Label { text: clone_string (&value.text) })
         }
         0",
        "E1019",
    );
}

#[test]
fn rejects_missing_or_overridden_instances_and_preserves_ownership() {
    for (source, code) in [
        (
            "record R { x: i64 }\ndef f :: R -> string\nfn f r = to_string r",
            "E1005",
        ),
        (
            "def f :: &string -> Option<string>\nfn f text = Parse.parse text",
            "E1005",
        ),
        (
            "def f :: &string -> Option<unit>\nfn f text = Parse.parse text",
            "E1005",
        ),
        ("def to_string :: i64\nfn to_string = 1", "E1001"),
        ("instance Display<i64> { fn display x = \"\" }", "E1016"),
        ("instance Parse<bool> { fn parse x = None }", "E1016"),
        (
            "let text = \"owned\"\nlet rendered = to_string text\ntext.length",
            "E1012",
        ),
    ] {
        rejects(source, code);
    }
    accepts(
        "let text = \"owned\"
         let rendered = Display.display (&text)
         let parsed: Option<i64> = Parse.parse (&text)
         text.length + rendered.length",
    );
    let ir = accepts("def transfer :: string -> string\nfn transfer text = to_string text");
    assert!(!ir.contains("call %tz.string @tz.string.new"));
    assert!(
        accepts("def copy :: &string -> string\nfn copy text = Display.display text")
            .contains("call %tz.string @tz.string.new")
    );
}

#[test]
fn missing_standard_option_is_diagnosed_only_when_parse_is_used() {
    analyze_modules_with_std(&[("Main", "to_string 42")], &[]).unwrap();
    let error =
        analyze_modules_with_std(&[("Main", "let text = \"42\"\nParse.parse (&text)")], &[])
            .unwrap_err();
    assert_eq!(error.code, "E1004");
    assert!(error.message.contains("Option"));
}

#[test]
fn numeric_and_boolean_console_output_uses_the_shared_buffered_path() {
    for source in ["0.1", "0.1f32", "42", "true"] {
        let module = analyze(source).unwrap();
        let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
        assert!(!ir.contains("@printf"));
        assert!(ir.contains("@tz.console.write"));
        if source != "true" {
            assert!(ir.contains("@tz_soft_format"));
        }
    }
}
