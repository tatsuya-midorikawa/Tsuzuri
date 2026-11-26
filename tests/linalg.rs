use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/linalg")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn run_tz_test(source: &str) {
    let root = scratch_dir("suite");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"linalg-suite\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
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
fn phase1_matmul_tensordot_einsum_and_complex() {
    run_tz_test(
        r#"using std

test "phase1_contractions_and_einsum" =
    // 1D x 1D, 1D x 2D, 2D x 1D, 2D x 2D
    let v1 = Tensor.of_array [3] [1, 2, 3]
    let v2 = Tensor.of_array [3] [4, 5, 6]
    let m23 = Tensor.of_array [2, 3] [1, 2, 3, 4, 5, 6]
    let m32 = Tensor.of_array [3, 2] [7, 8, 9, 10, 11, 12]
    let vv1 = Tensor.view (ref v1)
    let vv2 = Tensor.view (ref v2)
    let vm23 = Tensor.view (ref m23)
    let vm32 = Tensor.view (ref m32)
    match Linalg.matmul (ref vv1) (ref vv2) with
    | Result.Ok d -> assert (Tensor.to_array d == [32])
    | Result.Error _ -> assert false
    match Linalg.matmul (ref vm23) (ref vv1) with
    | Result.Ok mv -> assert (Tensor.to_array mv == [14, 32])
    | Result.Error _ -> assert false
    match Linalg.matmul (ref vv1) (ref vm32) with
    | Result.Ok vm -> assert (Tensor.to_array vm == [58, 64])
    | Result.Error _ -> assert false

    // Shared-B row-folding fast path: rank-2 B AND rank-3 B[1, k, n] (stride == 0) vs non-contiguous A fallback, plus k == 0
    let a_batch = Tensor.init [2, 2, 3] (\i -> (i + 1) as f64)
    let b_2d = Tensor.of_array [3, 2] [1.0, 2.0, 0.5, -1.0, 3.0, 0.25]
    let b_3d_bcast = Tensor.reshape [1, 3, 2] (Tensor.clone (ref b_2d))
    let vab = Tensor.view (ref a_batch)
    let vb2 = Tensor.view (ref b_2d)
    let vb3 = Tensor.view (ref b_3d_bcast)
    // Create non-contiguous A with identical logical values via permute -> clone -> permute
    let p021 = Tensor.permute [0, 2, 1] (ref vab)
    let a_perm_src = Tensor.to_tensor (ref p021)
    let vaps = Tensor.view (ref a_perm_src)
    let va_noncontig = Tensor.permute [0, 2, 1] (ref vaps)
    match (Linalg.matmul (ref vab) (ref vb2), Linalg.matmul (ref vab) (ref vb3), Linalg.matmul (ref va_noncontig) (ref vb2)) with
    | (Result.Ok r_fast2, Result.Ok r_fast3, Result.Ok r_slow) ->
        assert (r_fast2 == r_fast3)
        assert (r_fast2 == r_slow)
    | _ -> assert false

    let ak0: Tensor<f64> = Tensor.of_array [2, 2, 0] []
    let bk0: Tensor<f64> = Tensor.of_array [0, 3] []
    let vak0 = Tensor.view (ref ak0)
    let vbk0 = Tensor.view (ref bk0)
    match Linalg.matmul (ref vak0) (ref vbk0) with
    | Result.Ok rk0 ->
        let vrk0 = Tensor.view (ref rk0)
        assert (Tensor.shape (ref vrk0) == [2, 2, 3])
        assert (Tensor.as_array (ref rk0) == new [f64](12, \_ -> 0.0))
    | Result.Error _ -> assert false

    // Complex vdot conjugates first argument: conj(1+2i)*(3+4i) = (1-2i)*(3+4i) = 11 - 2i
    let ca = Tensor.of_array [1] [Complex.of 1.0 2.0]
    let cb = Tensor.of_array [1] [Complex.of 3.0 4.0]
    let vca = Tensor.view (ref ca)
    let vcb = Tensor.view (ref cb)
    match Linalg.vdot_complex (ref vca) (ref vcb) with
    | Result.Ok z -> assert (z.re == 11.0 && z.im == (-2.0))
    | Result.Error _ -> assert false

    // Attention einsum "bhqd,bhkd->bhqk", implicit mode, trace, diagonal, ellipsis, 3-way chain & path
    let q_att = Tensor.init [2, 2, 3, 4] (\i -> ((i % 7) + 1) as f64)
    let k_att = Tensor.init [2, 2, 5, 4] (\i -> ((i % 5) + 1) as f64)
    let vq_att = Tensor.view (ref q_att)
    let vk_att = Tensor.view (ref k_att)
    let vq_att2 = Tensor.view (ref q_att)
    let vk_att2 = Tensor.view (ref k_att)
    let kt_view = Tensor.swapaxes (-1) (-2) (ref vk_att2)
    match (Linalg.einsum "bhqd,bhkd->bhqk" [vq_att, vk_att], Linalg.matmul (ref vq_att2) (ref kt_view)) with
    | (Result.Ok att_ein, Result.Ok att_mm) ->
        let v1a = Tensor.view (ref att_ein)
        let v2a = Tensor.view (ref att_mm)
        assert (Tensor.allclose 1e-12 1e-12 false (ref v1a) (ref v2a))
    | _ -> assert false

    // 3-operand chain where (A*B)*C vs A*(B*C) differs in intermediate size: [2, 10] x [10, 10] x [10, 1] -> picks (1, 2) first!
    match Linalg.einsum_path "ij,jk,kl->il" [[2, 10], [10, 10], [10, 1]] with
    | Result.Ok path -> assert (path == [(1, 2), (0, 1)])
    | Result.Error _ -> assert false

    let sq = Tensor.of_array [3, 3] [1, 2, 3, 4, 5, 6, 7, 8, 9]
    let vsq = Tensor.view (ref sq)
    match Linalg.einsum "ii->" [vsq] with
    | Result.Ok tr -> assert (Tensor.to_array tr == [15])
    | Result.Error _ -> assert false

    let vsq2 = Tensor.view (ref sq)
    match Linalg.einsum "ij,jk->ik" [vsq2] with
    | Result.Error (Linalg.Error.InvalidEinsum _) -> ()
    | _ -> assert false

    Test.is_true true

def main :: unit -> i32 = \() -> 0i32
"#,
    );
}

