use tsuzuri::{analyze, llvm};

#[test]
fn consuming_update_signatures_preserve_ownership() {
    for source in [
        "Array.set [1, 2, 3] 1 9",
        "Array.update (new [string](2, index -> \"a\")) 1 (text -> text + \"x\")",
        "Array.swap [\"first\", \"second\"] 0 1",
        "List.tail (List.cons \"first\" (new [|string|](2, index -> \"rest\")))",
        "let values = [1, 2]\nlet updated = Array.set values 0 9\nvalues[0] + updated[0]",
        "let values = [1, 2]\nlet old = ref values[0]\nlet updated = Array.set values 0 9\nderef old + updated[0]",
        "let set = Array.set [1, 2] 0\nset 9",
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
        ("Array.set [1, 2] true 9", "E1003"),
        (
            "let values: [i64] = [1, 2]\nArray.set values 0 true",
            "E1003",
        ),
        (
            "let values = [\"owned\"]\nlet updated = Array.set values 0 \"next\"\nvalues.length",
            "E1012",
        ),
        (
            "let values = [\"owned\"]\nlet old = ref values[0]\nlet updated = Array.set values 0 \"next\"\nold.length",
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
        analyze("def update :: [i64] -> [i64]\nfn update values = Array.set values 0 42").unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let body = ir
        .split("define internal %tz.array @tz.builtin.Array.set.i64")
        .nth(1)
        .unwrap()
        .split("}\n\n")
        .next()
        .unwrap();
    assert!(body.find("icmp ult").unwrap() < body.find("getelementptr").unwrap());
    assert!(!body.contains("@tz.alloc"));
    assert!(!body.contains("phi"));
}
