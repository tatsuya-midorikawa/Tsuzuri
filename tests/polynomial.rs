//! NC09: `Polynomial` against independent references. Every test writes a project of one `Main.tz` (edition 2028) and
//! runs `tsuzuri test` on it. The expectations are closed forms and identities written in the tests and the
//! values that NumPy 2.5.1 gives; `python3 tests/fft_polynomial_reference.py polynomial` prints the block between
//! `reference:begin` and `reference:end`.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/polynomial")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn run_tz_test(body: &str) {
    let root = scratch_dir("suite");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Main.tz"),
        format!("{PRELUDE}\n{REFERENCE}\n{body}"),
    )
    .unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"polynomial-suite\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
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

const PRELUDE: &str = r#"using std

def gap :: ref [f64] -> ref [f64] -> f64
fn gap left right =
    assert (left.length == right.length)
    let mut worst = 0.0
    for i in 0i64 ..< left.length do
        let d = Math.abs (left[i] - right[i])
        if !(d <= worst) then worst = d
    worst

def coefficients_of :: Result<[f64], Polynomial.Error> -> [f64]
fn coefficients_of result =
    match result with
    | Result.Ok values -> values
    | Result.Error _ ->
        assert false
        let empty: [f64] = []
        empty

def roots_of :: Result<[Complex<f64>], Polynomial.Error> -> [Complex<f64>]
fn roots_of result =
    match result with
    | Result.Ok values -> values
    | Result.Error _ ->
        assert false
        let empty: [Complex<f64>] = []
        empty

def fails_with :: own Polynomial.Error -> Result<[f64], Polynomial.Error> -> bool
fn fails_with expected result =
    match result with
    | Result.Ok _ -> false
    | Result.Error problem -> problem == expected

// The distance from every expected root to the nearest found one.
def root_gap :: ref [Complex<f64>] -> ref [Complex<f64>] -> f64
fn root_gap found expected =
    assert (found.length == expected.length)
    let mut worst = 0.0
    for i in 0i64 ..< expected.length do
        let mut nearest = 1.0e300
        for j in 0i64 ..< found.length do
            let d = Math.hypot (found[j].re - expected[i].re) (found[j].im - expected[i].im)
            if d < nearest then nearest = d
        if !(nearest <= worst) then worst = nearest
    worst

// Whether the roots are in ascending order of the real part and then of the imaginary part.
def sorted_roots :: ref [Complex<f64>] -> bool
fn sorted_roots found =
    let mut sorted = true
    for i in 1i64 ..< found.length do
        let before = found[i - 1]
        let after = found[i]
        if before.re > after.re || (before.re == after.re && before.im > after.im) then sorted = false
    sorted
"#;

const REFERENCE: &str = r#"// reference:begin
def pa :: unit -> [f64]
fn pa _ = [(1.5), (-2.0), (0.25), (3.0)]

def pb :: unit -> [f64]
fn pb _ = [(-0.5), (4.0), (1.0)]

def xs5 :: unit -> [f64]
fn xs5 _ = [(-1.5), (-0.25), (0.0), (0.5), (2.0)]

def np_polyval_a :: unit -> [f64]
fn np_polyval_a _ = [(-5.0625), (1.96875), (1.5), (0.9375), (22.5)]

def np_polyadd :: unit -> [f64]
fn np_polyadd _ = [(1.0), (2.0), (1.25), (3.0)]

def np_polysub :: unit -> [f64]
fn np_polysub _ = [(2.0), (-6.0), (-0.75), (3.0)]

def np_polymul :: unit -> [f64]
fn np_polymul _ = [(-0.75), (7.0), (-6.625), (-2.5), (12.25), (3.0)]

def np_polydiv_q :: unit -> [f64]
fn np_polydiv_q _ = [(-11.75), (3.0)]

def np_polydiv_r :: unit -> [f64]
fn np_polydiv_r _ = [(-4.375), (46.5)]

def np_polypow3 :: unit -> [f64]
fn np_polypow3 _ = [(-0.125), (3.0), (-23.25), (52.0), (46.5), (12.0), (1.0)]

def np_polyder :: unit -> [f64]
fn np_polyder _ = [(-2.0), (0.5), (9.0)]

