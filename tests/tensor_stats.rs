use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/tensor-stats")
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
        "[package]\nname = \"tensor-stats\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
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

fn assert_tz_traps(name: &str, body: &str) {
    let root = scratch_dir(name);
    std::fs::create_dir_all(&root).unwrap();
    let src = format!(
        "using std\ntest \"{name}\" =\n    {body}\n    Test.is_true true\n\ndef main :: unit -> i32 = \\() -> 0i32\n"
    );
    std::fs::write(root.join("Main.tz"), src).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"tensor-trap\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success() && combined.contains("trapped"),
        "expected runtime assertion trap in `{name}`, got status={:?}\noutput:\n{combined}",
        out.status
    );
}

#[test]
fn reductions_nan_variants_and_signed_zero_match() {
    let src = r#"using std

test "reductions_and_nan" =
    let m = Tensor.of_array [2, 3] [1, 2, 3, 4, 5, 6]
    let mv = Tensor.view (ref m)
    assert (Tensor.sum (ref mv) == 21)
    assert (Tensor.to_array (Tensor.sum_axis 0 (ref mv)) == [5, 7, 9])
    assert (Tensor.to_array (Tensor.sum_axis (-1) (ref mv)) == [6, 15])
    let sk = Tensor.sum_with (Tensor.AxisOptions { axis: 1, keepdims: true }) (ref mv)
    let skv = Tensor.view (ref sk)
    assert (Tensor.shape (ref skv) == [2, 1])
    assert (Tensor.to_array sk == [6, 15])

    // Multi-axis and non-contiguous transposed view
    let c3 = Tensor.of_array [2, 2, 2] [1, 2, 3, 4, 5, 6, 7, 8]
    let c3v = Tensor.view (ref c3)
    assert (Tensor.to_array (Tensor.sum_axes (Tensor.AxesOptions { axes: [0, 2], keepdims: false }) (ref c3v)) == [14, 22])
    assert (Tensor.to_array (Tensor.prod_axes (Tensor.AxesOptions { axes: [1, 2], keepdims: false }) (ref c3v)) == [24, 1680])
    let tr = Tensor.transpose (ref mv)
    assert (Tensor.to_array (Tensor.sum_axis 0 (ref tr)) == [6, 15])
    assert (Tensor.to_array (Tensor.sum_axis 1 (ref tr)) == [5, 7, 9])

    // Kahan compensated sum
    let kt = Tensor.of_array [4] [1.0, 1e100, 1.0, -1e100]
    let ktv = Tensor.view (ref kt)
    assert (Tensor.sum_kahan (ref ktv) == 2.0)

    // Mean, weighted average, variance, std, ddof >= n -> NaN
    let ft = Tensor.of_array [2, 2] [1.0, 3.0, 2.0, 6.0]
    let ftv = Tensor.view (ref ft)
    assert (Tensor.mean (ref ftv) == 3.0)
    assert (Tensor.to_array (Tensor.mean_axis 1 (ref ftv)) == [2.0, 4.0])
    let w = Tensor.of_array [2] [1.0, 3.0]
    let wv = Tensor.view (ref w)
    assert (Tensor.to_array (Tensor.average_axis 1 (ref wv) (ref ftv)) == [2.5, 5.0])
    assert (Tensor.to_array (Tensor.var_axis 1 (ref ftv)) == [1.0, 4.0])
    assert (Tensor.to_array (Tensor.std_axis 1 (ref ftv)) == [1.0, 2.0])
    let v_ddof1 = Tensor.var_with (Tensor.VarianceOptions { axis: Maybe.Some 1, keepdims: false, ddof: 1 }) (ref ftv)
    assert (Tensor.to_array v_ddof1 == [2.0, 8.0])
    let v_ddof_hi = Tensor.var_with (Tensor.VarianceOptions { axis: Maybe.None, keepdims: false, ddof: 4 }) (ref ftv)
    assert (Math.is_nan (Tensor.as_array (ref v_ddof_hi))[0])

    // Signed zero & NaN propagation in min / max / argmin / argmax
    let sz = Tensor.of_array [2] [0.0, -0.0]
    let szv = Tensor.view (ref sz)
    assert (Math.signbit (Tensor.min (ref szv)))
    assert (!(Math.signbit (Tensor.max (ref szv))))

    let wnan = Tensor.of_array [4] [3.0, 0.0 / 0.0, 1.0, 0.0 / 0.0]
    let wnanv = Tensor.view (ref wnan)
    assert (Math.is_nan (Tensor.min (ref wnanv)))
    assert (Math.is_nan (Tensor.max (ref wnanv)))
    assert (Tensor.argmin (ref wnanv) == 1)
    assert (Tensor.argmax (ref wnanv) == 1)
    assert (Tensor.nansum (ref wnanv) == 4.0)
    assert (Tensor.nanprod (ref wnanv) == 3.0)
    assert (Tensor.nanmean (ref wnanv) == 2.0)
    assert (Tensor.nanvar (ref wnanv) == 1.0)
    assert (Tensor.nanstd (ref wnanv) == 1.0)
    assert (Tensor.nanmin (ref wnanv) == 1.0)
    assert (Tensor.nanmax (ref wnanv) == 3.0)
    assert (Tensor.nanargmin (ref wnanv) == 2)
    assert (Tensor.nanargmax (ref wnanv) == 0)

    let allnan = Tensor.of_array [2] [0.0 / 0.0, 0.0 / 0.0]
    let allnanv = Tensor.view (ref allnan)
    assert (Tensor.nansum (ref allnanv) == 0.0)
    assert (Tensor.nanprod (ref allnanv) == 1.0)
    assert (Math.is_nan (Tensor.nanmean (ref allnanv)))
    assert (Math.is_nan (Tensor.nanmin (ref allnanv)))
    assert (Math.is_nan (Tensor.nanmax (ref allnanv)))

    // Boolean & count_nonzero
    let bt = Tensor.of_array [2, 2] [true, true, true, false]
    let btv = Tensor.view (ref bt)
    assert (!(Tensor.all (ref btv)))
    assert (Tensor.any (ref btv))
    assert (Tensor.to_array (Tensor.all_axis 1 (ref btv)) == [true, false])
    assert (Tensor.to_array (Tensor.any_axis 0 (ref btv)) == [true, true])
    let cnz = Tensor.of_array [4] [0.0, -0.0, 2.5, 0.0 / 0.0]
    let cnzv = Tensor.view (ref cnz)
    assert (Tensor.count_nonzero (ref cnzv) == 2)
    Test.is_true true

def main :: unit -> i32 = \() -> 0i32
"#;
    run_tz_test(src);
}

