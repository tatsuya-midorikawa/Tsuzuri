use tsuzuri::{analyze, llvm, stdlib};

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

const ACCEPTED: &str = "let a = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]\nlet t = Matrix.transpose (ref a)\nlet p = Matrix.mul (ref a) (ref t)\nlet r = Matrix.row (ref p) 1\nlet s = Matrix.add (ref p) (ref p)\nlet flat = Matrix.to_array s\nArray.sum r + deref (Matrix.at (ref p) 0 1) + flat[3] + Maybe.get (Matrix.get (ref p) 1 0)";

const PREFIX: &str = "let m = Matrix.init 2 3 (\\i j -> i + j)\n";

#[test]
fn matrix_is_an_opaque_noncopy_std_record() {
    emits(ACCEPTED);
    for source in [
        "Matrix { rows: 1, cols: 1, data: [1] }",
        "m.rows",
        "let other = { m with rows = 2 }\nMatrix.rows (ref other)",
        "match m with | Matrix { data = d } -> d.length",
    ] {
        let source = if source.starts_with("Matrix {") {
            source.to_string()
        } else {
            format!("{PREFIX}{source}")
        };
        let message = rejects(&source, "E1022");
        assert!(message.contains("is opaque"), "{source}\n{message}");
    }
    for source in [
        format!("{PREFIX}let n = m\nMatrix.rows (ref m)"),
        "let m = Matrix.init 1 1 (\\i j -> 1.5)\nlet n = m\nMatrix.rows (ref m)".to_string(),
    ] {
        let message = rejects(&source, "E1012");
        assert!(
            message.starts_with("use of moved or partially moved value 'm'"),
            "{message}"
        );
    }
    rejects(
        "export def bad :: Matrix<i64>\nfn bad = Matrix.init 1 1 (\\i j -> 0)",
        "E1008",
    );
    let error = tsuzuri::analyze_modules(&[("Matrix", "")]).unwrap_err();
    assert_eq!(error.code, "E1011", "{}", error.message);
}

#[test]
fn matrix_borrows_follow_array_rules() {
    emits(&format!(
        "{PREFIX}let p = Matrix.mul (ref m) (ref (Matrix.transpose (ref m)))\nlet r = Matrix.row (ref p) 1\nlet s = Matrix.add (ref p) (ref p)\nArray.sum r + Matrix.rows (ref s)"
    ));
    emits(&format!(
        "{PREFIX}let again = Matrix.of_array (Matrix.rows (ref m)) (Matrix.cols (ref m)) (Matrix.to_array m)\nMatrix.rows (ref again)"
    ));
    emits(&format!(
        "{PREFIX}let copy = Matrix.of_array (Matrix.rows (ref m)) (Matrix.cols (ref m)) (deref (Matrix.as_array (ref m)))\nMatrix.rows (ref m) + Matrix.rows (ref copy)"
    ));
    let message = rejects(
        &format!("{PREFIX}let r = Matrix.row (ref m) 0\nlet n = m\nr[0]"),
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );
    let message = rejects(
        &format!("{PREFIX}let all = Matrix.as_array (ref m)\nlet n = m\nall[0]"),
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );
    let message = rejects(
        &format!("{PREFIX}let flat = Matrix.to_array m\nMatrix.rows (ref m)"),
        "E1012",
    );
    assert!(
        message.starts_with("use of moved or partially moved value 'm'"),
        "{message}"
    );
    rejects(
        "fn bad() -> &i64 { let m = Matrix.init 1 1 (\\i j -> 1); Matrix.at (&m) 0 0 }",
        "E1013",
    );
}

#[test]
fn matrix_constraints_match_array_apis() {
    let words = "let w = Matrix.init 1 2 (\\i j -> \"x\")\n";
    for source in [
        "let s = Matrix.at (ref w) 0 1\ns.length",
        "let r = Matrix.row (ref w) 0\nr.length",
        "Array.length (ref (Matrix.to_array w))",
        "Matrix.rows (ref w) + Matrix.cols (ref w)",
    ] {
        emits(&format!("{words}{source}"));
    }
    for source in [
        "let n = Matrix.map (\\s -> 1) (ref w)\nMatrix.rows (ref n)",
        "Matrix.fold (\\total s -> total + 1) 0 (ref w)",
        "let t = Matrix.transpose (ref w)\nMatrix.rows (ref t)",
        "Maybe.is_some (ref (Matrix.get (ref w) 0 0))",
        "let s = Matrix.add (ref w) (ref w)\nMatrix.rows (ref s)",
        "let p = Matrix.mul (ref w) (ref w)\nMatrix.rows (ref p)",
    ] {
        let message = rejects(&format!("{words}{source}"), "E1005");
        assert!(
            message.starts_with("no instance for "),
            "{source}\n{message}"
        );
    }
    rejects(
        "let b = Matrix.init 1 1 (\\i j -> true)\nlet p = Matrix.mul (ref b) (ref b)\nMatrix.rows (ref p)",
        "E1005",
    );
}

#[test]
fn matrix_ir_keeps_multiply_and_add_separate() {
    let forbidden = ["fmuladd", "llvm.fma", " fast ", "contract", "reassoc"];
    for element in ["f32", "f64", "i64"] {
        let literal = match element {
            "f32" => "1.5f32",
            "f64" => "1.5",
            _ => "3i64",
        };
        let source = format!(
            "let a = Matrix.init 2 2 (\\i j -> {literal})\nlet p = Matrix.mul (ref a) (ref a)\nMatrix.rows (ref p)"
        );
        for ir in emits(&source) {
            let mut bodies = Vec::new();
            let mut lines = ir.lines();
            while let Some(line) = lines.next() {
                if line.starts_with("define ") && line.contains("@tz.fn.Matrix.element.$mono.") {
                    let mut body = String::new();
                    for line in lines.by_ref().take_while(|line| *line != "}") {
                        body.push_str(line);
                        body.push('\n');
                    }
                    bodies.push(body);
                }
            }
            assert_eq!(bodies.len(), 1, "{element}: {ir}");
            let body = &bodies[0];
            let (multiply, add) = if element == "i64" {
                (" mul ", " add ")
            } else {
                (" fmul ", " fadd ")
            };
            assert!(
                body.contains(multiply) && body.contains(add),
                "{element}: {body}"
            );
            for word in forbidden {
                assert!(!ir.contains(word), "{element}: {word}\n{ir}");
            }
            assert!(
                !ir.contains("@tz.fn.Matrix.transpose"),
                "{element}: unused function"
            );
        }
    }
}

#[test]
fn matrix_is_loaded_only_when_a_program_names_it() {
    let loaded = |text: &str| {
        stdlib::sources_for([text])
            .iter()
            .any(|(path, _)| *path == "std/Matrix.tz")
    };
    assert!(loaded("Matrix.rows (ref m)"));
    assert!(loaded("let m: Matrix<i64> = Matrix.init 1 1 (\\i j -> 0)"));
    assert!(!loaded("let matrix = 1\nlet Matrices = 2"));
    assert!(!loaded("42"));
    let module = analyze("let total = Array.sum [1, 2, 3]\ntotal").unwrap();
    assert!(
        module
            .functions
            .iter()
            .all(|function| function.module != "Matrix")
    );
    let module = analyze(ACCEPTED).unwrap();
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.module == "Matrix")
    );
    let ir = llvm::emit_target(
        &analyze("let total = Array.sum [1, 2, 3]\ntotal").unwrap(),
        llvm::Entry::Library,
        false,
    )
    .unwrap();
    assert!(!ir.contains("Matrix"), "{ir}");
}
