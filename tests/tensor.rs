use tsuzuri::{analyze, llvm};

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

const BASE: &str = "let t = Tensor.init [2, 3, 4] (\\i -> i)\nlet v = Tensor.view (ref t)\n";

const ACCEPTED: &str = "let t = Tensor.init [2, 3, 4] (\\i -> i)\nlet v = Tensor.view (ref t)\nlet p = Tensor.permute [2, 0, 1] (ref v)\nlet slice = Tensor.index_axis 1 2 (ref v)\nlet window = Tensor.narrow 2 1 2 (ref v)\nlet packed = Tensor.to_tensor (ref p)\nlet sum = Tensor.fold (\\acc x -> acc * 3 + x) 0 (ref slice)\nlet flat = Tensor.reshape_view [6, 4] (ref v)\nlet matrix = Tensor.as_matrix_view (ref flat)\nlet back = Tensor.of_matrix_view matrix\nlet mapped = Tensor.map (\\x -> x + 1) (ref window)\nlet mapped_view = Tensor.view (ref mapped)\nlet raw = [1, 2, 3, 4, 5, 6]\nlet borrowed = Tensor.borrow [3, 2] (ref raw)\nderef (Tensor.at [1, 2, 3] (ref v)) + deref (Tensor.at [3, 1, 2] (ref p)) + Tensor.rank (ref slice) + Tensor.count (ref window) + Maybe.get (Tensor.get [1, 0, 0] (ref v)) + (if Maybe.is_some (ref (Tensor.get [2, 0, 0] (ref v))) then 1 else 0) + deref (Array.at 5 (Tensor.as_array (ref packed))) + sum + deref (MatrixView.at 5 3 matrix) + deref (Tensor.at [4, 2] (ref back)) + Array.length (Tensor.shape (ref mapped_view)) + deref (Tensor.at [2, 1] (ref borrowed)) + (if Tensor.is_contiguous (ref p) then 1 else 0)";

#[test]
fn tensors_accept_the_documented_operations() {
    emits(ACCEPTED);
    // Owners: construction, conversions with Matrix, reshape without copying, the flat buffer.
    emits(
        "let m = Matrix.init 2 3 (\\i j -> i * 10 + j)\nlet t = Tensor.of_matrix m\nlet r = Tensor.reshape [3, 2] t\nlet length = Array.length (Tensor.as_array (ref r))\nlet back = Tensor.to_matrix r\nlet flat = Tensor.to_array (Tensor.of_array [2, 2] [1, 2, 3, 4])\nlength + Matrix.rows (ref back) + flat.length",
    );
    emits(
        "let t = Tensor.of_array [] [7]\nlet v = Tensor.view (ref t)\nderef (Tensor.at [] (ref v))",
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
            "{BASE}let p = Tensor.permute [2, 0, 1] (ref v)\nlet moved = t\nTensor.rank (ref p)"
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
        "let s = Tensor.at [1] (ref v)\ns.length",
        "let first = Tensor.index_axis 0 1 (ref v)\nTensor.rank (ref first)",
        "Tensor.count (ref v)",
    ] {
        emits(&format!("{words}{source}"));
    }
    for source in [
        "let n = Tensor.to_tensor (ref v)\nlet view = Tensor.view (ref n)\nTensor.rank (ref view)",
        "Tensor.fold (\\total s -> total + 1) 0 (ref v)",
        "Maybe.is_some (ref (Tensor.get [0] (ref v)))",
        "let n = Tensor.map (\\s -> 1) (ref v)\nlet view = Tensor.view (ref n)\nTensor.rank (ref view)",
    ] {
        let message = rejects(&format!("{words}{source}"), "E1005");
        assert!(
            message.starts_with("no instance for "),
            "{source}\n{message}"
        );
    }
}

#[test]
fn tensor_mut_is_opaque_noncopy_and_exclusive() {
    let setup =
        "let mut data = [1, 2, 3, 4]\nlet mut m = Tensor.borrow_mut [2, 2] (ref mut data)\n";
    let message = rejects(&format!("{setup}m.shape"), "E1022");
    assert!(message.contains("is opaque"), "{message}");

    let message = rejects(
        &format!("{setup}let m2 = m\nTensor.write [0, 0] 9 (ref mut m)\n0"),
        "E1012",
    );
    assert!(
        message.starts_with("use of moved or partially moved value"),
        "{message}"
    );

    // Exclusive borrow conflicts while `m` is live.
    let message = rejects(
        &format!("{setup}let n = data.length\nTensor.write [0, 0] 9 (ref mut m)\nn"),
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );

    // Sub-view exclusively reborrows `m`.
    let message = rejects(
        &format!(
            "{setup}let mut sub = Tensor.flip_mut 0 (ref mut m)\nTensor.write [0, 0] 1 (ref mut m)\nTensor.write [0, 0] 2 (ref mut sub)\n0"
        ),
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );

    // Cannot escape the backing array's lifetime.
    let message = rejects(
        "def leak {r} :: ref mut {r} [i64..] -> Tensor.Mut<i64> {r}\nfn leak outer =\n    let mut local = [1, 2]\n    Tensor.borrow_mut [2] (ref mut local)\n0",
        "E1013",
    );
    assert!(message.contains("does not live long enough"), "{message}");

    // Valid NC02 operations compile cleanly to native and WASM IR.
    emits(
        "let t0: Tensor<i64> = Tensor.zeros [2, 2]\nlet t1 = Tensor.update (\\m -> Tensor.fill 3 m) t0\nlet t2 = Tensor.ones [2, 2]\nlet v1 = Tensor.view (ref t1)\nlet _s = $\"{t1} {v1}\"\nlet _b = Tensor.broadcast_to [2, 2] (ref v1)\nif t1 == t2 then 1 else 0",
    );
}