#[test]
fn cumulative_diff_gradient_trapezoid_convolve_and_interp_match() {
    let src = r#"using std

test "cumulative_diff_convolve_interp" =
    let t = Tensor.of_array [2, 3] [1, 2, 4, 3, 5, 9]
    let tv = Tensor.view (ref t)
    assert (Tensor.to_array (Tensor.cumsum 1 (ref tv)) == [1, 3, 7, 3, 8, 17])
    assert (Tensor.to_array (Tensor.cumsum_flat (ref tv)) == [1, 3, 7, 10, 15, 24])
    assert (Tensor.to_array (Tensor.cumprod 1 (ref tv)) == [1, 2, 8, 3, 15, 135])
    assert (Tensor.to_array (Tensor.diff 1 (ref tv)) == [1, 2, 2, 4])
    assert (Tensor.to_array (Tensor.diff_n 2 1 (ref tv)) == [1, 2])
    assert (Tensor.to_array (Tensor.ediff1d (ref tv)) == [1, 2, -1, 2, 4])

    let nt = Tensor.of_array [3] [2.0, 0.0 / 0.0, 3.0]
    let ntv = Tensor.view (ref nt)
    assert (Tensor.to_array (Tensor.nancumsum 0 (ref ntv)) == [2.0, 2.0, 5.0])
    assert (Tensor.to_array (Tensor.nancumprod 0 (ref ntv)) == [2.0, 2.0, 6.0])

    let quad = Tensor.of_array [4] [0.0, 1.0, 4.0, 9.0]
    let qv = Tensor.view (ref quad)
    assert (Tensor.to_array (Tensor.gradient 1.0 0 (ref qv)) == [1.0, 2.0, 4.0, 5.0])
    let trap1 = Tensor.trapezoid 1.0 0 (ref qv)
    let trap1v = Tensor.view (ref trap1)
    assert (Tensor.sum (ref trap1v) == 9.5)
    let x_pts = Tensor.of_array [3] [0.0, 1.0, 3.0]
    let y_pts = Tensor.of_array [3] [1.0, 2.0, 6.0]
    let xpv = Tensor.view (ref x_pts)
    let ypv = Tensor.view (ref y_pts)
    let trap2 = Tensor.trapezoid_x 0 (ref xpv) (ref ypv)
    let trap2v = Tensor.view (ref trap2)
    assert (Tensor.sum (ref trap2v) == 9.5)

    let sig = Tensor.of_array [3] [1.0, 2.0, 3.0]
    let ker = Tensor.of_array [3] [0.0, 1.0, 0.5]
    let sv = Tensor.view (ref sig)
    let kv = Tensor.view (ref ker)
    assert (Tensor.to_array (Tensor.convolve Tensor.ConvolveMode.Full (ref sv) (ref kv)) == [0.0, 1.0, 2.5, 4.0, 1.5])
    assert (Tensor.to_array (Tensor.convolve Tensor.ConvolveMode.Same (ref sv) (ref kv)) == [1.0, 2.5, 4.0])
    assert (Tensor.to_array (Tensor.convolve Tensor.ConvolveMode.Valid (ref sv) (ref kv)) == [2.5])
    assert (Tensor.to_array (Tensor.correlate Tensor.ConvolveMode.Full (ref sv) (ref kv)) == [0.5, 2.0, 3.5, 3.0, 0.0])
    assert (Tensor.to_array (Tensor.correlate Tensor.ConvolveMode.Same (ref sv) (ref kv)) == [2.0, 3.5, 3.0])
    assert (Tensor.to_array (Tensor.correlate Tensor.ConvolveMode.Valid (ref sv) (ref kv)) == [3.5])

    let qx = Tensor.of_array [5] [-1.0, 0.5, 2.0, 5.0, 0.0 / 0.0]
    let qxv = Tensor.view (ref qx)
    let interp_res = Tensor.to_array (Tensor.interp (ref xpv) (ref ypv) (ref qxv))
    assert (interp_res[0] == 1.0 && interp_res[1] == 1.5 && interp_res[2] == 4.0 && interp_res[3] == 6.0 && Math.is_nan interp_res[4])
    let extrap_res = Tensor.to_array (Tensor.interp_extrap (-10.0) 99.0 (ref xpv) (ref ypv) (ref qxv))
    assert (extrap_res[0] == (-10.0) && extrap_res[3] == 99.0)
    Test.is_true true

def main :: unit -> i32 = \() -> 0i32
"#;
    run_tz_test(src);
}