#[test]
fn phase2_decompositions_solvers_inv_det_power_and_norms() {
    run_tz_test(
        r#"using std

test "phase2_lu_cholesky_qr_solve_lstsq_norms" =
    // Rectangular & square LU: P * L * U == A
    let rect = Tensor.of_array [3, 2] [2.0, 1.0, 6.0, 4.0, -2.0, 5.0]
    let vrect = Tensor.view (ref rect)
    match Linalg.lu (ref vrect) with
    | Result.Ok (p, l, u, _) ->
        let vp = Tensor.view (ref p)
        let vl = Tensor.view (ref l)
        let vu = Tensor.view (ref u)
        match Linalg.matmul (ref vl) (ref vu) with
        | Result.Ok lu_p ->
            let vlu = Tensor.view (ref lu_p)
            match Linalg.matmul (ref vp) (ref vlu) with
            | Result.Ok rec_a ->
                let vra = Tensor.view (ref rec_a)
                assert (Tensor.allclose 1e-12 1e-12 false (ref vra) (ref vrect))
            | Result.Error _ -> assert false
        | Result.Error _ -> assert false
    | Result.Error _ -> assert false

    // Cholesky on SPD & NotPositiveDefinite on indefinite matrix
    let spd = Tensor.of_array [3, 3] [4.0, 12.0, -16.0, 12.0, 37.0, -43.0, -16.0, -43.0, 98.0]
    let vspd = Tensor.view (ref spd)
    match Linalg.cholesky (ref vspd) with
    | Result.Ok chol_l ->
        assert (Tensor.as_array (ref chol_l) == [2.0, 0.0, 0.0, 6.0, 1.0, 0.0, -8.0, 5.0, 3.0])
        let rhs2 = Tensor.of_array [3, 2] [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
        let vrhs2 = Tensor.view (ref rhs2)
        let vcl = Tensor.view (ref chol_l)
        match (Linalg.cholesky_solve (ref vcl) (ref vrhs2), Linalg.solve (ref vspd) (ref vrhs2)) with
        | (Result.Ok x_chol, Result.Ok x_lu) ->
            let vxc = Tensor.view (ref x_chol)
            let vxl = Tensor.view (ref x_lu)
            assert (Tensor.allclose 1e-11 1e-11 false (ref vxc) (ref vxl))
        | _ -> assert false
    | Result.Error _ -> assert false

    let indef = Tensor.of_array [2, 2] [1.0, 2.0, 2.0, 1.0]
    let vindef = Tensor.view (ref indef)
    match Linalg.cholesky (ref vindef) with
    | Result.Error Linalg.Error.NotPositiveDefinite -> ()
    | _ -> assert false

    // QR Complete & Reduced: Q^T * Q == I, Q * R == A
    match Linalg.qr Linalg.QrMode.Complete (ref vrect) with
    | Result.Ok (q, r) ->
        let vq = Tensor.view (ref q)
        let vr = Tensor.view (ref r)
        let vqt = Tensor.transpose (ref vq)
        let id3 = Tensor.identity 3
        let vid3 = Tensor.view (ref id3)
        match (Linalg.matmul (ref vqt) (ref vq), Linalg.matmul (ref vq) (ref vr)) with
        | (Result.Ok qtq, Result.Ok qr_p) ->
            let vqtq = Tensor.view (ref qtq)
            let vqrp = Tensor.view (ref qr_p)
            assert (Tensor.allclose 1e-12 1e-12 false (ref vqtq) (ref vid3))
            assert (Tensor.allclose 1e-12 1e-12 false (ref vqrp) (ref vrect))
        | _ -> assert false
    | Result.Error _ -> assert false

    // 4x4 Hilbert matrix solve residual & rank-deficient 3x3 SingularMatrix detection
    let hilb = Tensor.init [4, 4] (\idx -> 1.0 / (((idx / 4) + (idx % 4) + 1) as f64))
    let b_hilb = Tensor.of_array [4] [1.0, 2.0, 3.0, 4.0]
    let vhilb = Tensor.view (ref hilb)
    let vbh = Tensor.view (ref b_hilb)
    match Linalg.solve (ref vhilb) (ref vbh) with
    | Result.Ok xh ->
        let vxh = Tensor.view (ref xh)
        match Linalg.matmul (ref vhilb) (ref vxh) with
        | Result.Ok ax ->
            let vax = Tensor.view (ref ax)
            assert (Tensor.allclose 1e-9 1e-9 false (ref vax) (ref vbh))
        | Result.Error _ -> assert false
    | Result.Error _ -> assert false

    let sing3 = Tensor.of_array [3, 3] [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
    let vsing3 = Tensor.view (ref sing3)
    let b3 = Tensor.of_array [3] [1.0, 1.0, 1.0]
    let vb3 = Tensor.view (ref b3)
    match Linalg.solve (ref vsing3) (ref vb3) with
    | Result.Error Linalg.Error.SingularMatrix -> ()
    | _ -> assert false

    // inv, det, slogdet, matrix_power (-2), lstsq (rank-deficient)
    let m2 = Tensor.of_array [2, 2] [1.0, 2.0, 3.0, 4.0]
    let vm2 = Tensor.view (ref m2)
    match Linalg.det (ref vm2) with
    | Result.Ok dt -> assert (Math.abs ((Tensor.as_array (ref dt))[0] - (-2.0)) < 1e-12)
    | Result.Error _ -> assert false
    match Linalg.slogdet (ref vm2) with
    | Result.Ok (sgn, lad) ->
        assert ((Tensor.as_array (ref sgn))[0] == (-1.0))
        assert (Math.abs (Math.exp (Tensor.as_array (ref lad))[0] - 2.0) < 1e-12)
    | Result.Error _ -> assert false
    match (Linalg.matrix_power (-2) (ref vm2), Linalg.matrix_power 2 (ref vm2)) with
    | (Result.Ok p_neg2, Result.Ok p_pos2) ->
        let vpn = Tensor.view (ref p_neg2)
        let vpp = Tensor.view (ref p_pos2)
        let id2 = Tensor.identity 2
        let vid2 = Tensor.view (ref id2)
        match Linalg.matmul (ref vpn) (ref vpp) with
        | Result.Ok prod ->
            let vpr = Tensor.view (ref prod)
            assert (Tensor.allclose 1e-11 1e-11 false (ref vpr) (ref vid2))
        | Result.Error _ -> assert false
    | _ -> assert false

    match Linalg.lstsq (ref vsing3) (ref vb3) with
    | Result.Ok ls ->
        assert ((Tensor.as_array (ref ls.rank))[0] == 2)
    | Result.Error _ -> assert false

    Test.is_true true

def main :: unit -> i32 = \() -> 0i32
"#,
    );
}

#[test]
fn phase3_svd_pinv_rank_eigh_and_eig_identities() {
    run_tz_test(
        r#"using std

test "phase3_svd_eigh_eig_identities" =
    // Wide 2x4 SVD (`m < n`) for both `full_matrices: false` and `full_matrices: true`
    let wide = Tensor.of_array [2, 4] [1.0, 2.0, 3.0, 4.0, 4.0, 3.0, 2.0, 1.0]
    let vwide = Tensor.view (ref wide)
    match Linalg.svd (Linalg.SvdOptions { full_matrices: false }) (ref vwide) with
    | Result.Ok (u, s, vt) ->
        let vu = Tensor.view (ref u)
        let vs = Tensor.view (ref s)
        let vvt = Tensor.view (ref vt)
        let sd = Tensor.diag 0 (ref vs)
        let vsd = Tensor.view (ref sd)
        match Linalg.matmul (ref vu) (ref vsd) with
        | Result.Ok us ->
            let vus = Tensor.view (ref us)
            match Linalg.matmul (ref vus) (ref vvt) with
            | Result.Ok rec_w ->
                let vrw = Tensor.view (ref rec_w)
                Test.equal (ref "svd_reduced") (ref (if Tensor.allclose 1e-11 1e-11 false (ref vrw) (ref vwide) then "svd_reduced" else "fail"))
            | Result.Error _ -> Test.equal (ref "ok") (ref "svd_reduced_mm2")
        | Result.Error _ -> Test.equal (ref "ok") (ref "svd_reduced_mm1")
    | Result.Error _ -> Test.equal (ref "ok") (ref "svd_reduced_err")

    match Linalg.svd (Linalg.SvdOptions { full_matrices: true }) (ref vwide) with
    | Result.Ok (uf, _, vtf) ->
        let vuf = Tensor.view (ref uf)
        let vvtf = Tensor.view (ref vtf)
        let vvtf_t = Tensor.transpose (ref vvtf)
        let id4 = Tensor.identity 4
        let vid4 = Tensor.view (ref id4)
        Test.equal (ref [2, 2]) (Tensor.shape (ref vuf))
        Test.equal (ref [4, 4]) (Tensor.shape (ref vvtf))
        match Linalg.matmul (ref vvtf) (ref vvtf_t) with
        | Result.Ok orth4 ->
            let vo4 = Tensor.view (ref orth4)
            Test.equal (ref "svd_full_orth") (ref (if Tensor.allclose 1e-11 1e-11 false (ref vo4) (ref vid4) then "svd_full_orth" else "fail"))
        | Result.Error _ -> Test.equal (ref "ok") (ref "svd_full_mm")
    | Result.Error _ -> Test.equal (ref "ok") (ref "svd_full_err")

test "phase3_pinv" =
    // Moore-Penrose pseudoinverse on rank-deficient 3x3 matrix: A * A^+ * A == A
    let rd = Tensor.of_array [3, 3] [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
    let vrd = Tensor.view (ref rd)
    match Linalg.pinv 1e-12 (ref vrd) with
    | Result.Ok p_rd ->
        let vp = Tensor.view (ref p_rd)
        match Linalg.matmul (ref vrd) (ref vp) with
        | Result.Ok ap ->
            let vap = Tensor.view (ref ap)
            match Linalg.matmul (ref vap) (ref vrd) with
            | Result.Ok apa ->
                let vapa = Tensor.view (ref apa)
                Test.equal (ref "pinv") (ref (if Tensor.allclose 1e-10 1e-10 false (ref vapa) (ref vrd) then "pinv" else "fail"))
            | Result.Error _ -> Test.equal (ref "ok") (ref "pinv_mm2")
        | Result.Error _ -> Test.equal (ref "ok") (ref "pinv_mm1")
    | Result.Error _ -> Test.equal (ref "ok") (ref "pinv_err")

test "phase3_eigh" =
    // Real symmetric `eigh`: V * diag(w) * V^T == A, V^T * V == I, w ascending
    let sym = Tensor.of_array [3, 3] [4.0, 1.0, -2.0, 1.0, 3.0, 0.5, -2.0, 0.5, 5.0]
    let vsym = Tensor.view (ref sym)
    match Linalg.eigh (ref vsym) with
    | Result.Ok (w, v) ->
        let wa = Tensor.as_array (ref w)
        Test.is_true (wa[0] <= wa[1] && wa[1] <= wa[2])
        let vv = Tensor.view (ref v)
        let vvt = Tensor.transpose (ref vv)
        let vw = Tensor.view (ref w)
        let wd = Tensor.diag 0 (ref vw)
        let vwd = Tensor.view (ref wd)
        let id3 = Tensor.identity 3
        let vid3 = Tensor.view (ref id3)
        match (Linalg.matmul (ref vvt) (ref vv), Linalg.matmul (ref vv) (ref vwd)) with
        | (Result.Ok vtv, Result.Ok vw_p) ->
            let vvtv = Tensor.view (ref vtv)
            Test.equal (ref "eigh_orth") (ref (if Tensor.allclose 1e-11 1e-11 false (ref vvtv) (ref vid3) then "eigh_orth" else "fail"))
            let vvw = Tensor.view (ref vw_p)
            match Linalg.matmul (ref vvw) (ref vvt) with
            | Result.Ok rec_sym ->
                let vrs = Tensor.view (ref rec_sym)
                Test.equal (ref "eigh_rec") (ref (if Tensor.allclose 1e-11 1e-11 false (ref vrs) (ref vsym) then "eigh_rec" else "fail"))
            | Result.Error _ -> Test.equal (ref "ok") (ref "eigh_mm2")
        | _ -> Test.equal (ref "ok") (ref "eigh_mm1")
    | Result.Error _ -> Test.equal (ref "ok") (ref "eigh_err")

test "phase3_eigh_complex" =
    // Complex Hermitian `eigh_complex`: H * U == U * diag(w) and U^* * U == I
    let herm = Tensor.of_array [3, 3] [
        Complex.of 3.0 0.0, Complex.of 1.0 (-1.0), Complex.of 0.0 2.0,
        Complex.of 1.0 1.0, Complex.of 4.0 0.0, Complex.of (-1.0) 0.5,
        Complex.of 0.0 (-2.0), Complex.of (-1.0) (-0.5), Complex.of 5.0 0.0
    ]
    let vherm = Tensor.view (ref herm)
    match Linalg.eigh_complex (ref vherm) with
    | Result.Ok (wh, uh) ->
        let wha = Tensor.as_array (ref wh)
        Test.is_true (wha[0] <= wha[1] && wha[1] <= wha[2])
        let uha = Tensor.as_array (ref uh)
        let ha = Tensor.as_array (ref herm)
        for k in 0i64 ..< 3 do
            for row in 0i64 ..< 3 do
                let mut lhs: Complex<f64> = Complex.zero ()
                for col in 0i64 ..< 3 do
                    let p = Complex.mul (ref ha[row * 3 + col]) (ref uha[col * 3 + k])
                    lhs = Complex.add (ref lhs) (ref p)
                let rhs = Complex.of (wha[k] * uha[row * 3 + k].re) (wha[k] * uha[row * 3 + k].im)
                let diff = Complex.sub (ref lhs) (ref rhs)
                Test.equal (ref "eigh_cplx") (ref (if Complex.abs (ref diff) < 1e-10 then "eigh_cplx" else $"eigh_cplx diff={Complex.abs (ref diff)}"))
    | Result.Error _ -> Test.equal (ref "ok") (ref "eigh_cplx_err")

test "phase3_eig_cyc_and_gen3" =
    // General nonsymmetric `eig`: cyclic permutation matrix (cube roots of unity!) & general 3x3 `A * v_k == lambda_k * v_k`
    let cyc = Tensor.of_array [3, 3] [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    let vcyc = Tensor.view (ref cyc)
    match Linalg.eig (ref vcyc) with
    | Result.Ok (wc, vc) ->
        let wca = Tensor.as_array (ref wc)
        let vca = Tensor.as_array (ref vc)
        let ca = Tensor.as_array (ref cyc)
        for k in 0i64 ..< 3 do
            Test.equal (ref "cyc_abs") (ref (if Math.abs (Complex.abs (ref wca[k]) - 1.0) < 1e-10 then "cyc_abs" else $"cyc_abs={Complex.abs (ref wca[k])}"))
            for row in 0i64 ..< 3 do
                let mut lhs: Complex<f64> = Complex.zero ()
                for col in 0i64 ..< 3 do
                    let ac = Complex.of ca[row * 3 + col] 0.0
                    let p = Complex.mul (ref ac) (ref vca[col * 3 + k])
                    lhs = Complex.add (ref lhs) (ref p)
                let rhs = Complex.mul (ref wca[k]) (ref vca[row * 3 + k])
                let diff = Complex.sub (ref lhs) (ref rhs)
                Test.equal (ref "cyc_vec") (ref (if Complex.abs (ref diff) < 1e-10 then "cyc_vec" else $"cyc_vec diff={Complex.abs (ref diff)}"))
    | Result.Error _ -> Test.equal (ref "ok") (ref "cyc_err")

    let gen3 = Tensor.of_array [3, 3] [1.0, 2.0, -1.0, 0.5, -3.0, 4.0, 2.0, 1.0, 3.0]
    let vgen3 = Tensor.view (ref gen3)
    match Linalg.eig (ref vgen3) with
    | Result.Ok (wg, vg) ->
        let wga = Tensor.as_array (ref wg)
        let vga = Tensor.as_array (ref vg)
        let ga = Tensor.as_array (ref gen3)
        for k in 0i64 ..< 3 do
            for row in 0i64 ..< 3 do
                let mut lhs: Complex<f64> = Complex.zero ()
                for col in 0i64 ..< 3 do
                    let gc = Complex.of ga[row * 3 + col] 0.0
                    let p = Complex.mul (ref gc) (ref vga[col * 3 + k])
                    lhs = Complex.add (ref lhs) (ref p)
                let rhs = Complex.mul (ref wga[k]) (ref vga[row * 3 + k])
                let diff = Complex.sub (ref lhs) (ref rhs)
                Test.equal (ref "gen3_vec") (ref (if Complex.abs (ref diff) < 1e-10 then "gen3_vec" else $"gen3_vec diff={Complex.abs (ref diff)}"))
    | Result.Error _ -> Test.equal (ref "ok") (ref "gen3_err")

    Test.is_true true

def main :: unit -> i32 = \() -> 0i32
"#,
    );
}

#[test]
fn wasm_o0_and_o3_linalg_einsum_solve_svd_and_eig_match_native() {
    let src = r#"export def verify_linalg_wasm :: own i64 -> i64
fn verify_linalg_wasm seed =
    let a = Tensor.of_array [2, 2] [4.0, 1.0, 1.0, 3.0]
    let b = Tensor.of_array [2] [6.0, 7.0]
    let va = Tensor.view (ref a)
    let vb = Tensor.view (ref b)
    let solved_ok =
        match Linalg.solve (ref va) (ref vb) with
        | Result.Ok x ->
            let xa = Tensor.as_array (ref x)
            Math.abs (xa[0] - 1.0) < 1e-12 && Math.abs (xa[1] - 2.0) < 1e-12
        | Result.Error _ -> false
    let svd_ok =
        match Linalg.svdvals (ref va) with
        | Result.Ok s ->
            let sa = Tensor.as_array (ref s)
            sa[0] >= sa[1] && Math.abs (sa[0] * sa[1] - 11.0) < 1e-11
        | Result.Error _ -> false
    let rot = Tensor.of_array [2, 2] [0.0, -1.0, 1.0, 0.0]
    let vrot = Tensor.view (ref rot)
    let eig_ok =
        match Linalg.eigvals (ref vrot) with
        | Result.Ok w ->
            let wa = Tensor.as_array (ref w)
            Math.abs wa[0].re < 1e-12 && Math.abs (Math.abs wa[0].im - 1.0) < 1e-12
        | Result.Error _ -> false
    let va1 = Tensor.view (ref a)
    let va2 = Tensor.view (ref a)
    let ein_ok =
        match Linalg.einsum "ij,jk->ik" [va1, va2] with
        | Result.Ok sq -> Tensor.to_array sq == [17.0, 7.0, 7.0, 10.0]
        | Result.Error _ -> false
    if solved_ok && svd_ok && eig_ok && ein_ok then 99 + seed else 0

0
"#;
    for opt in ["-O0", "-O3"] {
        let root = scratch_dir("wasm");
        std::fs::create_dir_all(&root).unwrap();
        let src_path = root.join("Main.tz");
        std::fs::write(&src_path, src).unwrap();
        let wasm_path = root.join("out.wasm");
        let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                "build",
                opt,
                "--edition",
                "2028",
                "--target",
                "wasm32-unknown-unknown",
                src_path.to_str().unwrap(),
                "-o",
                wasm_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "wasm build {opt} failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let js = format!(
            "const fs = require('node:fs');\n\
             const bytes = fs.readFileSync({});\n\
             const imports = {{ tsuzuri_trap: {{ trap: () => {{ throw new Error('wasm trap'); }} }} }};\n\
             WebAssembly.instantiate(bytes, imports).then(({{ instance }}) => {{\n\
               const res = instance.exports.tz_verify_linalg_wasm(0n);\n\
               if (BigInt(res) !== 99n) {{ console.error('got', res); process.exit(1); }}\n\
             }}).catch(e => {{ console.error(e); process.exit(1); }});",
            serde_json::to_string(wasm_path.to_str().unwrap()).unwrap()
        );
        let run = Command::new("node").args(["-e", &js]).output().unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            run.status.success(),
            "wasm run {opt} failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
    }
}

#[test]
fn programs_not_naming_linalg_do_not_emit_linalg_ir() {
    let root = scratch_dir("optin-ir");
    std::fs::create_dir_all(&root).unwrap();
    let src_path = root.join("Main.tz");
    std::fs::write(
        &src_path,
        "let t = Tensor.of_array [2] [1, 2]\nlet tv = Tensor.view (ref t)\nTensor.sum (ref tv)\n",
    )
    .unwrap();
    let ir_path = root.join("out.ll");
    let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args([
            "build",
            "--edition",
            "2028",
            "--emit",
            "llvm",
            src_path.to_str().unwrap(),
            "-o",
            ir_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let ir = std::fs::read_to_string(&ir_path).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        !ir.contains("@tz.fn.Linalg."),
        "unexpected Linalg IR in program not naming Linalg"
    );
}
