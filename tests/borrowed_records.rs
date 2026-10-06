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
        "def bad {r s} :: ref {r} string -> ref {s} string -> ref {r} string\nfn bad left = \\right -> right",
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
        "record View { text: &string }\nlet text = \"borrowed\"\nlet view = View { text: &text }\nlet read = \\() -> view.text.length\nread () + read ()",
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

const PAIR: &str = "record Pair {r s} { left: ref {r} string, right: ref {s} string }\n";
const FUNCTIONS: &str = "def make_pair {r s} :: ref {r} string -> ref {s} string -> Pair {r s}\n\
    fn make_pair left right = Pair { left: left, right: right }\n\
    def left_of {r s} :: Pair {r s} -> ref {r} string\nfn left_of pair = pair.left\n\
    def swap {r s} :: Pair {r s} -> Pair {s r}\nfn swap pair = Pair { left: pair.right, right: pair.left }\n\
    def pick {r s} :: bool -> ref {r} string -> ref {r} string -> ref {s} string -> Pair {r s}\n\
    fn pick flag first second other = if flag then Pair { left: first, right: other } else Pair { left: second, right: other }\n";

fn program(declarations: &str, body: &str) -> String {
    format!("{declarations}let long = \"abcdefg\"\n{body}")
}

fn many_regions(count: usize) -> String {
    let regions: Vec<_> = (0..count).map(|index| format!("r{index}")).collect();
    let fields: Vec<_> = regions
        .iter()
        .map(|region| format!("f{region}: ref {{{region}}} i64"))
        .collect();
    format!(
        "record Big {{{}}} {{ {} }}",
        regions.join(" "),
        fields.join(", ")
    )
}

fn accepts(source: &str) {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
}

fn rejects(source: &str, code: &str) {
    match analyze(source) {
        Ok(_) => panic!("accepted: {source}"),
        Err(error) => assert_eq!(error.code, code, "{source}\n{error:?}"),
    }
}

#[test]
fn record_regions_are_bounded() {
    accepts(&many_regions(16));
    let error = analyze(&many_regions(17)).unwrap_err();
    assert_eq!(error.code, "E1017");
    assert!(error.message.contains("at most 16"), "{}", error.message);
    let source = many_regions(17);
    assert_eq!(&source[error.span.start..error.span.end], "r16");
}

#[test]
fn multiple_record_regions_validate_declarations() {
    for source in [
        program(
            PAIR,
            "let short = \"xy\"\nlet pair = Pair { left: ref long, right: ref short }\npair.left.length + pair.right.length",
        ),
        "record Pair<'a, 'b> {r s} { left: ref {r} 'a, right: ref {s} 'b }".to_owned(),
        format!("{PAIR}record Swapped {{a b}} {{ pair: Pair {{b a}} }}"),
        "record Keep<'a> {r s} { left: ref {r} i64, right: ref {s} i64, extra: 'a, call: i64 -> i64 }"
            .to_owned(),
    ] {
        accepts(&source);
    }
    for (source, message) in [
        (
            format!("{PAIR}record Outer {{r}} {{ pair: Pair {{r}} }}"),
            "the record declares 2 regions; write exactly 2 region names in declaration order",
        ),
        (
            format!("{PAIR}record Outer {{r s}} {{ pairs: [Pair {{r s}}] }}"),
            "one field cannot mix distinct named regions; give each field one region or use a record type with several regions",
        ),
        (
            "record Bad {r s} { left: ref {r} string, right: ref {s} string, other: ref string }"
                .to_owned(),
            "field 'other' stores a borrow without a region; name one of the record's regions in its type, for example 'ref {r} T'",
        ),
        (
            "record Bad {r s} { both: (ref {r} string * ref {s} string) }".to_owned(),
            "one field cannot mix distinct named regions; give each field one region or use a record type with several regions",
        ),
        (
            "record Bad {r s t} { left: ref {r} string, right: ref {s} string }".to_owned(),
            "unused region 't'; remove it or use it in a borrowed type",
        ),
        (
            "record Bad {r s} { left: ref {r} string, right: ref {q} string }".to_owned(),
            "undeclared region 'q'; add it after the declaration name",
        ),
        (
            "record View {r} { text: ref {r} string }\nrecord Bad {r s} { view: View {r s} }"
                .to_owned(),
            "each borrowed value has one named region; split values with independent regions",
        ),
    ] {
        let error = analyze(&source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1013", message),
            "{source}"
        );
    }
    assert_eq!(
        tsuzuri::parser::parse(PAIR).unwrap().records[0]
            .regions
            .len(),
        2
    );
    let source = format!("{PAIR}{FUNCTIONS}");
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
    accepts(&formatted.formatted);
}