#[test]
fn quantiles_histograms_cov_sorting_and_set_ops_match() {
    let src = r#"using std

test "quantiles_histograms_cov_sorting_set_ops" =
    let s = Tensor.of_array [4] [40.0, 10.0, 30.0, 20.0]
    let sv = Tensor.view (ref s)
    assert (Tensor.median (ref sv) == 25.0)
    assert (Tensor.percentile 25.0 (ref sv) == 17.5)
    assert (Tensor.to_array (Tensor.quantile_with (Tensor.QuantileOptions { method: Tensor.QuantileMethod.Lower, axis: Maybe.None, keepdims: false }) 0.25 (ref sv)) == [10.0])
    assert (Tensor.to_array (Tensor.quantile_with (Tensor.QuantileOptions { method: Tensor.QuantileMethod.Higher, axis: Maybe.None, keepdims: false }) 0.25 (ref sv)) == [20.0])
    assert (Tensor.to_array (Tensor.quantile_with (Tensor.QuantileOptions { method: Tensor.QuantileMethod.Midpoint, axis: Maybe.None, keepdims: false }) 0.25 (ref sv)) == [15.0])
    // Nearest ties-to-even: n=4 -> v = 0.5 * 3 = 1.5 -> lo=1 (odd) -> ceil=2 -> sorted[2] = 30.0
    assert (Tensor.to_array (Tensor.quantile_with (Tensor.QuantileOptions { method: Tensor.QuantileMethod.Nearest, axis: Maybe.None, keepdims: false }) 0.5 (ref sv)) == [30.0])

    // Histograms, bincount, digitize (increasing & decreasing)
    let h_in = Tensor.of_array [5] [0.0, 0.5, 1.0, 1.5, 2.0]
    let (h_cnt, h_edg) = Tensor.histogram 2 (ref (Tensor.view (ref h_in)))
    assert (Tensor.to_array h_cnt == [2, 3])
    assert (Tensor.to_array h_edg == [0.0, 1.0, 2.0])

    let const_in = Tensor.of_array [3] [5.0, 5.0, 5.0]
    let (c_cnt, c_edg) = Tensor.histogram 2 (ref (Tensor.view (ref const_in)))
    assert (Tensor.to_array c_cnt == [0, 3])
    assert (Tensor.to_array c_edg == [4.5, 5.0, 5.5])

    let bc_in = Tensor.of_array [5] [0, 2, 2, 4, 0]
    assert (Tensor.to_array (Tensor.bincount 6 (ref (Tensor.view (ref bc_in)))) == [2, 0, 2, 0, 1, 0])

    let bins_inc = Tensor.of_array [3] [0.0, 1.0, 2.0]
    let bins_dec = Tensor.of_array [3] [2.0, 1.0, 0.0]
    let dx = Tensor.of_array [4] [-0.5, 0.0, 1.0, 2.5]
    assert (Tensor.to_array (Tensor.digitize false (ref (Tensor.view (ref bins_inc))) (ref (Tensor.view (ref dx)))) == [0, 1, 2, 3])
    assert (Tensor.to_array (Tensor.digitize true (ref (Tensor.view (ref bins_inc))) (ref (Tensor.view (ref dx)))) == [0, 0, 1, 3])
    assert (Tensor.to_array (Tensor.digitize false (ref (Tensor.view (ref bins_dec))) (ref (Tensor.view (ref dx)))) == [3, 3, 2, 0])

    // Covariance & correlation (including zero-variance row -> NaN)
    let cm = Tensor.of_array [2, 3] [0.0, 1.0, 2.0, 4.0, 2.0, 0.0]
    let cmv = Tensor.view (ref cm)
    assert (Tensor.to_array (Tensor.cov (ref cmv)) == [1.0, -2.0, -2.0, 4.0])
    assert (Tensor.to_array (Tensor.corrcoef (ref cmv)) == [1.0, -1.0, -1.0, 1.0])
    let zvar = Tensor.of_array [2, 3] [1.0, 1.0, 1.0, 1.0, 2.0, 3.0]
    let zcorr = Tensor.to_array (Tensor.corrcoef (ref (Tensor.view (ref zvar))))
    assert (Math.is_nan zcorr[0] && Math.is_nan zcorr[1] && zcorr[3] == 1.0)

    // Stable sort, argsort with NaN at end, lexsort, partition/argpartition, searchsorted, unique & set ops
    let fsort = Tensor.of_array [5] [2.0, 0.0 / 0.0, 1.0, 2.0, -1.0]
    let fsv = Tensor.view (ref fsort)
    let s_out = Tensor.to_array (Tensor.sort 0 (ref fsv))
    assert (s_out[0] == (-1.0) && s_out[1] == 1.0 && s_out[2] == 2.0 && s_out[3] == 2.0 && Math.is_nan s_out[4])
    assert (Tensor.to_array (Tensor.argsort 0 (ref fsv)) == [4, 2, 0, 3, 1])

    let keys = Tensor.of_array [2, 4] [2, 1, 1, 2, 10, 20, 10, 10]
    assert (Tensor.to_array (Tensor.lexsort (ref (Tensor.view (ref keys)))) == [2, 0, 3, 1])

    let p_in = Tensor.of_array [7] [9, 3, 7, 3, 1, 8, 5]
    let pv = Tensor.view (ref p_in)
    let part = Tensor.to_array (Tensor.partition 3 0 (ref pv))
    assert (part[3] == 5)
    for i in 0i64 ..< 3 do assert (part[i] <= 5)
    for i in 4i64 ..< 7 do assert (part[i] >= 5)
    let apart = Tensor.to_array (Tensor.argpartition (-2) 0 (ref pv))
    assert ((Tensor.as_array (ref p_in))[apart[5]] == 8)

    let ss_base = Tensor.of_array [4] [10, 20, 20, 30]
    let ss_q = Tensor.of_array [3] [5, 20, 35]
    assert (Tensor.to_array (Tensor.searchsorted Tensor.SearchSide.Left (ref (Tensor.view (ref ss_base))) (ref (Tensor.view (ref ss_q)))) == [0, 1, 4])
    assert (Tensor.to_array (Tensor.searchsorted Tensor.SearchSide.Right (ref (Tensor.view (ref ss_base))) (ref (Tensor.view (ref ss_q)))) == [0, 3, 4])

    let u_in = Tensor.of_array [6] [2.0, 0.0 / 0.0, 1.0, 2.0, 0.0 / 0.0, 1.0]
    let uf = Tensor.unique_full (ref (Tensor.view (ref u_in)))
    let uv = Tensor.as_array (ref uf.values)
    assert (uv.length == 3 && uv[0] == 1.0 && uv[1] == 2.0 && Math.is_nan uv[2])
    assert (Tensor.to_array uf.indices == [2, 0, 1])
    assert (Tensor.to_array uf.counts == [2, 2, 2])
    assert (Tensor.to_array uf.inverse == [1, 2, 0, 1, 2, 0])

    let sa = Tensor.of_array [4] [4, 1, 2, 2]
    let sb = Tensor.of_array [3] [2, 3, 4]
    let sav = Tensor.view (ref sa)
    let sbv = Tensor.view (ref sb)
    assert (Tensor.to_array (Tensor.isin (ref sbv) (ref sav)) == [true, false, true, true])
    assert (Tensor.to_array (Tensor.intersect1d (ref sav) (ref sbv)) == [2, 4])
    assert (Tensor.to_array (Tensor.union1d (ref sav) (ref sbv)) == [1, 2, 3, 4])
    assert (Tensor.to_array (Tensor.setdiff1d (ref sav) (ref sbv)) == [1])
    assert (Tensor.to_array (Tensor.setxor1d (ref sav) (ref sbv)) == [1, 3])
    Test.is_true true

def main :: unit -> i32 = \() -> 0i32
"#;
    run_tz_test(src);
}

