use tsuzuri::{analyze, docgen, llvm, parser};

#[test]
fn attaches_docs_to_api_declarations_without_changing_ir() {
    let source = "/// Answer.\r\n/// More detail.\r\ndef answer :: i64\nfn answer = 42\n\
        /// Hidden.\nprivate def hidden :: i64\nfn hidden = 0\n\
        /// Point.\nrecord Point { x: i64 }\n\
        /// Choice.\nunion Choice = Yes | No\n\
        /// Alias.\ntype Count = i64\n\
        /// Constant.\nconst limit: i64 = 42\n\
        /// Host.\nextern def host :: i64 -> i64\n\
        /// Class.\nclass Measure<'a> {\n/// Method.\ndef measure :: ref 'a -> i64\n}\n";
    let program = parser::parse(source).unwrap();
    assert_eq!(
        program.functions[0].doc.as_ref().unwrap().text,
        "Answer.\nMore detail."
    );
    assert_eq!(program.functions[1].doc.as_ref().unwrap().text, "Hidden.");
    assert_eq!(program.records[0].doc.as_ref().unwrap().text, "Point.");
    assert_eq!(program.unions[0].doc.as_ref().unwrap().text, "Choice.");
    assert_eq!(program.type_aliases[0].doc.as_ref().unwrap().text, "Alias.");
    assert_eq!(program.constants[0].doc.as_ref().unwrap().text, "Constant.");
    assert_eq!(program.externs[0].doc.as_ref().unwrap().text, "Host.");
    assert_eq!(program.classes[0].doc.as_ref().unwrap().text, "Class.");
    assert_eq!(
        program.classes[0].methods[0].doc.as_ref().unwrap().text,
        "Method."
    );
    let without_docs = source
        .lines()
        .filter(|line| !line.starts_with("///"))
        .collect::<Vec<_>>()
        .join("\n");
    let documented = analyze(source).unwrap();
    let plain = analyze(&without_docs).unwrap();
    assert_eq!(
        llvm::emit(&documented, llvm::Entry::Library).unwrap(),
        llvm::emit(&plain, llvm::Entry::Library).unwrap()
    );
}

#[test]
fn rejects_misplaced_docs_and_ignores_ordinary_comments() {
    for source in [
        "/// dangling",
        "/// entry\n42",
        "/// binding\nlet value = 42\nvalue",
        "def answer :: i64\n/// implementation\nfn answer = 42",
        "def answer :: i64 -> i64\n/// implementation\nlet answer = value -> value",
        "/// instance\ninstance Score<i64> {}",
        "record Point { /// field\nx: i64 }",
        "class Score<'a> { /// dangling\n}",
        "class Score<'a> { def score :: 'a -> i64\n/// implementation\nfn score value = 0 }",
        "fn main() -> i64 { /// local\nlet value = 1; value }",
        "/// test\ntest \"example\" = assert true",
    ] {
        assert_eq!(parser::parse(source).unwrap_err().code, "E0002", "{source}");
    }
    let program =
        parser::parse("// ordinary\n/** ordinary */\ndef answer :: i64\nfn answer = 42").unwrap();
    assert!(program.functions[0].doc.is_none());
}

#[test]
fn renders_public_apis_in_signature_order_and_keeps_markdown() {
    let source = "/// First.\n/// ```tsuzuri\n/// first()\n/// ```\ndef first :: i64\n\
        /// Coordinates.\nrecord Point { x: f64, y: f64 }\n\
        private def hidden :: i64\nfn hidden = 0\n\
        /// <em>Second.</em>\ndef second :: i64\nfn second = 2\nfn first = 1";
    let program = parser::parse(source).unwrap();
    let expected = "# Main\n\n## `first`\n\n```tsuzuri\ndef first :: i64\n```\n\nFirst.\n```tsuzuri\nfirst()\n```\n\n\
        ## `Point`\n\n```tsuzuri\nrecord Point {\n  x: f64\n  y: f64\n}\n```\n\nCoordinates.\n\n\
        ## `second`\n\n```tsuzuri\ndef second :: i64\n```\n\n<em>Second.</em>\n\n";
    assert_eq!(docgen::render_module("Main", &program), expected);
}

#[test]
fn renders_builder_aliases_below_the_title() {
    let alias = |lines: &str| {
        let program = parser::parse(lines).unwrap();
        docgen::render_module("Async", &program)
    };
    assert_eq!(
        alias("@alias async\n"),
        "# Async\n\nBuilder alias: `async`\n\n"
    );
    assert_eq!(
        alias("@alias async\n@alias job\n"),
        "# Async\n\nBuilder aliases: `async`, `job`\n\n"
    );
    assert_eq!(alias("\n"), "# Async\n\n");
}

#[test]
fn renders_declared_constraints_regions_and_class_method_docs() {
    let source = "/// Select.\ndef first {r s} :: (Copy<'a>, Eq<'a>) => ref {r} 'a -> ref {s} 'a -> ref {r} 'a\nfn first left right = left\n\
        record View<'a> {r} { value: ref {r} 'a }\n\
        type Callback<'a> = ('a -> 'a)\n\
        class Measure<'a> {\n/// Read the size.\ndef measure :: ref 'a -> i64\n}\n";
    let text = docgen::render_module("Library", &parser::parse(source).unwrap());
    assert!(text.contains(
        "def first {r s} :: (Copy<'a>, Eq<'a>) => ref {r} 'a -> ref {s} 'a -> ref {r} 'a"
    ));
    assert!(text.contains("record View<'a> {r} {\n  value: ref {r} 'a\n}"));
    assert!(text.contains("type Callback<'a> = ('a -> 'a)"));
    assert!(text.contains(
        "### `Measure.measure`\n\n```tsuzuri\ndef measure :: ref 'a -> i64\n```\n\nRead the size."
    ));
}

#[test]
fn renders_active_pattern_names_and_module_function_constraints() {
    let source = "/// Extract.\ndef (|Whole|) :: i64 -> i64\nfn (|Whole|) value = value\n\
        def use :: 'a -> 'a\n    @'a: #identity\nfn use value = value\n\
        /// Classify.\ndef (|Small|Large|) :: i64 -> 'T = \\value -> if value < 10 then Small value else Large value";
    let rendered = docgen::render_module("Main", &parser::parse(source).unwrap());
    assert!(rendered.contains("## `(|Whole|)`\n\n```tsuzuri\ndef (|Whole|) :: i64 -> i64"));
    assert!(rendered.contains("def use :: 'a -> 'a\n    @'a: #identity"));
    assert!(!rendered.contains("#identity<'a>"));
    assert!(rendered.contains("def (|Small|Large|) :: i64 -> 'T"));
    assert!(rendered.contains("Classify."));
    assert!(!rendered.contains("Active$"));
    assert!(!rendered.contains("$active_payload"));
}

#[test]
fn inline_definitions_keep_docs_on_functions_and_typed_continuations() {
    let source = "/// Even.\ndef rec even :: i64 -> bool = \\value -> if value == 0 then true else odd (value - 1)\n/// Odd.\nand odd :: i64 -> bool = \\value -> if value == 0 then false else even (value - 1)\nclass Answer<'a> {\n/// Answer.\ndef answer :: 'a -> i64 = \\_value -> 42\n}\ninstance Answer<i64> {}\nAnswer.answer 0i64";
    let program = parser::parse(source).unwrap();
    assert_eq!(program.functions[0].doc.as_ref().unwrap().text, "Even.");
    assert_eq!(program.functions[1].doc.as_ref().unwrap().text, "Odd.");
    assert_eq!(program.classes[0].defaults.len(), 1);
    analyze(source).unwrap();
}