def np_polyder2 :: unit -> [f64]
fn np_polyder2 _ = [(0.5), (18.0)]

def np_polyint :: unit -> [f64]
fn np_polyint _ = [(0.0), (1.5), (-1.0), (0.08333333333333333), (0.75)]

def np_polyint_k :: unit -> [f64]
fn np_polyint_k _ = [(2.5), (1.5), (-1.0), (0.08333333333333333), (0.75)]

def np_polyfromroots :: unit -> [f64]
fn np_polyfromroots _ = [(3.0), (-5.5), (-1.5), (1.0)]

def np_polyvander :: unit -> [f64]
fn np_polyvander _ = [(1.0), (-1.5), (2.25), (-3.375), (1.0), (-0.25), (0.0625), (-0.015625), (1.0), (0.0), (0.0), (0.0), (1.0), (0.5), (0.25), (0.125), (1.0), (2.0), (4.0), (8.0)]

def fit_x :: unit -> [f64]
fn fit_x _ = [(-1.0), (-0.75), (-0.5), (-0.25), (0.0), (0.25), (0.5), (0.75), (1.0)]

def fit_y :: unit -> [f64]
fn fit_y _ = [(0.8678794411714423), (0.7536165527410147), (0.7315306597126334), (0.8100507830714049), (1.0), (1.3152754166877414), (1.7737212707001282), (2.398250016612675), (3.218281828459045)]

def fit_w :: unit -> [f64]
fn fit_w _ = [(0.5), (0.75), (1.0), (1.25), (1.5), (1.75), (2.0), (2.25), (2.5)]

def np_polyfit3 :: unit -> [f64]
fn np_polyfit3 _ = [(0.9947583782762042), (0.9973213411423792), (1.0442081505787328), (0.17765384670924514)]

def np_polyfit3_w :: unit -> [f64]
fn np_polyfit3_w _ = [(0.996792451662616), (0.9859152316294485), (1.0374816705267624), (0.19631853282044298)]

def np_chebfit4 :: unit -> [f64]
fn np_chebfit4 _ = [(1.5160802699069746), (1.130561726174314), (0.5215174375454199), (0.04441346167731122), (0.005475285610174753)]

def np_legfit4 :: unit -> [f64]
fn np_legfit4 _ = [(1.3418761050178227), (1.1039136491679267), (0.6911849372147064), (0.07106153868369772), (0.010011950830039557)]

def cc5 :: unit -> [f64]
fn cc5 _ = [(0.5), (-1.0), (2.0), (0.25), (-0.75)]

def np_chebval :: unit -> [f64]
fn np_chebval _ = [(-10.875), (-1.2265625), (-2.25), (-0.875), (-53.75)]

def np_legval :: unit -> [f64]
fn np_legval _ = [(-4.361328125000001), (-0.09680175781250011), (-0.78125), (-0.1425781250000001), (-27.78125)]

def np_roots_real :: unit -> [f64]
fn np_roots_real _ = [(-2.000000000000001), (1.0000000000000013), (2.9999999999999996)]

def np_roots_complex :: unit -> [Complex<f64>]
fn np_roots_complex _ = [Complex.of (-0.1335517491618166) (-1.6838022718621541), Complex.of (-0.1335517491618166) (1.6838022718621541), Complex.of (1.1335517491618166) (-0.683802271862155), Complex.of (1.1335517491618166) (0.683802271862155)]
// reference:end
"#;

