use tsuzuri::{analyze, llvm};

#[test]
fn sequences_are_opaque_linear_values_with_owned_or_borrowed_elements() {
    for source in [
        "let sequence = Seq.once 42\nmatch Seq.next sequence with | (rest, Option.Some value) -> value | (_, Option.None) -> 0",
        "let sequence: Seq<i64> = Seq.empty()\nmatch Seq.next sequence with | (_, value) -> Option.is_none (&value)",
        "let text = \"borrowed\"\nlet sequence = Seq.once (ref text)\nmatch Seq.next sequence with | (_, Option.Some value) -> value.length | (_, Option.None) -> 0",
        "let sequence = Seq.once (task { return 42 })\nmatch Seq.next sequence with | (_, Option.Some value) -> Task.run value | (_, Option.None) -> 0",
        "let sequence = Seq.defer (\\() -> (Seq.empty(), Option.Some 42))\nmatch Seq.next sequence with | (_, value) -> Option.get value",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for (source, code) in [
        (
            "let sequence = Seq.once 42\nlet first = Seq.next sequence\nSeq.next sequence",
            "E1012",
        ),
        ("let sequence = Seq.once 42\nsequence.head", "E1022"),
        (
            "fn bad() -> Seq<ref string> { let value = \"x\"; Seq.once (ref value) }",
            "E1013",
        ),
        (
            "let value = 1\nlet sequence = Seq.once (ref value)\ntask { return Seq.next sequence }",
            "E1013",
        ),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}

#[test]
fn sequence_loops_consume_state_and_preserve_existing_control_rules() {
    for source in [
        "let mut total = 0\nfor value in Seq.once 42 do total = total + value\ntotal",
        "let mut total = 0\nlet sequence: Seq<i64> = Seq.empty()\nfor value in sequence do total = total + value\ntotal",
        "let mut total = 0\nfor (text, value) in Seq.once (\"owned\", 37) do total = total + text.length + value\ntotal",
        "let text = \"borrowed\"\nlet mut total = 0\nfor value in Seq.once (ref text) do { total = total + value.length; continue }\ntotal",
        "for value in Seq.once \"owned\" do { assert (value.length == 5); break }",
        "let sequence = Seq.once 42\nlet Seq = 0\nlet mut total = Seq\nfor value in sequence do total = total + value\ntotal",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for (source, code) in [
        (
            "let sequence = Seq.once 1\nfor value in sequence do ()\nSeq.next sequence",
            "E1012",
        ),
        (
            "let text = \"outer\"\nlet mut value = ref text\nfor item in Seq.once \"inner\" do value = ref item\nvalue.length",
            "E1013",
        ),
        (
            "record Counter { finish: i64 }\nlet counter = Counter { finish: 1 }\nfor value in counter do ()",
            "E1005",
        ),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
    let direct =
        analyze("let mut total = 0\nfor value in [1, 2] do total = total + value\ntotal").unwrap();
    assert!(
        !llvm::emit(&direct, llvm::Entry::Console)
            .unwrap()
            .contains("@tz.fn.Seq.next")
    );
}

#[test]
fn lazy_combinators_and_explicit_iterators_preserve_loans() {
    for source in [
        "let sequence = Seq.unfold (\\value -> if value < 4 then Option.Some (value, value + 1) else Option.None) 0\nlet mapped = Seq.map (\\value -> value * 2) sequence\nlet filtered = Seq.filter (\\value -> deref value > 2) mapped\nlet values = Seq.to_array filtered\nArray.sum (&values)",
        "let strings = [\"a\", \"bc\"]\nlet mut total = 0\nfor value in Array.iter (&strings) do total = total + value.length\ntotal",
        "let strings = [|\"a\", \"bc\"|]\nlet mut total = 0\nfor value in List.iter (&strings) do total = total + value.length\ntotal",
        "let values = Vec.push (Vec.empty()) \"owned\"\nlet mut total = 0\nfor value in Vec.iter (&values) do total = total + value.length\ntotal",
        "let map = Map.singleton \"key\" \"value\"\nlet mut total = 0\nfor (key, value) in Map.iter (&map) do total = total + key.length + value.length\ntotal",
        "let set = Set.singleton \"key\"\nlet mut total = 0\nfor key in Set.iter (&set) do total = total + key.length\ntotal",
        "let sequence = Seq.filter (\\value -> value.length > 0) (Seq.once \"value\")\nlet values = Seq.to_array sequence\nvalues[0].length",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for source in [
        "fn bad() -> Seq<ref string> { let values = [\"x\"]; Array.iter (&values) }",
        "let values = [1]\nlet sequence = Array.iter (&values)\ntask { return Seq.to_array sequence }",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1013", "{source}");
    }
}

#[test]
fn bounds_expanded_sequence_loop_nesting() {
    let mut source = "()".to_owned();
    for _ in 0..24 {
        source = format!("for _value in Seq.once 1 do {{ {source} }}");
    }
    assert_eq!(analyze(&source).unwrap_err().code, "E1017");
}

#[test]
fn sequence_next_validates_custom_standard_library_layout() {
    let error = tsuzuri::analyze_modules_with_std(
        &[("Main.tz", "fn use_sequence(sequence: Seq<i64>) -> i64 { let _step = Seq.next sequence; 0 }")],
        &[
            ("std/Option.tc", "union Option<'a> = None | Some of ['a]\ndef Return :: 'a -> 'a\nfn Return value = value"),
            ("std/Seq.tz", "record Seq<'a> { head: Option.Option<'a>, step: Option.Option<unit -> (Seq<'a> * Option.Option<'a>)> }"),
        ],
    ).unwrap_err();
    assert_eq!(error.code, "E1005");
}
