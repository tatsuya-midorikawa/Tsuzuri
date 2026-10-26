use tsuzuri::{analyze, llvm, stdlib};

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.into_data().message
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
        assert!(
            llvm::duplicate_declarations(&ir).is_empty(),
            "{source}: declarations"
        );
        ir
    })
}

const BASE: &str =
    "let m = Matrix.init 3 4 (\\i j -> i * 10 + j)\nlet whole = MatrixView.of_matrix (ref m)\n";

const READ_ONLY: &str = "let m = Matrix.init 3 4 (\\i j -> i * 10 + j)\nlet whole = MatrixView.of_matrix (ref m)\nlet t = MatrixView.transpose whole\nlet window = MatrixView.sub 1 1 2 2 whole\nlet column = MatrixView.col 2 whole\nlet top = MatrixView.row 0 whole\nlet copy = MatrixView.to_matrix t\nlet sum = MatrixView.fold (\\acc x -> acc + x) 0 window\nlet product = MatrixView.mul t whole\nlet mapped = MatrixView.map (\\x -> x * 2) window\nlet strided = MatrixView.strided 1 2 2 4 2 (Matrix.as_array (ref m))\nderef (MatrixView.at 2 1 t) + MatrixView.rows column + MatrixView.cols top + sum + Maybe.default_value 0 (MatrixView.get 0 3 top) + deref (Matrix.at 3 2 (ref copy)) + deref (Matrix.at 3 3 (ref product)) + deref (Matrix.at 1 0 (ref mapped)) + MatrixView.offset strided + MatrixView.row_stride strided + MatrixView.col_stride strided + Array.length (MatrixView.data whole)";

const MUTABLE: &str = "let mut data = [0, 0, 0, 0, 0, 0, 0, 0, 0]\nlet mut view = MatrixView.of_array_mut 3 3 (ref mut data)\nMatrixView.write 1 2 7 (ref mut view)\nMatrixView.fill 4 (ref mut view)\nlet row = MatrixView.row_mut 1 (ref mut view)\nArray.write 0 9 row\nlet mut inner = MatrixView.sub_mut 1 1 2 2 (ref mut view)\nMatrixView.fill 1 (ref mut inner)\nlet source = [10, 20, 30, 40]\nlet from = MatrixView.of_array 2 2 (ref source)\nlet mut corner = MatrixView.sub_mut 0 0 2 2 (ref mut view)\nMatrixView.copy_from (MatrixView.transpose from) (ref mut corner)\nMatrixView.map_in_place (\\x -> x + 1) (ref mut corner)\nlet seen = MatrixView.freeze (ref view)\nlet total = MatrixView.fold (\\acc x -> acc + x) 0 seen\nlet flipped = MatrixView.transpose_mut view\nlet last = MatrixView.freeze (ref flipped)\ntotal + MatrixView.rows last";

#[test]
fn matrix_views_accept_the_documented_operations() {
    emits(READ_ONLY);
    emits(MUTABLE);
    // A view of a view, and several views of one owner, at once.
    emits(&format!(
        "{BASE}let a = MatrixView.sub 0 0 2 2 whole\nlet b = MatrixView.transpose a\nlet c = MatrixView.of_matrix (ref m)\nMatrixView.rows a + MatrixView.rows b + MatrixView.cols c"
    ));
    // The owner is usable again after the last use of its views.
    emits(&format!(
        "{BASE}let rows = MatrixView.rows whole\nlet moved = m\nrows + Matrix.rows (ref moved)"
    ));
    // Views of other buffers: a plain array, a tensor-free GPU-style flat array.
    emits(
        "let values = [1, 2, 3, 4, 5, 6]\nlet v = MatrixView.of_array 2 3 (ref values)\nMatrixView.rows v * MatrixView.cols v",
    );
}

#[test]
fn matrix_view_records_are_opaque() {
    for source in [
        "whole.rows",
        "let other = { whole with rows = 1 }\nMatrixView.rows other",
        "match whole with | MatrixView { rows = r } -> r",
        "let values = [1]\nlet fake = MatrixView { data: ref values, offset: 0, rows: 1, cols: 1, row_stride: 1, col_stride: 1 }\nMatrixView.rows fake",
    ] {
        let source = format!("{BASE}{source}");
        let message = rejects(&source, "E1022");
        assert!(message.contains("is opaque"), "{source}\n{message}");
    }
    for source in [
        "let mut data = [1, 2]\nlet mut view = MatrixView.of_array_mut 1 2 (ref mut data)\nview.rows",
        "let mut data = [1, 2]\nlet mut view = MatrixView.of_array_mut 1 2 (ref mut data)\nlet again = { view with rows = 0 }\n0",
    ] {
        let message = rejects(source, "E1022");
        assert!(message.contains("is opaque"), "{source}\n{message}");
    }
}

#[test]
fn views_borrow_their_owner_and_cannot_outlive_it() {
    let message = rejects(
        "def leak {r} :: ref {r} Matrix<i64> -> MatrixView<i64> {r}\nfn leak outer =\n    let m = Matrix.init 2 2 (\\i j -> i + j)\n    MatrixView.of_matrix (ref m)\n0",
        "E1013",
    );
    assert!(message.contains("does not live long enough"), "{message}");
    for source in [
        "let moved = m\nMatrixView.rows whole + Matrix.rows (ref moved)",
        "let changed = Matrix.set 0 0 1 m\nMatrixView.rows whole",
        "let flat = Matrix.to_array m\nMatrixView.rows whole",
    ] {
        let message = rejects(&format!("{BASE}{source}"), "E1014");
        assert!(
            message.starts_with("access conflicts with a live borrow"),
            "{source}\n{message}"
        );
    }
    // A view is Copy and holds only a shared borrow, so a task cannot take it.
    let message = rejects(
        &format!("{BASE}Task.run (task {{ return MatrixView.rows whole }})"),
        "E1013",
    );
    assert!(
        message.starts_with("tasks require owned values"),
        "{message}"
    );
}

