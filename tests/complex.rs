use std::process::Command;
use tsuzuri::{
    analyze,
    llvm::{self, Allocator, EmitOptions, Entry, F16Path},
};

fn run_native(source: &str, opt: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "tsuzuri-complex-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let main_path = dir.join("Main.tz");
    std::fs::write(&main_path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["run", main_path.to_str().unwrap(), "--no-cache", opt])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "opt={opt}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    stdout
}

#[test]
fn complex_arithmetic_smith_div_and_transcendentals() {
    let source = r#"
def approx :: f64 -> f64 -> f64 -> bool = \a b tol -> Math.abs (a - b) <= tol

def main :: unit -> i32 = \() ->
    let a = Complex.of 3.0 4.0
    let b = Complex.of 1.0 (-2.0)
    let sum = a + b
    let diff = a - b
    let prod = a * b
    let quot = Complex.div (Complex.of 1.0e200 2.0e200) (Complex.of 1.0e200 0.0)
    let quot2 = Complex.div (Complex.of 2.0e200 1.0e200) (Complex.of 0.0 1.0e200)
    let c = Complex.conj a
    let n = Complex.abs a
    let p = Complex.polar 2.0 0.0
    let sq = Complex.sqrt (Complex.of (-4.0) 0.0)
    let ex = Complex.exp (Complex.of 0.0 (Math.pi()))
    let lg = Complex.log (Complex.exp (Complex.of 1.5 0.5))
    let s = Complex.sin (Complex.of 0.5 0.25)
    let cs = Complex.cos (Complex.of 0.5 0.25)
    let id = s * s + cs * cs
    let sh = Complex.sinh (Complex.of 0.5 0.25)
    let ch = Complex.cosh (Complex.of 0.5 0.25)
    let hid = ch * ch - sh * sh
    let f32_c = Complex.of 3.0f32 4.0f32
    let f32_ok = Complex.abs f32_c == 5.0f32
    let disp_ok = $"{a}" == "3+4i" && $"{b}" == "1-2i"
    let ok =
        sum == Complex.of 4.0 2.0 &&
        diff == Complex.of 2.0 6.0 &&
        prod == Complex.of 11.0 (-2.0) &&
        quot == Complex.of 1.0 2.0 &&
        quot2 == Complex.of 1.0 (-2.0) &&
        c == Complex.of 3.0 (-4.0) &&
        n == 5.0 &&
        p == Complex.of 2.0 0.0 &&
        approx sq.re 0.0 1.0e-12 && approx sq.im 2.0 1.0e-12 &&
        approx ex.re (-1.0) 1.0e-12 && approx ex.im 0.0 1.0e-12 &&
        approx lg.re 1.5 1.0e-12 && approx lg.im 0.5 1.0e-12 &&
        approx id.re 1.0 1.0e-12 && approx id.im 0.0 1.0e-12 &&
        approx hid.re 1.0 1.0e-12 && approx hid.im 0.0 1.0e-12 &&
        f32_ok && disp_ok && Complex.is_finite a && !Complex.is_nan a
    do! IO.writeln (if ok then "PASS" else "FAIL")
    0i32
"#;
    for opt in ["-O0", "-O3"] {
        assert_eq!(run_native(source, opt).trim(), "PASS", "opt={opt}");
    }
}

