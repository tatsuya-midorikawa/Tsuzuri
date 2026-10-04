use tsuzuri::{analyze, check::Type, llvm, numeric};

#[test]
fn simd_types_are_copy_values_but_not_public_abi() {
    for name in [
        "f32x4", "f64x2", "i32x4", "i64x2", "i16x8", "i8x16", "i32ux4", "i64ux2", "i16ux8",
        "i8ux16", "mask32x4", "mask64x2", "mask16x8", "mask8x16",
    ] {
        let ty = numeric::primitive(name).unwrap();
        assert!(matches!(ty, Type::Simd(_)));
        let source = format!("fn identity(value: {name}) -> {name} {{ value }}");
        let module = analyze(&source).unwrap();
        assert!(ty.is_copy(&module.types()));
        assert!(!ty.needs_drop(&module.types()));
        assert_eq!(ty.display(&module.types()), name);
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
        assert_eq!(
            analyze(&format!("export {source}")).unwrap_err().code,
            "E1008"
        );
    }
    for name in ["f32x8", "i8x8", "mask4x32", "f16x8"] {
        assert!(numeric::primitive(name).is_none());
    }
}

#[test]
fn simd_operations_use_vectors_masks_and_guarded_access() {
    for (name, scalar, literal) in [
        ("i32x4", "i32", "1i32"),
        ("f32x4", "f32", "1.0f32"),
        ("i8ux16", "ubyte", "1ubyte"),
        ("f64x2", "f64", "1.0"),
    ] {
        let source = format!(
            "fn add(left: {name}, right: {name}) -> {name} {{ left + right }}\nlet first: {name} = Simd.splat {literal}\nlet second = Simd.replace first 0 {literal}\nlet mask = Simd.eq first second\nassert (Simd.all mask)\nlet result = Simd.select mask (add first second) first\nlet total: {scalar} = Simd.sum_lanes result\ntotal"
        );
        let module = analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert!(
                ir.contains("insertelement")
                    && ir.contains("extractelement")
                    && ir.contains("select <")
            );
            assert!(!ir.contains("fadd fast") && !ir.contains("add nsw <"));
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
    analyze("let data = [1i64, 2]\nlet value: i64x2 = Simd.load (&data) 0\nSimd.extract value 1")
        .unwrap();
    analyze(
        "let value: i32x4 = Simd.of_lanes4 1i32 2i32 3i32 4i32\nSimd.sum_lanes ((~value) <<< value)",
    )
    .unwrap();
    for source in [
        "let value: i32x4 = Simd.of_lanes2 1i32 2i32\nvalue",
        "let value: i32x4 = Simd.splat 1i32\nvalue / value",
        "let value: f32x4 = Simd.splat 1.0f32\nvalue == value",
        "let value: mask32x4 = Simd.splat true\nSimd.sum_lanes value",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1005", "{source}");
    }
}
