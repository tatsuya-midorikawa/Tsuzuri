use tsuzuri::{analyze, check::Type, llvm};

#[test]
fn vec_types_are_parametric_owned_and_noncopy() {
    let module = analyze("def identity :: Vec<'a> -> Vec<'a>\nfn identity value = value").unwrap();
    let ty = Type::Vec(Box::new(Type::I64));
    assert_eq!(ty.display(&module.types()), "Vec<i64>");
    assert!(!ty.is_copy(&module.types()));
    assert!(ty.needs_drop(&module.types()));
    for (source, code) in [
        (
            "def twice :: Vec<i64> -> Vec<i64>\nfn twice value = { let copy = value; value }",
            "E1012",
        ),
        ("record Invalid { value: Vec<ref mut i64> }", "E1013"),
        (
            "export def bad :: Vec<i64> -> i64\nfn bad values = values.length",
            "E1008",
        ),
        (
            "def bad :: Vec<i64, i64> -> unit\nfn bad value = ()",
            "E1004",
        ),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}

#[test]
fn vec_access_cannot_move_borrowed_elements_or_invalidate_loans() {
    for (source, code) in [
        (
            "let values = Vec.push (Vec.empty()) \"owned\"\nvalues[0]",
            "E1012",
        ),
        (
            "let values = Vec.push (Vec.empty()) \"owned\"\nVec.get (ref values) 0",
            "E1005",
        ),
        (
            "let values = Vec.push (Vec.empty()) 1\nlet borrowed = Vec.at (ref values) 0\nlet changed = Vec.push values 2\nderef borrowed",
            "E1014",
        ),
        (
            "let borrowed = { let values = Vec.push (Vec.empty()) 1; Vec.at (ref values) 0 }\nderef borrowed",
            "E1013",
        ),
        (
            "let mut values = Vec.push (Vec.empty()) 1\nref mut values[0]",
            "E1014",
        ),
        ("type Vec = i64", "E1001"),
        ("union Vec = Empty", "E1001"),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}

#[test]
fn vec_capacity_and_clone_lowering() {
    for source in [
        "let values = Vec.push (Vec.empty()) 42\nvalues[0]",
        "let values = Vec.push (Vec.empty()) \"owned\"\n(Vec.at (ref values) 0).length",
        "let values = Vec.of_array [1, 2]\nlet values = Vec.reserve values 10\nlet values = Vec.swap values 0 1\nlet values = Vec.set values 0 42\nlet array = Vec.to_array values\narray[0]",
        "let values = Vec.push (Vec.empty()) 42\nmatch Vec.pop values with | (remaining, item) -> Vec.length (ref remaining) + Maybe.get item",
        "let values = Vec.push (Vec.empty()) \"owned\"\nlet values = Vec.truncate values 0\nlet values = Vec.clear values\nVec.length (ref values)",
        "let values: Vec<i64> = Vec.empty()\nVec.length (ref values)",
        "let values: Vec<string> = Vec.with_capacity 4\nVec.capacity (ref values)",
        "let values: Vec<i64> = Vec.with_capacity 4\nlet copy = Vec.clone (ref values)\nVec.is_empty (ref copy)",
    ] {
        let module = analyze(source).unwrap();
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(ir.matches("%tz.vec = type").count(), 1);
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
}