#[test]
fn arithmetic_matches_numpy() {
    run_tz_test(
        r#"test "eval, add, sub and mul" =
    let a = pa ()
    let b = pb ()
    let xs = xs5 ()
    let expected = np_polyval_a ()
    for i in 0i64 ..< xs.length do
        assert (Math.abs (Polynomial.eval xs[i] (ref a) - expected[i]) < 1e-14)
    let points = Tensor.of_array [5] (Array.copy (ref xs))
    let on_points = Polynomial.eval_tensor (ref a) (Tensor.view points)
    assert (gap (Tensor.as_array on_points) (ref expected) < 1e-14)
    let grid = Tensor.of_array [5, 1] (Array.copy (ref xs))
    let on_grid = Polynomial.eval_tensor (ref a) (Tensor.view grid)
    let grid_view = Tensor.view on_grid
    assert (Tensor.shape grid_view == [5, 1])
    assert (Polynomial.add (ref a) (ref b) == np_polyadd ())
    assert (Polynomial.add (ref b) (ref a) == np_polyadd ())
    assert (Polynomial.sub (ref a) (ref b) == np_polysub ())
    assert (gap (ref (Polynomial.mul (ref a) (ref b))) (ref (np_polymul ())) < 1e-14)
    assert (gap (ref (Polynomial.mul (ref b) (ref a))) (ref (np_polymul ())) < 1e-14)
    let none: [f64] = []
    assert (Polynomial.eval 3.0 (ref none) == 0.0)
    assert (Polynomial.eval 3.0 [4.5] == 4.5)
    assert (Polynomial.add (ref none) (ref none) == [0.0])
    assert (Polynomial.mul (ref none) (ref a) == [0.0])
    assert (Polynomial.sub (ref none) [1.0, 2.0] == [-1.0, -2.0])
    // Zero coefficients stay until `trim` drops them.
    assert (Polynomial.sub [1.0, 2.0, 3.0] [0.0, 0.0, 3.0] == [1.0, 2.0, 0.0])
    assert (Polynomial.trim 0.0 [1.0, 2.0, 0.0] == [1.0, 2.0])
    assert (Polynomial.trim 0.5 [1.0, 0.25, -0.5] == [1.0])
    assert (Polynomial.trim 0.0 [0.0, 0.0] == [0.0])
    assert (Polynomial.trim 0.0 (ref none) == [0.0])

test "eval_complex agrees with eval on the real axis and vanishes at a complex root" =
    let a = pa ()
    let on_axis = Polynomial.eval_complex (Complex.of 0.5 0.0) (ref a)
    assert (Math.abs (on_axis.re - Polynomial.eval 0.5 (ref a)) < 1e-15 && on_axis.im == 0.0)
    // x^2 - 2 x + 5 = (x - (1 + 2i)) (x - (1 - 2i))
    let at_root = Polynomial.eval_complex (Complex.of 1.0 2.0) [5.0, -2.0, 1.0]
    assert (at_root.re == 0.0 && at_root.im == 0.0)
    let none: [f64] = []
    let empty = Polynomial.eval_complex (Complex.of 1.0 2.0) (ref none)
    assert (empty.re == 0.0 && empty.im == 0.0)

test "divmod and pow" =
    let a = pa ()
    let b = pb ()
    match Polynomial.divmod (ref a) (ref b) with
    | Result.Ok (quotient, remainder) ->
        assert (gap (ref quotient) (ref (np_polydiv_q ())) < 1e-13)
        assert (gap (ref remainder) (ref (np_polydiv_r ())) < 1e-13)
        // dividend = quotient * divisor + remainder
        let rebuilt = Polynomial.add (ref (Polynomial.mul (ref quotient) (ref b))) (ref remainder)
        assert (gap (ref rebuilt) (ref a) < 1e-13)
    | Result.Error _ -> assert false
    match Polynomial.divmod (ref b) (ref a) with
    | Result.Ok (quotient, remainder) -> assert (quotient == [0.0] && remainder == b)
    | Result.Error _ -> assert false
    match Polynomial.divmod [2.0, 4.0, 6.0] [2.0] with
    | Result.Ok (quotient, remainder) -> assert (quotient == [1.0, 2.0, 3.0] && remainder == [0.0])
    | Result.Error _ -> assert false
    // The highest zero coefficients of both operands are dropped first.
    match Polynomial.divmod [-1.0, 0.0, 1.0, 0.0] [1.0, 1.0, 0.0] with
    | Result.Ok (quotient, remainder) -> assert (quotient == [-1.0, 1.0] && remainder == [0.0])
    | Result.Error _ -> assert false
    match Polynomial.divmod (ref a) [0.0, 0.0] with
    | Result.Error problem -> assert (problem == Polynomial.Error.ZeroDivisor)
    | Result.Ok _ -> assert false
    let none: [f64] = []
    match Polynomial.divmod (ref a) (ref none) with
    | Result.Error problem -> assert (problem == Polynomial.Error.ZeroDivisor)
    | Result.Ok _ -> assert false
    assert (gap (ref (Polynomial.pow 3 (ref b))) (ref (np_polypow3 ())) < 1e-12)
    assert (Polynomial.pow 0 (ref b) == [1.0])
    assert (Polynomial.pow 1 (ref b) == b)
"#,
    );
}

