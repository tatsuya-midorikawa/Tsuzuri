use tsuzuri::{analyze, llvm};

#[test]
fn basic_math_preserves_each_float_type_and_old_signatures() {
    for ty in ["f16", "f32", "f64", "f128", "d32", "d64", "d128"] {
        let mut source = String::new();
        for name in [
            "sqrt",
            "floor",
            "ceil",
            "trunc",
            "round",
            "round_even",
            "abs",
        ] {
            source.push_str(&format!(
                "def {name}_value :: {ty} -> {ty}\nfn {name}_value value = Math.{name} value\n"
            ));
        }
        for name in ["min", "max", "copysign"] {
            source.push_str(&format!("def {name}_value :: {ty} -> {ty} -> {ty}\nfn {name}_value left right = Math.{name} left right\n"));
        }
        for name in ["is_nan", "is_infinite", "is_finite"] {
            source.push_str(&format!(
                "def {name}_value :: {ty} -> bool\nfn {name}_value value = Math.{name} value\n"
            ));
        }
        source.push_str(&format!("def clamp_value :: {ty} -> {ty} -> {ty} -> {ty}\nfn clamp_value value low high = Math.clamp value low high\ndef fma_value :: {ty} -> {ty} -> {ty} -> {ty}\nfn fma_value left right addend = Math.fma left right addend\ndef pi_value :: {ty}\nfn pi_value = Math.pi()\ndef e_value :: {ty}\nfn e_value = Math.e()"));
        let module = analyze(&source).unwrap_or_else(|error| panic!("{ty}: {error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
            assert!(!ir.contains("@llvm.round.") && !ir.contains("@llvm.rint."));
            if wasm {
                assert!(!ir.contains("@llvm.fma."));
                assert!(ir.contains("call void @tz_soft_fma("));
            }
        }
    }
    analyze("sqrt 4.0 + floor 1.5 + ceil 1.5 + abs (-1.0)").unwrap();
    for name in ["sqrt", "floor", "ceil", "abs"] {
        let legacy = analyze(&format!(
            "def value :: f64 -> f64\nfn value input = {name} input"
        ))
        .unwrap();
        let modern = analyze(&format!(
            "def value :: f64 -> f64\nfn value input = Math.{name} input"
        ))
        .unwrap();
        let legacy = llvm::emit(&legacy, llvm::Entry::Library).unwrap();
        let modern = llvm::emit(&modern, llvm::Entry::Library).unwrap();
        let legacy = legacy.replace(
            &format!("$builtin.{name}"),
            &format!("$builtin.Math.{name}.1"),
        );
        assert_eq!(
            legacy.replace(
                &format!("@tz.builtin.{name}"),
                &format!("@tz.builtin.Math.{name}.f64")
            ),
            modern
        );
    }
    assert_eq!(analyze("Math.sqrt 4i64").unwrap_err().code, "E1005");
    assert_eq!(analyze("Math.fma 1 2 3").unwrap_err().code, "E1005");
    assert_eq!(
        analyze("Math.fma 1.0f64 2.0f32 3.0f64").unwrap_err().code,
        "E1003"
    );
    assert_eq!(
        analyze("let value: f64 = Math.pi\nvalue").unwrap_err().code,
        "E1003"
    );
}

#[test]
fn elementary_math_is_portable_and_accepts_only_f32_f64() {
    for ty in ["f32", "f64"] {
        let mut source = String::new();
        for operation in [
            "sin", "cos", "tan", "asin", "acos", "atan", "exp", "exp2", "log", "log2", "log10",
            "cbrt",
        ] {
            source.push_str(&format!("def {operation}_value :: {ty} -> {ty}\nfn {operation}_value value = Math.{operation} value\n"));
        }
        for operation in ["pow", "atan2", "hypot"] {
            source.push_str(&format!("def {operation}_value :: {ty} -> {ty} -> {ty}\nfn {operation}_value left right = Math.{operation} left right\n"));
        }
        let module = analyze(&source).unwrap();
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        assert!(ir.contains(&format!("@tz_math_sin_{ty}")));
        for forbidden in [
            "@sin(",
            "@cos(",
            "@pow(",
            "@log(",
            "@llvm.sin.",
            "llvm.fmuladd",
            "llvm.fma.",
            "x86_fp80",
            " contract ",
            " fast ",
            " reassoc ",
        ] {
            assert!(!ir.contains(forbidden), "{forbidden}");
        }
    }
    for ty in ["f16", "f128", "d32", "d64", "d128", "i64", "bool"] {
        let error = analyze(&format!(
            "def sine :: {ty} -> {ty}\nfn sine value = Math.sin value"
        ))
        .unwrap_err();
        assert_eq!(error.code, "E1005");
        assert!(error.message.contains("Elementary"));
    }
    let module = analyze("Math.sqrt 4.0 + 1.0f64").unwrap();
    assert!(
        !llvm::emit(&module, llvm::Entry::Library)
            .unwrap()
            .contains("@tz_math_")
    );
}
