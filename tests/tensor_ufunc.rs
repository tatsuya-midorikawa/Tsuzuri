use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::driver::Project;

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/tensor-ufunc")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn run_tz_test(source: &str) {
    let root = scratch_dir("test");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"tensor-ufunc\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("test")
        .arg(&root)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        out.status.success(),
        "tsuzuri test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn broadcast_shapes_strides_scalars_empties_and_ufuncs() {
    run_tz_test(
        r#"using std

record Mat2 { a: i64, b: i64, c: i64, d: i64 }
instance Mul<Mat2> {
    fn mul l r = Mat2 {
        a: l.a * r.a + l.b * r.c,
        b: l.a * r.b + l.b * r.d,
        c: l.c * r.a + l.d * r.c,
        d: l.c * r.b + l.d * r.d
    }
}
instance Eq<Mat2> {
    fn eq l r = l.a == r.a && l.b == r.b && l.c == r.c && l.d == r.d
    fn ne l r = !(l == r)
}

test "broadcast_1d_to_4d_scalar_empty_and_negative_strides" =
    // [10, 100] + [10, 1] broadcast
    let big = Tensor.init [10, 100] (\i -> i)
    let col = Tensor.init [10, 1] (\i -> (i + 1) * 1000)
    let sum_big = big + col
    assert (sum_big[(0, 0)] == 1000 && sum_big[(9, 99)] == 999 + 10000)

    // 0D scalar + 2D strided (transposed & flipped)
    let base = Tensor.of_array [2, 3] [1, 2, 3, 4, 5, 6]
    let bv = Tensor.view base
    let tr = Tensor.transpose bv
    let fl = Tensor.flip tr
    let sc = Tensor.scalar 100
    let sc_v = Tensor.view sc
    match Tensor.add sc_v fl with
    | Result.Ok res ->
        assert (res[(0, 0)] == 106 && res[(2, 1)] == 101)
    | Result.Error _ -> assert false

    // 4D broadcast: [2, 1, 3, 1] - [1, 2, 1, 2]
    let t4a = Tensor.init [2, 1, 3, 1] (\i -> (i + 1) * 10)
    let t4b = Tensor.init [1, 2, 1, 2] (\i -> i + 1)
    let diff4 = t4a - t4b
    let d4v = Tensor.view diff4
    assert (Tensor.shape d4v == [2, 2, 3, 2])
    assert (diff4[(1, 1, 2, 1)] == 60 - 4)

    // Empty tensors: compatible shapes succeed, incompatible shapes return Error
    let e1: Tensor<i64> = Tensor.zeros [0, 3]
    let e2: Tensor<i64> = Tensor.zeros [1, 3]
    let e_sum = e1 + e2
    let e_sum_v = Tensor.view e_sum
    assert (Tensor.shape e_sum_v == [0, 3] && (Tensor.as_array e_sum).length == 0)
    let e_bad: Tensor<i64> = Tensor.zeros [0, 4]
    match Tensor.add (Tensor.view e1) (Tensor.view e_bad) with
    | Result.Error (Tensor.Error.IncompatibleBroadcast _) -> ()
    | _ -> assert false

test "noncommutative_order_floor_and_matrix_ops" =
    let l = Tensor.of_array [2] [20, -7]
    let r = Tensor.of_array [2] [3, 2]
    let lv = Tensor.view l
    let rv = Tensor.view r
    match Tensor.sub lv rv with
    | Result.Ok d -> assert (d == l - r && d == Tensor.of_array [2] [17, -9])
    | Result.Error _ -> assert false
    match Tensor.div lv rv with
    | Result.Ok q -> assert (q == l / r && q == Tensor.of_array [2] [6, -3])
    | Result.Error _ -> assert false
    match Tensor.rem lv rv with
    | Result.Ok m -> assert (m == l % r && m == Tensor.of_array [2] [2, -1])
    | Result.Error _ -> assert false
    match Tensor.divmod lv rv with
    | Result.Ok (fq, fr) -> assert (fq == Tensor.of_array [2] [6, -4] && fr == Tensor.of_array [2] [2, 1])
    | Result.Error _ -> assert false

    let pbase = Tensor.of_array [2] [2, 3]
    let pexp = Tensor.of_array [2] [5, 2]
    let pexp_v = Tensor.view pexp
    assert (pbase ** pexp == Tensor.of_array [2] [32, 9])
    assert (Tensor.sub_scalar 3 pexp_v == Tensor.of_array [2] [2, -1])
    assert (Tensor.rsub_scalar 3 pexp_v == Tensor.of_array [2] [-2, 1])
    assert (Tensor.rpow_scalar 2 pexp_v == Tensor.of_array [2] [32, 4])
    assert (Tensor.rrem_scalar 11 pexp_v == Tensor.of_array [2] [1, 1])

    // Non-commutative Mul<Mat2>: mul_scalar vs scale
    let m_s = Mat2 { a: 1, b: 2, c: 0, d: 1 }
    let m_x = Mat2 { a: 1, b: 0, c: 3, d: 1 }
    let mt = Tensor.of_array [1] [m_x]
    let mv = Tensor.view mt
    let right_mul = Tensor.mul_scalar m_s mv
    let left_mul = Tensor.scale m_s mv
    assert ((Tensor.as_array right_mul)[0] == m_x * m_s)
    assert ((Tensor.as_array left_mul)[0] == m_s * m_x)
    assert (!((Tensor.as_array right_mul)[0] == (Tensor.as_array left_mul)[0]))

    // Matrix.sub, Matrix.hadamard, Matrix.scale
    let ma = Matrix.of_array 2 2 [10, 20, 30, 40]
    let mb = Matrix.of_array 2 2 [1, 2, 3, 4]
    assert (Matrix.to_array (Matrix.sub ma mb) == [9, 18, 27, 36])
    assert (Matrix.to_array (Matrix.hadamard ma mb) == [10, 40, 90, 160])
    assert (Matrix.to_array (Matrix.scale 2 mb) == [2, 4, 6, 8])

test "float_special_values_isclose_allclose_and_complex" =
    let nan = 0.0 / 0.0
    let pinf = 1.0 / 0.0
    let ninf = -1.0 / 0.0
    let a = Tensor.of_array [4] [1.0, nan, pinf, -0.0]
    let b = Tensor.of_array [4] [1.0 + 1e-9, nan, pinf, 0.0]
    let av = Tensor.view a
    let bv = Tensor.view b
    assert (!(Tensor.allclose 1e-5 1e-8 false av bv))
    assert (Tensor.allclose 1e-5 1e-8 true av bv)
    let b_alt = Tensor.of_array [4] [1.0, nan, ninf, 0.0]
    match Tensor.isclose 1e-5 1e-8 false av (Tensor.view b_alt) with
    | Result.Ok m -> assert (Tensor.to_array m == [true, false, false, true])
    | Result.Error _ -> assert false

    let raw_sp = Tensor.of_array [3] [nan, pinf, ninf]
    let replaced = Tensor.nan_to_num 0.0 100.0 (-100.0) (Tensor.view raw_sp)
    assert (replaced == Tensor.of_array [3] [0.0, 100.0, -100.0])

    // Signed zero & NaN propagation in maximum/minimum vs fmax/fmin
    let z_neg = Tensor.of_array [2] [-0.0, nan]
    let z_pos = Tensor.of_array [2] [0.0, 5.0]
    let z_neg_v = Tensor.view z_neg
    let z_pos_v = Tensor.view z_pos
    match Tensor.maximum z_neg_v z_pos_v with
    | Result.Ok mx ->
        let arr = Tensor.as_array mx
        assert (!(Math.signbit arr[0]) && Math.is_nan arr[1])
    | Result.Error _ -> assert false
    match Tensor.minimum z_pos_v z_neg_v with
    | Result.Ok mn ->
        let arr = Tensor.as_array mn
        assert (Math.signbit arr[0] && Math.is_nan arr[1])
    | Result.Error _ -> assert false
    match Tensor.fmax z_neg_v z_pos_v with
    | Result.Ok fmx -> assert (fmx == Tensor.of_array [2] [0.0, 5.0])
    | Result.Error _ -> assert false

    // Complex ufuncs & Complex Tensor operators
    let cz = Tensor.of_array [2] [Complex.of 3.0 4.0, Complex.of 0.0 (-2.0)]
    let cz2 = cz + cz
    let cz2_v = Tensor.view cz2
    let cz_conj = Tensor.conj (Tensor.view cz)
    let cz_conj_v = Tensor.view cz_conj
    assert (Tensor.real cz2_v == Tensor.of_array [2] [6.0, 0.0])
    assert (Tensor.imag cz_conj_v == Tensor.of_array [2] [-4.0, 2.0])
"#,
    );
}

