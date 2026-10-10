//! SPIR-V emission for Vulkan (F09 Phase 3): the structure and determinism of the modules, `spirv-val` for every kernel
//! shape, the rejections that keep the strict float contract, and the execution of the modules on a Vulkan device
//! through the runtime harness (`tests/gpu_vulkan_runtime.c`, which includes `src/runtime/gpu-vulkan.c`).
//!
//! Tests that need a tool or a device print a loud `SKIPPED` line and pass; set `TSUZURI_REQUIRE_SPIRV_TOOLS=1` or
//! `TSUZURI_REQUIRE_VULKAN=1` to make such a skip a failure. Strict `f32` kernels execute only on a device that reports
//! the float controls (the others are checked for the `Unavailable` path, and run once uncertified for information).

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use tsuzuri::{
    analyze,
    check::CheckedModule,
    gpu::{self, FEATURE_INT64, FEATURE_STRICT_FLOAT, Lane, SpirvKernel},
};

fn kernel_id(module: &CheckedModule) -> usize {
    module
        .functions
        .iter()
        .position(|function| function.qualified_name() == "Main.kernel")
        .unwrap()
}

fn emit(source: &str, relaxed: bool) -> Result<SpirvKernel, tsuzuri::diagnostic::Diagnostic> {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let kernel = gpu::extract_kernel(&module, kernel_id(&module)).unwrap();
    if relaxed {
        kernel.spirv_relaxed()
    } else {
        kernel.spirv()
    }
}