#[test]
fn calculus_and_roots_match_numpy() {
    run_tz_test(
        r#"test "deriv and integ" =
    let a = pa ()
    assert (Polynomial.deriv (ref a) == np_polyder ())
    assert (Polynomial.deriv_n 2 (ref a) == np_polyder2 ())
    assert (Polynomial.deriv_n 0 (ref a) == a)
    assert (Polynomial.deriv_n 9 (ref a) == [0.0])
    assert (Polynomial.deriv [7.0] == [0.0])
    assert (Polynomial.integ (ref a) == np_polyint ())
    assert (Polynomial.integ_const 2.5 (ref a) == np_polyint_k ())
    // The derivative of the antiderivative is the polynomial, bit for bit when the divisions are exact.
    assert (Polynomial.deriv (ref (Polynomial.integ [3.0, 4.0, 6.0, 8.0])) == [3.0, 4.0, 6.0, 8.0])
    assert (gap (ref (Polynomial.deriv (ref (Polynomial.integ (ref a))))) (ref a) < 1e-15)
    let none: [f64] = []
    assert (Polynomial.deriv (ref none) == [0.0])
    assert (Polynomial.integ (ref none) == [0.0])

test "from_roots and roots" =
    assert (gap (ref (Polynomial.from_roots [-2.0, 0.5, 3.0])) (ref (np_polyfromroots ())) < 1e-14)
    let none: [f64] = []
    assert (Polynomial.from_roots (ref none) == [1.0])
    // Real roots: x^3 - 2 x^2 - 5 x + 6 = (x + 2) (x - 1) (x - 3)
    let real = roots_of (Polynomial.roots [6.0, -5.0, -2.0, 1.0])
    let expected = np_roots_real ()
    assert (real.length == 3 && sorted_roots (ref real))
    for i in 0i64 ..< 3 do
        assert (Math.abs (real[i].re - expected[i]) < 1e-12 && Math.abs real[i].im < 1e-12)
    // Two conjugate pairs.
    let complex = roots_of (Polynomial.roots [5.0, -6.0, 4.0, -2.0, 1.0])
    assert (root_gap (ref complex) (ref (np_roots_complex ())) < 1e-12 && sorted_roots (ref complex))
    // The round trip from_roots -> roots, with real, repeated and conjugate roots.
    let back = roots_of (Polynomial.roots (ref (Polynomial.from_roots [-1.5, 0.25, 2.0, 4.0])))
    assert (root_gap (ref back) [Complex.of (-1.5) 0.0, Complex.of 0.25 0.0, Complex.of 2.0 0.0, Complex.of 4.0 0.0] < 1e-12)
    let double = roots_of (Polynomial.roots (ref (Polynomial.from_roots [2.0, 2.0, -1.0])))
    assert (root_gap (ref double) [Complex.of 2.0 0.0, Complex.of 2.0 0.0, Complex.of (-1.0) 0.0] < 1e-6)
    let pairs = Polynomial.from_roots_complex [Complex.of 1.0 2.0, Complex.of 1.0 (-2.0), Complex.of (-0.5) 0.0]
    let real_parts = new [f64](pairs.length, \i -> pairs[i].re)
    for i in 0i64 ..< pairs.length do
        assert (Math.abs pairs[i].im < 1e-15)
    assert (gap (ref real_parts) [2.5, 4.0, -1.5, 1.0] < 1e-14)
    let found = roots_of (Polynomial.roots (ref real_parts))
    assert (root_gap (ref found) [Complex.of (-0.5) 0.0, Complex.of 1.0 (-2.0), Complex.of 1.0 2.0] < 1e-12 && sorted_roots (ref found))
    // Roots at zero, a line, constants and the highest zero coefficients.
    let with_zero = roots_of (Polynomial.roots [0.0, 0.0, -3.0, 1.0])
    assert (root_gap (ref with_zero) [Complex.of 0.0 0.0, Complex.of 0.0 0.0, Complex.of 3.0 0.0] < 1e-12)
    let line = roots_of (Polynomial.roots [3.0, -2.0])
    assert (line.length == 1 && line[0].re == 1.5 && line[0].im == 0.0)
    assert ((roots_of (Polynomial.roots [3.0, -2.0, 0.0, 0.0])).length == 1)
    assert ((roots_of (Polynomial.roots [3.0])).length == 0)
    assert ((roots_of (Polynomial.roots [0.0, 0.0])).length == 0)
    assert ((roots_of (Polynomial.roots (ref none))).length == 0)
    let complex_none: [Complex<f64>] = []
    assert ((Polynomial.from_roots_complex (ref complex_none)).length == 1)
"#,
    );
}