#[test]
fn tensor_compound_assign_and_scalar_in_place_semantics_and_traps() {
    let dir = std::env::temp_dir().join(format!("tsuzuri-nc20-tensor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Tsuzuri.toml"),
        "[package]\nname = \"nc20tensor\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let main_tz = dir.join("Main.tz");
    std::fs::write(
        &main_tz,
        r#"def main :: unit -> i32 = \() ->
    // Same shape, row & col broadcast, 0-D scalar, empty tensor, negative stride, non-contiguous window.
    let mut x = Tensor.zeros [2, 3]
    let row = Tensor.of_array [3] [1.0, 2.0, 3.0]
    let col = Tensor.of_array [2, 1] [10.0, 20.0]
    x += row
    x += col
    x -= 1.0
    x *= 2.0
    x /= 2.0
    let ok_bcast = x == Tensor.of_array [2, 3] [10.0, 11.0, 12.0, 20.0, 21.0, 22.0]

    let mut s0 = Tensor.of_array [] [5.0]
    s0 += Tensor.of_array [] [2.5]
    s0 *= 2.0
    let ok_0d = s0 == Tensor.of_array [] [15.0]

    let mut emp: Tensor<f64> = Tensor.zeros [0, 3]
    emp += 9.0
    emp += Tensor.zeros [1, 3]
    let ok_emp = Array.length (Tensor.as_array (ref emp)) == 0

    let mut raw = [1, 2, 3, 4, 5, 6]
    let mut m = Tensor.borrow_mut [2, 3] (ref mut raw)
    let mut flipped = Tensor.flip_mut 1 (ref mut m)
    Tensor.add_scalar_in_place 100 (ref mut flipped)
    flipped -= Tensor.of_array [3] [1, 2, 3]
    let ok_flip = raw == [98, 100, 102, 101, 103, 105]

    let mut raw2 = [10, 20, 30, 40, 50, 60]
    let mut m2 = Tensor.borrow_mut [2, 3] (ref mut raw2)
    let mut perm = Tensor.permute_mut [1, 0] (ref mut m2)
    Tensor.mul_scalar_in_place 2 (ref mut perm)
    Tensor.div_scalar_in_place 5 (ref mut perm)
    Tensor.rem_scalar_in_place 7 (ref mut perm)
    Tensor.sub_scalar_in_place 1 (ref mut perm)
    let ok_perm = raw2 == [3, 0, 4, 1, 5, 2]

    // Bit-exact match between `x += y` and `Tensor.add_in_place` across -0.0, NaN, +/-Inf.
    let nan = 0.0 / 0.0
    let inf = 1.0 / 0.0
    let special_init = [-0.0, 0.0, nan, inf, -inf, 1.25]
    let special_rhs = Tensor.of_array [6] [-0.0, -0.0, 1.0, 2.0, 3.0, -1.25]
    let mut t_op = Tensor.of_array [6] special_init
    t_op += special_rhs
    let mut buf_ip = [-0.0, 0.0, nan, inf, -inf, 1.25]
    let mut m_ip = Tensor.borrow_mut [6] (ref mut buf_ip)
    let _ = Tensor.add_in_place (Tensor.view (ref special_rhs)) (ref mut m_ip)
    let arr_op = Tensor.as_array (ref t_op)
    let mut ok_bits = true
    for i in 0i64 ..< 6 do
        let a = arr_op[i]
        let b = buf_ip[i]
        let same = (Math.is_nan a && Math.is_nan b) || (a == b && Math.copysign 1.0 a == Math.copysign 1.0 b)
        if not same then ok_bits = false

    // Integer wrapping (`i8`).
    let mut ti8 = Tensor.of_array [2] [120i8, -120i8]
    ti8 += 20i8
    let ok_wrap = ti8 == Tensor.of_array [2] [-116i8, -100i8]

    if ok_bcast && ok_0d && ok_emp && ok_flip && ok_perm && ok_bits && ok_wrap then 0i32 else 1i32
"#,
    )
    .unwrap();
    for opt in ["-O0", "-O3"] {
        let exe = dir.join(if cfg!(windows) {
            format!("t{opt}.exe")
        } else {
            format!("t{opt}")
        });
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["build", opt, "-o"])
            .arg(&exe)
            .arg(&dir)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{opt} build stderr:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let out = std::process::Command::new(&exe).output().unwrap();
        assert!(
            out.status.success(),
            "{opt} stderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Shape-mismatch trap writes 0 elements before trapping, and integer division by zero traps.
    for (trap_name, body) in [
        (
            "shape_mismatch",
            "def main :: unit -> i32 = \\() ->\n    let mut a: Tensor<f64> = Tensor.ones [2, 3]\n    let b: Tensor<f64> = Tensor.ones [2, 4]\n    a += b\n    0i32\n",
        ),
        (
            "div_zero",
            "def main :: unit -> i32 = \\() ->\n    let mut a = Tensor.of_array [3] [10, 20, 30]\n    let b = Tensor.of_array [3] [2, 0, 5]\n    a /= b\n    0i32\n",
        ),
    ] {
        std::fs::write(&main_tz, body).unwrap();
        let exe = dir.join(if cfg!(windows) {
            format!("{trap_name}.exe")
        } else {
            trap_name.to_owned()
        });
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["build", "-O0", "-o"])
            .arg(&exe)
            .arg(&dir)
            .output()
            .unwrap();
        assert!(build.status.success(), "{trap_name} build failed");
        let out = std::process::Command::new(&exe).output().unwrap();
        assert!(!out.status.success(), "{trap_name} should trap");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