fn emit_ok(source: &str, relaxed: bool) -> SpirvKernel {
    emit(source, relaxed)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

// ---- the kernels and their reference semantics ----

#[derive(Clone, Copy, PartialEq)]
enum Compare {
    /// Bit for bit; NaN equals NaN.
    Exact,
    /// Within that many ULPs of the reference (relaxed `f32` kernels, D6 of the F09 ticket).
    Ulps(f32),
}

struct Case {
    name: &'static str,
    source: &'static str,
    relaxed: bool,
    inputs: fn() -> Vec<u64>,
    reference: fn(u64) -> u64,
    compare: Compare,
    /// Whether the input lane is an `i32` (the module has `init_main`).
    init: bool,
}

fn lcg(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    *seed
}

fn ints32() -> Vec<u64> {
    let mut values: Vec<u32> = vec![
        0,
        1,
        2,
        3,
        4,
        5,
        99,
        100,
        255,
        256,
        257,
        (-1i32) as u32,
        (-2i32) as u32,
        (-3i32) as u32,
        i32::MIN as u32,
        i32::MAX as u32,
        0x8000_0001,
    ];
    let mut seed = 0x1234_5678;
    values.extend((0..683).map(|_| lcg(&mut seed)));
    values.into_iter().map(u64::from).collect()
}

fn ints64() -> Vec<u64> {
    let mut values: Vec<u64> = vec![
        0,
        1,
        5,
        u64::MAX,
        1 << 63,
        (1 << 63) - 1,
        0xFFFF_FFFF,
        0x1_0000_0000,
    ];
    let mut seed = 0x9E37_79B9;
    for _ in 0..800 {
        let high = u64::from(lcg(&mut seed));
        let low = u64::from(lcg(&mut seed));
        values.push(high << 32 | low);
    }
    values
}

fn float_bits(values: &[f32]) -> Vec<u64> {
    values
        .iter()
        .map(|value| u64::from(value.to_bits()))
        .collect()
}

/// Positive normal values in `[2^-8, 2^8)`: no cancellation, so ULP tolerances are meaningful (D6).
fn positive_floats() -> Vec<u64> {
    let mut seed = 0x0BAD_CAFE;
    let mut values = vec![2f32.powi(-8), 0.5, 1.0, 1.5, 2.0, 3.0, 255.0, 255.99];
    values.extend((0..700).map(|_| {
        let fraction = (lcg(&mut seed) >> 8) as f32 / (1u32 << 24) as f32;
        (2f32.powf(fraction * 16.0 - 8.0)).min(255.99)
    }));
    float_bits(&values)
}

/// Values around the casts and the special values of IEEE 754, without subnormals (which the float controls of the
/// device decide).
fn special_floats() -> Vec<u64> {
    let mut values = vec![
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.0,
        -1.0,
        1.5,
        -1.5,
        2.5,
        0.999_999_94,
        2_147_483_520.0,
        2_147_483_648.0,
        -2_147_483_648.0,
        -2_147_483_904.0,
        4_294_967_040.0,
        4_294_967_296.0,
        -4_294_967_296.0,
        9.223_371_5e18,
        9.223_372e18,
        -9.223_372e18,
        -9.223_373e18,
        1.844_674_3e19,
        3.0e38,
        -3.0e38,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        -f32::NAN,
        16_777_216.0,
        16_777_217.0,
        1.0e-30,
    ];
    let mut seed = 0x5EED_5EED;
    values.extend((0..500).map(|_| {
        let bits = lcg(&mut seed);
        let exponent = ((bits >> 23) % 80) as i32 - 40;
        let magnitude = f32::from_bits((bits & 0x007F_FFFF) | (((127 + exponent) as u32) << 23));
        if bits >> 31 == 1 {
            -magnitude
        } else {
            magnitude
        }
    }));
    float_bits(&values)
}

fn f(bits: u64) -> f32 {
    f32::from_bits(bits as u32)
}

/// Values around 1.0 whose square loses low bits: a fused multiply-add keeps them, two roundings do not.
fn near_one_floats() -> Vec<u64> {
    let mut seed = 0x00F1_5ED0;
    let mut values = vec![
        1.0f32,
        1.0 + 2f32.powi(-12),
        1.0 - 2f32.powi(-12),
        1.0 + 2f32.powi(-23),
    ];
    values.extend((0..600).map(|_| {
        let offset = (lcg(&mut seed) >> 9) as f32 / (1u32 << 23) as f32 - 0.5;
        1.0 + offset * 2f32.powi(-10)
    }));
    float_bits(&values)
}

/// Subnormal values and the smallest normal: the device decides (`DenormPreserve`) whether they survive.
fn denormal_floats() -> Vec<u64> {
    let mut values = vec![
        1u32,
        2,
        3,
        0x007F_FFFF,
        0x0040_0000,
        0x0000_1234,
        0x8000_0001,
        0x807F_FFFF,
    ];
    let mut seed = 0xDE40_0123;
    values.extend((0..200).map(|_| (lcg(&mut seed) & 0x807F_FFFF) | 1));
    values.into_iter().map(u64::from).collect()
}

fn bits_of(value: f32) -> u64 {
    u64::from(value.to_bits())
}

/// Unsigned 32-bit values whose conversion to `f32` must round to nearest even: the spacing of `f32` is 256 above 2^31,
/// so the values just below 2^32 and around 2^31, and the ties above 2^24, are where a device that rounds the
/// conversion wrongly differs (a driver rounded 4294967167 up to 4294967296 while the CPU gives 4294967040).
fn large_u32s() -> Vec<u64> {
    let mut values: Vec<u32> = (0xFFFF_FF00..=0xFFFF_FFFF).collect();
    values.extend(0x7FFF_FF80..=0x8000_0080);
    values.extend([
        0x0100_0001,
        0x0100_0003,
        0x0200_0001,
        0x0200_0002,
        0x0200_0003,
    ]);
    let mut seed = 0x0BAD_5EED;
    values.extend((0..300).map(|_| lcg(&mut seed) | 0x8000_0000));
    values.into_iter().map(u64::from).collect()
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "mix",
            source: "export def kernel :: i32 -> i32\nfn kernel value = (value * 1664525 + 1013904223) ^ (Bits.ushr value 13)",
            relaxed: false,
            inputs: ints32,
            reference: |x| {
                let v = x as u32;
                u64::from(v.wrapping_mul(1664525).wrapping_add(1013904223) ^ (v >> 13))
            },
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "cast",
            source: "export def kernel :: i32 -> i32\nfn kernel value = ((value as i32u) + 4294967295i32u) as i32",
            relaxed: false,
            inputs: ints32,
            reference: |x| u64::from((x as u32).wrapping_sub(1)),
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "locals_calls_shifts",
            source: "def square :: i32 -> i32\nfn square value = value * value\n\ndef twice :: i32 -> i32\nfn twice value = square (value + 1) + square (value - 1)\n\nexport def kernel :: i32 -> i32\nfn kernel value = {\n    let mut current = value;\n    let before = current;\n    current = current + 1;\n    let a = if before < 0 && (value & 1) == 0 then 1 else (if value >= 100 || value == -3 then 2 else 3);\n    let b = (value <<< (value & 63)) + (value >>> 2) + (Bits.ushr value 5);\n    let c = (-value) ^ (~~~value);\n    twice (current ^ a) + b + c\n}",
            relaxed: false,
            inputs: ints32,
            reference: |x| {
                let v = x as u32 as i32;
                let current = v.wrapping_add(1);
                let a = if v < 0 && (v & 1) == 0 {
                    1
                } else if v >= 100 || v == -3 {
                    2
                } else {
                    3
                };
                let b = v
                    .wrapping_shl((v & 63) as u32)
                    .wrapping_add(v >> 2)
                    .wrapping_add(((v as u32) >> 5) as i32);
                let c = v.wrapping_neg() ^ !v;
                let square = |t: i32| t.wrapping_mul(t);
                let argument = current ^ a;
                let result = square(argument.wrapping_add(1))
                    .wrapping_add(square(argument.wrapping_sub(1)))
                    .wrapping_add(b)
                    .wrapping_add(c);
                u64::from(result as u32)
            },
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "unsigned32",
            source: "export def kernel :: i32u -> i32u\nfn kernel value = ((value * 2654435761u) >>> 7u) ^ value",
            relaxed: false,
            inputs: ints32,
            reference: |x| {
                let v = x as u32;
                u64::from((v.wrapping_mul(2654435761) >> 7) ^ v)
            },
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "mutable_parameters",
            source: "def bump :: i32 -> i32\nfn bump mut x = { x = x + 1; x * 2 }\ndef toggle :: bool -> bool\nfn toggle mut flag = { flag = !flag; flag }\ndef wrap :: i32u -> i32u\nfn wrap mut x = { x = x * 3i32u; x ^ 7i32u }\nexport def kernel :: i32 -> i32\nfn kernel value = if toggle (value > 0) then bump value else (wrap (value as i32u)) as i32",
            relaxed: false,
            inputs: ints32,
            reference: |x| {
                let v = x as u32 as i32;
                if v <= 0 {
                    u64::from(v.wrapping_add(1).wrapping_mul(2) as u32)
                } else {
                    u64::from((v as u32).wrapping_mul(3) ^ 7)
                }
            },
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "mix64",
            source: "def helper :: i64 -> i64\nfn helper value = value * 6364136223846793005l + 1442695040888963407l\n\nexport def kernel :: i64 -> i64\nfn kernel value = {\n    let mut state = helper value;\n    state = state ^ (state >>> 13l);\n    state = state ^ (state <<< 7l);\n    if state < 0l && value != 5l then -state else (Bits.ushr state 3l)\n}",
            relaxed: false,
            inputs: ints64,
            reference: |x| {
                let v = x as i64;
                let mut state = v
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                state ^= state >> 13;
                state ^= state.wrapping_shl(7);
                if state < 0 && v != 5 {
                    state.wrapping_neg() as u64
                } else {
                    (state as u64) >> 3
                }
            },
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "widen",
            source: "export def kernel :: i32 -> i64\nfn kernel value = ((value as i64) * 3000000000l) + ((value as i32u) as i64)",
            relaxed: false,
            inputs: ints32,
            reference: |x| {
                let v = x as u32 as i32;
                (i64::from(v).wrapping_mul(3_000_000_000)).wrapping_add(i64::from(v as u32)) as u64
            },
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "narrow",
            source: "export def kernel :: i64 -> i32\nfn kernel value = (value >>> 17l) as i32",
            relaxed: false,
            inputs: ints64,
            reference: |x| u64::from(((x as i64) >> 17) as u32),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "unsigned64",
            source: "export def kernel :: i64u -> i64u\nfn kernel value = (value * 6364136223846793005ul) ^ (value >>> 29ul)",
            relaxed: false,
            inputs: ints64,
            reference: |x| x.wrapping_mul(6364136223846793005) ^ (x >> 29),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "poly_strict",
            source: "export def kernel :: f32 -> f32\nfn kernel value = value * value + value",
            relaxed: false,
            inputs: special_floats,
            reference: |x| bits_of(f(x) * f(x) + f(x)),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "negate_compare_strict",
            source: "export def kernel :: f32 -> i32\nfn kernel value = if (-value) < 1.0 then 1 else (if value == value then (if value != 0.0 then 2 else 3) else 4)",
            relaxed: false,
            inputs: special_floats,
            reference: |x| {
                let v = f(x);
                u64::from(if -v < 1.0 {
                    1u32
                } else if !v.is_nan() {
                    if v != 0.0 { 2 } else { 3 }
                } else {
                    4
                })
            },
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "to_i32_strict",
            source: "export def kernel :: f32 -> i32\nfn kernel value = value as i32",
            relaxed: false,
            inputs: special_floats,
            reference: |x| u64::from(f(x) as i32 as u32),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "to_u32_strict",
            source: "export def kernel :: f32 -> i32u\nfn kernel value = value as i32u",
            relaxed: false,
            inputs: special_floats,
            reference: |x| u64::from(f(x) as u32),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "to_i64_strict",
            source: "export def kernel :: f32 -> i64\nfn kernel value = value as i64",
            relaxed: false,
            inputs: special_floats,
            reference: |x| f(x) as i64 as u64,
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "to_u64_strict",
            source: "export def kernel :: f32 -> i64u\nfn kernel value = value as i64u",
            relaxed: false,
            inputs: special_floats,
            reference: |x| f(x) as u64,
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "from_i32_strict",
            source: "export def kernel :: i32 -> f32\nfn kernel value = (value as f32) * 0.5 + 0.25",
            relaxed: false,
            inputs: ints32,
            reference: |x| bits_of((x as u32 as i32 as f32) * 0.5 + 0.25),
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "from_i64_strict",
            source: "export def kernel :: i64 -> f32\nfn kernel value = value as f32",
            relaxed: false,
            inputs: ints64,
            reference: |x| bits_of(x as i64 as f32),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "from_u32_strict",
            source: "export def kernel :: i32u -> f32\nfn kernel value = value as f32",
            relaxed: false,
            inputs: large_u32s,
            reference: |x| bits_of(x as u32 as f32),
            compare: Compare::Exact,
            init: true,
        },
        Case {
            name: "negated_zero_product_strict",
            source: "export def kernel :: f32 -> f32\nfn kernel value = -(value * 0.0)",
            relaxed: false,
            inputs: special_floats,
            reference: |x| bits_of(-(f(x) * 0.0)),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "mutable_float_parameter_strict",
            source: "export def kernel :: f32 -> f32\nfn kernel mut x = { x = x * 2.0f32; x + 1.0f32 }",
            relaxed: false,
            inputs: special_floats,
            reference: |x| bits_of(f(x) * 2.0 + 1.0),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "fusion_probe_strict",
            source: "export def kernel :: f32 -> f32\nfn kernel value = (value * value - 1.0) + (value * 0.1 + 0.2) * value",
            relaxed: false,
            inputs: near_one_floats,
            reference: |x| {
                let v = f(x);
                bits_of((v * v - 1.0) + (v * 0.1 + 0.2) * v)
            },
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "denormal_probe_strict",
            source: "export def kernel :: f32 -> f32\nfn kernel value = (value + value) * 0.5",
            relaxed: false,
            inputs: denormal_floats,
            reference: |x| bits_of((f(x) + f(x)) * 0.5),
            compare: Compare::Exact,
            init: false,
        },
        Case {
            name: "poly_relaxed",
            source: "export def kernel :: f32 -> f32\nfn kernel value = value * value + value",
            relaxed: true,
            inputs: positive_floats,
            reference: |x| bits_of(f(x) * f(x) + f(x)),
            compare: Compare::Ulps(4.0),
            init: false,
        },
        Case {
            name: "horner_relaxed",
            source: "export def kernel :: f32 -> f32\nfn kernel value = ((value * 0.5 + 0.25) * value + 0.125) * value + 1.0",
            relaxed: true,
            inputs: positive_floats,
            reference: |x| {
                let v = f(x);
                bits_of(((v * 0.5 + 0.25) * v + 0.125) * v + 1.0)
            },
            compare: Compare::Ulps(12.0),
            init: false,
        },
        Case {
            name: "ratio_relaxed",
            source: "export def kernel :: f32 -> f32\nfn kernel value = (value + 1.0) / (value * value + 2.0)",
            relaxed: true,
            inputs: positive_floats,
            reference: |x| {
                let v = f(x);
                bits_of((v + 1.0) / (v * v + 2.0))
            },
            compare: Compare::Ulps(9.0),
            init: false,
        },
        Case {
            name: "index_relaxed",
            source: "export def kernel :: i32 -> f32\nfn kernel value = (value as f32) * 0.5 + 0.25",
            relaxed: true,
            inputs: || (0..700u64).collect(),
            reference: |x| bits_of((x as u32 as i32 as f32) * 0.5 + 0.25),
            compare: Compare::Ulps(5.0),
            init: true,
        },
        Case {
            name: "threshold_relaxed",
            source: "export def kernel :: f32 -> i32\nfn kernel value = if value * value > 2.0 then 1 else 0",
            relaxed: true,
            inputs: || {
                positive_floats()
                    .into_iter()
                    .filter(|bits| {
                        // away from the threshold, where a one-ULP difference could change the answer
                        let v = f(*bits);
                        (v * v - 2.0).abs() > 0.001
                    })
                    .collect()
            },
            reference: |x| u64::from(f(x) * f(x) > 2.0),
            compare: Compare::Exact,
            init: false,
        },
    ]
}