#[test]
fn multiple_record_regions_split_local_loans() {
    let swapped = format!("{PAIR}record Swapped {{a b}} {{ pair: Pair {{b a}} }}\n");
    let named =
        "record Named {r s} { left: ref {r} string, right: ref {s} string, name: string }\n";
    for source in [
        program(
            PAIR,
            "let kept = { let short = \"xy\"; let pair = Pair { left: ref long, right: ref short }; pair.left }\nkept.length",
        ),
        program(
            &swapped,
            "let kept = { let short = \"xy\"; let swapped = Swapped { pair: Pair { left: ref long, right: ref short } }; swapped.pair.left }\nkept.length",
        ),
        program(
            PAIR,
            "let kept = { let short = \"xy\"; let pair = if long.length > 3 then Pair { left: ref long, right: ref short } else Pair { left: ref long, right: ref short }; pair.left }\nkept.length",
        ),
        program(
            PAIR,
            "let kept = { let short = \"xy\"; let pair = match long.length with | 7 -> Pair { left: ref long, right: ref short } | _ -> Pair { left: ref long, right: ref short }; pair.left }\nkept.length",
        ),
        program(
            PAIR,
            "let mut short = \"xy\"\nlet kept = { let pair = Pair { left: ref long, right: ref short }; pair.left }\nshort = \"changed\"\nkept.length",
        ),
        program(
            PAIR,
            "let t = (Pair { left: ref long, right: ref long }, 1)\nlong.length",
        ),
        program(
            named,
            "let short = \"xy\"\nlet named = Named { left: ref long, right: ref short, name: \"n\" }\nlet name = named.name\nnamed.left.length + name.length",
        ),
    ] {
        accepts(&source);
    }
    for (source, code) in [
        (
            program(
                PAIR,
                "let kept = { let short = \"xy\"; let pair = Pair { left: ref long, right: ref short }; pair.right }\nkept.length",
            ),
            "E1013",
        ),
        (
            program(
                &swapped,
                "let kept = { let short = \"xy\"; let swapped = Swapped { pair: Pair { left: ref long, right: ref short } }; swapped.pair.right }\nkept.length",
            ),
            "E1013",
        ),
        (
            program(
                PAIR,
                "let kept = { let short = \"xy\"; let mut pair = Pair { left: ref long, right: ref short }; pair.left }\nkept.length",
            ),
            "E1013",
        ),
        (
            program(
                PAIR,
                "let kept = { let short = \"xy\"; let pair = Pair { left: ref long, right: ref short }; (pair, 1) }\nlong.length",
            ),
            "E1013",
        ),
        (
            program(
                PAIR,
                "let mut short = \"xy\"\nlet kept = { let pair = Pair { left: ref long, right: ref short }; pair.right }\nshort = \"changed\"\nkept.length",
            ),
            "E1014",
        ),
        (
            program(
                named,
                "let short = \"xy\"\nlet named = Named { left: ref long, right: ref short, name: \"n\" }\nlet moved = named\nnamed.left.length + moved.left.length",
            ),
            "E1012",
        ),
    ] {
        rejects(&source, code);
    }
}

#[test]
fn multiple_regions_follow_record_updates() {
    let declarations = format!("{PAIR}{FUNCTIONS}");
    let block = |pair: &str, update: &str, result: &str| {
        program(
            &declarations,
            &format!(
                "let kept = {{ let short = \"xy\"; let pair = Pair {{ {pair} }}; let updated = {{ pair with {update} }}; {result} }}\nkept.length"
            ),
        )
    };
    // The replacement's loans go only to the replaced field's slot.
    accepts(&block(
        "left: ref long, right: ref long",
        "left = ref short",
        "updated.right",
    ));
    rejects(
        &block(
            "left: ref long, right: ref long",
            "left = ref short",
            "updated.left",
        ),
        "E1013",
    );
    // The fields that the update keeps keep their slots.
    accepts(&block(
        "left: ref long, right: ref short",
        "left = ref long",
        "updated.left",
    ));
    rejects(
        &block(
            "left: ref long, right: ref short",
            "left = ref long",
            "updated.right",
        ),
        "E1013",
    );
    // An update keeps the base's loans in the replaced slot too, which stays conservative.
    rejects(
        &block(
            "left: ref short, right: ref long",
            "left = ref long",
            "updated.left",
        ),
        "E1013",
    );
    // A direct call selects the slots of an updated argument.
    accepts(&block(
        "left: ref long, right: ref long",
        "right = ref short",
        "left_of updated",
    ));
    for (update, code) in [
        ("right = ref short", None),
        ("left = ref short", Some("E1013")),
    ] {
        let source = program(
            &declarations,
            &format!(
                "let kept = {{ let short = \"xy\"; let pair = Pair {{ left: ref long, right: ref long }}; left_of ({{ pair with {update} }}) }}\nkept.length"
            ),
        );
        match code {
            None => accepts(&source),
            Some(code) => rejects(&source, code),
        }
    }
}

#[test]
fn multiple_region_contracts_select_input_slots() {
    let declarations = format!("{PAIR}{FUNCTIONS}");
    let generic = "record Pair2<'a, 'b> {r s} { left: ref {r} 'a, right: ref {s} 'b }\n\
        def first_of {r s} :: Pair2<'a, 'b> {r s} -> ref {r} 'a\nfn first_of pair = pair.left\n";
    for source in [
        program(&declarations, "0"),
        program(
            &declarations,
            "let kept = { let short = \"xy\"; left_of (make_pair (ref long) (ref short)) }\nkept.length",
        ),
        program(
            &declarations,
            "let kept = { let short = \"xy\"; (swap (make_pair (ref short) (ref long))).left }\nkept.length",
        ),
        program(
            &declarations,
            "let kept = { let other = \"xy\"; (pick false (ref long) (ref long) (ref other)).left }\nkept.length",
        ),
        format!(
            "{generic}let n = 42\nlet kept = {{ let s = \"xy\"; first_of (Pair2 {{ left: ref n, right: ref s }}) }}\nderef kept"
        ),
    ] {
        accepts(&source);
    }
    for (source, code) in [
        (
            format!(
                "{PAIR}def bad {{r s}} :: Pair {{r s}} -> ref {{r}} string\nfn bad pair = pair.right"
            ),
            "E1013",
        ),
        (
            format!(
                "{PAIR}def bad {{r s}} :: ref {{r}} string -> ref {{s}} string -> Pair {{r s}}\nfn bad left right = Pair {{ left: right, right: left }}"
            ),
            "E1013",
        ),
        (
            format!("{PAIR}def bad {{r}} :: Pair {{r}} -> i64\nfn bad pair = pair.left.length"),
            "E1013",
        ),
        (
            format!("{PAIR}def bad {{r s}} :: ref Pair {{r s}} -> i64\nfn bad pair = 0"),
            "E1013",
        ),
        (
            format!("{PAIR}def bad {{r}} :: Pair {{r r}} -> i64\nfn bad pair = 0"),
            "E1013",
        ),
        (
            program(
                &declarations,
                "let kept = { let short = \"xy\"; left_of (make_pair (ref short) (ref long)) }\nkept.length",
            ),
            "E1013",
        ),
        (
            program(
                &declarations,
                "let kept = { let short = \"xy\"; (swap (make_pair (ref long) (ref short))).left }\nkept.length",
            ),
            "E1013",
        ),
    ] {
        rejects(&source, code);
    }
}

