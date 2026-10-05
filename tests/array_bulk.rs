use tsuzuri::{analyze, llvm};

#[test]
fn array_bulk_source_apis_and_borrowed_results() {
    for source in [
        "let values = [3, 1, 2]\nlet sorted = Array.sort (ref values)\nArray.sum (ref sorted)",
        "let values = [1, 2, 3]\nlet lists = Array.to_list (ref values)\nlet mapped = List.map (value -> value + 1) (ref lists)\nlet reversed = List.reverse (ref mapped)\nlet result = List.to_array (ref reversed)\nArray.sum (ref result)",
        "let values = [|\"a\", \"bb\"|]\nlet mapped = List.map_ref (text -> text.length) (ref values)\nList.fold (total -> value -> total + value) 0 (ref mapped)",
        "let values = [|\"a\", \"bb\"|]\nList.fold_ref (total -> value -> total + value.length) 0 (ref values)",
        "let values = [[1, 2], [], [3]]\nlet combined = Array.concat (ref values)\nArray.sum (ref combined)",
        "let values = [1, 2, 3]\nArray.sum (ref values) + Array.product (ref values)",
        "let values = [1.0, 2.0, 3.0]\nArray.sum (ref values) + Array.product (ref values)",
        "let values = [\"a\", \"bb\"]\nlet mapped = Array.map_ref (text -> text.length) (ref values)\nArray.sum (ref mapped)",
        "let values = [\"a\", \"bb\"]\n(Array.at (ref values) 1).length",
        "let values = [\"a\", \"bb\"]\n(Maybe.get (Array.find (text -> text.length == 2) (ref values))).length",
        "let values = [3, 1, 2]\n*(Maybe.get (Array.min (ref values)))",
        "let values = [1, 2, 3]\nArray.fold_back_ref (value -> state -> deref value + state) (ref values) 0",
        "let values = [1, 2, 2, 3]\nlet target = 2\nMaybe.get (Array.binary_search (ref values) (ref target))",
        "let left = [\"first\", \"second\"]\nlet right = [\"first\", \"second\"]\nArray.equal (ref left) (ref right)",
    ] {
        let module = analyze(source)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
}

#[test]
fn ordered_reductions_preserve_float_types_and_explicit_fusion() {
    for ty in ["f16", "f32", "f64", "f128", "d32", "d64", "d128"] {
        let source = format!(
            "def reductions :: ref [{ty}] -> {ty}\nfn reductions values = Array.sum_pairwise values + Array.sum_kahan values + Array.dot values values + Array.dot_fma values values"
        );
        let module = analyze(&source).unwrap_or_else(|error| panic!("{ty}: {error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert!(ir.contains("@tz_soft_fma") || ir.contains("@llvm.fma."));
            assert!(
                !ir.contains(" contract ") && !ir.contains(" reassoc ") && !ir.contains(" fast ")
            );
        }
    }
    for name in ["sum_pairwise", "sum_kahan", "dot", "dot_fma"] {
        let arguments = if name.starts_with("dot") {
            "(ref values) (ref values)"
        } else {
            "(ref values)"
        };
        assert_eq!(
            analyze(&format!("let values = [1, 2]\nArray.{name} {arguments}"))
                .unwrap_err()
                .code,
            "E1005"
        );
    }
    assert_eq!(
        analyze("let left = [1.0f32]\nlet right = [1.0f64]\nArray.dot (ref left) (ref right)")
            .unwrap_err()
            .code,
        "E1003"
    );
    let module = analyze("def separate :: f64 -> f64 -> f64 -> f64\nfn separate left right addend = left * right + addend\ndef dot :: ref [f64] -> ref [f64] -> f64\nfn dot left right = Array.dot left right").unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(!ir.contains("@llvm.fma.") && !ir.contains("call void @tz_soft_fma("));
    assert!(ir.contains("fmul double") && ir.contains("fadd double"));
}