#[test]
fn invalid_inputs_and_empty_extrema_trap_cleanly() {
    assert_tz_traps(
        "empty_min",
        "let t: Tensor<i64> = Tensor.of_array [0] []\n    let tv = Tensor.view (ref t)\n    let _ = Tensor.min (ref tv)",
    );
    assert_tz_traps(
        "all_nan_nanargmin",
        "let t = Tensor.of_array [2] [0.0 / 0.0, 0.0 / 0.0]\n    let tv = Tensor.view (ref t)\n    let _ = Tensor.nanargmin (ref tv)",
    );
    assert_tz_traps(
        "duplicate_axes",
        "let t = Tensor.of_array [2, 2] [1, 2, 3, 4]\n    let tv = Tensor.view (ref t)\n    let _ = Tensor.sum_axes (Tensor.AxesOptions { axes: [0, -2], keepdims: false }) (ref tv)",
    );
    assert_tz_traps(
        "quantile_out_of_range",
        "let t = Tensor.of_array [2] [1.0, 2.0]\n    let tv = Tensor.view (ref t)\n    let _ = Tensor.quantile 1.5 (ref tv)",
    );
    assert_tz_traps(
        "negative_bincount",
        "let t = Tensor.of_array [2] [1, -1]\n    let tv = Tensor.view (ref t)\n    let _ = Tensor.bincount 0 (ref tv)",
    );
    assert_tz_traps(
        "zero_weight_average",
        "let w = Tensor.of_array [2] [1.0, -1.0]\n    let x = Tensor.of_array [2] [2.0, 4.0]\n    let wv = Tensor.view (ref w)\n    let xv = Tensor.view (ref x)\n    let _ = Tensor.average (ref wv) (ref xv)",
    );
}