#[test]
fn fitting_matches_numpy() {
    run_tz_test(
        r#"test "vander and fit" =
    let xs = Tensor.of_array [5] (xs5 ())
    match Polynomial.vander 3 (Tensor.view xs) with
    | Result.Ok matrix ->
        let view = Tensor.view matrix
        assert (Tensor.shape view == [5, 4])
        assert (gap (Tensor.as_array matrix) (ref (np_polyvander ())) < 1e-15)
    | Result.Error _ -> assert false
    let x = Tensor.of_array [9] (fit_x ())
    let y = Tensor.of_array [9] (fit_y ())
    let w = Tensor.of_array [9] (fit_w ())
    let xv = Tensor.view x
    let yv = Tensor.view y
    assert (gap (ref (coefficients_of (Polynomial.fit 3 xv yv))) (ref (np_polyfit3 ())) < 1e-12)
    assert (gap (ref (coefficients_of (Polynomial.fit_weighted 3 (Tensor.view w) xv yv))) (ref (np_polyfit3_w ())) < 1e-12)
    assert (gap (ref (coefficients_of (Polynomial.cheb_fit 4 xv yv))) (ref (np_chebfit4 ())) < 1e-12)
    assert (gap (ref (coefficients_of (Polynomial.leg_fit 4 xv yv))) (ref (np_legfit4 ())) < 1e-12)

test "a fit of exact data gives the polynomial back" =
    let c = [0.5, -1.25, 2.0, 0.75]
    let points = new [f64](12, \i -> -1.0 + 0.2 * (i as f64))
    let values = new [f64](12, \i -> Polynomial.eval points[i] (ref c))
    let x = Tensor.of_array [12] (Array.copy (ref points))
    let y = Tensor.of_array [12] values
    assert (gap (ref (coefficients_of (Polynomial.fit 3 (Tensor.view x) (Tensor.view y)))) (ref c) < 1e-12)
    // A higher degree than the data needs adds coefficients that are zero up to rounding.
    let wider = coefficients_of (Polynomial.fit 5 (Tensor.view x) (Tensor.view y))
    assert (gap (ref wider) [0.5, -1.25, 2.0, 0.75, 0.0, 0.0] < 1e-10)
    let series = [0.5, -1.0, 2.0, 0.25]
    let cheb_values = new [f64](12, \i -> Polynomial.cheb_eval points[i] (ref series))
    let cy = Tensor.of_array [12] cheb_values
    assert (gap (ref (coefficients_of (Polynomial.cheb_fit 3 (Tensor.view x) (Tensor.view cy)))) (ref series) < 1e-12)
    let leg_values = new [f64](12, \i -> Polynomial.leg_eval points[i] (ref series))
    let ly = Tensor.of_array [12] leg_values
    assert (gap (ref (coefficients_of (Polynomial.leg_fit 3 (Tensor.view x) (Tensor.view ly)))) (ref series) < 1e-12)
    // f32 data fits to f32 accuracy.
    let narrow_x = Tensor.of_array [4] [0.0f32, 1.0f32, 2.0f32, 3.0f32]
    let narrow_y = Tensor.of_array [4] [1.0f32, 3.0f32, 5.0f32, 7.0f32]
    match Polynomial.fit 1 (Tensor.view narrow_x) (Tensor.view narrow_y) with
    | Result.Ok line -> assert (Math.abs (line[0] - 1.0f32) < 1.0e-4f32 && Math.abs (line[1] - 2.0f32) < 1.0e-4f32)
    | Result.Error _ -> assert false

test "fit errors" =
    let x = Tensor.of_array [3] [0.0, 1.0, 2.0]
    let y = Tensor.of_array [4] [0.0, 1.0, 2.0, 3.0]
    let grid = Tensor.of_array [2, 2] [0.0, 1.0, 2.0, 3.0]
    let empty: Tensor<f64> = Tensor.of_array [0] []
    let xv = Tensor.view x
    assert (fails_with (Polynomial.Error.ShapeMismatch ([3], [4])) (Polynomial.fit 1 xv (Tensor.view y)))
    assert (fails_with (Polynomial.Error.InvalidDegree (-1)) (Polynomial.fit (-1) xv xv))
    assert (fails_with (Polynomial.Error.InvalidRank 2) (Polynomial.fit 1 (Tensor.view grid) (Tensor.view y)))
    assert (fails_with (Polynomial.Error.InvalidRank 2) (Polynomial.cheb_fit 1 (Tensor.view y) (Tensor.view grid)))
    assert (fails_with (Polynomial.Error.EmptyData) (Polynomial.leg_fit 1 (Tensor.view empty) (Tensor.view empty)))
    assert (fails_with (Polynomial.Error.ShapeMismatch ([3], [4])) (Polynomial.fit_weighted 1 (Tensor.view y) xv xv))
    assert (fails_with (Polynomial.Error.InvalidRank 2) (Polynomial.fit_weighted 1 (Tensor.view grid) xv xv))
    match Polynomial.vander (-2) xv with
    | Result.Error problem -> assert (problem == Polynomial.Error.InvalidDegree (-2))
    | Result.Ok _ -> assert false
    match Polynomial.vander 2 (Tensor.view grid) with
    | Result.Error problem -> assert (problem == Polynomial.Error.InvalidRank 2)
    | Result.Ok _ -> assert false
    assert ($"{Polynomial.Error.ZeroDivisor}" == "division by the zero polynomial")
"#,
    );
}

