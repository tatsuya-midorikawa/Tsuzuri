use tsuzuri::{analyze, llvm};

#[test]
fn consuming_update_signatures_preserve_ownership() {
    for source in [
        "Array.__set 1 9 [1, 2, 3]",
        "Array.update 1 (text -> text + \"x\") (new [string](2, index -> \"a\"))",
        "Array.swap 0 1 [\"first\", \"second\"]",
        "List.tail (List.cons \"first\" (new [|string|](2, index -> \"rest\")))",
        "let values = [1, 2]\nlet updated = Array.__set 0 9 values\nvalues[0] + updated[0]",
        "let values = [1, 2]\nlet old = ref values[0]\nlet updated = Array.__set 0 9 values\nderef old + updated[0]",
        "let set = Array.__set 0 9\nset [1, 2]",
        "let owned = [1, 2]\nlet set = \\value -> Array.__set 0 value owned\nset 9",
        "let tail = List.tail\ntail [|1, 2|]",
    ] {
        let module = analyze(source)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
    for (source, code) in [
        ("Array.__set true 9 [1, 2]", "E1003"),
        (
            "let values: [i64] = [1, 2]\nArray.__set 0 true values",
            "E1003",
        ),
        (
            "let values = [\"owned\"]\nlet updated = Array.__set 0 \"next\" values\nvalues.length",
            "E1012",
        ),
        (
            "let values = [\"owned\"]\nlet old = ref values[0]\nlet updated = Array.__set 0 \"next\" values\nold.length",
            "E1014",
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(error.code, code, "{source}\n{}", error.message);
    }
}

#[test]
fn update_builtins_do_not_clone_their_input_buffers() {
    let module =
        analyze("def update :: [i64] -> [i64]\nfn update values = Array.__set 0 42 values")
            .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let body = ir
        .split("define internal %tz.array @tz.builtin.Array.__set.i64")
        .nth(1)
        .unwrap()
        .split("}\n\n")
        .next()
        .unwrap();
    assert!(body.find("icmp ult").unwrap() < body.find("getelementptr").unwrap());
    assert!(!body.contains("@tz.alloc"));
    assert!(!body.contains("phi"));
}
