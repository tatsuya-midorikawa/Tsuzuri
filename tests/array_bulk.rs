use tsuzuri::{analyze, llvm};

#[test]
fn array_bulk_source_apis_and_borrowed_results() {
    for source in [
        "let values = [3, 1, 2]\nlet sorted = Array.sort (ref values)\nArray.sum (ref sorted)",
        "let values = [1, 2, 3]\nlet lists = Array.to_list (ref values)\nlet mapped = List.map (ref lists) (value -> value + 1)\nlet reversed = List.reverse (ref mapped)\nlet result = List.to_array (ref reversed)\nArray.sum (ref result)",
        "let values = [|\"a\", \"bb\"|]\nlet mapped = List.map_ref (ref values) (text -> text.length)\nList.fold (ref mapped) 0 (total -> value -> total + value)",
        "let values = [|\"a\", \"bb\"|]\nList.fold_ref (ref values) 0 (total -> value -> total + value.length)",
        "let values = [[1, 2], [], [3]]\nlet combined = Array.concat (ref values)\nArray.sum (ref combined)",
        "let values = [1, 2, 3]\nArray.sum (ref values) + Array.product (ref values)",
        "let values = [1.0, 2.0, 3.0]\nArray.sum (ref values) + Array.product (ref values)",
        "let values = [\"a\", \"bb\"]\nlet mapped = Array.map_ref (ref values) (text -> text.length)\nArray.sum (ref mapped)",
        "let values = [\"a\", \"bb\"]\n(Array.at (ref values) 1).length",
        "let values = [\"a\", \"bb\"]\n(Option.get (Array.find (ref values) (text -> text.length == 2))).length",
        "let values = [3, 1, 2]\n*(Option.get (Array.min (ref values)))",
        "let values = [1, 2, 3]\nArray.fold_back_ref (ref values) 0 (value -> state -> deref value + state)",
        "let values = [1, 2, 2, 3]\nlet target = 2\nOption.get (Array.binary_search (ref values) (ref target))",
        "let left = [\"first\", \"second\"]\nlet right = [\"first\", \"second\"]\nArray.equal (ref left) (ref right)",
    ] {
        let module = analyze(source)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
}