#[test]
fn extended_math_and_int_builtins_accuracy_and_edges() {
    let source = r#"
def approx :: f64 -> f64 -> f64 -> bool = \a b tol -> Math.abs (a - b) <= tol

def main :: unit -> i32 = \() ->
    // Math extended Elementary (f64 & f32)
    let ok_expm1 = approx (Math.expm1 1.0e-12) 1.0000000000005e-12 1.0e-26 && Math.signbit (Math.expm1 (-0.0))
    let ok_log1p = approx (Math.log1p 1.0e-12) 9.999999999995e-13 1.0e-26 && Math.signbit (Math.log1p (-0.0))
    let ok_hyp =
        approx (Math.sinh 1.0) 1.1752011936438014 1.0e-14 &&
        approx (Math.cosh 1.0) 1.5430806348152437 1.0e-14 &&
        approx (Math.tanh 1.0) 0.7615941559557649 1.0e-14 &&
        approx (Math.asinh (Math.sinh 1.5)) 1.5 1.0e-14 &&
        approx (Math.acosh (Math.cosh 1.5)) 1.5 1.0e-14 &&
        approx (Math.atanh (Math.tanh 0.7)) 0.7 1.0e-14
    let ok_lae =
        approx (Math.logaddexp 1000.0 1000.0) (1000.0 + Math.log 2.0) 1.0e-12 &&
        approx (Math.logaddexp2 1000.0 1000.0) 1001.0 1.0e-12
    let ok_sinc =
        Math.sinc 0.0 == 1.0 &&
        Math.sinc 1.0 == 0.0 &&
        approx (Math.sinc 0.5) (2.0 / Math.pi()) 1.0e-14
    let ok_ang =
        approx (Math.degrees (Math.pi())) 180.0 1.0e-12 &&
        approx (Math.radians 180.0) (Math.pi()) 1.0e-12
    let ok_rem =
        Math.fmod (-7.0) 3.0 == -1.0 &&
        Math.floor_div (-7.0) 3.0 == -3.0 &&
        Math.floor_mod (-7.0) 3.0 == 2.0 &&
        Math.ieee_remainder (-7.0) 3.0 == -1.0 &&
        Math.ieee_remainder 3.0 2.0 == -1.0 &&
        Math.ieee_remainder 5.0 2.0 == 1.0
    let (mf_frac, mf_int) = Math.modf (-3.25)
    let ok_modf = mf_frac == -0.25 && mf_int == -3.0
    let (fr_m, fr_e) = Math.frexp 12.0
    let ok_frexp = fr_m == 0.75 && fr_e == 4i32 && Math.ldexp fr_m fr_e == 12.0
    let ok_next = Math.nextafter 1.0 2.0 > 1.0 && Math.nextafter (Math.nextafter 1.0 2.0) 0.0 == 1.0
    let ok_sign =
        Math.sign 42.0 == 1.0 &&
        Math.sign (-42.0) == -1.0 &&
        Math.sign (-0.0) == 0.0 && Math.signbit (Math.sign (-0.0)) &&
        Math.is_nan (Math.sign (0.0 / 0.0)) &&
        Math.signbit (-0.0f32) && !Math.signbit 0.0f32 &&
        Math.sign 3.0f16 == 1.0f16 && Math.signbit (-0.0f16)
    // Int gcd, lcm, checked_gcd, checked_lcm, div_floor, rem_floor
    let ok_gcd =
        Int.gcd 48 18 == 6 &&
        Int.gcd (-48) 18 == 6 &&
        Int.gcd 0 0 == 0 &&
        Int.gcd (-128i8) 0i8 == -128i8 &&
        Maybe.is_none (ref (Int.checked_gcd (-128i8) 0i8)) &&
        Maybe.get (Int.checked_gcd (-48i8) 18i8) == 6i8 &&
        Int.lcm 12 18 == 36 &&
        Int.lcm 0 5 == 0 &&
        Maybe.is_none (ref (Int.checked_lcm 100i8 99i8)) &&
        Maybe.get (Int.checked_lcm 12i8 18i8) == 36i8 &&
        Int.div_floor (-7) 3 == -3 &&
        Int.rem_floor (-7) 3 == 2 &&
        Int.div_floor 7 (-3) == -3 &&
        Int.rem_floor 7 (-3) == -2 &&
        Int.div_floor (-6) 3 == -2 &&
        Int.rem_floor (-6) 3 == 0 &&
        Int.div_floor 7i8u 3i8u == 2i8u &&
        Int.rem_floor 7i8u 3i8u == 1i8u
    let all = ok_expm1 && ok_log1p && ok_hyp && ok_lae && ok_sinc && ok_ang && ok_rem && ok_modf && ok_frexp && ok_next && ok_sign && ok_gcd
    let huge = Complex.of 1.0e308 1.0e308
    let q_fn = Complex.div huge huge
    let q_op = huge / huge
    let ok_huge = q_fn.re == 1.0 && q_fn.im == 0.0 && q_op.re == 1.0 && q_op.im == 0.0
    do! IO.writeln (if all && ok_huge then "PASS" else "FAIL")
    0i32
"#;
    for opt in ["-O0", "-O3"] {
        assert_eq!(run_native(source, opt).trim(), "PASS", "opt={opt}");
    }
}