// ---- a word-level reader, independent of the emitter ----

#[derive(Debug)]
struct Inst {
    op: u16,
    args: Vec<u32>,
}

fn parse(words: &[u32]) -> Vec<Inst> {
    assert_eq!(words[0], 0x0723_0203, "magic number");
    assert_eq!(words[1], 0x0001_0300, "SPIR-V 1.3");
    assert_eq!(words[2], 0, "generator");
    assert_eq!(words[4], 0, "schema");
    let mut instructions = Vec::new();
    let mut position = 5;
    while position < words.len() {
        let size = (words[position] >> 16) as usize;
        assert!(
            size >= 1 && position + size <= words.len(),
            "word count at {position}"
        );
        instructions.push(Inst {
            op: (words[position] & 0xFFFF) as u16,
            args: words[position + 1..position + size].to_vec(),
        });
        position += size;
    }
    assert!(words[3] > 1, "bound");
    instructions
}

fn count(instructions: &[Inst], op: u16) -> usize {
    instructions
        .iter()
        .filter(|instruction| instruction.op == op)
        .count()
}

fn string_at(args: &[u32]) -> String {
    let bytes: Vec<u8> = args
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .take_while(|byte| *byte != 0)
        .collect();
    String::from_utf8(bytes).unwrap()
}

const OP_CAPABILITY: u16 = 17;
const OP_EXTENSION: u16 = 10;
const OP_MEMORY_MODEL: u16 = 14;
const OP_ENTRY_POINT: u16 = 15;
const OP_EXECUTION_MODE: u16 = 16;
const OP_DECORATE: u16 = 71;
const OP_F_ADD: u16 = 129;
const OP_F_SUB: u16 = 131;
const OP_F_MUL: u16 = 133;
const OP_F_DIV: u16 = 136;
const OP_F_NEGATE: u16 = 127;
const OP_F_REM: u16 = 140;
const OP_F_MOD: u16 = 141;
const OP_CONVERT_S_TO_F: u16 = 111;
const OP_CONVERT_U_TO_F: u16 = 112;
const OP_CONVERT_F_TO_S: u16 = 110;
const OP_CONVERT_F_TO_U: u16 = 109;
const OP_S_DIV: u16 = 135;
const OP_U_DIV: u16 = 134;
const OP_S_REM: u16 = 138;
const OP_U_MOD: u16 = 137;
const OP_IS_NAN: u16 = 156;
const OP_SELECT: u16 = 169;
const OP_TYPE_FLOAT: u16 = 22;
const OP_TYPE_INT: u16 = 21;

fn capabilities(instructions: &[Inst]) -> BTreeSet<u32> {
    instructions
        .iter()
        .filter(|instruction| instruction.op == OP_CAPABILITY)
        .map(|instruction| instruction.args[0])
        .collect()
}

fn float_arithmetic_count(instructions: &[Inst]) -> usize {
    [
        OP_F_ADD,
        OP_F_SUB,
        OP_F_MUL,
        OP_F_DIV,
        OP_F_NEGATE,
        OP_CONVERT_S_TO_F,
        OP_CONVERT_U_TO_F,
    ]
    .iter()
    .map(|op| count(instructions, *op))
    .sum()
}

fn no_contraction_count(instructions: &[Inst]) -> usize {
    instructions
        .iter()
        .filter(|instruction| instruction.op == OP_DECORATE && instruction.args[1] == 42)
        .count()
}

fn uses_float(source: &str) -> bool {
    source.contains("f32")
}

fn uses_int64(source: &str) -> bool {
    source.contains("i64") || source.contains("l)") || source.contains("ul)")
}

#[test]
fn spirv_modules_have_the_documented_structure() {
    for case in cases() {
        let kernel = emit_ok(case.source, case.relaxed);
        let instructions = parse(&kernel.words);
        let caps = capabilities(&instructions);
        let int64 = uses_int64(case.source);
        let strict_float = !case.relaxed && uses_float(case.source);
        assert!(caps.contains(&1), "{}: Shader", case.name);
        assert_eq!(caps.contains(&11), int64, "{}: Int64 capability", case.name);
        assert_eq!(
            kernel.features & FEATURE_INT64 != 0,
            int64,
            "{}: Int64 feature",
            case.name
        );
        assert_eq!(
            kernel.features & FEATURE_STRICT_FLOAT != 0,
            strict_float,
            "{}: strict float feature",
            case.name
        );
        assert_eq!(
            kernel.features & 1,
            0,
            "{}: bit 0 belongs to WebGPU",
            case.name
        );
        for capability in [4464, 4466, 4467] {
            assert_eq!(
                caps.contains(&capability),
                strict_float,
                "{}: capability {capability}",
                case.name
            );
        }
        let extensions: Vec<String> = instructions
            .iter()
            .filter(|instruction| instruction.op == OP_EXTENSION)
            .map(|instruction| string_at(&instruction.args))
            .collect();
        let expected_extensions: Vec<&str> = if strict_float {
            vec!["SPV_KHR_float_controls"]
        } else {
            Vec::new()
        };
        assert_eq!(extensions, expected_extensions, "{}", case.name);
        let model = instructions
            .iter()
            .find(|instruction| instruction.op == OP_MEMORY_MODEL)
            .unwrap();
        assert_eq!(model.args, [0, 1], "{}: Logical GLSL450", case.name);
        let entry_points: Vec<&Inst> = instructions
            .iter()
            .filter(|instruction| instruction.op == OP_ENTRY_POINT)
            .collect();
        let names: Vec<String> = entry_points
            .iter()
            .map(|entry| string_at(&entry.args[2..]))
            .collect();
        let expected: Vec<&str> = if case.init {
            vec!["map_main", "init_main"]
        } else {
            vec!["map_main"]
        };
        assert_eq!(names, expected, "{}", case.name);
        assert!(
            entry_points.iter().all(|entry| entry.args[0] == 5),
            "{}: GLCompute",
            case.name
        );
        assert_eq!(kernel.init, case.init, "{}", case.name);
        for entry in &entry_points {
            let function = entry.args[1];
            let modes: Vec<(u32, Vec<u32>)> = instructions
                .iter()
                .filter(|instruction| {
                    instruction.op == OP_EXECUTION_MODE && instruction.args[0] == function
                })
                .map(|mode| (mode.args[1], mode.args[2..].to_vec()))
                .collect();
            assert!(
                modes.contains(&(17, vec![256, 1, 1])),
                "{}: LocalSize",
                case.name
            );
            for mode in [4459, 4461, 4462] {
                assert_eq!(
                    modes.contains(&(mode, vec![32])),
                    strict_float,
                    "{}: mode {mode}",
                    case.name
                );
            }
        }
        let arithmetic = float_arithmetic_count(&instructions);
        assert_eq!(
            no_contraction_count(&instructions),
            if case.relaxed { 0 } else { arithmetic },
            "{}: NoContraction",
            case.name
        );
        for forbidden in [OP_F_REM, OP_F_MOD, OP_S_DIV, OP_U_DIV, OP_S_REM, OP_U_MOD] {
            assert_eq!(
                count(&instructions, forbidden),
                0,
                "{}: opcode {forbidden}",
                case.name
            );
        }
        if !case.relaxed {
            assert_eq!(
                count(&instructions, OP_F_DIV),
                0,
                "{}: strict kernels have no OpFDiv",
                case.name
            );
        }
        assert_eq!(
            count(&instructions, OP_TYPE_FLOAT) > 0,
            uses_float(case.source),
            "{}: float type",
            case.name
        );
        let wide_types = instructions
            .iter()
            .filter(|instruction| instruction.op == OP_TYPE_INT && instruction.args[1] == 64)
            .count();
        assert_eq!(wide_types > 0, int64, "{}: 64-bit type", case.name);
        assert!(kernel.weight >= 1);
        assert_eq!(kernel.bytes().len(), kernel.words.len() * 4);
    }
}

