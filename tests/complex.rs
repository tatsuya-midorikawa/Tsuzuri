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
    do! IO.writeln (if all then "PASS" else "FAIL")
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
