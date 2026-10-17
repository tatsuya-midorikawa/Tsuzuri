use tsuzuri::{analyze, llvm};

fn options(wasm: bool) -> llvm::EmitOptions {
    llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm,
        debug_output: false,
        runtime_objects: true,
        wasm_simd128: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    }
}

#[test]
fn object_runtime_emission_is_deterministic_and_preserves_intrinsic_wrappers() {
    let module = analyze(
        "export def value :: f64 -> f64 = \\x -> Math.sin x + Math.sqrt x\n\
        export def shown :: i64 = (to_string 4.5).length\n",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_with_options(&module, options(wasm)).unwrap();
        assert_eq!(ir, llvm::emit_with_options(&module, options(wasm)).unwrap());
        assert!(ir.contains("declare hidden i32 @tz_soft_format("));
        assert!(ir.contains("declare hidden double @tz_math_sin_f64("));
        assert!(ir.contains("define internal noundef double @tz_math_sqrt_f64("));
        assert!(!ir.contains("define weak hidden i32 @tz_soft_format("));
        assert!(!ir.contains("define internal double @tz_math_sin_f64("));
        assert!(llvm::duplicate_declarations(&ir).is_empty());
        let embedded = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(embedded.contains("define weak hidden i32 @tz_soft_format("));
        assert!(embedded.contains("define internal double @tz_math_sin_f64("));
    }
}

#[test]
fn integer_text_needs_no_numeric_runtime_object() {
    // The integer formatter is part of the module, so the numeric object is not declared at all.
    let module = analyze("export def shown :: i64 = (to_string 42).length\n").unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_with_options(&module, options(wasm)).unwrap();
        assert!(ir.contains("define internal i32 @tz.integer.format("));
        assert!(!ir.contains("@tz_soft_"), "{ir}");
        assert!(llvm::duplicate_declarations(&ir).is_empty());
    }
}

#[test]
fn integer_runtime_never_divides_an_i128() {
    // LLVM turns an i128 division into a call of __udivti3 or a relative. On Windows x64 that call passes the operands
    // by pointer and returns in XMM0, which the helpers of runtime/wasm.ll do not follow: the digits come out wrong.
    let source = include_str!("../src/runtime/integer.ll");
    for instruction in ["udiv i128", "urem i128", "sdiv i128", "srem i128"] {
        assert!(!source.contains(instruction), "integer.ll has `{instruction}`");
    }
}

#[test]
fn object_runtimes_reject_trap_instrumentation() {
    let module = analyze("export def value :: i64 = 42\n").unwrap();
    let error = llvm::emit_with_trap_info(&module, options(false), &[]).unwrap_err();
    assert_eq!(error.code, "E2000");
    assert!(error.message.contains("external runtime objects"));
}

#[test]
fn literal_runtime_names_do_not_select_runtime_modules() {
    let module =
        analyze("export def message :: string = \"@tz_soft_op( @tz_math_sin_f64( @tz.rec.fake\"\n")
            .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_with_options(&module, options(wasm)).unwrap();
        assert!(!ir.contains("declare hidden i32 @tz_soft_format("));
        assert!(!ir.contains("declare hidden double @tz_math_sin_f64("));
        assert!(!ir.contains("define internal ptr @tz.rec."));
        assert!(llvm::duplicate_declarations(&ir).is_empty());
    }
}