#[test]
fn spirv_lanes_and_features_match_the_kernel_types() {
    let kernel = emit_ok(
        "export def kernel :: i32 -> i32\nfn kernel value = value + 1",
        false,
    );
    assert_eq!(
        (kernel.input, kernel.output, kernel.features, kernel.init),
        (Lane::Int32, Lane::Int32, 0, true)
    );
    let kernel = emit_ok(
        "export def kernel :: i64u -> f32\nfn kernel value = (value as i64) as f32",
        false,
    );
    assert_eq!((kernel.input, kernel.output), (Lane::Int64, Lane::Float32));
    assert_eq!(kernel.features, FEATURE_INT64 | FEATURE_STRICT_FLOAT);
    assert!(!kernel.init);
    assert_eq!(
        (Lane::Int32.kind(), Lane::Float32.kind(), Lane::Int64.kind()),
        (1, 2, 4)
    );
    let kernel = emit_ok(
        "export def kernel :: f32 -> f32\nfn kernel value = (value + 1.0) / 3.0",
        true,
    );
    assert_eq!(
        kernel.features, 0,
        "a relaxed float kernel needs no device feature"
    );
    let single = emit_ok(
        "def helper :: i32 -> i32\nfn helper value = value * value + 1\nexport def kernel :: i32 -> i32\nfn kernel value = helper value",
        false,
    );
    let double = emit_ok(
        "def helper :: i32 -> i32\nfn helper value = value * value + 1\nexport def kernel :: i32 -> i32\nfn kernel value = helper (helper value)",
        false,
    );
    assert!(
        double.weight > single.weight,
        "weight counts a callee at every call"
    );
}

#[test]
fn spirv_weight_is_a_lower_bound_that_prices_the_cheapest_path_of_a_lane() {
    // step is 5 instructions, and each helper calls the one below it four times: 20, 80, 320.
    const HELPERS: &str = "def step :: i32 -> i32\nfn step value = ((value * 3 + 1) ^ 5) * 7 + 11\ndef r4 :: i32 -> i32\nfn r4 value = step (step (step (step value)))\ndef r16 :: i32 -> i32\nfn r16 value = r4 (r4 (r4 (r4 value)))\ndef r64 :: i32 -> i32\nfn r64 value = r16 (r16 (r16 (r16 value)))\n";
    let weight = |body: &str| {
        let source = format!("{HELPERS}export def kernel :: i32 -> i32\nfn kernel value = {body}");
        let first = emit_ok(&source, false);
        assert_eq!(first, emit_ok(&source, false), "{body}: deterministic");
        first.weight
    };
    // Without branches every emitted instruction runs, so the weight is the instruction count, as it was.
    assert_eq!(weight("value * 3 + 1"), 2);
    assert_eq!(weight("step value"), 5);
    assert_eq!(weight("r64 (r64 (r64 (r64 value)))"), 1280);
    assert_eq!(weight("value"), 1, "at least 1");
    // A conditional costs its condition and the cheaper arm: a lane runs one arm. This kernel was priced at 1282 (both
    // arms, 1280 + 1 + the comparison), and Gpu.Auto then ran it on the device although a lane of it takes two operations.
    assert_eq!(
        weight("if value < 0 then r64 (r64 (r64 (r64 value))) else value + 1"),
        2
    );
    assert_eq!(
        weight("if value < 0 then value + 1 else r64 (r64 (r64 (r64 value)))"),
        2,
        "the arms in the other order"
    );
    assert_eq!(weight("if value < 0 then r64 value else r16 value"), 1 + 80);
    assert_eq!(
        weight("if value < 0 then (if value < 5 then r64 value else value * 2) else r16 value"),
        1 + 2,
        "nested: the inner conditional is priced by the same rule, and then compared with the other arm"
    );
    assert_eq!(
        weight("if value < 0 then r16 value else (if value > 5 then r64 value else r4 value)"),
        1 + 21,
        "an arm that is a conditional is the cheaper one when its own cheaper arm is"
    );
    // `&&` and `||` run their right operand only when the left one does not decide: the left operand is the bound.
    assert_eq!(weight("if value < 0 && r64 value > 5 then 1 else 2"), 1);
    assert_eq!(weight("if value < 0 || r64 value > 5 then 1 else 2"), 1);
    assert_eq!(
        weight("if value * 3 < 0 && value + 1 > 5 then 1 else 2"),
        2,
        "the left operand and nothing of the right one"
    );
    // A call counts its callee by the same rule, at every call.
    let pick =
        "def pick :: i32 -> i32\nfn pick value = if value < 0 then r64 value else value + 1\n";
    let source = format!(
        "{HELPERS}{pick}export def kernel :: i32 -> i32\nfn kernel value = pick (pick value)"
    );
    assert_eq!(emit_ok(&source, false).weight, 4);
    // The relaxed emitter follows the same rule, for f32 too.
    let relaxed = emit_ok(
        "export def kernel :: f32 -> f32\nfn kernel value = if value < 0.0 then value * value * value else value + 1.0",
        true,
    );
    assert_eq!(relaxed.weight, 1 + 1);
}

#[test]
fn spirv_output_is_deterministic() {
    for case in cases() {
        let first = emit_ok(case.source, case.relaxed);
        let second = emit_ok(case.source, case.relaxed);
        assert_eq!(first, second, "{}", case.name);
        assert_eq!(first.bytes(), second.bytes(), "{}", case.name);
    }
}

#[test]
fn spirv_rejects_what_it_cannot_reproduce() {
    for (name, source, relaxed, fragment) in [
        (
            "strict division",
            "export def kernel :: f32 -> f32\nfn kernel value = value / 3.0",
            false,
            "division",
        ),
        (
            "f64 lane",
            "export def kernel :: f64 -> f64\nfn kernel value = value + 1.0",
            false,
            "f64",
        ),
        (
            "f64 local",
            "export def kernel :: f32 -> f32\nfn kernel value = { let wide = (value as f64) + 1.0; wide as f32 }",
            false,
            "f64",
        ),
        (
            "bool lane",
            "export def kernel :: i32 -> bool\nfn kernel value = value > 1",
            false,
            "bool lanes",
        ),
        (
            "integer division",
            "export def kernel :: i32 -> i32\nfn kernel value = 100 / value",
            false,
            "division/remainder",
        ),
        (
            "integer remainder",
            "export def kernel :: i64 -> i64\nfn kernel value = value % 7l",
            false,
            "division/remainder",
        ),
        (
            "power",
            "export def kernel :: i32 -> i32\nfn kernel value = value ** 2",
            false,
            "'**'",
        ),
        (
            "relaxed i64 lane",
            "export def kernel :: i64 -> i64\nfn kernel value = value + 1l",
            true,
            "64-bit",
        ),
        (
            "relaxed float to integer",
            "export def kernel :: f32 -> i32\nfn kernel value = value as i32",
            true,
            "float-to-integer",
        ),
        (
            "relaxed f64",
            "export def kernel :: f64 -> f64\nfn kernel value = value + 1.0",
            true,
            "f64",
        ),
    ] {
        let error = emit(source, relaxed).expect_err(name);
        assert_eq!(error.code, "E1018", "{name}: {}", error.message);
        assert!(
            error.message.contains(fragment),
            "{name}: {}",
            error.message
        );
    }
    assert!(
        emit(
            "export def kernel :: f32 -> f32\nfn kernel value = value / 3.0",
            true
        )
        .is_ok(),
        "the relaxed contract allows division"
    );
}

