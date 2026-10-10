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
    // The borrows a function returns must outlive the function: a borrow of its own local matrix is rejected.
    for (result, call) in [
        ("ref { r } i64", "Matrix.at (ref m) 0 0"),
        ("ref { r } [i64]", "Matrix.row (ref m) 0"),
        ("ref { r } [i64]", "Matrix.as_array (ref m)"),
    ] {
        let source = format!(
            "def leak {{ r }} :: ref {{ r }} Matrix<i64> -> {result}\nfn leak outer =\n    let m = Matrix.init 1 1 (\\i j -> 1)\n    {call}\n0"
        );
        let message = rejects(&source, "E1013");
        assert!(
            message.contains("does not live long enough"),
            "{source}\n{message}"
        );
    }
    emits(
        "def keep :: i64 -> i64\nfn keep n =\n    let m = Matrix.init 1 1 (\\i j -> n)\n    deref (Matrix.at (ref m) 0 0)\n0",
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
    let forbidden = [
        "fmuladd",
        "llvm.fma",
        "tz_soft_fma",
        "Math.fma",
        " fast ",
        "contract",
        "reassoc",
    ];
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
            let bodies = definitions(&ir, "@tz.fn.Matrix.multiply_rows.$mono.");
            assert_eq!(bodies.len(), 1, "{element}: {ir}");
            let (multiply, add) = if element == "i64" {
                (" mul ", " add ")
            } else {
                (" fmul ", " fadd ")
            };
            assert!(
                bodies[0].contains(multiply) && bodies[0].contains(add),
                "{element}: {}",
                bodies[0]
            );
            for word in forbidden {
                assert!(!ir.contains(word), "{element}: {word}\n{ir}");
            }
            // The sequential product needs neither threads nor reference counts.
            for word in [
                "tsuzuri_task_parallel",
                "atomicrmw",
                "@tz.fn.Matrix.fma_rows",
            ] {
                assert!(!ir.contains(word), "{element}: {word}\n{ir}");
            }
            assert!(
                !ir.contains("@tz.fn.Matrix.transpose"),
                "{element}: unused function"
            );
        }
    }
}

/// The bodies of the functions whose `define` line names `symbol`.
fn definitions(ir: &str, symbol: &str) -> Vec<String> {
    let mut bodies = Vec::new();
    let mut lines = ir.lines();
    while let Some(line) = lines.next() {
        if line.starts_with("define ") && line.contains(symbol) {
            let mut body = String::new();
            for line in lines.by_ref().take_while(|line| *line != "}") {
                body.push_str(line);
                body.push('\n');
            }
            bodies.push(body);
        }
    }
    bodies
}

#[test]
fn explicit_fma_and_parallel_products_are_separate_apis() {
    for element in ["f32", "f64"] {
        let literal = if element == "f32" { "1.5f32" } else { "1.5" };
        let (scalar, fma) = if element == "f32" {
            ("float", "@llvm.fma.f32")
        } else {
            ("double", "@llvm.fma.f64")
        };
        let fused = format!(
            "let a = Matrix.init 2 2 (\\i j -> {literal})\nlet p = Matrix.mul_fma (ref a) (ref a)\nMatrix.rows (ref p)"
        );
        for (index, ir) in emits(&fused).into_iter().enumerate() {
            let bodies = definitions(&ir, "@tz.fn.Matrix.fma_rows.$mono.");
            assert_eq!(bodies.len(), 1, "{element}: {ir}");
            let call = format!("call {scalar} @tz.fn.$builtin.Math.fma.");
            assert!(bodies[0].contains(&call), "{element}: {}", bodies[0]);
            assert!(
                !bodies[0].contains("fmul") && !bodies[0].contains("fadd"),
                "{element}: {}",
                bodies[0]
            );
            // `Math.fma` is the intrinsic only in native code of a compiler built for AArch64
            // (`src/llvm_math.rs`); every other host, and every wasm32 build, calls the soft routine.
            let hardware = ir.contains(fma);
            let software = ir.contains("@tz_soft_fma");
            if index == 0 {
                assert!(
                    hardware != software,
                    "{element}: {fma} xor @tz_soft_fma\n{ir}"
                );
            } else {
                assert!(
                    software && !hardware,
                    "{element}: wasm32 uses @tz_soft_fma\n{ir}"
                );
            }
            for word in [
                "fmuladd",
                " fast ",
                "contract",
                "reassoc",
                "@tz.fn.Matrix.multiply_rows",
            ] {
                assert!(!ir.contains(word), "{element}: {word}\n{ir}");
            }
        }
        let parallel = format!(
            "let a = Matrix.init 2 2 (\\i j -> {literal})\nlet p = Matrix.mul_parallel (ref a) (ref a)\nlet q = Matrix.mul_fma_parallel (ref a) (ref a)\nMatrix.rows (ref p) + Matrix.rows (ref q)"
        );
        for ir in emits(&parallel) {
            assert!(ir.contains("@tsuzuri_task_parallel"), "{element}: {ir}");
            for word in ["fmuladd", " fast ", "contract", "reassoc"] {
                assert!(!ir.contains(word), "{element}: {word}\n{ir}");
            }
        }
    }
    // The fused and the parallel products need a floating-point and a sendable element type.
    for source in [
        "let a = Matrix.init 1 1 (\\i j -> 1)\nlet p = Matrix.mul_fma (ref a) (ref a)\nMatrix.rows (ref p)",
        "let a = Matrix.init 1 1 (\\i j -> true)\nlet p = Matrix.mul_parallel (ref a) (ref a)\nMatrix.rows (ref p)",
        "let a = Matrix.init 1 1 (\\i j -> \"x\")\nlet p = Matrix.mul_parallel (ref a) (ref a)\nMatrix.rows (ref p)",
    ] {
        let message = rejects(source, "E1005");
        assert!(
            message.starts_with("no instance for "),
            "{source}\n{message}"
        );
    }
    // Integers have no fused multiply-add, but the parallel product works for them.
    emits(
        "let a = Matrix.init 2 2 (\\i j -> 3i64)\nlet p = Matrix.mul_parallel (ref a) (ref a)\nMatrix.rows (ref p)",
    );
}

#[test]
fn matrices_move_through_gpu_cpu_reference_buffers() {
    // The public composition documented in matrix.md: flat arrays in and out of a Gpu buffer. There is
    // no matrix-product kernel; Gpu.map is element-wise.
    emits(
        "let device = Result.get (Gpu.request Gpu.CpuReference)\nlet m = Matrix.init 2 3 (\\row col -> row * 10 + col)\nlet buffer = Gpu.from_array (ref device) (Matrix.as_array (ref m))\nlet doubled = Gpu.map (ref device) (\\x -> x * 2) buffer\nlet result = Matrix.of_array (Matrix.rows (ref m)) (Matrix.cols (ref m)) (Gpu.to_array doubled)\nderef (Matrix.at (ref result) 1 2)",
    );
}

#[test]
fn set_replaces_an_element_of_a_consumed_matrix() {
    emits(
        "let m = Matrix.init 2 2 (\\i j -> i + j)\nlet m = Matrix.set m 1 1 9\nderef (Matrix.at (ref m) 1 1)",
    );
    rejects(
        "let m = Matrix.init 2 2 (\\i j -> i + j)\nlet n = Matrix.set m 0 0 1\nMatrix.rows (ref m)",
        "E1012",
    );
    let message = rejects(
        "let m = Matrix.init 2 2 (\\i j -> i + j)\nlet r = Matrix.row (ref m) 0\nlet n = Matrix.set m 0 0 1\nr[0]",
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );
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