#[test]
fn multiple_regions_stay_conservative_through_values() {
    let declarations = format!("{PAIR}{FUNCTIONS}");
    for source in [
        program(
            &declarations,
            "let get = left_of\nlet kept = { let short = \"xy\"; get (make_pair (ref long) (ref short)) }\nkept.length",
        ),
        program(
            &declarations,
            "let partial = make_pair (ref long)\nlet kept = { let short = \"xy\"; left_of (partial (ref short)) }\nkept.length",
        ),
        program(
            &declarations,
            "let reader = { let short = \"xy\"; let pair = make_pair (ref long) (ref short); \\() -> pair.left.length }\nreader ()",
        ),
        program(
            &declarations,
            "let view = make_pair (ref long) (ref long)\ntask { return view }",
        ),
        program(
            &declarations,
            "let kept = { let short = \"xy\"; let pair = make_pair (ref long) (ref short); (pair |> swap).right }\nkept.length",
        ),
    ] {
        rejects(&source, "E1013");
    }
    accepts(&program(
        &declarations,
        "let get = left_of\n(get (make_pair (ref long) (ref long))).length",
    ));
}

#[test]
fn multiple_regions_do_not_change_generated_ir() {
    let annotated = format!(
        "{PAIR}def make_pair {{r s}} :: ref {{r}} string -> ref {{s}} string -> Pair {{r s}}\n\
        fn make_pair left right = Pair {{ left: left, right: right }}\n\
        def left_of {{r s}} :: Pair {{r s}} -> ref {{r}} string\nfn left_of pair = pair.left\n\
        let long = \"abcdefg\"\nlet short = \"xy\"\n(left_of (make_pair (ref long) (ref short))).length"
    );
    let erased = annotated
        .replace(" {r s}", "")
        .replace(" {r}", "")
        .replace(" {s}", "");
    let annotated = analyze(&annotated).unwrap();
    let erased = analyze(&erased).unwrap();
    for wasm in [false, true] {
        assert_eq!(
            llvm::emit_target(&annotated, llvm::Entry::Library, wasm).unwrap(),
            llvm::emit_target(&erased, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

const APPLY: &str = "def apply {r} :: ({s} ref {s} string -> ref {s} string) -> ref {r} string -> ref {r} string\n\
    fn apply f text = f text\n\
    def trim {s} :: ref {s} string -> ref {s} string\nfn trim text = text\n\
    def pick_first {s t} :: ref {s} string -> ref {t} string -> ref {s} string\nfn pick_first a b = a\n\
    def pick_second {s t} :: ref {s} string -> ref {t} string -> ref {t} string\nfn pick_second a b = b\n\
    def keep {r} :: ({s t} ref {s} string -> ref {t} string -> ref {s} string) -> ref {r} string -> ref string -> ref {r} string\n\
    fn keep f kept other = f kept other\n\
    def twice {r} :: ({s} ref {s} string -> ref {s} string) -> ref {r} string -> ref {r} string\n\
    fn twice f text = apply f (f text)\n";

#[test]
fn region_quantified_parameters_select_their_inputs() {
    for source in [
        program(
            APPLY,
            "let kept = { let other = \"x\"; apply trim (ref long) }\nkept.length",
        ),
        program(
            APPLY,
            "let kept = { let other = \"x\"; apply (\\t -> t) (ref long) }\nkept.length",
        ),
        program(
            APPLY,
            "let kept = { let short = \"xy\"; let r = ref short; apply (\\t -> { assert (r.length > 0); t }) (ref long) }\nkept.length",
        ),
        program(
            APPLY,
            "let kept = { let short = \"xy\"; keep pick_first (ref long) (ref short) }\nkept.length",
        ),
        program(
            APPLY,
            "let kept = { let other = \"x\"; twice trim (ref long) }\nkept.length",
        ),
        program(
            APPLY,
            "let kept = { let short = \"xy\"; keep (\\a b -> { assert (b.length > 0); a }) (ref long) (ref short) }\nkept.length",
        ),
        "def apply_any {r} :: ({s} ref {s} 'a -> ref {s} 'a) -> ref {r} 'a -> ref {r} 'a\n\
         fn apply_any f value = f value\n\
         def same {s} :: ref {s} i64 -> ref {s} i64\nfn same v = v\n\
         let n = 42\nlet kept = { let other = 1; apply_any same (ref n) }\nderef kept"
            .to_owned(),
        "def measure :: ({s} ref {s} string -> i64) -> i64\n\
         fn measure f = { let text = \"abc\"; f (ref text) }\nmeasure (\\t -> t.length)"
            .to_owned(),
    ] {
        let module = analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
    rejects(
        &program(
            APPLY,
            "let kept = { let short = \"xy\"; apply trim (ref short) }\nkept.length",
        ),
        "E1013",
    );
    // Without a quantifier, the parameter's result keeps the function value's own borrows.
    rejects(
        "def apply {r} :: (ref string -> ref string) -> ref {r} string -> ref {r} string\nfn apply f text = f text",
        "E1013",
    );
}

#[test]
fn region_quantified_arguments_keep_the_contract() {
    let unkept = "this function does not keep the region-quantified parameter type; its result may borrow from an input or a capture that the type does not name";
    let unsupported = "pass a named function with matching named regions, a lambda, or a parameter with the same region-quantified type here";
    let direct = "'apply' takes a function with a region-quantified type, so call it directly with all of its arguments";
    for (body, message) in [
        (
            "let kept = { let short = \"xy\"; let r = ref short; apply (\\t -> r) (ref long) }\nkept.length",
            unkept,
        ),
        (
            "let kept = { let short = \"xy\"; keep pick_second (ref long) (ref short) }\nkept.length",
            unkept,
        ),
        (
            "let kept = { let short = \"xy\"; keep (\\a b -> b) (ref long) (ref short) }\nkept.length",
            unkept,
        ),
        (
            "let f = trim\nlet kept = { let other = \"x\"; apply f (ref long) }\nkept.length",
            unsupported,
        ),
        ("let g = apply trim\n(g (ref long)).length", direct),
        ("let h = apply\n0", direct),
    ] {
        let source = program(APPLY, body);
        let error = analyze(&source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1013", message),
            "{source}"
        );
    }
    let error = analyze(&program(
        APPLY,
        "let kept = { let short = \"xy\"; let r = ref short; apply (\\t -> r) (ref long) }\nkept.length",
    ))
    .unwrap_err();
    let source = program(
        APPLY,
        "let kept = { let short = \"xy\"; let r = ref short; apply (\\t -> r) (ref long) }\nkept.length",
    );
    assert_eq!(&source[error.span.start..error.span.end], "(\\t -> r)");
}

#[test]
fn region_quantified_types_only_type_named_function_parameters() {
    let misplaced = "region-quantified function types are only supported as whole parameter types of a named function";
    let trim = "def trim {s} :: ref {s} string -> ref {s} string\nfn trim text = text\n";
    for (source, message) in [
        (
            "record Holder { call: {s} ref {s} string -> ref {s} string }".to_owned(),
            misplaced,
        ),
        (
            format!("def bad :: i64 -> ({{s}} ref {{s}} string -> ref {{s}} string)\nfn bad x = trim\n{trim}"),
            misplaced,
        ),
        (
            "def bad :: (({s} ref {s} string -> ref {s} string) -> i64) -> i64\nfn bad f = 0".to_owned(),
            misplaced,
        ),
        (
            "def bad {r} :: ({s} ref {s} string -> ref {r} string) -> ref {r} string -> ref {r} string\nfn bad f text = text".to_owned(),
            "region 'r' belongs to the function, not to the quantified function type; quantify a region of its own",
        ),
        (
            "def bad {r} :: ({r} ref {r} string -> ref {r} string) -> ref {r} string -> ref {r} string\nfn bad f text = f text".to_owned(),
            "region 'r' is already declared by the function; give the quantified region another name",
        ),
        (
            "def bad {r} :: ({s t} ref {s} string -> ref {s} string) -> ref {r} string -> ref {r} string\nfn bad f text = f text".to_owned(),
            "unused region 't'; remove it or use it in a borrowed type",
        ),
        (
            "def bad :: ({s} ref {s} string) -> i64\nfn bad text = 0".to_owned(),
            "a region quantifier applies to a function type, as in '{r} ref {r} T -> ref {r} T'",
        ),
        (
            "def bad :: ({s} ref {s} string -> ref string) -> i64\nfn bad f = 0".to_owned(),
            "annotate the whole borrowed result with one region, for example 'View {r}' or 'ref {r} T'",
        ),
        (
            "def bad {r} :: ({s} ref {s} string -> ref {s} string) -> ref {r} string -> ref {r} string\nfn bad mut f text = f text".to_owned(),
            "a parameter with a region-quantified function type cannot be mutable",
        ),
        (
            format!("{trim}let f: {{s}} ref {{s}} string -> ref {{s}} string = trim\n0"),
            "named regions belong in function signatures or record declarations; let local borrows be inferred",
        ),
    ] {
        let error = analyze(&source).unwrap_err();
        assert_eq!((error.code, error.message.as_str()), ("E1013", message), "{source}");
    }
    let program = tsuzuri::parser::parse(APPLY).unwrap();
    assert!(matches!(
        program.functions[0].parameters[0].ty.kind,
        tsuzuri::syntax::TypeExprKind::Quantified(..)
    ));
    let formatted =
        tsuzuri::formatter::format_source("Main.tz", APPLY, tsuzuri::syntax::SourceKind::Code)
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
    accepts(&formatted.formatted);
}

const EXCLUSIVE: &str = "record Counter {r} { value: ref mut {r} i64, step: i64 }\n\
    record Meter {r} { counter: Counter {r}, scale: i64 }\n\
    record Split {r s} { left: ref mut {r} i64, right: ref mut {s} i64 }\n\
    record Mixed {r s} { value: ref mut {r} i64, name: ref {s} string }\n\
    record Slot<'a> {r} { value: ref mut {r} 'a }\n\
    def add_to :: ref mut i64 -> i64 -> unit = \\target amount -> { deref target = deref target + amount; }\n";

/// The source rejects with `code`, and with a message containing `fragment` unless it is empty.
fn rejects_with(source: &str, code: &str, fragment: &str) {
    match analyze(source) {
        Ok(_) => panic!("accepted: {source}"),
        Err(error) => {
            assert_eq!(error.code, code, "{source}\n{error:?}");
            assert!(
                error.message.contains(fragment),
                "{source}\n{}",
                error.message
            );
        }
    }
}

#[test]
fn exclusive_borrow_fields_accept_reborrows_moves_and_regions() {
    for body in [
        // Reborrows from a binding without `mut`, call-site reborrows, and NLL: 7.
        "let mut total = 1\nlet counter = Counter { value: ref mut total, step: 2 }\nadd_to counter.value counter.step\nadd_to (ref mut counter.value) 3\nlet again = ref mut counter.value\nderef again = deref again + 1\ntotal",
        // A record parameter's external loan allows writes: 12.
        "def advance :: Counter -> Counter = \\counter -> { add_to counter.value counter.step; counter }\nlet mut total = 0\nlet counter = advance (advance (Counter { value: ref mut total, step: 4 }))\nlet step = counter.step\nstep + total",
        // Through `ref mut Counter`: 15.
        "def bump :: ref mut Counter -> unit = \\counter -> { deref counter.value = deref counter.value + counter.step; }\nlet mut total = 0\nlet mut counter = Counter { value: ref mut total, step: 5 }\nbump (ref mut counter)\nbump (ref mut counter)\nlet step = counter.step\nstep + total",
        // A result with a named region: 7.
        "def counter_of {r} :: ref mut {r} i64 -> Counter {r} = \\value -> Counter { value: value, step: 7 }\nlet mut total = 0\nlet counter = counter_of (ref mut total)\nadd_to counter.value counter.step\ntotal",
        // A partial move out of a parameter: 9.
        "def value_of {r} :: Counter {r} -> ref mut {r} i64 = \\counter -> counter.value\nlet mut total = 3\nlet value = value_of (Counter { value: ref mut total, step: 1 })\nderef value = 9\ntotal",
        // A nested record: 17.
        "let mut total = 2\nlet meter = Meter { counter: Counter { value: ref mut total, step: 5 }, scale: 3 }\nadd_to meter.counter.value (meter.counter.step * meter.scale)\ntotal",
        // Two exclusive regions reborrowed at once: 1125.
        "let mut left = 10\nlet mut right = 20\nlet split = Split { left: ref mut left, right: ref mut right }\nlet held = ref mut split.left\nadd_to split.right 5\nderef held = deref held + 1\nleft * 100 + right",
        // The shared region stays readable while the exclusive one is reborrowed: 3.
        "let mut total = 0\nlet name = \"abc\"\nlet mixed = Mixed { value: ref mut total, name: ref name }\nlet held = ref mut mixed.value\nadd_to held mixed.name.length\ntotal",
        // Replacing the record while its old borrow is reborrowed: 32.
        "let mut a = 1\nlet mut b = 2\nlet mut counter = Counter { value: ref mut a, step: 0 }\nlet held = ref mut counter.value\ncounter = Counter { value: ref mut b, step: 0 }\nderef held = 10\nadd_to counter.value 20\na + b",
        // An unguarded pattern moves the field out: 11.
        "let mut total = 5\nlet counter = Counter { value: ref mut total, step: 6 }\nmatch counter with | Counter { value: target, step: amount } -> add_to target amount\ntotal",
        // A generic target: 8.
        "let mut total = 1\nlet slot = Slot { value: ref mut total }\nderef slot.value = 8\ntotal",
        // Replacing an owned target frees the old value: 13.
        "let mut text = \"short\"\nlet slot = Slot { value: ref mut text }\nderef slot.value = \"a longer text\"\ntext.length",
        // A guard reads the matched field before the arm moves it: 14.
        "let mut total = 4\nlet counter = Counter { value: ref mut total, step: 1 }\nlet result = match counter with | Counter { value: target } when deref target > 0 -> 1 | _ -> 0\nresult * 10 + total",
        // A mutable binding mixes the regions (A12 D6), and the exclusive loan still selects the target: 3.
        "let mut total = 0\nlet name = \"abc\"\nlet mut mixed = Mixed { value: ref mut total, name: ref name }\nlet length = mixed.name.length\nadd_to mixed.value length\nlet handle = ref mut mixed\nadd_to (deref handle).value 0\ntotal",
        // A parameter with several regions writes through its exclusive slot: 7.
        "def bump_mixed {r s} :: Mixed {r s} -> ref {s} string = \\mixed -> { add_to mixed.value mixed.name.length; mixed.name }\nlet mut total = 1\nlet name = \"abc\"\nlet kept = bump_mixed (Mixed { value: ref mut total, name: ref name })\nkept.length + total",
        // Shared references read the exclusive field: 9.
        "def peek :: ref Counter -> i64 = \\counter -> deref counter.value + counter.step\nlet mut total = 4\nlet counter = Counter { value: ref mut total, step: 1 }\nlet view = ref counter\nlet seen = view.step + deref view.value + peek (ref counter)\nseen - 1",
    ] {
        let source = format!("{EXCLUSIVE}{body}");
        let module = analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
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
fn exclusive_borrow_field_declarations_are_validated() {
    for (source, code, fragment) in [
        (
            "record Bad { value: ref mut i64 }",
            "E1013",
            "exclusive borrow field 'value' needs a named region",
        ),
        (
            "record Bad {r} { value: ref mut i64 }",
            "E1013",
            "exclusive borrow field 'value' needs a named region",
        ),
        (
            "record Bad {r} { first: ref mut {r} i64, second: ref mut {r} i64 }",
            "E1013",
            "region 'r' belongs to an exclusive borrow, so only one field position may use it; give field 'second' its own region",
        ),
        (
            "record Bad {r} { value: ref mut {r} i64, name: ref {r} string }",
            "E1013",
            "give field 'name' its own region",
        ),
        (
            "record Bad {r} { value: ref mut {r} i64, name: ref string }",
            "E1013",
            "give field 'name' its own region",
        ),
        (
            "record View { text: ref string }\nrecord Bad {r} { value: ref mut {r} View }",
            "E1013",
            "exclusive borrow field 'value' must point to data without borrows",
        ),
        (
            "record Bad {r} { values: [ref mut {r} i64] }",
            "E1013",
            "can hold an exclusive reference only",
        ),
        (
            "record Bad {r} { value: Maybe<ref mut {r} i64> }",
            "E1013",
            "can hold an exclusive reference only",
        ),
        (
            "record Counter {r} { value: ref mut {r} i64 }\nrecord Bad { counter: Counter }",
            "E1013",
            "field 'counter' stores an exclusive borrow inside record 'Counter'; apply its regions, for example 'Counter {r}'",
        ),
        (
            "record Split {r s} { left: ref mut {r} i64, right: ref mut {s} i64 }\nrecord Bad {r s} { split: Split, other: ref {r} i64, more: ref {s} i64 }",
            "E1013",
            "for example 'Split {r s}'",
        ),
        (
            "record Counter {r} { value: ref mut {r} i64 }\nrecord Bad {r} { counter: Counter {r}, name: ref {r} string }",
            "E1013",
            "give field 'name' its own region",
        ),
        (
            "record Counter {r} { value: ref mut {r} i64 }\nrecord Bad {r} { view: ref {r} Counter }",
            "E1013",
            "can hold an exclusive reference only",
        ),
        (
            "record Counter {r} { value: ref mut {r} i64 }\nrecord Bad {r} { pair: (Counter {r} * i64) }",
            "E1013",
            "can hold an exclusive reference only",
        ),
        (
            "record Slot<'a> {r} { value: ref mut {r} 'a }\ndef bad :: Slot<ref i64> -> i64\nfn bad slot = 0",
            "E1013",
            "Slot<ref i64> would store borrowed data behind the exclusive field 'value'",
        ),
        (
            "record Holder<'a> { value: 'a }\nrecord Counter {r} { value: ref mut {r} i64 }\ndef bad :: Holder<Counter> -> i64\nfn bad holder = 0",
            "E1013",
            "would store a mutable reference in field 'value'",
        ),
        (
            "record Holder<'a> { value: 'a }\nrecord Counter {r} { value: ref mut {r} i64 }\ndef bad :: Holder<ref Counter> -> i64\nfn bad holder = 0",
            "E1013",
            "would store a mutable reference in field 'value'",
        ),
        (
            "record Counter {r} { value: ref mut {r} i64 }\nlet mut total = 0\nlet all = [Counter { value: ref mut total }]\n0",
            "E1005",
            "array and list elements cannot contain mutable references",
        ),
        (
            "record Counter {r} { value: ref mut {r} i64 }\nlet mut total = 0\nlet maybe = Some (Counter { value: ref mut total })\n0",
            "E1013",
            "union payloads cannot store mutable references",
        ),
        (
            "record Counter {r} { value: ref mut {r} i64 }\nexport def bad :: Counter -> i64\nfn bad counter = 0",
            "E1008",
            "",
        ),
    ] {
        rejects_with(source, code, fragment);
    }
    accepts(
        "record Counter {r} { value: ref mut {r} i64, step: i64 }\nrecord Pair {a b} { first: Counter {a}, second: Counter {b} }\nrecord Swapped {a b} { split: Split {b a} }\nrecord Split {r s} { left: ref mut {r} i64, right: ref mut {s} i64 }\nrecord Handler {r} { target: ref mut {r} i64, callback: i64 -> i64 }",
    );
    let source = format!("{EXCLUSIVE}0");
    let formatted =
        tsuzuri::formatter::format_source("Main.tz", &source, tsuzuri::syntax::SourceKind::Code)
            .unwrap();
    accepts(&formatted.formatted);
}

#[test]
fn exclusive_borrow_fields_preserve_exclusivity() {
    for (body, code, fragment) in [
        // The owner is used while the record lives.
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet seen = total\nadd_to counter.value 1\nseen",
            "E1014",
            "access conflicts with a live borrow",
        ),
        (
            "let mut total = 0\nlet first = Counter { value: ref mut total, step: 1 }\nlet second = Counter { value: ref mut total, step: 1 }\nadd_to first.value 1\nadd_to second.value 1\ntotal",
            "E1014",
            "access conflicts with a live borrow",
        ),
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet moved = counter\nadd_to counter.value 1\nadd_to moved.value 1\ntotal",
            "E1012",
            "use of moved or partially moved value 'counter'",
        ),
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet held = ref mut counter.value\nlet moved = counter\nderef held = 3\nadd_to moved.value 1\ntotal",
            "E1014",
            "cannot move an exclusive reference while it is reborrowed",
        ),
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet held = ref mut counter.value\nadd_to counter.value 1\nderef held = 3\ntotal",
            "E1014",
            "access conflicts with a live borrow",
        ),
        // `let` does not reborrow, so it moves the field out.
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet taken = counter.value\nadd_to counter.value 1\nadd_to taken 1\ntotal",
            "E1012",
            "use of moved or partially moved value 'counter'",
        ),
        (
            "let mut total = 0\nlet name = \"abc\"\nlet mixed = Mixed { value: ref mut total, name: ref name }\nlet bad = ref mut mixed.name\n0",
            "E1014",
            "cannot mutate or exclusively reborrow through a shared reference",
        ),
        // Shared references to a record with exclusive regions only read it.
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet view = ref counter\nadd_to view.value 1\ntotal",
            "E1014",
            "cannot mutate or exclusively reborrow through a shared reference",
        ),
        (
            "def bad :: ref Counter -> unit = \\counter -> add_to counter.value 1\n0",
            "E1014",
            "cannot mutate or exclusively reborrow through a shared reference",
        ),
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet view = ref counter\nderef view.value = 5\ntotal",
            "E1014",
            "cannot mutate or exclusively reborrow through a shared reference",
        ),
        (
            "let mut total = 0\nlet meter = Meter { counter: Counter { value: ref mut total, step: 1 }, scale: 2 }\nlet view = ref meter\nlet held = ref mut view.counter.value\n0",
            "E1014",
            "cannot mutate or exclusively reborrow through a shared reference",
        ),
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet read = \\() -> counter.step\nread ()",
            "E1005",
            "Counter in a reusable function",
        ),
        (
            "let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\ntask { return counter.step }",
            "E1013",
            "",
        ),
        (
            "def bad :: i64 -> Counter = \\seed -> { let mut total = seed; Counter { value: ref mut total, step: 1 } }\n0",
            "E1013",
            "",
        ),
        // Phase 1 records keep replacing borrowed aggregates through parameters out.
        (
            "def replace :: ref mut Counter -> ref mut i64 -> unit = \\target value -> { deref target = Counter { value: value, step: 0 }; }\n0",
            "E1013",
            "assigning borrowed values through references requires explicit lifetimes",
        ),
        (
            "def bad {r s} :: ref mut {r} i64 -> ref mut {s} i64 -> Counter {r} = \\left right -> Counter { value: right, step: 0 }\n0",
            "E1013",
            "returned borrow does not match the declared result region",
        ),
        // A tuple parameter keeps its shared external loan.
        (
            "def bad :: (Counter * i64) -> unit = \\pair -> match pair with | (counter, amount) -> add_to counter.value amount\n0",
            "E1014",
            "",
        ),
    ] {
        rejects_with(&format!("{EXCLUSIVE}{body}"), code, fragment);
    }
}