#[test]
fn relaxed_spirv_has_no_float_controls_and_no_decorations() {
    let kernel = emit_ok(
        "export def kernel :: f32 -> f32\nfn kernel value = ((value + 1.0) / (value * value + 2.0)) - value",
        true,
    );
    let instructions = parse(&kernel.words);
    assert_eq!(capabilities(&instructions), BTreeSet::from([1]));
    assert_eq!(no_contraction_count(&instructions), 0);
    assert_eq!(count(&instructions, OP_F_DIV), 1);
    assert_eq!(kernel.features, 0);
    assert!(kernel.relaxed);
}

#[test]
fn strict_float_to_integer_casts_are_guarded_selects() {
    let kernel = emit_ok(
        "export def kernel :: f32 -> i32\nfn kernel value = (value as i32) + ((-value) as i32)",
        false,
    );
    let instructions = parse(&kernel.words);
    assert_eq!(count(&instructions, OP_CONVERT_F_TO_S), 2);
    assert_eq!(count(&instructions, OP_CONVERT_F_TO_U), 0);
    assert_eq!(count(&instructions, OP_IS_NAN), 2);
    assert!(count(&instructions, OP_SELECT) >= 8);
    assert_eq!(
        no_contraction_count(&instructions),
        1,
        "only the negation is arithmetic"
    );
}

// ---- tools ----

fn find_tool(variable: &str, candidates: &[&str], name: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(variable) {
        return Some(PathBuf::from(path));
    }
    for candidate in candidates {
        if Path::new(candidate).exists() {
            return Some(PathBuf::from(candidate));
        }
    }
    Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| PathBuf::from(name))
}

/// A skipped test is announced on stderr through the handle itself: `eprintln!` is captured by the test harness and
/// shown only for a failing test, so a skip written with it would leave a plain `cargo test` printing `ok` and nothing
/// else. The requirement variable (`TSUZURI_REQUIRE_SPIRV_TOOLS` or `TSUZURI_REQUIRE_VULKAN`) turns the skip into a
/// failure.
fn skip(requirement: &str, reason: &str) {
    use std::io::Write;
    let _ = std::io::stderr().write_all(
        format!("\n*** SKIPPED (tests/gpu_spirv.rs, {requirement}): {reason} ***\n\n").as_bytes(),
    );
    assert!(
        std::env::var_os(requirement).is_none(),
        "{requirement} is set but the test would be skipped: {reason}"
    );
}

