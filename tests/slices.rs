use tsuzuri::{analyze, llvm};

#[test]
fn slice_types_and_borrowed_boundaries() {
    for source in [
        "let values = [10, 20, 30]\nlet slice = &values[1..3]\nslice[0] + slice.length",
        "let values = [10, 20, 30]\nlet slice = ref values[1..]\nlet nested = ref slice[..1]\nnested[0]",
        "def first :: ref [string] -> ref string\nfn first values = ref values[0]\nlet values = [\"hello\", \"there\"]\n(first (ref values[1..])).length",
        "def middle :: ref [i64] -> ref [i64]\nfn middle values = ref values[1..]\nlet values = [1, 2]\n(middle (ref values)).length",
        "let values = [1, 2, 3]\nlet slice = ref values[..2]\nlet read = ignored -> slice.length\nread ()",
        "let values = [1, 2, 3]\nlet slices = [ref values[..1], ref values[1..]]\nslices[0].length + slices[1].length",
        "def copy :: ref [i64] -> [i64]\nfn copy values = deref values",
        "let values = [1, 2, 3]\nlet mut total = 0\nfor value in ref values[1..] do total = total + value\ntotal",
        "let values = [1, 2, 3]\nOption.get (Option { let slice = ref values[1..]; return slice.length })",
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
}

#[test]
fn shared_views_use_descriptors_and_copy_only_on_value_dereference() {
    let module =
        analyze("def length :: ref [i64] -> i64\nfn length values = values.length").unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.contains("@tz.fn.Main.length(%tz.array"));
    assert!(!ir.contains("call ptr @tz.alloc"));
    let module = analyze("def copy :: ref [i64] -> [i64]\nfn copy values = deref values").unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.contains("call ptr @tz.alloc"));
    let module =
        analyze("def replace :: ref mut [i64] -> unit\nfn replace values = deref values = [42]")
            .unwrap();
    assert!(
        llvm::emit(&module, llvm::Entry::Library)
            .unwrap()
            .contains("@tz.fn.Main.replace(ptr")
    );
}

#[test]
fn slices_reject_escaping_loans_and_source_mutations() {
    for (source, code) in [
        ("let values = [1, 2]\nvalues[0..1]", "E0002"),
        ("let values = [|1, 2|]\nref values[..1]", "E1005"),
        ("let text = \"text\"\nref text[1..]", "E1005"),
        ("let values = [1, 2]\nref values[true..]", "E1003"),
        ("record Stored { values: ref [i64] }", "E1013"),
        (
            "let slice = { let values = [1, 2]; ref values[..1] }\nslice.length",
            "E1013",
        ),
        (
            "let mut values = [1, 2]\nlet slice = ref values[..1]\nvalues = [3]\nslice.length",
            "E1014",
        ),
        (
            "let mut values = [1, 2]\nlet slice = ref values[{ values = [3]; 0 }..1]\nslice.length",
            "E1014",
        ),
        (
            "let values = [\"owned\"]\nlet slice = ref values[..1]\nlet copy = deref slice\ncopy.length",
            "E1012",
        ),
        (
            "let values = [1, 2]\nlet work = task { return ref values[..1] }\n()",
            "E1013",
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(error.code, code, "{source}\n{}", error.message);
    }
}