#[test]
fn complex_is_pure_no_alloc_and_opt_in() {
    let pure_src = r#"
@no_alloc
@no_trap
pure def f :: Complex<f64> -> Complex<f64> -> f64 = \a b ->
    Complex.abs (Complex.div (a + b) (Complex.conj a))
f (Complex.of 3.0 4.0) (Complex.of 1.0 2.0)
"#;
    let m = analyze(pure_src).expect("Complex ops must be pure, @no_alloc, @no_trap");
    let wasm_ir = llvm::emit_with_options(
        &m,
        EmitOptions {
            entry: Entry::Library,
            wasm: true,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: Allocator::System,
            f16: F16Path::Soft,
        },
    )
    .unwrap();
    assert!(wasm_ir.contains("@tz_math_hypot_f64"));

    // Program not mentioning Complex does not load Complex.
    let plain = analyze("1 + 2").unwrap();
    assert!(!plain.records.iter().any(|r| r.name.contains("Complex")));
}

#[test]
fn complex_real_scalar_operators_have_eight_forms() {
    let source = r#"
def main :: unit -> i32 = \() ->
    let c = Complex.of 1.0 2.0
    let lit_line = $"{c + 0.5} {0.5 + c} {c - 0.5} {0.5 - c} {c * 2.0} {2.0 * c} {c / 2.0} {2.0 / Complex.of 1.0 1.0}"
    let cf: Complex<f64> = Complex.of 1.0 2.0
    let k: f64 = 2.0
    let var_line = $"{cf + k} {k + cf} {cf - k} {k - cf} {cf * k} {k * cf} {cf / k} {k / cf}"
    do! IO.writeln lit_line
    do! IO.writeln var_line
    0i32
"#;
    for opt in ["-O0", "-O3"] {
        let out = run_native(source, opt);
        let lines: Vec<&str> = out.trim().lines().collect();
        assert_eq!(
            lines,
            vec![
                "1.5+2i 1.5+2i 0.5+2i -0.5-2i 2+4i 2+4i 0.5+1i 1-1i",
                "3+2i 3+2i -1+2i 1-2i 2+4i 2+4i 0.5+1i 0.4-0.8i",
            ],
            "opt={opt}"
        );
    }
}

#[test]
fn complex_real_scalars_keep_signed_zero_and_infinity() {
    let source = r#"
def main :: unit -> i32 = \() ->
    let inf = 1.0 / 0.0
    let direct = $"{Complex.of 1.0 inf * 2.0} {Complex.of 1.0 (-0.0) + 2.0} {0.0 - Complex.of 1.0 0.0}"
    let promoted = $"{Complex.of 1.0 inf * Complex.from_real 2.0} {Complex.of 1.0 (-0.0) + Complex.from_real 2.0} {Complex.from_real 0.0 - Complex.of 1.0 0.0}"
    do! IO.writeln direct
    do! IO.writeln promoted
    0i32
"#;
    for opt in ["-O0", "-O3"] {
        let out = run_native(source, opt);
        let lines: Vec<&str> = out.trim().lines().collect();
        assert_eq!(
            lines,
            vec!["2+infi 3-0i -1-0i", "nan+infi 3+0i -1+0i"],
            "opt={opt}"
        );
    }
}

#[test]
fn complex_real_scalar_matches_scale_and_div() {
    let source = r#"
def main :: unit -> i32 = \() ->
    let inf = 1.0 / 0.0
    let vals = [0.0, -0.0, 1.0, -2.5, 5.0e-324, 1.0e308, inf, -inf]
    let mut ok = true
    for re in vals do
        for im in vals do
            let c = Complex.of re im
            for s in vals do
                let m1 = s * c
                let m2 = Complex.scale s c
                let d1 = s / c
                let d2 = Complex.div (Complex.from_real s) c
                if $"{m1}" != $"{m2}" || $"{d1}" != $"{d2}" then
                    ok = false
                else ()
    do! IO.writeln (if ok then "PASS" else "FAIL")
    0i32
"#;
    for opt in ["-O0", "-O3"] {
        assert_eq!(run_native(source, opt).trim(), "PASS", "opt={opt}");
    }
}