fn scratch(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("gpu_spirv")
        .join(name);
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn write_module(directory: &Path, name: &str, kernel: &SpirvKernel) -> PathBuf {
    let path = directory.join(format!("{name}.spv"));
    fs::write(&path, kernel.bytes()).unwrap();
    path
}

#[test]
fn spirv_modules_validate_for_vulkan() {
    let Some(validator) = find_tool(
        "SPIRV_VAL",
        &[
            "/opt/homebrew/opt/spirv-tools/bin/spirv-val",
            "/usr/local/bin/spirv-val",
            "/usr/bin/spirv-val",
        ],
        "spirv-val",
    ) else {
        skip(
            "TSUZURI_REQUIRE_SPIRV_TOOLS",
            "spirv-val was not found; SPIR-V validation did not run",
        );
        return;
    };
    let directory = scratch("validate");
    let mut validated = 0;
    for case in cases() {
        let kernel = emit_ok(case.source, case.relaxed);
        let path = write_module(&directory, case.name, &kernel);
        for environment in ["vulkan1.1", "vulkan1.2"] {
            let output = Command::new(&validator)
                .args(["--target-env", environment])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "spirv-val --target-env {environment} rejects {}:\n{}{}",
                case.name,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            validated += 1;
        }
    }
    assert_eq!(validated, cases().len() * 2);
    eprintln!("spirv-val accepted {validated} module/environment pairs");
}

// ---- the conformance probe of the strict float32 controls ----

/// The words of a table of `src/runtime/gpu-vulkan.c` (`static const uint32_t name[...] = { ... };`).
fn runtime_table(name: &str) -> Vec<u32> {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/gpu-vulkan.c"))
            .unwrap();
    let start = source
        .find(&format!("static const uint32_t {name}["))
        .unwrap_or_else(|| panic!("{name} is not declared in the runtime"));
    let open = start + source[start..].find('{').unwrap();
    let close = open + source[open..].find('}').unwrap();
    source[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(|word| {
            let digits = word
                .strip_prefix("0x")
                .and_then(|word| word.strip_suffix('U'))
                .unwrap_or_else(|| panic!("{name}: {word} is not a hexadecimal word"));
            u32::from_str_radix(digits, 16).unwrap()
        })
        .collect()
}

fn runtime_constant(name: &str) -> usize {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/gpu-vulkan.c"))
            .unwrap();
    let prefix = format!("#define {name} ");
    source
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("{name} is not defined in the runtime"))
        .trim()
        .parse()
        .unwrap()
}

/// What the probe expects of a device is what the CPU reference computes, and each group of lanes can tell a wrong
/// behaviour from the right one (a fused multiply-add, a flush of subnormals, a truncation).
#[test]
fn the_probe_tables_hold_what_the_cpu_reference_computes() {
    let lanes = runtime_constant("TZ_VK_PROBE_LANES");
    let input = runtime_table("tz_vk_probe_input");
    let expected = runtime_table("tz_vk_probe_expected");
    assert_eq!((lanes, input.len(), expected.len()), (24, 48, 24));
    let c0 = f32::from_bits(0xBF80_1000);
    for lane in 0..lanes {
        let (a_bits, b_bits) = (input[2 * lane], input[2 * lane + 1]);
        let (a, b) = (f32::from_bits(a_bits), f32::from_bits(b_bits));
        let result = match lane / 3 {
            0 => std::hint::black_box(a * b) + c0,
            1 => -(a * b),
            2 => a + b,
            3 | 4 => a * b,
            5 => a - b,
            6 => a_bits as f32,
            _ => a_bits as i32 as f32,
        };
        let bits = if lane / 3 == 5 && result.is_nan() {
            0x7FC0_0000
        } else {
            result.to_bits()
        };
        assert_eq!(expected[lane], bits, "lane {lane} (operation {})", lane / 3);
    }
    for lane in 0..3 {
        let (a, b) = (
            f32::from_bits(input[2 * lane]),
            f32::from_bits(input[2 * lane + 1]),
        );
        assert_ne!(
            a.mul_add(b, c0).to_bits(),
            expected[lane],
            "lane {lane} must tell a fused multiply-add"
        );
    }
    for (lane, bits) in expected.iter().enumerate().take(11).skip(6) {
        assert_ne!(
            bits & 0x7FFF_FFFF,
            0,
            "lane {lane} must tell a flush of subnormals"
        );
    }
    for lane in 12..15 {
        let exact = f64::from(f32::from_bits(input[2 * lane]))
            * f64::from(f32::from_bits(input[2 * lane + 1]));
        let nearest = exact as f32;
        let truncated = if f64::from(nearest).abs() > exact.abs() {
            f32::from_bits(nearest.to_bits() - 1)
        } else {
            nearest
        };
        assert_ne!(
            truncated.to_bits(),
            expected[lane],
            "lane {lane} must tell a truncation from rounding to nearest even"
        );
    }
    assert_eq!(expected[3], 0, "-(-1 * 0) is +0");
    assert_eq!(expected[4], 0x8000_0000, "-(1 * 0) is -0");
}

#[test]
fn the_probe_module_is_the_assembly_of_its_source_and_validates() {
    let Some(assembler) = find_tool(
        "SPIRV_AS",
        &[
            "/opt/homebrew/opt/spirv-tools/bin/spirv-as",
            "/usr/local/bin/spirv-as",
            "/usr/bin/spirv-as",
        ],
        "spirv-as",
    ) else {
        skip(
            "TSUZURI_REQUIRE_SPIRV_TOOLS",
            "spirv-as was not found; the words of the conformance probe were not compared with their source",
        );
        return;
    };
    let directory = scratch("probe");
    let module = directory.join("probe.spv");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gpu_vulkan_probe.spvasm");
    let output = Command::new(&assembler)
        .args(["--target-env", "vulkan1.1"])
        .arg(&source)
        .arg("-o")
        .arg(&module)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembled: Vec<u32> = fs::read(&module)
        .unwrap()
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    let embedded = runtime_table("tz_vk_probe_module");
    assert_eq!(
        embedded.len(),
        runtime_constant("TZ_VK_PROBE_WORDS"),
        "the declared size is the size of the table"
    );
    assert_eq!(
        embedded, assembled,
        "src/runtime/gpu-vulkan.c must hold the words of tests/gpu_vulkan_probe.spvasm: assemble it again"
    );

    // The module declares the strict controls, and every floating-point result carries NoContraction.
    let mut declared = BTreeSet::new();
    let (mut arithmetic, mut no_contraction, mut position) = (0, 0, 5);
    while position < embedded.len() {
        let size = (embedded[position] >> 16) as usize;
        match (embedded[position] & 0xFFFF) as u16 {
            OP_CAPABILITY => {
                declared.insert(embedded[position + 1]);
            }
            OP_F_ADD | OP_F_SUB | OP_F_MUL | OP_F_NEGATE | OP_CONVERT_S_TO_F
            | OP_CONVERT_U_TO_F => arithmetic += 1,
            OP_DECORATE if embedded[position + 2] == 42 => no_contraction += 1,
            _ => {}
        }
        position += size;
    }
    assert!(declared.is_superset(&BTreeSet::from([4464, 4466, 4467])));
    assert_eq!(arithmetic, no_contraction);

    let Some(validator) = find_tool(
        "SPIRV_VAL",
        &[
            "/opt/homebrew/opt/spirv-tools/bin/spirv-val",
            "/usr/local/bin/spirv-val",
            "/usr/bin/spirv-val",
        ],
        "spirv-val",
    ) else {
        skip(
            "TSUZURI_REQUIRE_SPIRV_TOOLS",
            "spirv-val was not found; the conformance probe module was not validated",
        );
        return;
    };
    for environment in ["vulkan1.1", "vulkan1.2"] {
        let output = Command::new(&validator)
            .args(["--target-env", environment])
            .arg(&module)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "spirv-val --target-env {environment} rejects the probe:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// ---- execution on a device ----

/// The runtime harness together with the environment of one Vulkan implementation: the default one of the machine, and
/// every driver manifest listed in `TSUZURI_VULKAN_TEST_ICDS` (a path list; each runs the same checks through the
/// Vulkan loader with `VK_DRIVER_FILES` set, for example the SwiftShader that a browser ships).
#[derive(Clone)]
struct Harness {
    path: PathBuf,
    label: String,
    environment: Vec<(String, String)>,
}

/// Why the runtime harness is not there. A compiler that cannot be run is a missing tool, and a test may skip for it. A
/// compiler that runs and rejects the harness is a defect of the runtime or the harness, which no skip may hide.
enum HarnessError {
    Missing(String),
    Build(String),
}

static HARNESS: OnceLock<Result<PathBuf, HarnessError>> = OnceLock::new();

fn compile_harness(clang: &Path, source: &Path, path: &Path) -> Result<(), HarnessError> {
    let output = Command::new(clang)
        .args(["-std=c11", "-O1", "-g"])
        .arg(source)
        .arg("-o")
        .arg(path)
        .output()
        .map_err(|error| {
            HarnessError::Missing(format!("{} cannot run: {error}", clang.display()))
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(HarnessError::Build(format!(
            "{} does not compile {}:\n{}",
            clang.display(),
            source.display(),
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

fn harness_binary() -> &'static Result<PathBuf, HarnessError> {
    HARNESS.get_or_init(|| {
        if cfg!(windows) {
            return Err(HarnessError::Missing(
                "the runtime harness uses dlopen and pthreads; the Windows loader path is checked by type checks only"
                    .to_owned(),
            ));
        }
        let clang = std::env::var_os("TSUZURI_CLANG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("clang"));
        let path = scratch("harness").join("gpu_vulkan_harness");
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gpu_vulkan_runtime.c");
        compile_harness(&clang, &source, &path)?;
        Ok(path)
    })
}

/// What the device tests do with the harness: run with it, or skip for a reason. A build failure is neither: it fails.
enum Verdict<'a> {
    Run(&'a PathBuf),
    Skip(String),
}

fn classify(result: &Result<PathBuf, HarnessError>) -> Verdict<'_> {
    match result {
        Ok(path) => Verdict::Run(path),
        Err(HarnessError::Missing(reason)) => {
            Verdict::Skip(format!("no Vulkan runtime harness: {reason}"))
        }
        Err(HarnessError::Build(reason)) => {
            panic!("the runtime harness does not compile although the C compiler runs: {reason}")
        }
    }
}

/// The harness of the device tests: a missing tool is a loud skip (`None`), and a build failure is a failure of the test.
fn harness_or_skip(result: &Result<PathBuf, HarnessError>) -> Option<&PathBuf> {
    match classify(result) {
        Verdict::Run(path) => Some(path),
        Verdict::Skip(reason) => {
            skip("TSUZURI_REQUIRE_VULKAN", &reason);
            None
        }
    }
}

struct Outcome {
    status: i32,
    log: String,
    output: Vec<u8>,
}

impl Harness {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.path);
        command.env("TSUZURI_GPU_DEBUG", "1");
        for (name, value) in &self.environment {
            command.env(name, value);
        }
        command
    }

    fn run(&self, directory: &Path, arguments: &[String], input: Option<&[u8]>) -> Outcome {
        let output_path = directory.join("output.bin");
        let _ = fs::remove_file(&output_path);
        let mut command = self.command();
        command.args(arguments);
        if let Some(input) = input {
            let input_path = directory.join("input.bin");
            fs::write(&input_path, input).unwrap();
            command.arg("--input").arg(&input_path);
        }
        command.arg("--output").arg(&output_path);
        let result = command.output().unwrap();
        Outcome {
            status: result.status.code().unwrap_or(-1),
            log: format!(
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            ),
            output: fs::read(&output_path).unwrap_or_default(),
        }
    }

    fn probe(&self, features: u32) -> Outcome {
        let result = self
            .command()
            .args(["probe", "--features", &features.to_string()])
            .output()
            .unwrap();
        Outcome {
            status: result.status.code().unwrap_or(-1),
            log: format!(
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            ),
            output: Vec::new(),
        }
    }

    fn scratch(&self, name: &str) -> PathBuf {
        scratch(&format!("{}_{name}", self.label))
    }
}

/// The harness of every usable Vulkan implementation; a loud skip when there is none.
fn devices() -> Vec<Harness> {
    let Some(binary) = harness_or_skip(harness_binary()) else {
        return Vec::new();
    };
    let mut candidates = vec![Harness {
        path: binary.clone(),
        label: "default".to_owned(),
        environment: Vec::new(),
    }];
    if let Some(list) = std::env::var_os("TSUZURI_VULKAN_TEST_ICDS") {
        for (index, manifest) in std::env::split_paths(&list).enumerate() {
            let manifest = manifest.display().to_string();
            candidates.push(Harness {
                path: binary.clone(),
                label: format!("icd{index}"),
                environment: vec![
                    ("VK_DRIVER_FILES".to_owned(), manifest.clone()),
                    ("VK_ICD_FILENAMES".to_owned(), manifest),
                ],
            });
        }
    }
    let mut usable = Vec::new();
    for harness in candidates {
        let probe = harness.probe(0);
        if probe.status == 0 {
            eprintln!(
                "Vulkan implementation {}: {}",
                harness.label,
                probe
                    .log
                    .lines()
                    .find(|line| line.contains("device="))
                    .unwrap_or("")
                    .trim()
            );
            usable.push(harness);
        } else {
            skip(
                "TSUZURI_REQUIRE_VULKAN",
                &format!(
                    "no usable Vulkan device for {} (status {}):\n{}",
                    harness.label, probe.status, probe.log
                ),
            );
        }
    }
    usable
}

fn lane_bytes(lane: Lane) -> usize {
    if lane == Lane::Int64 { 8 } else { 4 }
}

/// The reviewer's defect, kept as a regression: a C compiler that runs but rejects the runtime (an `#error` appended to
/// `src/runtime/gpu-vulkan.c`) once made `devices()` skip, so the three device tests passed without running anything.
#[test]
fn a_harness_that_does_not_compile_fails_and_a_missing_compiler_skips() {
    let clang = std::env::var_os("TSUZURI_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    if Command::new(&clang).arg("--version").output().is_err() {
        skip(
            "TSUZURI_REQUIRE_VULKAN",
            "no C compiler to break a harness with",
        );
        return;
    }
    let directory = scratch("broken_harness");
    let source = directory.join("broken.c");
    fs::write(
        &source,
        "#error the runtime does not compile\nint main(void) { return 0; }\n",
    )
    .unwrap();
    match compile_harness(&clang, &source, &directory.join("broken")) {
        Err(HarnessError::Build(reason)) => {
            assert!(reason.contains("the runtime does not compile"), "{reason}")
        }
        Err(HarnessError::Missing(reason)) => {
            panic!("a compile error is not a missing tool: {reason}")
        }
        Ok(()) => panic!("the broken harness compiled"),
    }
    match compile_harness(
        Path::new("/nonexistent/tsuzuri-clang"),
        &source,
        &directory.join("missing"),
    ) {
        Err(HarnessError::Missing(_)) => {}
        _ => panic!("a compiler that cannot run is a missing tool"),
    }
    // The policy of the device tests: a build failure is a failure, never a skip; a missing tool is a skip.
    let outcome = std::panic::catch_unwind(|| {
        matches!(
            classify(&Err(HarnessError::Build("broken".to_owned()))),
            Verdict::Skip(_)
        )
    });
    assert!(
        outcome.is_err(),
        "a harness that does not compile must fail the device tests"
    );
    assert!(matches!(
        classify(&Err(HarnessError::Missing("none".to_owned()))),
        Verdict::Skip(_)
    ));
}

fn encode(values: &[u64], lane: Lane) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| {
            if lane == Lane::Int64 {
                value.to_le_bytes().to_vec()
            } else {
                (*value as u32).to_le_bytes().to_vec()
            }
        })
        .collect()
}

fn decode(bytes: &[u8], lane: Lane) -> Vec<u64> {
    bytes
        .chunks(lane_bytes(lane))
        .map(|chunk| {
            let mut padded = [0u8; 8];
            padded[..chunk.len()].copy_from_slice(chunk);
            u64::from_le_bytes(padded)
        })
        .collect()
}

fn ulp(value: f32) -> f32 {
    let magnitude = value.abs();
    f32::from_bits(magnitude.to_bits() + 1) - magnitude
}

fn matches(compare: Compare, actual: u64, expected: u64, float_output: bool) -> bool {
    if actual == expected {
        return true;
    }
    if !float_output {
        return false;
    }
    let (a, e) = (f(actual), f(expected));
    if a.is_nan() && e.is_nan() {
        return true;
    }
    match compare {
        Compare::Exact => false,
        Compare::Ulps(tolerance) => {
            a.is_finite() && e.is_finite() && (a - e).abs() <= tolerance * ulp(e)
        }
    }
}

fn expected_lane(case: &Case, kernel: &SpirvKernel, input: u64) -> u64 {
    let expected = (case.reference)(input);
    if kernel.output == Lane::Int64 {
        expected
    } else {
        expected & 0xFFFF_FFFF
    }
}

fn lanes_argument(kernel: &SpirvKernel) -> String {
    format!("{},{}", kernel.input.kind(), kernel.output.kind())
}

fn run_arguments(
    spv: &Path,
    kernel: &SpirvKernel,
    mode: &str,
    count: usize,
    extra: &[&str],
) -> Vec<String> {
    let mut arguments = vec![
        "run".to_owned(),
        "--spirv".to_owned(),
        spv.display().to_string(),
        "--mode".to_owned(),
        mode.to_owned(),
        "--lanes".to_owned(),
        lanes_argument(kernel),
        "--count".to_owned(),
        count.to_string(),
        "--features".to_owned(),
        kernel.features.to_string(),
    ];
    arguments.extend(extra.iter().map(|argument| (*argument).to_owned()));
    arguments
}

fn check_outputs(case: &Case, kernel: &SpirvKernel, inputs: &[u64], actual: &[u64], what: &str) {
    assert_eq!(
        actual.len(),
        inputs.len(),
        "{}: {what}: lane count",
        case.name
    );
    let float_output = kernel.output == Lane::Float32;
    let mismatches: Vec<(usize, u64, u64, u64)> = inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            let expected = expected_lane(case, kernel, *input);
            (!matches(case.compare, actual[index], expected, float_output)).then_some((
                index,
                *input,
                actual[index],
                expected,
            ))
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{}: {what}: {} of {} lanes differ; first (index, input, actual, expected): {:x?}",
        case.name,
        mismatches.len(),
        inputs.len(),
        &mismatches[..mismatches.len().min(4)]
    );
}

fn execute(case: &Case, harness: &Harness, staged: bool) -> usize {
    let kernel = emit_ok(case.source, case.relaxed);
    let directory = harness.scratch(&format!(
        "run_{}_{}",
        case.name,
        if staged { "staged" } else { "direct" }
    ));
    let spv = write_module(&directory, case.name, &kernel);
    let inputs = (case.inputs)();
    let extra: &[&str] = if staged { &["--staged"] } else { &[] };
    let outcome = harness.run(
        &directory,
        &run_arguments(&spv, &kernel, "map", inputs.len(), extra),
        Some(&encode(&inputs, kernel.input)),
    );
    assert_eq!(
        outcome.status, 0,
        "{}: map failed:\n{}",
        case.name, outcome.log
    );
    check_outputs(
        case,
        &kernel,
        &inputs,
        &decode(&outcome.output, kernel.output),
        "map",
    );
    let mut checked = inputs.len();
    if case.init && !staged {
        for count in [1usize, 255, 256, 257, 1000] {
            let outcome = harness.run(
                &directory,
                &run_arguments(&spv, &kernel, "init", count, &[]),
                None,
            );
            assert_eq!(
                outcome.status, 0,
                "{}: init {count} failed:\n{}",
                case.name, outcome.log
            );
            let indexes: Vec<u64> = (0..count as u64).collect();
            check_outputs(
                case,
                &kernel,
                &indexes,
                &decode(&outcome.output, kernel.output),
                "init",
            );
            checked += count;
        }
    }
    checked
}

#[test]
fn integer_kernels_match_the_reference_on_a_vulkan_device() {
    for harness in devices() {
        integer_kernels(&harness);
    }
}

fn integer_kernels(harness: &Harness) {
    let int64 = harness.probe(FEATURE_INT64);
    let mut executed = 0;
    let mut lanes = 0;
    for case in cases()
        .iter()
        .filter(|case| !case.relaxed && !uses_float(case.source))
    {
        let kernel = emit_ok(case.source, false);
        if kernel.features & FEATURE_INT64 != 0 && int64.status != 0 {
            eprintln!(
                "{}: the device has no shaderInt64; the Unavailable path is checked instead",
                case.name
            );
            let directory = harness.scratch(&format!("run_{}_unavailable", case.name));
            let spv = write_module(&directory, case.name, &kernel);
            let inputs = [1u64, 2, 3];
            let outcome = harness.run(
                &directory,
                &run_arguments(&spv, &kernel, "map", inputs.len(), &[]),
                Some(&encode(&inputs, kernel.input)),
            );
            assert_eq!(outcome.status, 2, "{}: {}", case.name, outcome.log);
            continue;
        }
        for staged in [false, true] {
            lanes += execute(case, harness, staged);
        }
        executed += 1;
    }
    // five 32-bit kernels, and four that need shaderInt64
    assert_eq!(
        executed,
        if int64.status == 0 { 9 } else { 5 },
        "{}",
        harness.label
    );
    eprintln!(
        "Vulkan ({}): {executed} integer kernels matched the reference on {lanes} lanes (direct and staged transfers)",
        harness.label
    );
}

#[test]
fn relaxed_float_kernels_stay_within_the_relaxed_tolerance_on_a_vulkan_device() {
    for harness in devices() {
        let mut lanes = 0;
        for case in cases().iter().filter(|case| case.relaxed) {
            lanes += execute(case, &harness, false);
        }
        eprintln!(
            "Vulkan ({}): relaxed f32 kernels stayed within the D6 tolerances on {lanes} lanes",
            harness.label
        );
    }
}

#[test]
fn strict_float_kernels_run_only_where_the_device_reports_the_controls() {
    for harness in devices() {
        strict_float_kernels(&harness);
    }
}

fn strict_float_kernels(harness: &Harness) {
    let probe = harness.probe(FEATURE_STRICT_FLOAT);
    let certified = probe.status == 0;
    let int64 = harness.probe(FEATURE_INT64).status == 0;
    let mut executed = 0;
    for case in cases()
        .iter()
        .filter(|case| !case.relaxed && uses_float(case.source))
    {
        let kernel = emit_ok(case.source, false);
        assert_ne!(kernel.features & FEATURE_STRICT_FLOAT, 0, "{}", case.name);
        if kernel.features & FEATURE_INT64 != 0 && !int64 {
            continue;
        }
        if certified {
            for staged in [false, true] {
                executed += execute(case, harness, staged);
            }
            continue;
        }
        let directory = harness.scratch(&format!("strict_{}", case.name));
        let spv = write_module(&directory, case.name, &kernel);
        let inputs = (case.inputs)();
        let head = &inputs[..8.min(inputs.len())];
        // The device does not report the strict controls: the backend refuses, with and without the feature bit
        // (the runtime also reads the capabilities of the module itself).
        let refused = harness.run(
            &directory,
            &run_arguments(&spv, &kernel, "map", head.len(), &[]),
            Some(&encode(head, kernel.input)),
        );
        assert_eq!(
            refused.status, 2,
            "{}: expected the Unavailable path:\n{}",
            case.name, refused.log
        );
        let mut stripped = kernel.clone();
        stripped.features = 0;
        let refused = harness.run(
            &directory,
            &run_arguments(&spv, &stripped, "map", head.len(), &[]),
            Some(&encode(head, kernel.input)),
        );
        assert_eq!(
            refused.status, 2,
            "{}: the module's own capabilities must refuse:\n{}",
            case.name, refused.log
        );
        // Information only: what the translation layer does when the check is bypassed (UNCERTIFIED).
        let outcome = harness.run(
            &directory,
            &run_arguments(&spv, &kernel, "map", inputs.len(), &["--assume-strict"]),
            Some(&encode(&inputs, kernel.input)),
        );
        if outcome.status == 0 {
            let actual = decode(&outcome.output, kernel.output);
            let float_output = kernel.output == Lane::Float32;
            let differing = inputs
                .iter()
                .zip(&actual)
                .filter(|(input, actual)| {
                    !matches(
                        case.compare,
                        **actual,
                        expected_lane(case, &kernel, **input),
                        float_output,
                    )
                })
                .count();
            eprintln!(
                "UNCERTIFIED [{}] {}: with the capability check bypassed, {} of {} lanes equal the CPU reference",
                harness.label,
                case.name,
                inputs.len() - differing,
                inputs.len()
            );
        }
    }
    if certified {
        eprintln!(
            "Vulkan: strict f32 kernels matched the CPU reference bit for bit on {executed} lanes"
        );
    } else {
        eprintln!(
            "Vulkan [{}]: strict f32 kernels are Unavailable here (the device must report the float controls and pass the conformance probe): {}",
            harness.label,
            probe
                .log
                .lines()
                .find(|line| line.contains("device="))
                .unwrap_or("")
                .trim()
        );
    }
}

// ---- the defects of the WGSL emitter that the SPIR-V emitter must not share ----

/// The arms of the `else if` chain: Tint's limit of 127 nested statements stops WGSL at 62.
const CHAIN_ARMS: usize = 100;

fn chain_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let mut source = String::from("export def kernel :: i32 -> i32\nfn kernel value = ");
        for arm in 0..CHAIN_ARMS {
            source.push_str(&format!("if value == {arm} then {} else ", 1000 + arm * 3));
        }
        source.push('7');
        source
    })
}