#[test]
fn into_and_in_place_perform_zero_heap_allocations_and_reject_bad_shapes() {
    run_tz_test(
        r#"using std

test "zero_allocations_on_into_and_in_place" =
    let a = Tensor.of_array [2, 3] [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
    let b = Tensor.of_array [2, 3] [10.0, 20.0, 30.0, 40.0, 50.0, 60.0]
    let row = Tensor.of_array [1, 3] [1.0, 2.0, 3.0]
    let av = Tensor.view a
    let bv = Tensor.view b
    let rv = Tensor.view row
    let tr_av = Tensor.transpose av
    let mut out_buf = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    let mut dst = Tensor.borrow_mut [2, 3] (ref mut out_buf)
    let mut tr_buf = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    let mut tr_dst = Tensor.borrow_mut [3, 2] (ref mut tr_buf)

    // Pre-warm views and mut targets before snapshot as noted in NC06 test plan.
    let _ = Tensor.count av + Tensor.count bv + Tensor.count rv + Tensor.count tr_av
    let _ = Tensor.is_contiguous_mut (ref dst) && Tensor.is_contiguous_mut (ref tr_dst)

    let snap = Alloc.snapshot ()
    let r1 = Tensor.add_into av bv (ref mut dst)
    let r2 = Tensor.mul_add_into av bv rv (ref mut dst)
    let r3 = Tensor.fma_into av bv rv (ref mut dst)
    let r4 = Tensor.scale_into 2.0 av (ref mut dst)
    let r5 = Tensor.map_into (\x -> Math.exp (x - 1.0)) tr_av (ref mut tr_dst)
    let r6 = Tensor.add_in_place rv (ref mut dst)
    let r7 = Tensor.sub_in_place av (ref mut dst)
    let delta = Alloc.since snap
    Test.equal (ref 0) (ref delta.allocs)
    Test.equal (ref 0) (ref delta.bytes)
    match (r1, r2, r3, r4, r5, r6, r7) with
    | (Result.Ok (), Result.Ok (), Result.Ok (), Result.Ok (), Result.Ok (), Result.Ok (), Result.Ok ()) -> ()
    | _ -> Test.equal (ref "ok") (ref "r1..r7")

    // Incompatible shapes return Result.Error without modifying target buffer.
    let mut small_buf = [7.0, 8.0, 9.0]
    let mut small_dst = Tensor.borrow_mut [3] (ref mut small_buf)
    match Tensor.add_in_place av (ref mut small_dst) with
    | Result.Error _ -> ()
    | Result.Ok () -> Test.equal (ref "err") (ref "add_in_place")
    Test.equal (ref [7.0, 8.0, 9.0]) (ref small_buf)

    match Tensor.add_into rv rv (ref mut dst) with
    | Result.Error _ -> ()
    | Result.Ok () -> Test.equal (ref "err") (ref "add_into")
    Test.equal (ref [2.0, 4.0, 6.0, 5.0, 7.0, 9.0]) (ref out_buf)

    // Operator vs Tensor.add allocation count & bit equality
    let s_op = Alloc.snapshot ()
    let c_op = a + b
    let d_op = Alloc.since s_op
    let s_fn = Alloc.snapshot ()
    let va2 = Tensor.view a
    let vb2 = Tensor.view b
    let c_fn = match Tensor.add va2 vb2 with
        | Result.Ok t -> t
        | Result.Error _ -> Tensor.zeros [0]
    let d_fn = Alloc.since s_fn
    Test.is_true (c_op == c_fn)
    Test.equal (ref d_fn.allocs) (ref d_op.allocs)
"#,
    );
}

#[test]
fn user_tensor_operator_instance_is_rejected_as_orphan_not_overlap() {
    let root = scratch_dir("orphan");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "using std\n\
         instance Add<Tensor<f64>> {\n\
         \x20   fn add l _ = Tensor.clone l\n\
         }\n\
         def main :: unit -> i32 = \\() -> 0i32\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"orphan-check\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let err = Project::load(&root).unwrap().analyze().unwrap_err();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(err.code, "E1016");
    assert_eq!(err.kind, Some("orphan_instance"), "{err:?}");
}

#[test]
fn std_tensor_operator_instances_share_or_trap_kernels_and_use_half_open_slices() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("std/Tensor.tz"),
    )
    .unwrap();
    let nc06_start = src
        .find("// Broadcasting Element-wise Engines, Ufuncs, Comparisons & Operators (NC06)")
        .expect("NC06 section marker");
    let nc06 = &src[nc06_start..];

    for (class, method, helper) in [
        (
            "Add",
            "add",
            "add_or_trap (ref (view left)) (ref (view right))",
        ),
        (
            "Sub",
            "sub",
            "sub_or_trap (ref (view left)) (ref (view right))",
        ),
        (
            "Mul",
            "mul",
            "mul_or_trap (ref (view left)) (ref (view right))",
        ),
        (
            "Div",
            "div",
            "div_or_trap (ref (view left)) (ref (view right))",
        ),
        (
            "Rem",
            "rem",
            "rem_or_trap (ref (view left)) (ref (view right))",
        ),
        (
            "Pow",
            "pow",
            "pow_or_trap (ref (view left)) (ref (view right))",
        ),
        ("Neg", "neg", "Tensor.neg (ref (view value))"),
    ] {
        let needle = if class == "Neg" {
            format!(
                "instance ({class}<'a>, Copy<'a>) => {class}<Tensor<'a>> {{\n    fn rec {method} value = {helper}\n}}"
            )
        } else {
            format!(
                "instance ({class}<'a>, Copy<'a>) => {class}<Tensor<'a>> {{\n    fn rec {method} left right = {helper}\n}}"
            )
        };
        assert!(
            nc06.contains(&needle),
            "missing single-call operator body:\n{needle}"
        );
    }

    // Public functions and `*_or_trap` twins share `map2_kernel` and `map2_in_place_kernel`
    // (1 `fn` definition + 4 calls = 5; 1 `fn` definition + 2 calls = 3).
    assert_eq!(nc06.matches("map2_kernel f ").count(), 5);
    assert_eq!(nc06.matches("map2_in_place_kernel f ").count(), 3);

    // No bare `..` slice in the NC06 section (only `..<` or `..=`).
    for (idx, line) in nc06.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        assert!(
            !trimmed.contains(" .. "),
            "bare `..` found in NC06 section line {idx}: {line}"
        );
    }
}

