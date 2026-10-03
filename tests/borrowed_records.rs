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
