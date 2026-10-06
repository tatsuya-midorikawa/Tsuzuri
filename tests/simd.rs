use tsuzuri::{analyze, check::Type, llvm, numeric};

#[test]
fn simd_types_are_copy_values_but_not_public_abi() {
    for name in [
        "f32x4",
        "f64x2",
        "i32x4",
        "i64x2",
        "i16x8",
        "i8x16",
        "i32ux4",
        "i64ux2",
        "i16ux8",
        "i8ux16",
        "mask32x4",
        "mask64x2",
        "mask16x8",
        "mask8x16",
        "f32x8",
        "f64x4",
        "i32x8",
        "i64x4",
        "i16x16",
        "i8x32",
        "i32ux8",
        "i64ux4",
        "i16ux16",
        "i8ux32",
        "mask32x8",
        "mask64x4",
        "mask16x16",
        "mask8x32",
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
    for name in [
        "i8x8", "mask4x32", "f16x8", "f16x16", "f32x16", "f64x8", "i64x8", "i8x64", "mask8x8",
        "i32x08",
    ] {
        assert!(numeric::primitive(name).is_none(), "{name}");
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
    analyze("let mut data = [1i64, 2, 3]\nlet value: i64x2 = Simd.splat 4\nSimd.store (ref mut data[1..]) 0 value\ndata[2]")
        .unwrap();
    for source in [
        "let value: i32x4 = Simd.of_lanes2 1i32 2i32\nvalue",
        "let value: i32x4 = Simd.splat 1i32\nvalue / value",
        "let value: f32x4 = Simd.splat 1.0f32\nvalue == value",
        "let value: mask32x4 = Simd.splat true\nSimd.sum_lanes value",
        "let value: i8x32 = Simd.of_lanes16 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8 1i8\nvalue",
        "let mut data = [true, false]\nlet value: i32x4 = Simd.splat 1i32\nSimd.store (ref mut data) 0 (Simd.eq value value)\n0",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1005", "{source}");
    }
    for source in [
        "let data = [1i32, 2i32, 3i32, 4i32]\nlet value: i32x4 = Simd.splat 1i32\nSimd.store (ref data) 0 value\n0",
        "let mut data = [1i64, 2, 3, 4]\nlet value: i32x4 = Simd.splat 1i32\nSimd.store (ref mut data) 0 value\n0",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1003", "{source}");
    }
}

#[test]
fn wide_vectors_access_storage_at_most_16_byte_aligned() {
    let source = r"record Pair { tag: i64, lanes: i32x8 }
union Shape = Small of i32x4 | Wide of f64x4 | Empty
let bytes: i8x32 = Simd.of_lanes32 1i8 2i8 3i8 4i8 5i8 6i8 7i8 8i8 9i8 10i8 11i8 12i8 13i8 14i8 15i8 16i8 17i8 18i8 19i8 20i8 21i8 22i8 23i8 24i8 25i8 26i8 27i8 28i8 29i8 30i8 31i8 32i8
let source = [1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32]
let values: i32x8 = Simd.load (ref source) 0
let mut out: [i32] = [0i32, 0i32, 0i32, 0i32, 0i32, 0i32, 0i32, 0i32, 0i32]
Simd.store (ref mut out[1..]) 0 (values + values)
let pairs = [Pair { tag: 1, lanes: values }]
let shapes = [Wide (Simd.splat 1.0), Small (Simd.splat 1i32), Empty]
let read = \() -> Simd.sum_lanes values
let singles: f32x8 = Simd.splat 0.5f32
let halves: i16ux16 = Simd.splat 1i16u
let longs: i64x4 = Simd.splat 2
let mask = Simd.gt bytes (Simd.splat 0i8)
assert (Simd.all mask && Simd.sum_lanes singles == 4.0f32 && Simd.sum_lanes halves == 16i16u)
(read ()) as i64 + pairs[0].tag + Simd.sum_lanes longs + shapes.length";
    let module = analyze(source).unwrap_or_else(|error| panic!("{error:?}"));
    let wide = [
        "<32 x i8>",
        "<16 x i16>",
        "<8 x i32>",
        "<4 x i64>",
        "<8 x float>",
        "<4 x double>",
        "%tz.record.Main.Pair",
    ];
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        for ty in wide.iter().chain(&["<32 x i1>"]) {
            assert!(ir.contains(ty), "{ty}");
        }
        let mut accesses = 0;
        for line in ir.lines().map(str::trim) {
            let accessed = wide.iter().any(|ty| {
                line.starts_with(&format!("store {ty} "))
                    || line.contains(&format!(" = load {ty}, "))
            });
            if accessed {
                accesses += 1;
                assert!(
                    line.ends_with(", align 16") || line.ends_with(", align 1"),
                    "{line}"
                );
            }
        }
        assert!(accesses > 4, "{accesses}");
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}
