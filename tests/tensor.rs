use tsuzuri::{analyze, llvm};

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.message
}

fn emits(source: &str) -> [String; 2] {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    [false, true].map(|wasm| {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR is deterministic"
        );
        let declarations: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("declare "))
            .collect();
        let unique: std::collections::BTreeSet<_> = declarations.iter().collect();
        assert_eq!(unique.len(), declarations.len(), "{source}: declarations");
        ir
    })
}

const BASE: &str = "let t = Tensor.init [2, 3, 4] (\\i -> i)\nlet v = Tensor.view (ref t)\n";

const ACCEPTED: &str = "let t = Tensor.init [2, 3, 4] (\\i -> i)\nlet v = Tensor.view (ref t)\nlet p = Tensor.permute (ref v) [2, 0, 1]\nlet slice = Tensor.index_axis (ref v) 1 2\nlet window = Tensor.narrow (ref v) 2 1 2\nlet packed = Tensor.to_tensor (ref p)\nlet sum = Tensor.fold (\\acc x -> acc * 3 + x) 0 (ref slice)\nlet flat = Tensor.reshape_view (ref v) [6, 4]\nlet matrix = Tensor.as_matrix_view (ref flat)\nlet back = Tensor.of_matrix_view matrix\nlet mapped = Tensor.map (\\x -> x + 1) (ref window)\nlet mapped_view = Tensor.view (ref mapped)\nlet raw = [1, 2, 3, 4, 5, 6]\nlet borrowed = Tensor.borrow [3, 2] (ref raw)\nderef (Tensor.at (ref v) [1, 2, 3]) + deref (Tensor.at (ref p) [3, 1, 2]) + Tensor.rank (ref slice) + Tensor.count (ref window) + Maybe.get (Tensor.get (ref v) [1, 0, 0]) + (if Maybe.is_some (ref (Tensor.get (ref v) [2, 0, 0])) then 1 else 0) + deref (Array.at (Tensor.as_array (ref packed)) 5) + sum + deref (MatrixView.at matrix 5 3) + deref (Tensor.at (ref back) [4, 2]) + Array.length (Tensor.shape (ref mapped_view)) + deref (Tensor.at (ref borrowed) [2, 1]) + (if Tensor.is_contiguous (ref p) then 1 else 0)";

#[test]
fn tensors_accept_the_documented_operations() {
    emits(ACCEPTED);
    // Owners: construction, conversions with Matrix, reshape without copying, the flat buffer.
    emits(
        "let m = Matrix.init 2 3 (\\i j -> i * 10 + j)\nlet t = Tensor.of_matrix m\nlet r = Tensor.reshape [3, 2] t\nlet length = Array.length (Tensor.as_array (ref r))\nlet back = Tensor.to_matrix r\nlet flat = Tensor.to_array (Tensor.of_array [2, 2] [1, 2, 3, 4])\nlength + Matrix.rows (ref back) + flat.length",
    );
    emits(
        "let t = Tensor.of_array [] [7]\nlet v = Tensor.view (ref t)\nderef (Tensor.at (ref v) [])",
    );
}

#[test]
fn tensor_records_are_opaque_and_noncopy() {
    for source in [
        "t.shape",
        "let other = { t with data = [] }\nlet view = Tensor.view (ref other)\nTensor.rank (ref view)",
        "match t with | Tensor { data = d } -> d.length",
        "let fake = Tensor { shape: [1], data: [1] }\n0",
    ] {
        let source = format!("let t = Tensor.init [2] (\\i -> i)\n{source}");
        let message = rejects(&source, "E1022");
        assert!(message.contains("is opaque"), "{source}\n{message}");
    }
    let message = rejects(&format!("{BASE}v.shape"), "E1022");
    assert!(message.contains("is opaque"), "{message}");
    for source in [
        "let t = Tensor.init [2] (\\i -> i)\nlet u = t\nlet view = Tensor.view (ref t)\nTensor.rank (ref view)",
        "let t = Tensor.init [2] (\\i -> 1.5)\nlet u = t\nlet view = Tensor.view (ref t)\nTensor.rank (ref view)",
        "let t = Tensor.init [2] (\\i -> i)\nlet v = Tensor.view (ref t)\nlet w = v\nTensor.rank (ref v)",
    ] {
        let message = rejects(source, "E1012");
        assert!(
            message.starts_with("use of moved or partially moved value"),
            "{source}\n{message}"
        );
    }
    rejects(
        "export def bad :: Tensor<i64>\nfn bad = Tensor.init [1] (\\i -> 0)",
        "E1008",
    );
    let error = tsuzuri::analyze_modules(&[("Tensor", "")]).unwrap_err();
    assert_eq!(error.code, "E1011", "{}", error.message);
}

#[test]
fn tensor_views_borrow_their_owner() {
    let message = rejects(
        "def leak {r} :: ref {r} Tensor<i64> -> Tensor.View<i64> {r}\nfn leak outer =\n    let t = Tensor.init [2, 2] (\\i -> i)\n    Tensor.view (ref t)\n0",
        "E1013",
    );
    assert!(message.contains("does not live long enough"), "{message}");
    for source in [
        "let moved = t\nTensor.rank (ref v) + Array.length (Tensor.as_array (ref moved))",
        "let reshaped = Tensor.reshape [24] t\nTensor.rank (ref v)",
        "let flat = Tensor.to_array t\nTensor.rank (ref v)",
    ] {
        let message = rejects(&format!("{BASE}{source}"), "E1014");
        assert!(
            message.starts_with("access conflicts with a live borrow"),
            "{source}\n{message}"
        );
    }
    // Views made from a view keep borrowing the same owner.
    let message = rejects(
        &format!(
            "{BASE}let p = Tensor.permute (ref v) [2, 0, 1]\nlet moved = t\nTensor.rank (ref p)"
        ),
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );
    // The owner is usable again after the last use of the views.
    emits(&format!(
        "{BASE}let rank = Tensor.rank (ref v)\nlet moved = t\nrank + Array.length (Tensor.as_array (ref moved))"
    ));
}

#[test]
fn tensor_constraints_follow_array_rules() {
    let words = "let w = Tensor.init [2] (\\i -> \"x\")\nlet v = Tensor.view (ref w)\n";
    for source in [
        "let s = Tensor.at (ref v) [1]\ns.length",
        "let first = Tensor.index_axis (ref v) 0 1\nTensor.rank (ref first)",
        "Tensor.count (ref v)",
    ] {
        emits(&format!("{words}{source}"));
    }
    for source in [
        "let n = Tensor.to_tensor (ref v)\nlet view = Tensor.view (ref n)\nTensor.rank (ref view)",
        "Tensor.fold (\\total s -> total + 1) 0 (ref v)",
        "Maybe.is_some (ref (Tensor.get (ref v) [0]))",
        "let n = Tensor.map (\\s -> 1) (ref v)\nlet view = Tensor.view (ref n)\nTensor.rank (ref view)",
    ] {
        let message = rejects(&format!("{words}{source}"), "E1005");
        assert!(
            message.starts_with("no instance for "),
            "{source}\n{message}"
        );
    }
}