#[test]
fn wasm_o0_and_o3_reductions_quantiles_convolve_and_sorting_match_native() {
    let src = r#"export def verify_wasm :: own i64 -> i64
fn verify_wasm seed =
    let m = Tensor.of_array [2, 3] [3.0, 1.0, 2.0, 6.0, 4.0, 5.0]
    let mv = Tensor.view (ref m)
    let med = Tensor.median (ref mv)
    let wnan = Tensor.of_array [4] [2.0, 0.0 / 0.0, 1.0, 3.0]
    let wnv = Tensor.view (ref wnan)
    let nmean = Tensor.nanmean (ref wnv)
    let sorted = Tensor.to_array (Tensor.sort 0 (ref wnv))
    let q = Tensor.quantile 0.5 (ref mv)
    let u = Tensor.to_array (Tensor.unique (ref wnv))
    let sig = Tensor.of_array [3] [1.0, 2.0, 3.0]
    let ker = Tensor.of_array [2] [1.0, 0.5]
    let sv = Tensor.view (ref sig)
    let kv = Tensor.view (ref ker)
    let cv = Tensor.to_array (Tensor.convolve Tensor.ConvolveMode.Valid (ref sv) (ref kv))
    if med == 3.5 && nmean == 2.0 && q == 3.5 && sorted[0] == 1.0 && Math.is_nan sorted[3] && u.length == 4 && Math.is_nan u[3] && cv == [2.5, 4.0] then 42 + seed
    else 0

verify_wasm 0
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
               const res = instance.exports.tz_verify_wasm(0n);\n\
               if (BigInt(res) !== 42n) {{ console.error('got', res, Object.keys(instance.exports)); process.exit(1); }}\n\
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
