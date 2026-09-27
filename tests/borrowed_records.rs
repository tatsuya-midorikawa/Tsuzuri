use tsuzuri::{analyze, llvm};

#[test]
fn parses_named_regions_without_conflicting_with_type_parameters() {
    let source = "record View<'a> {r} { value: ref {r} 'a }\ndef choose {r s} :: ref {r} i64 -> ref {s} i64 -> ref {r} i64\nfn choose left right = left";
    let program = tsuzuri::parser::parse(source).unwrap();
    assert_eq!(program.records[0].regions[0].text, "r");
    assert_eq!(program.functions[0].regions.len(), 2);
    tsuzuri::parser::parse("fn value() -> i64 { 42 }").unwrap();
}

#[test]
fn named_regions_validate_declarations_and_returned_borrows() {
    for source in [
        "def first {r s} :: ref {r} string -> ref {s} string -> ref {r} string\nfn first left right = { assert (right.length > 0); left }",
        "def choose {r} :: bool -> ref {r} string -> ref {r} string -> ref {r} string\nfn choose flag left right = if flag then left else right",
        "record View {r} { text: ref {r} string }\ndef view {r} :: ref {r} string -> View {r}\nfn view text = View { text: text }",
    ] {
        analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    for source in [
        "def bad {r s} :: ref {r} string -> ref {s} string -> ref {r} string\nfn bad left right = right",
        "def bad :: ref {missing} string -> i64\nfn bad value = value.length",
        "def bad {unused} :: i64 -> i64\nfn bad value = value",
        "def bad {r} :: i64 {r} -> i64\nfn bad value = value",
        "record View { text: ref {r} string }",
        "record View {r} { text: ref {r} string }\ndef bad {r s} :: ref {r} string -> ref {s} string -> View {r}\nfn bad left right = View { text: right }",
        "def bad {r s} :: ref {r} string -> ref {s} string -> ref {r} string\nfn bad left = fx right -> right",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1013", "{source}");
    }
}

#[test]
fn named_return_regions_exclude_unrelated_input_lifetimes() {
    let prefix = "def first {r s} :: ref {r} string -> ref {s} string -> ref {r} string\nfn first left right = { assert (right.length > 0); left }\n";
    let source = format!(
        "{prefix}let outer = \"long lived\"\nlet selected = {{ let inner = \"short\"; first (&outer) (&inner) }}\nselected.length"
    );
    let module = analyze(&source).unwrap();
    for wasm in [false, true] {
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    }
    let through_value = format!(
        "{prefix}let select = first\nlet outer = \"long lived\"\nlet selected = {{ let inner = \"short\"; select (&outer) (&inner) }}\nselected.length"
    );
    assert_eq!(analyze(&through_value).unwrap_err().code, "E1013");
    analyze("record View<'a> {r} { value: ref {r} 'a }\ndef view {r s} :: ref {r} 'a -> ref {s} string -> View<'a> {r}\nfn view value unused = View { value: value }\nlet outer = 42\nlet selected = { let inner = \"short\"; view (&outer) (&inner) }\nderef selected.value").unwrap();
    let formatted =
        tsuzuri::formatter::format_source("Main.tz", &source, tsuzuri::syntax::SourceKind::Code)
            .unwrap();
    assert_eq!(
        formatted.formatted,
        tsuzuri::formatter::format_source(
            "Main.tz",
            &formatted.formatted,
            tsuzuri::syntax::SourceKind::Code
        )
        .unwrap()
        .formatted
    );
}

#[test]
fn shared_borrowed_fields_nesting_and_generic_views_are_supported() {
    for source in [
        "record View { data: &[i64] }\nfn view(data: &[i64]) -> View { View { data: data } }\nlet values = [20, 22]\nlet view = view (&values)\nview.data[0] + view.data[1]",
        "record TextView { text: &string }\nrecord Nested { view: TextView }\nfn wrap(text: &string) -> Nested { Nested { view: TextView { text: text } } }\nlet text = \"borrowed\"\nlet view = wrap (&text)\nview.view.text.length",
        "record Holder<'a> { value: 'a }\nlet text = \"borrowed\"\nlet view = Holder { value: &text }\nlet other = view\nview.value.length + other.value.length",
        "record View { text: &string, owned: string }\nlet text = \"borrowed\"\nlet view = View { text: &text, owned: \"owned\" }\nlet owned = view.owned\nview.text.length + owned.length",
        "record View { text: &string }\nlet text = \"borrowed\"\nlet view = View { text: &text }\nlet read = fx () -> view.text.length\nread () + read ()",
        "record PairView { left: &string, right: &string }\nfn pair(left: &string, right: &string) -> PairView { PairView { left: left, right: right } }\nlet left = \"a\"\nlet right = \"bc\"\nlet view = pair (&left) (&right)\nview.left.length + view.right.length",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
}

#[test]
fn borrowed_records_preserve_owner_lifetimes_and_exclusivity() {
    for (source, code) in [
        ("record Bad { value: &mut i64 }", "E1013"),
        ("record Bad { value: [&mut i64] }", "E1013"),
        (
            "record Holder<'a> { value: 'a }\nfn bad(value: &mut i64) -> Holder<&mut i64> { Holder { value: value } }",
            "E1013",
        ),
        (
            "record View { data: &[i64] }\nfn bad() -> View { let values = [1, 2]; View { data: &values } }",
            "E1013",
        ),
        (
            "record View { text: &string }\nlet text = \"x\"\nlet view = View { text: &text }\nlet moved = text\nview.text.length",
            "E1014",
        ),
        (
            "record View { text: &string }\nlet text = \"x\"\nlet view = View { text: &text }\ntask { return view }",
            "E1013",
        ),
        (
            "record View { text: &string }\nlet text = \"outer\"\nlet mut view = View { text: &text }\nfor item in Seq.once \"inner\" do view = View { text: &item }\nview.text.length",
            "E1013",
        ),
        (
            "record View { text: &string }\nfn update(target: &mut View, text: &string) -> unit { *target = View { text: text } }",
            "E1013",
        ),
        (
            "record View { text: &string }\nexport def bad :: View -> i64\nfn bad view = view.text.length",
            "E1008",
        ),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}