#[test]
fn clang_o3_inlines_closures_and_autovectorizes_contiguous_fast_path() {
    let root = scratch_dir("clang-o3");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Main.tz"),
        r#"using std

export def bench_add_into :: [f64] -> [f64] -> [f64]
fn bench_add_into a b =
    let n = a.length
    let va = Tensor.borrow [n] a
    let vb = Tensor.borrow [n] b
    let mut out = new [f64](n, \_ -> 0.0)
    let mut dst = Tensor.borrow_mut [n] (ref mut out)
    let _ = Tensor.mul_add_into (ref va) (ref vb) (ref va) (ref mut dst)
    let _ = Tensor.map_into (\x -> Math.exp (x - 1.0)) (ref va) (ref mut dst)
    out

def main :: unit -> i32 = \() -> 0i32
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"clang-o3\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let raw_ll = root.join("raw.ll");
    let opt_ll = root.join("opt.ll");
    let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args([
            "build",
            root.to_str().unwrap(),
            "--emit",
            "llvm",
            "-O3",
            "-o",
            raw_ll.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let clang_check = Command::new(&clang).arg("--version").output();
    if clang_check.is_ok_and(|o| o.status.success()) {
        let opt = Command::new(&clang)
            .args([
                "-x",
                "ir",
                "-O3",
                "-S",
                "-emit-llvm",
                "-Wno-override-module",
                raw_ll.to_str().unwrap(),
                "-o",
                opt_ll.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            opt.status.success(),
            "{}",
            String::from_utf8_lossy(&opt.stderr)
        );
        let ir = std::fs::read_to_string(&opt_ll).unwrap();
        assert!(
            !ir.contains("llvm.fma"),
            "unexpected implicit FMA in mul_add_into"
        );
        assert!(
            !ir.contains(" reassoc ") && !ir.contains(" fast "),
            "unexpected fast-math flags"
        );
        // Verify vector operations (`<2 x double>` or `<4 x double>`) are generated by LLVM -O3
        assert!(
            ir.contains("x double>"),
            "expected Clang -O3 to vectorize f64 contiguous fast path"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