fn chain_inputs() -> Vec<u64> {
    let mut values: Vec<u64> = (0..CHAIN_ARMS as u64 + 20).collect();
    values.extend([u64::from(u32::MAX), u64::from(i32::MIN as u32), 5000]);
    values
}

/// The checker of a debug build recurses once per arm and needs a large stack; the compiler's limits are not involved.
fn on_large_stack(test: impl FnOnce() + Send + 'static) {
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(test)
        .unwrap();
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
}

/// A WGSL function parameter cannot be assigned, so the WGSL emitter copies the ones that a body assigns; and an `else if`
/// chain of 63 arms exceeds the statement nesting of Tint, which the WGSL emitter rejects with E1017. A SPIR-V function
/// has neither limit: the long chain emits deterministically, validates, and runs like the CPU reference (the mutable
/// parameters are in the kernels `mutable_parameters` and `mutable_float_parameter_strict` of the shared list).
#[test]
fn a_long_else_if_chain_emits_valid_spirv_where_wgsl_has_a_nesting_limit() {
    on_large_stack(|| {
        let module = analyze(chain_source()).unwrap();
        let kernel = gpu::extract_kernel(&module, kernel_id(&module)).unwrap();
        let wgsl = kernel
            .wgsl()
            .expect_err("WGSL cannot nest this many statements");
        assert_eq!(wgsl.code, "E1017", "{}", wgsl.message);
        let first = kernel.spirv().unwrap();
        assert_eq!(
            first.bytes(),
            kernel.spirv().unwrap().bytes(),
            "the module is deterministic"
        );
        assert!(!first.relaxed);

        let case = Case {
            name: "else_if_chain",
            source: chain_source(),
            relaxed: false,
            inputs: chain_inputs,
            reference: |x| {
                let value = x as u32 as i32;
                if (0..CHAIN_ARMS as i32).contains(&value) {
                    1000 + 3 * value as u64
                } else {
                    7
                }
            },
            compare: Compare::Exact,
            init: true,
        };
        if let Some(validator) = find_tool(
            "SPIRV_VAL",
            &[
                "/opt/homebrew/opt/spirv-tools/bin/spirv-val",
                "/usr/local/bin/spirv-val",
                "/usr/bin/spirv-val",
            ],
            "spirv-val",
        ) {
            let directory = scratch("else_if_chain");
            let path = write_module(&directory, case.name, &first);
            for environment in ["vulkan1.1", "vulkan1.2"] {
                let output = Command::new(&validator)
                    .args(["--target-env", environment])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "spirv-val --target-env {environment} rejects the chain:\n{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            skip(
                "TSUZURI_REQUIRE_SPIRV_TOOLS",
                "spirv-val was not found; the long else-if chain was not validated",
            );
        }
        let mut lanes = 0;
        for harness in devices() {
            for staged in [false, true] {
                lanes += execute(&case, &harness, staged);
            }
        }
        eprintln!(
            "Vulkan: the {CHAIN_ARMS}-arm else-if chain matched the reference on {lanes} lanes"
        );
    });
}