#[test]
fn complex_real_scalar_literal_types_and_rejections() {
    let ok = r#"
def main :: unit -> i32 = \() ->
    let c64: Complex<f64> = Complex.of 1.0 2.0
    let a = c64 * 2
    let b = 2 * c64
    let c32: Complex<f32> = Complex.of 1.0 2.0
    let d: Complex<f32> = c32 * 2.0
    if a == b && d == Complex.of 2.0f32 4.0f32 then 0i32 else 1i32
"#;
    analyze(ok).expect("typed Complex literal forms must check");

    let unannotated = analyze("let c = Complex.of 1.0 2.0\nc * 2\n");
    let scalar_unannotated = analyze("let c = Complex.of 1.0 2.0\nc.re * 2\n");
    assert_eq!(
        unannotated.as_ref().map(|_| ()).map_err(|e| e.code),
        scalar_unannotated.as_ref().map(|_| ()).map_err(|e| e.code),
        "unannotated c * 2 must match c.re * 2"
    );

    for bad in [
        "let c: Complex<f64> = Complex.of 1.0 2.0\nlet x: f32 = 2.0\nc * x\n",
        "let c = Complex.of 1.0 2.0\nc ** 2.0\n",
        "let c = Complex.of 1.0 2.0\nc % 2.0\n",
        "let t = Tensor.init [2] (\\_ -> Complex.of 1.0 0.0)\nt * 2.0\n",
        "let big = BigInt.of_i64 5\nlet n: i64 = 2\nbig * n\n",
    ] {
        let err = analyze(bad).expect_err(&format!("expected E1003 for:\n{bad}"));
        assert_eq!(err.code, "E1003", "{bad}: {err:?}");
    }

    // BigInt with integer literals remains valid (C6).
    analyze("let big = BigInt.of_i64 5\nlet _ = big * 2 + 3 * big\n0\n")
        .expect("BigInt * literal must stay accepted");
}

#[test]
fn complex_and_tensor_coexist_and_user_overlap_is_reported() {
    let coexist = r#"
def main :: unit -> i32 = \() ->
    let c: Complex<f64> = Complex.of 1.0 2.0
    let t: Tensor<f64> = Tensor.of_array [2] [3.0, 4.0]
    let c2 = c * 2.0 + 1.0
    let t2 = t * 2.0 + 1.0
    if c2.re == 3.0 && t2[0] == 7.0 then 0i32 else 1i32
"#;
    analyze(coexist).expect("Complex and Tensor OEIs must coexist without E1016");

    let overlap = r#"
record Money { cents: i64 }
instance Mul<Complex<Money>, Money, Complex<Money>> {
    fn mul l r = Complex { re: Money { cents: l.re.cents * r.cents }, im: Money { cents: l.im.cents * r.cents } }
}
0
"#;
    let err =
        analyze(overlap).expect_err("Complex<Money> * Money overlaps Mul<Complex<'a>, 'a, _>");
    assert_eq!(err.code, "E1016");
    assert_eq!(err.kind, Some("overlapping_instance"));
    assert_eq!(err.span.source, Some(0));
    assert_eq!(&overlap[err.span.start..err.span.end], "Mul");
    assert!(
        err.related.iter().any(|rel| {
            rel.role == tsuzuri::diagnostic::RelatedRole::DeclaredHere
                && rel.message.contains("standard library instance")
        }),
        "expected declared_here related note:\n{err:?}"
    );

    let non_overlap = r#"
record Money { cents: i64 }
instance Mul<Money, Complex<f64>, Complex<f64>> {
    fn mul l r = (l.cents as f64) * deref r
}
let m = Money { cents: 3 }
let z = m * Complex.of 2.0 4.0
0
"#;
    analyze(non_overlap).expect("Money * Complex<f64> does not overlap Mul<'a, Complex<'a>, _>");
}