#[test]
fn writable_views_hold_an_exclusive_borrow() {
    let prefix =
        "let mut data = [1, 2, 3, 4]\nlet mut view = MatrixView.of_array_mut 2 2 (ref mut data)\n";
    for source in [
        "let first = data[0]\nMatrixView.write 0 0 9 (ref mut view)\nfirst",
        "let mut second = MatrixView.of_array_mut 2 2 (ref mut data)\nMatrixView.write 0 0 9 (ref mut view)\nMatrixView.write 0 0 1 (ref mut second)",
        "let seen = MatrixView.freeze (ref view)\nMatrixView.write 0 0 9 (ref mut view)\nMatrixView.rows seen",
        "let row = MatrixView.row_mut 0 (ref mut view)\nMatrixView.write 0 0 9 (ref mut view)\nArray.write 0 1 row",
    ] {
        let message = rejects(&format!("{prefix}{source}"), "E1014");
        assert!(
            message.starts_with("access conflicts with a live borrow"),
            "{source}\n{message}"
        );
    }
    // The data is readable again once the view is no longer used.
    emits(&format!(
        "{prefix}MatrixView.write 0 0 9 (ref mut view)\nlet first = data[0]\nfirst"
    ));
    let message = rejects(
        &format!("{prefix}let moved = view\nMatrixView.write 0 0 9 (ref mut view)"),
        "E1012",
    );
    assert!(
        message.starts_with("use of moved or partially moved value 'view'"),
        "{message}"
    );
    let message = rejects(
        &format!("{prefix}Task.run (task {{ return MatrixView.write 0 0 9 (ref mut view) }})"),
        "E1013",
    );
    assert!(
        message.starts_with("tasks require owned values"),
        "{message}"
    );
}

#[test]
fn view_constraints_follow_array_rules() {
    let words = "let w = Matrix.init 2 2 (\\i j -> \"x\")\nlet v = MatrixView.of_matrix (ref w)\n";
    for source in [
        "let s = MatrixView.at 0 1 v\ns.length",
        "MatrixView.rows (MatrixView.transpose (MatrixView.sub 0 0 1 2 v))",
    ] {
        emits(&format!("{words}{source}"));
    }
    for source in [
        "let n = MatrixView.to_matrix v\nMatrix.rows (ref n)",
        "MatrixView.fold (\\total s -> total + 1) 0 v",
        "Maybe.is_some (ref (MatrixView.get 0 0 v))",
        "let n = MatrixView.map (\\s -> 1) v\nMatrix.rows (ref n)",
        "let p = MatrixView.mul v v\nMatrix.rows (ref p)",
    ] {
        let message = rejects(&format!("{words}{source}"), "E1005");
        assert!(
            message.starts_with("no instance for "),
            "{source}\n{message}"
        );
    }
    let mut_words = "let mut data = [\"a\", \"b\"]\nlet mut view = MatrixView.of_array_mut 1 2 (ref mut data)\n";
    emits(&format!(
        "{mut_words}MatrixView.write 0 0 \"z\" (ref mut view)\n0"
    ));
    for source in [
        "MatrixView.fill \"z\" (ref mut view)",
        "MatrixView.map_in_place (\\s -> s) (ref mut view)",
    ] {
        let message = rejects(&format!("{mut_words}{source}"), "E1005");
        assert!(
            message.starts_with("no instance for "),
            "{source}\n{message}"
        );
    }
}

#[test]
fn view_modules_load_with_the_modules_they_use() {
    let loaded = |text: &str| -> Vec<String> {
        stdlib::sources_for([text])
            .iter()
            .map(|(path, _)| path.to_string())
            .filter(|path| {
                ["std/Matrix.tz", "std/MatrixView.tz", "std/Tensor.tz"].contains(&path.as_str())
            })
            .collect()
    };
    assert_eq!(loaded("42"), Vec::<String>::new());
    assert_eq!(loaded("Matrix.rows (ref m)"), ["std/Matrix.tz"]);
    assert_eq!(
        loaded("MatrixView.rows v"),
        ["std/Matrix.tz", "std/MatrixView.tz"]
    );
    assert_eq!(
        loaded("Tensor.rank (ref v)"),
        ["std/Matrix.tz", "std/MatrixView.tz", "std/Tensor.tz"]
    );
    // Programs that name only Matrix never see the view and tensor modules.
    let module = analyze("let m = Matrix.init 1 1 (\\i j -> 0)\nMatrix.rows (ref m)").unwrap();
    assert!(
        module
            .functions
            .iter()
            .all(|function| function.module != "MatrixView" && function.module != "Tensor")
    );
    let error = tsuzuri::analyze_modules(&[("MatrixView", "")]).unwrap_err();
    assert_eq!(error.code, "E1011", "{}", error.message);
}

#[test]
fn view_products_use_the_same_separately_rounded_kernel() {
    for literal in ["1.5f32", "1.5", "3i64"] {
        let source = format!(
            "let a = Matrix.init 2 2 (\\i j -> {literal})\nlet v = MatrixView.of_matrix (ref a)\nlet p = MatrixView.mul (MatrixView.transpose v) v\nMatrix.rows (ref p)"
        );
        for ir in emits(&source) {
            assert!(
                ir.contains("@tz.fn.Matrix.multiply_rows"),
                "{literal}: {ir}"
            );
            for word in ["fmuladd", "llvm.fma", " fast ", "contract", "reassoc"] {
                assert!(!ir.contains(word), "{literal}: {word}\n{ir}");
            }
        }
    }
}