#[test]
fn chebyshev_and_legendre_series_match_numpy_and_the_closed_forms() {
    run_tz_test(
        r#"test "cheb_eval and leg_eval against NumPy" =
    let c = cc5 ()
    let xs = xs5 ()
    let cheb = np_chebval ()
    let leg = np_legval ()
    for i in 0i64 ..< xs.length do
        assert (Math.abs (Polynomial.cheb_eval xs[i] (ref c) - cheb[i]) < 1e-13)
        assert (Math.abs (Polynomial.leg_eval xs[i] (ref c) - leg[i]) < 1e-13)

test "the closed forms of the first polynomials" =
    for x in [-1.0, -0.6, -0.25, 0.0, 0.3, 0.75, 1.0] do
        // T_n(cos t) = cos(n t)
        let t = Math.acos x
        for n in 0i64 ..< 7 do
            let unit = new [f64](n + 1, \i -> if i == n then 1.0 else 0.0)
            assert (Math.abs (Polynomial.cheb_eval x (ref unit) - Math.cos ((n as f64) * t)) < 1e-13)
        // P_2 = (3 x^2 - 1) / 2, P_3 = (5 x^3 - 3 x) / 2, P_4 = (35 x^4 - 30 x^2 + 3) / 8
        assert (Math.abs (Polynomial.leg_eval x [0.0, 0.0, 1.0] - (3.0 * x * x - 1.0) / 2.0) < 1e-14)
        assert (Math.abs (Polynomial.leg_eval x [0.0, 0.0, 0.0, 1.0] - (5.0 * x * x * x - 3.0 * x) / 2.0) < 1e-14)
        assert (Math.abs (Polynomial.leg_eval x [0.0, 0.0, 0.0, 0.0, 1.0] - (35.0 * x * x * x * x - 30.0 * x * x + 3.0) / 8.0) < 1e-14)
        assert (Polynomial.cheb_eval x [2.5] == 2.5 && Polynomial.leg_eval x [2.5] == 2.5)
        assert (Polynomial.cheb_eval x [1.0, 2.0] == 1.0 + 2.0 * x && Polynomial.leg_eval x [1.0, 2.0] == 1.0 + 2.0 * x)
    let none: [f64] = []
    assert (Polynomial.cheb_eval 0.5 (ref none) == 0.0 && Polynomial.leg_eval 0.5 (ref none) == 0.0)
"#,
    );
}