const TARGETS: &str = "record View {r} { text: ref {r} string }\n\
    record Editor {r s} { view: ref mut {r} View {s}, edits: i64 }\n\
    def replace {r s} :: ref mut {r} View {s} -> ref {s} string -> unit = \\target text -> { deref target = View { text: text }; }\n\
    def swap_text {r s} :: ref mut {r} View {s} -> ref {s} string -> ref {s} string = \\target text -> { let old = (deref target).text; deref target = View { text: text }; old }\n\
    def text_of {r s} :: ref {r} View {s} -> ref {s} string = \\view -> view.text\n\
    def retarget {r s} :: Editor {r s} -> ref {s} string -> Editor {r s} = \\editor text -> { deref editor.view = View { text: text }; editor }\n\
    def copy_text {r q s} :: ref mut {r} View {s} -> ref {q} View {s} -> unit = \\target source -> { deref target = View { text: (deref source).text }; }\n";

/// `TARGETS`, then `declarations`, then two strings, then `code`.
fn targets(declarations: &str, code: &str) -> String {
    format!("{TARGETS}{declarations}let a = \"alpha\"\nlet b = \"beta!!\"\n{code}")
}

#[test]
fn named_target_regions_store_borrowed_aggregates_through_references() {
    for (declarations, code) in [
        // A local owner keeps the stored borrow: 6.
        (
            "",
            "let mut view = View { text: ref a }\nlet target = ref mut view\nderef target = View { text: ref b }\nview.text.length",
        ),
        // A call stores an input with the target's region: 6.
        (
            "",
            "let mut view = View { text: ref a }\nreplace (ref mut view) (ref b)\nview.text.length",
        ),
        // The old target outlives the reference that replaced it: 65.
        (
            "",
            "let mut view = View { text: ref a }\nlet old = swap_text (ref mut view) (ref b)\nview.text.length * 10 + old.length",
        ),
        // The target's borrow has its own region, so it leaves the block of the view: 5.
        (
            "",
            "let kept = { let view = View { text: ref a }; text_of (ref view) }\nkept.length",
        ),
        // An exclusive field with a named target region: 7.
        (
            "",
            "let mut view = View { text: ref a }\nlet editor = retarget (Editor { view: ref mut view, edits: 1 }) (ref b)\nlet edits = editor.edits\nview.text.length + edits",
        ),
        // A local write through an exclusive field: 6.
        (
            "",
            "let mut view = View { text: ref a }\nlet editor = Editor { view: ref mut view, edits: 0 }\nderef editor.view = View { text: ref b }\nview.text.length",
        ),
        // The stored borrow comes from another reference's target, not from that reference: 6.
        (
            "",
            "let mut first = View { text: ref a }\nlet copied = { let second = View { text: ref b }; copy_text (ref mut first) (ref second); 0 }\nfirst.text.length + copied",
        ),
        // A record parameter reads its exclusive field's target with the target's region: 5.
        (
            "def text_in {r s} :: Editor {r s} -> ref {s} string = \\editor -> (deref editor.view).text\n",
            "let mut view = View { text: ref a }\nlet kept = text_in (Editor { view: ref mut view, edits: 0 })\nkept.length",
        ),
        // A callee forwards its parameters to another call that stores: 6.
        (
            "def forward {r s} :: ref mut {r} View {s} -> ref {s} string -> unit = \\target text -> replace target text\n",
            "let mut view = View { text: ref a }\nforward (ref mut view) (ref b)\nview.text.length",
        ),
        // The reference and the target may share one region, as in A09: 5.
        (
            "def same {r} :: ref mut {r} View {r} -> i64 = \\target -> (deref target).text.length\n",
            "let mut view = View { text: ref a }\nsame (ref mut view)",
        ),
    ] {
        let source = targets(declarations, code);
        let module = analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
    let formatted =
        tsuzuri::formatter::format_source("Main.tz", TARGETS, tsuzuri::syntax::SourceKind::Code)
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
    accepts(&formatted.formatted);
}

#[test]
fn named_target_regions_keep_stored_borrows_alive() {
    let local = "cannot store a borrow of a local value through a reference parameter";
    let region = "the stored borrow does not have the region of the reference's target";
    let direct = "'replace' may store an input through an exclusive reference, so call it directly with all of its arguments";
    for (declarations, code, error, fragment) in [
        // The owner outlives the borrow that a reference stored in it.
        (
            "",
            "let mut view = View { text: ref a }\n{ let short = \"xy\"; let target = ref mut view; deref target = View { text: ref short }; }\nview.text.length",
            "E1013",
            "",
        ),
        (
            "",
            "let mut view = View { text: ref a }\n{ let short = \"xy\"; replace (ref mut view) (ref short); }\nview.text.length",
            "E1013",
            "",
        ),
        (
            "",
            "let mut view = View { text: ref a }\nlet editor = Editor { view: ref mut view, edits: 0 }\n{ let short = \"xy\"; deref editor.view = View { text: ref short }; }\nview.text.length",
            "E1013",
            "",
        ),
        // The stored borrow keeps its owner borrowed.
        (
            "",
            "let mut view = View { text: ref a }\nlet target = ref mut view\nderef target = View { text: ref b }\nlet moved = b\nview.text.length",
            "E1014",
            "access conflicts with a live borrow",
        ),
        (
            "",
            "let mut view = View { text: ref a }\nlet old = swap_text (ref mut view) (ref b)\nlet moved = a\nold.length",
            "E1014",
            "access conflicts with a live borrow",
        ),
        // A callee stores only inputs with the target's region.
        (
            "def bad {r s t} :: ref mut {r} View {s} -> ref {t} string -> unit = \\target text -> { deref target = View { text: text }; }\n",
            "0",
            "E1013",
            region,
        ),
        (
            "def bad {r s} :: ref mut {r} View {s} -> unit = \\target -> { let local = \"x\"; deref target = View { text: ref local }; }\n",
            "0",
            "E1013",
            local,
        ),
        (
            "def bad {r s} :: Editor {r s} -> ref {r} string -> unit = \\editor text -> { deref editor.view = View { text: text }; }\n",
            "0",
            "E1013",
            region,
        ),
        (
            "def bad :: ref mut View -> ref string -> unit = \\target text -> { deref target = View { text: text }; }\n",
            "0",
            "E1013",
            "assigning borrowed values through references requires explicit lifetimes; name the target's region",
        ),
        // The target's borrows have their own region.
        (
            "def bad {r s} :: ref {r} View {s} -> ref {r} string = \\view -> view.text\n",
            "0",
            "E1013",
            "returned borrow does not match the declared result region",
        ),
        (
            "def bad {r s} :: ref {r} string -> ref {r} View {s} = \\text -> text\n",
            "0",
            "E1013",
            "result region 's' has no matching input",
        ),
        // A function that stores through a reference runs only in direct calls.
        ("", "let f = replace\n0", "E1013", direct),
        (
            "",
            "let mut view = View { text: ref a }\nlet g = replace (ref mut view)\n0",
            "E1005",
            "cannot capture ref mut",
        ),
        (
            "def put {r s} :: ref {s} string -> ref mut {r} View {s} -> unit = \\text target -> replace target text\n",
            "let g = put (ref b)\n0",
            "E1013",
            "'put' may store an input through an exclusive reference, so call it directly with all of its arguments",
        ),
        (
            "",
            "let mut view = View { text: ref a }\n(ref b) |> replace (ref mut view)\n0",
            "E1013",
            direct,
        ),
        // The target of a reference with its own region holds shared borrows only.
        (
            "record Counter {r} { value: ref mut {r} i64 }\ndef bad {r s} :: ref mut {r} Counter {s} -> unit = \\target -> ()\n",
            "0",
            "E1013",
            "the data behind a reference with its own target region cannot hold exclusive borrows",
        ),
        (
            "record Bad {r s} { view: ref mut {r} View }\n",
            "0",
            "E1013",
            "or name the target's region as in 'ref mut {r} T {s}'",
        ),
        (
            "record Bad {r s} { view: ref mut {r} View {s}, other: ref mut {r} i64 }\n",
            "0",
            "E1013",
            "region 'r' belongs to an exclusive borrow",
        ),
        (
            "def bad :: ({r s} ref mut {r} View {s} -> unit) -> unit = \\f -> ()\n",
            "0",
            "E1013",
            "one parameter or result cannot mix distinct named regions",
        ),
    ] {
        rejects_with(&targets(declarations, code), error, fragment);
    }
}

#[test]
fn named_target_regions_do_not_change_generated_ir() {
    let annotated = targets(
        "",
        "let mut view = View { text: ref a }\nlet old = swap_text (ref mut view) (ref b)\nview.text.length + old.length",
    );
    let erased = annotated
        .replace(" {r q s}", "")
        .replace(" {r s}", "")
        .replace(" {r}", "")
        .replace(" {q}", "")
        .replace(" {s}", "");
    let annotated = analyze(&annotated).unwrap();
    // Without named regions the stores through references are rejected, so compare one
    // shape that only reads.
    let reading = "record View { text: ref string }\ndef text_of :: ref View -> ref string = \\view -> view.text\nlet a = \"alpha\"\nlet view = View { text: ref a }\n(text_of (ref view)).length";
    let named = "record View {r} { text: ref {r} string }\ndef text_of {r s} :: ref {r} View {s} -> ref {s} string = \\view -> view.text\nlet a = \"alpha\"\nlet view = View { text: ref a }\n(text_of (ref view)).length";
    for wasm in [false, true] {
        llvm::emit_target(&annotated, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            llvm::emit_target(&analyze(reading).unwrap(), llvm::Entry::Library, wasm).unwrap(),
            llvm::emit_target(&analyze(named).unwrap(), llvm::Entry::Library, wasm).unwrap()
        );
    }
    assert_eq!(analyze(&erased).unwrap_err().code, "E1013");
}