#[test]
fn generic_and_pure_complex_real_scalar_across_editions() {
    use tsuzuri::syntax::Edition;

    let generic_src = r#"
def mul_right :: (Float<'a>, Mul<'a>, Copy<'a>) => Complex<'a> -> 'a -> Complex<'a>
fn mul_right c s = c * s

def mul_left :: (Float<'a>, Mul<'a>, Copy<'a>) => Complex<'a> -> 'a -> Complex<'a>
fn mul_left c s = s * c

def div_left :: (Float<'a>, Numeric<'a>, Add<'a>, Sub<'a>, Mul<'a>, Div<'a>, Ord<'a>, Copy<'a>) => Complex<'a> -> 'a -> Complex<'a>
fn div_left c s = s / c

@no_alloc
@no_trap
pure def pure_combo :: Complex<f64> -> f64 -> Complex<f64> = \c s -> c * s + s - s / c

let a64 = mul_right (Complex.of 1.0 2.0) 3.0
let b64 = mul_left (Complex.of 1.0 2.0) 3.0
let d64 = div_left (Complex.of 1.0 1.0) 2.0
let a32 = mul_right (Complex.of 1.0f32 2.0f32) 3.0f32
let b32 = mul_left (Complex.of 1.0f32 2.0f32) 3.0f32
let d32 = div_left (Complex.of 1.0f32 1.0f32) 2.0f32
let p = pure_combo a64 2.0
0
"#;
    for edition in [Edition::E2026, Edition::E2027, Edition::LATEST] {
        tsuzuri::analyze_with_edition(generic_src, edition)
            .unwrap_or_else(|e| panic!("edition {edition:?} failed: {e:?}"));
    }

    // C17 & C18 hint propagation remains intact (S7).
    let c17_c18 = r#"
def main :: unit -> i32 = \() ->
    let c: Complex<f64> = Complex.of 1.0 2.0
    let b = true
    let k_idx = 3
    let r1 = c + (if b then Complex.of 1 2 else Complex.of 3 4)
    let r2 = c + { Complex.of 1 2 }
    let r3 = c * (match k_idx with | 3 -> Complex.of 1 0 | _ -> Complex.of 0 1)
    let r4 = c + Complex { re: 1, im: 2 }
    let k: f64 = 2.0
    let r5 = k * (if b then 1 else 2)
    do! IO.writeln $"{r1} {r2} {r3} {r4} {r5}"
    0i32
"#;
    assert_eq!(run_native(c17_c18, "-O0").trim(), "2+4i 2+4i 1+2i 2+4i 2");
}

#[test]
fn complex_real_scalar_bits_native_and_wasm() {
    fn same_f64_bits(actual: u64, expected: f64) -> bool {
        if expected.is_nan() {
            f64::from_bits(actual).is_nan()
        } else {
            actual == expected.to_bits()
        }
    }
    fn same_f32_bits(actual: u32, expected: f32) -> bool {
        if expected.is_nan() {
            f32::from_bits(actual).is_nan()
        } else {
            actual == expected.to_bits()
        }
    }

    let f64_vals: [f64; 9] = [
        0.0,
        -0.0,
        1.0,
        -2.5,
        5.0e-324,
        1.0e308,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    let f32_vals: [f32; 9] = [
        0.0,
        -0.0,
        1.0,
        -2.5,
        f32::from_bits(1),
        1.0e38,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];

    // Subset of representative triples covering signed zeros, subnormals, huge values, infinities, NaN, and exact Spec 2 cases.
    let idx_triples: &[(usize, usize, usize)] = &[
        (2, 6, 3), // (1, inf) with -2.5
        (2, 1, 2), // (1, -0.0) with 1.0
        (2, 0, 0), // (1, +0.0) with +0.0 (0.0 - (1+0i) -> (-1, -0.0))
        (0, 1, 1), // (+0.0, -0.0) with -0.0
        (1, 0, 0), // (-0.0, +0.0) with +0.0
        (4, 5, 3), // (subnormal, huge) with -2.5
        (5, 4, 5), // (huge, subnormal) with huge
        (6, 7, 0), // (inf, -inf) with +0.0
        (8, 2, 3), // (NaN, 1.0) with -2.5
        (2, 3, 8), // (1.0, -2.5) with NaN
        (2, 2, 3), // (1.0, 1.0) with -2.5 (exact s / c)
        (0, 2, 2), // (0.0, 1.0) with 1.0 (1 / i = -i)
    ];

    let program = r#"
def val64 :: i64 -> f64 = \idx ->
    match idx with
    | 0 -> 0.0
    | 1 -> -0.0
    | 2 -> 1.0
    | 3 -> -2.5
    | 4 -> 5.0e-324
    | 5 -> 1.0e308
    | 6 -> 1.0 / 0.0
    | 7 -> -1.0 / 0.0
    | _ -> 0.0 / 0.0

def val32 :: i64 -> f32 = \idx ->
    match idx with
    | 0 -> 0.0f32
    | 1 -> -0.0f32
    | 2 -> 1.0f32
    | 3 -> -2.5f32
    | 4 -> Math.ldexp 1.0f32 (-149i32)
    | 5 -> 1.0e38f32
    | 6 -> 1.0f32 / 0.0f32
    | 7 -> -1.0f32 / 0.0f32
    | _ -> 0.0f32 / 0.0f32

export def op64 :: i64 -> i64 -> i64 -> i64 -> Complex<f64> = \kind ri ii si ->
    let c: Complex<f64> = Complex.of (val64 ri) (val64 ii)
    let s: f64 = val64 si
    match kind with
    | 0 -> c + s
    | 1 -> s + c
    | 2 -> c - s
    | 3 -> s - c
    | 4 -> c * s
    | 5 -> s * c
    | 6 -> c / s
    | 7 -> s / c
    | _ -> Complex.div (Complex.from_real s) c

export def op32 :: i64 -> i64 -> i64 -> i64 -> Complex<f32> = \kind ri ii si ->
    let c: Complex<f32> = Complex.of (val32 ri) (val32 ii)
    let s: f32 = val32 si
    match kind with
    | 0 -> c + s
    | 1 -> s + c
    | 2 -> c - s
    | 3 -> s - c
    | 4 -> c * s
    | 5 -> s * c
    | 6 -> c / s
    | 7 -> s / c
    | _ -> Complex.div (Complex.from_real s) c

def fmt64 :: f64 -> string = \v ->
    if Math.is_nan v then "nan"
    elif v == 0.0 && Math.signbit v then "-0.0"
    else $"{v}"

def fmt32 :: f32 -> string = \v ->
    if Math.is_nan v then "nan"
    elif v == 0.0f32 && Math.signbit v then "-0.0"
    else $"{v}"

def main :: unit -> i32 = \() ->
    let triples = [(2, 6, 3), (2, 1, 2), (2, 0, 0), (0, 1, 1), (1, 0, 0), (4, 5, 3), (5, 4, 5), (6, 7, 0), (8, 2, 3), (2, 3, 8), (2, 2, 3), (0, 2, 2)]
    for (ri, ii, si) in triples do
        for kind = 0 to 8 do
            let z64 = op64 kind ri ii si
            let z32 = op32 kind ri ii si
            do! IO.writeln $"{fmt64 z64.re} {fmt64 z64.im} {fmt32 z32.re} {fmt32 z32.im}"
    0i32
"#;

    let parse_f64 = |tok: &str| -> u64 {
        match tok {
            "nan" => f64::NAN.to_bits(),
            "inf" => f64::INFINITY.to_bits(),
            "-inf" => f64::NEG_INFINITY.to_bits(),
            "-0.0" | "-0" => (-0.0f64).to_bits(),
            _ => tok.parse::<f64>().expect(tok).to_bits(),
        }
    };
    let parse_f32 = |tok: &str| -> u32 {
        match tok {
            "nan" => f32::NAN.to_bits(),
            "inf" => f32::INFINITY.to_bits(),
            "-inf" => f32::NEG_INFINITY.to_bits(),
            "-0.0" | "-0" => (-0.0f32).to_bits(),
            _ => tok.parse::<f32>().expect(tok).to_bits(),
        }
    };

    let expected64 = |kind: usize, re: f64, im: f64, s: f64| -> Option<(f64, f64)> {
        Some(match kind {
            0 => (re + s, im),
            1 => (s + re, im),
            2 => (re - s, im),
            3 => (s - re, -im),
            4 => (re * s, im * s),
            5 => (s * re, s * im),
            6 => (re / s, im / s),
            _ => return None,
        })
    };
    let expected32 = |kind: usize, re: f32, im: f32, s: f32| -> Option<(f32, f32)> {
        Some(match kind {
            0 => (re + s, im),
            1 => (s + re, im),
            2 => (re - s, im),
            3 => (s - re, -im),
            4 => (re * s, im * s),
            5 => (s * re, s * im),
            6 => (re / s, im / s),
            _ => return None,
        })
    };

    let dir = std::env::temp_dir().join(format!("tsuzuri-nc21-bits-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let native_dir = dir.join("native");
    let lib_dir = dir.join("wasm_lib");
    std::fs::create_dir_all(&native_dir).unwrap();
    std::fs::create_dir_all(&lib_dir).unwrap();
    let main_tz = native_dir.join("Main.tz");
    std::fs::write(&main_tz, program).unwrap();
    let wasm_lib_src = program.split("def fmt64").next().unwrap();
    let lib_tz = lib_dir.join("Main.tz");
    std::fs::write(&lib_tz, wasm_lib_src).unwrap();
    let bin = env!("CARGO_BIN_EXE_tsuzuri");

    for opt in ["-O0", "-O3"] {
        let native = Command::new(bin)
            .args(["run", "--no-cache", opt])
            .arg(&main_tz)
            .output()
            .unwrap();
        assert!(
            native.status.success(),
            "native {opt} failed:\n{}",
            String::from_utf8_lossy(&native.stderr)
        );
        let native_stdout = String::from_utf8_lossy(&native.stdout);
        let mut lines = native_stdout.lines();

        for &(ri, ii, si) in idx_triples {
            let (re64, im64, s64) = (f64_vals[ri], f64_vals[ii], f64_vals[si]);
            let (re32, im32, s32) = (f32_vals[ri], f32_vals[ii], f32_vals[si]);
            let mut s_div_c64 = (0u64, 0u64);
            let mut s_div_c32 = (0u32, 0u32);
            for kind in 0..=8 {
                let parts: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
                let (got64_re, got64_im) = (parse_f64(parts[0]), parse_f64(parts[1]));
                let (got32_re, got32_im) = (parse_f32(parts[2]), parse_f32(parts[3]));
                if let Some((exp_re, exp_im)) = expected64(kind, re64, im64, s64) {
                    assert!(
                        same_f64_bits(got64_re, exp_re) && same_f64_bits(got64_im, exp_im),
                        "native f64 {opt} kind={kind} ({ri},{ii},{si}): got ({:#x},{:#x}), expected ({exp_re:?},{exp_im:?})",
                        got64_re,
                        got64_im
                    );
                } else if kind == 7 {
                    s_div_c64 = (got64_re, got64_im);
                } else {
                    assert!(
                        same_f64_bits(s_div_c64.0, f64::from_bits(got64_re))
                            && same_f64_bits(s_div_c64.1, f64::from_bits(got64_im)),
                        "native f64 s/c != Complex.div {opt}"
                    );
                }
                if let Some((exp_re, exp_im)) = expected32(kind, re32, im32, s32) {
                    assert!(
                        same_f32_bits(got32_re, exp_re) && same_f32_bits(got32_im, exp_im),
                        "native f32 {opt} kind={kind} ({ri},{ii},{si})"
                    );
                } else if kind == 7 {
                    s_div_c32 = (got32_re, got32_im);
                } else {
                    assert!(
                        same_f32_bits(s_div_c32.0, f32::from_bits(got32_re))
                            && same_f32_bits(s_div_c32.1, f32::from_bits(got32_im)),
                        "native f32 s/c != Complex.div {opt}"
                    );
                }
            }
            if (ri, ii, si) == (2, 2, 3) {
                // -2.5 / (1 + 1i) = -1.25 + 1.25i
                assert!(same_f64_bits(s_div_c64.0, -1.25) && same_f64_bits(s_div_c64.1, 1.25));
                assert!(same_f32_bits(s_div_c32.0, -1.25) && same_f32_bits(s_div_c32.1, 1.25));
            }
            if (ri, ii, si) == (0, 2, 2) {
                // 1.0 / (0 + 1i) = 0 - 1i
                assert!(f64::from_bits(s_div_c64.0) == 0.0 && same_f64_bits(s_div_c64.1, -1.0));
                assert!(f32::from_bits(s_div_c32.0) == 0.0 && same_f32_bits(s_div_c32.1, -1.0));
            }
        }

        // WASM module with exported op64/op32 (no console IO entry so imports == 0): read raw IEEE bits via DataView.
        let wasm_lib_src = program.split("def fmt64").next().unwrap();
        let lib_dir = dir.join("wasm_lib");
        std::fs::create_dir_all(&lib_dir).unwrap();
        let lib_tz = lib_dir.join("Main.tz");
        std::fs::write(&lib_tz, wasm_lib_src).unwrap();
        let wasm_path = dir.join(format!("nc21{opt}.wasm"));
        let build = Command::new(bin)
            .args(["build", "--target", "wasm32", "--no-cache", opt, "-o"])
            .arg(&wasm_path)
            .arg(&lib_tz)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "wasm build {opt} failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let js = r#"
const fs = require("node:fs");
const bytes = fs.readFileSync(process.argv[1]);
const mod = new WebAssembly.Module(bytes);
if (WebAssembly.Module.imports(mod).length !== 0) process.exit(2);
const inst = new WebAssembly.Instance(mod).exports;
const out = Number(inst.tsuzuri_alloc(16n)) >>> 0;
const triples = [[2,6,3],[2,1,2],[2,0,0],[0,1,1],[1,0,0],[4,5,3],[5,4,5],[6,7,0],[8,2,3],[2,3,8],[2,2,3],[0,2,2]];
const lines = [];
for (const [ri, ii, si] of triples) {
  for (let kind = 0n; kind <= 8n; kind++) {
    inst.tz_op64(out, kind, BigInt(ri), BigInt(ii), BigInt(si));
    let dv = new DataView(inst.memory.buffer, out, 16);
    const r64 = dv.getBigUint64(0, true).toString(16);
    const i64 = dv.getBigUint64(8, true).toString(16);
    inst.tz_op32(out, kind, BigInt(ri), BigInt(ii), BigInt(si));
    dv = new DataView(inst.memory.buffer, out, 8);
    const r32 = dv.getUint32(0, true).toString(16);
    const i32 = dv.getUint32(4, true).toString(16);
    lines.push(`${r64} ${i64} ${r32} ${i32}`);
  }
}
inst.tsuzuri_free(out);
process.stdout.write(lines.join("\n") + "\n");
"#;
        let node_run = Command::new("node")
            .args(["-e", js])
            .arg(&wasm_path)
            .output()
            .unwrap();
        assert!(
            node_run.status.success(),
            "wasm node run {opt} failed (code {:?}):\n{}",
            node_run.status.code(),
            String::from_utf8_lossy(&node_run.stderr)
        );
        let wasm_stdout = String::from_utf8_lossy(&node_run.stdout);
        let mut wlines = wasm_stdout.lines();
        for &(ri, ii, si) in idx_triples {
            let (re64, im64, s64) = (f64_vals[ri], f64_vals[ii], f64_vals[si]);
            let (re32, im32, s32) = (f32_vals[ri], f32_vals[ii], f32_vals[si]);
            let mut s_div_c64 = (0u64, 0u64);
            let mut s_div_c32 = (0u32, 0u32);
            for kind in 0..=8 {
                let parts: Vec<&str> = wlines.next().unwrap().split_whitespace().collect();
                let got64_re = u64::from_str_radix(parts[0], 16).unwrap();
                let got64_im = u64::from_str_radix(parts[1], 16).unwrap();
                let got32_re = u32::from_str_radix(parts[2], 16).unwrap();
                let got32_im = u32::from_str_radix(parts[3], 16).unwrap();
                if let Some((exp_re, exp_im)) = expected64(kind, re64, im64, s64) {
                    assert!(
                        same_f64_bits(got64_re, exp_re) && same_f64_bits(got64_im, exp_im),
                        "wasm f64 {opt} kind={kind} ({ri},{ii},{si})"
                    );
                } else if kind == 7 {
                    s_div_c64 = (got64_re, got64_im);
                } else {
                    assert!(
                        same_f64_bits(s_div_c64.0, f64::from_bits(got64_re))
                            && same_f64_bits(s_div_c64.1, f64::from_bits(got64_im)),
                        "wasm f64 s/c != Complex.div {opt}"
                    );
                }
                if let Some((exp_re, exp_im)) = expected32(kind, re32, im32, s32) {
                    assert!(
                        same_f32_bits(got32_re, exp_re) && same_f32_bits(got32_im, exp_im),
                        "wasm f32 {opt} kind={kind} ({ri},{ii},{si})"
                    );
                } else if kind == 7 {
                    s_div_c32 = (got32_re, got32_im);
                } else {
                    assert!(
                        same_f32_bits(s_div_c32.0, f32::from_bits(got32_re))
                            && same_f32_bits(s_div_c32.1, f32::from_bits(got32_im)),
                        "wasm f32 s/c != Complex.div {opt}"
                    );
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
