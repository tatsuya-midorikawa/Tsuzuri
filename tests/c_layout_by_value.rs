// W36: a `@c_layout` record without `ref` in an `extern def` parameter or result position is the C
// struct by value. The expected `declare` lines are written by hand from the reference table of the
// ticket (what Clang prints for the same C prototype on each target); `tests/c_layout_by_value.mjs`
// compares the same lowering with Clang itself.
use std::collections::BTreeMap;

use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, analyze_modules, llvm};

const WINDOWS_X64: [Triple; 2] = [Triple::X86_64WindowsMsvc, Triple::X86_64WindowsGnu];
const WINDOWS_ARM64: [Triple; 2] = [Triple::Aarch64WindowsMsvc, Triple::Aarch64WindowsGnu];
const LINUX_ARM64: [Triple; 2] = [Triple::Aarch64LinuxGnu, Triple::Aarch64LinuxMusl];
const LINUX_X64: [Triple; 2] = [Triple::X86_64LinuxGnu, Triple::X86_64LinuxMusl];

fn emit(source: &str, triple: Triple) -> String {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let _scope = PlatformScope::enter(Platform::new(Some(triple)));
    let ir = llvm::emit(&module, llvm::Entry::Library)
        .unwrap_or_else(|e| panic!("{triple:?}\n{source}\n{}: {}", e.code, e.message));
    assert_eq!(
        ir,
        llvm::emit(&module, llvm::Entry::Library).unwrap(),
        "IR is deterministic"
    );
    ir
}

/// The `declare` of every symbol, without `declare ` and a linkage word.
fn declares(ir: &str) -> BTreeMap<String, String> {
    ir.lines()
        .filter_map(|line| line.strip_prefix("declare "))
        .filter_map(|line| {
            let line = line.strip_prefix("dllimport ").unwrap_or(line);
            let name = line.split_once('@')?.1.split_once('(')?.0;
            Some((name.to_owned(), line.to_owned()))
        })
        .collect()
}

/// A program with, for every `(name, declaration)`, the record `name`, `take_<name> :: name -> i32`
/// and `ret_<name> :: i32 -> name`, and an export that calls each (an extern that is not reached
/// has no `declare`).
fn take_and_return(records: &[(&str, &str)], prelude: &str) -> String {
    let mut source = String::from(prelude);
    for (name, declaration) in records {
        source.push_str(declaration);
        source.push('\n');
        source.push_str(&format!(
            "@trusted\nextern \"take_{name}\" def take_{lower} :: {name} -> i32\n@trusted\nextern \"ret_{name}\" def ret_{lower} :: i32 -> {name}\n",
            lower = name.to_lowercase()
        ));
    }
    source.push_str("export def run :: own i32 -> i32\nfn run n =\n    let mut total = 0i32\n");
    for (name, _) in records {
        source.push_str(&format!(
            "    total = total + take_{lower} (ret_{lower} n)\n",
            lower = name.to_lowercase()
        ));
    }
    source.push_str("    total\n");
    source
}

/// The pointee of `byval` and `sret`: the bytes of the record, whose size the name tells (`B12` is
/// 12 bytes), except for the float records listed here.
fn bytes(name: &str) -> String {
    let size: usize = match name {
        "F5" => 20,
        "D3" => 24,
        "D4" => 32,
        _ => name[1..]
            .parse()
            .unwrap_or_else(|_| panic!("no size recorded for {name}")),
    };
    format!("[{size} x i8]")
}

/// Asserts `declare i32 @take_<name>(<argument>)` and the `declare` of `ret_<name>`: `result` is
/// the returned IR type, or `sret` for a result in memory. `%R` in `argument` is the pointee.
fn assert_passing(ir: &str, triple: Triple, name: &str, argument: &str, result: &str) {
    let declares = declares(ir);
    let argument = if argument.contains("%R") {
        argument.replace("%R", &bytes(name))
    } else {
        argument.to_owned()
    };
    assert_eq!(
        declares.get(&format!("take_{name}")).map(String::as_str),
        Some(format!("i32 @take_{name}({argument})").as_str()),
        "{triple:?}: argument {name}"
    );
    let expected = if result == "sret" {
        format!("void @ret_{name}(ptr sret({}), i32)", bytes(name))
    } else {
        format!("{result} @ret_{name}(i32)")
    };
    assert_eq!(
        declares.get(&format!("ret_{name}")).map(String::as_str),
        Some(expected.as_str()),
        "{triple:?}: result {name}"
    );
    // The call site spells the same types and attributes as the declaration.
    let call = ir
        .lines()
        .find(|line| line.contains("call ") && line.contains(&format!("@take_{name}(")))
        .unwrap_or_else(|| panic!("{triple:?}: no call of take_{name}"));
    let types: Vec<&str> = argument.split(", ").collect();
    let mut rest = call.split_once(&format!("@take_{name}(")).unwrap().1;
    for ty in types {
        let after = rest.strip_prefix(ty).unwrap_or_else(|| {
            panic!("{triple:?}: the call of take_{name} has not '{ty}': {call}")
        });
        rest = after
            .trim_start()
            .split_once(", ")
            .map_or("", |(_, rest)| rest);
    }
}

const SIZES: &[(&str, &str)] = &[
    ("B1", "@c_layout\nrecord B1 { a: i8 }"),
    ("B2", "@c_layout\nrecord B2 { a: i16 }"),
    ("B3", "@c_layout\nrecord B3 { r: i8u, g: i8u, b: i8u }"),
    ("B4", "@c_layout\nrecord B4 { f: f32 }"),
    (
        "B5",
        "@c_layout(pack = 1)\nrecord B5 { tag: i8u, value: i32 }",
    ),
    ("B6", "@c_layout\nrecord B6 { a: i16u, b: i16u, c: i16u }"),
    ("B7", "@c_layout\nrecord B7 { b: [i8u; 7] }"),
    ("B8", "@c_layout\nrecord B8 { x: i32, y: i32 }"),
    ("F8", "@c_layout\nrecord F8 { x: f32, y: f32 }"),
    ("D8", "@c_layout\nrecord D8 { d: f64 }"),
    ("H8", "@c_layout\nrecord H8 { h: Tag }"),
    ("B9", "@c_layout\nrecord B9 { b: [i8u; 9] }"),
    ("B12", "@c_layout\nrecord B12 { x: f32, y: f32, z: f32 }"),
    (
        "B16",
        "@c_layout\nrecord B16 { left: i32, top: i32, right: i32, bottom: i32 }",
    ),
    ("D16", "@c_layout\nrecord D16 { x: f64, y: f64 }"),
    ("H16", "@c_layout\nrecord H16 { h: Tag, x: i32 }"),
    ("B24", "@c_layout\nrecord B24 { a: i64, b: i64, c: i64 }"),
];
const TAG: &str = "extern type copy Tag\n";

#[test]
fn windows_x64_passes_by_size() {
    // 1, 2, 4 and 8 bytes are one integer whatever the fields are (`{float x, y}` is `i64`: Windows
    // x64 has no HFA); every other size is a pointer to a copy, and `sret` as a result.
    let expected: &[(&str, &str, &str)] = &[
        ("B1", "i8", "i8"),
        ("B2", "i16", "i16"),
        ("B3", "ptr", "sret"),
        ("B4", "i32", "i32"),
        ("B5", "ptr", "sret"),
        ("B6", "ptr", "sret"),
        ("B7", "ptr", "sret"),
        ("B8", "i64", "i64"),
        ("F8", "i64", "i64"),
        ("D8", "i64", "i64"),
        ("H8", "i64", "i64"),
        ("B9", "ptr", "sret"),
        ("B12", "ptr", "sret"),
        ("B16", "ptr", "sret"),
        ("D16", "ptr", "sret"),
        ("H16", "ptr", "sret"),
        ("B24", "ptr", "sret"),
    ];
    let source = take_and_return(SIZES, TAG);
    for triple in WINDOWS_X64 {
        let ir = emit(&source, triple);
        for (name, argument, result) in expected {
            assert_passing(&ir, triple, name, argument, result);
        }
    }
}

const FLOATS: &[(&str, &str)] = &[
    ("F1", "@c_layout\nrecord F1 { a: f32 }"),
    ("F2", "@c_layout\nrecord F2 { a: f32, b: f32 }"),
    ("F3", "@c_layout\nrecord F3 { a: f32, b: f32, c: f32 }"),
    (
        "F4",
        "@c_layout\nrecord F4 { a: f32, b: f32, c: f32, d: f32 }",
    ),
    (
        "F5",
        "@c_layout\nrecord F5 { a: f32, b: f32, c: f32, d: f32, e: f32 }",
    ),
    ("D1", "@c_layout\nrecord D1 { a: f64 }"),
    ("D2", "@c_layout\nrecord D2 { a: f64, b: f64 }"),
    ("D3", "@c_layout\nrecord D3 { a: f64, b: f64, c: f64 }"),
    (
        "D4",
        "@c_layout\nrecord D4 { a: f64, b: f64, c: f64, d: f64 }",
    ),
    ("Mixed", "@c_layout\nrecord Mixed { d: f64, f: f32 }"),
    ("Nested", "@c_layout\nrecord Nested { p: F2, z: f32 }"),
    ("Quad", "@c_layout\nrecord Quad { v: [f32; 4] }"),
    (
        "Packed",
        "@c_layout(pack = 1)\nrecord Packed { a: f32, b: f32 }",
    ),
    (
        "PackedQuad",
        "@c_layout(pack = 2)\nrecord PackedQuad { a: [f32; 3], b: f32 }",
    ),
    ("FloatInt", "@c_layout\nrecord FloatInt { f: f32, i: i32 }"),
];

/// `(record, argument, result)` on AArch64 without the `alignstack(8)` of Linux.
const ARM64_FLOATS: &[(&str, &str, &str)] = &[
    ("F1", "[1 x float]", "{ float }"),
    ("F2", "[2 x float]", "{ float, float }"),
    ("F3", "[3 x float]", "{ float, float, float }"),
    ("F4", "[4 x float]", "{ float, float, float, float }"),
    // Five floats are no HFA: 20 bytes go to memory.
    ("F5", "ptr", "sret"),
    ("D1", "[1 x double]", "{ double }"),
    ("D2", "[2 x double]", "{ double, double }"),
    ("D3", "[3 x double]", "{ double, double, double }"),
    ("D4", "[4 x double]", "{ double, double, double, double }"),
    // `float` beside `double` is no HFA: 16 bytes in two integer registers.
    ("Mixed", "[2 x i64]", "[2 x i64]"),
    ("Nested", "[3 x float]", "{ float, float, float }"),
    ("Quad", "[4 x float]", "{ float, float, float, float }"),
    ("Packed", "[2 x float]", "{ float, float }"),
    (
        "PackedQuad",
        "[4 x float]",
        "{ float, float, float, float }",
    ),
    ("FloatInt", "i64", "i64"),
];

#[test]
fn windows_arm64_passes_hfa_in_float_registers() {
    let source = take_and_return(FLOATS, "");
    for triple in WINDOWS_ARM64 {
        let ir = emit(&source, triple);
        for (name, argument, result) in ARM64_FLOATS {
            assert_passing(&ir, triple, name, argument, result);
        }
        assert!(!ir.contains("alignstack"), "{triple:?}");
    }
}

#[test]
fn aapcs64_macos_and_linux_pass_like_windows_arm64_and_linux_aligns_the_stack() {
    let source = take_and_return(FLOATS, "");
    let ir = emit(&source, Triple::Aarch64AppleDarwin);
    for (name, argument, result) in ARM64_FLOATS {
        assert_passing(&ir, Triple::Aarch64AppleDarwin, name, argument, result);
    }
    assert!(!ir.contains("alignstack"));
    for triple in LINUX_ARM64 {
        let ir = emit(&source, triple);
        for (name, argument, result) in ARM64_FLOATS {
            // Only an HFA argument carries `alignstack(8)`, in the declaration and at the call.
            let argument = if argument.starts_with('[') && !argument.ends_with("i64]") {
                format!("{argument} alignstack(8)")
            } else {
                (*argument).to_owned()
            };
            assert_passing(&ir, triple, name, &argument, result);
        }
    }
}

#[test]
fn windows_arm64_small_results_are_exact() {
    // An argument of at most 8 bytes fills an `i64`; a result is exactly as wide as the record.
    let expected: &[(&str, &str, &str)] = &[
        ("B1", "i64", "i8"),
        ("B2", "i64", "i16"),
        ("B3", "i64", "i24"),
        ("B5", "i64", "i40"),
        ("B6", "i64", "i48"),
        ("B7", "i64", "i56"),
        ("B8", "i64", "i64"),
        ("H8", "i64", "i64"),
        ("B9", "[2 x i64]", "[2 x i64]"),
        ("B16", "[2 x i64]", "[2 x i64]"),
        ("H16", "[2 x i64]", "[2 x i64]"),
        ("B24", "ptr", "sret"),
    ];
    let source = take_and_return(SIZES, TAG);
    for triple in WINDOWS_ARM64
        .into_iter()
        .chain(LINUX_ARM64)
        .chain([Triple::Aarch64AppleDarwin])
    {
        let ir = emit(&source, triple);
        for (name, argument, result) in expected {
            assert_passing(&ir, triple, name, argument, result);
        }
    }
}

#[test]
fn system_v_passes_eightbytes() {
    let sizes: &[(&str, &str, &str)] = &[
        ("B1", "i8", "i8"),
        ("B2", "i16", "i16"),
        ("B3", "i24", "i24"),
        // A field that `pack` moved off its alignment: memory.
        ("B5", "ptr byval(%R) align 8", "sret"),
        ("B6", "i48", "i48"),
        ("B7", "i56", "i56"),
        ("B8", "i64", "i64"),
        ("F8", "<2 x float>", "<2 x float>"),
        ("D8", "double", "double"),
        ("H8", "i64", "i64"),
        ("B9", "i64, i8", "{ i64, i8 }"),
        ("B12", "<2 x float>, float", "{ <2 x float>, float }"),
        ("B16", "i64, i64", "{ i64, i64 }"),
        ("D16", "double, double", "{ double, double }"),
        ("H16", "i64, i32", "{ i64, i32 }"),
        ("B24", "ptr byval(%R) align 8", "sret"),
    ];
    let floats: &[(&str, &str, &str)] = &[
        ("F1", "float", "float"),
        (
            "F4",
            "<2 x float>, <2 x float>",
            "{ <2 x float>, <2 x float> }",
        ),
        ("F5", "ptr byval(%R) align 8", "sret"),
        ("D4", "ptr byval(%R) align 8", "sret"),
        ("Mixed", "double, float", "{ double, float }"),
        ("Nested", "<2 x float>, float", "{ <2 x float>, float }"),
        // `pack` that leaves every field on its alignment still passes in registers.
        ("Packed", "<2 x float>", "<2 x float>"),
        (
            "PackedQuad",
            "<2 x float>, <2 x float>",
            "{ <2 x float>, <2 x float> }",
        ),
        // An integer in the eightbyte makes it an integer eightbyte.
        ("FloatInt", "i64", "i64"),
    ];
    for triple in LINUX_X64 {
        let ir = emit(&take_and_return(SIZES, TAG), triple);
        for (name, argument, result) in sizes {
            assert_passing(&ir, triple, name, argument, result);
        }
        let ir = emit(&take_and_return(FLOATS, ""), triple);
        for (name, argument, result) in floats {
            assert_passing(&ir, triple, name, argument, result);
        }
    }
}

const COUNTING: &str = "\
@c_layout
record Rect { left: i32, top: i32, right: i32, bottom: i32 }
@c_layout
record P2D { x: f64, y: f64 }
@c_layout
record L3 { a: i64, b: i64, c: i64 }
@trusted
extern \"five_then_rect\" def five_then_rect :: i64 -> i64 -> i64 -> i64 -> i64 -> Rect -> i64 -> i32
@trusted
extern \"seven_doubles_then_p2d\" def seven_doubles_then_p2d :: f64 -> f64 -> f64 -> f64 -> f64 -> f64 -> f64 -> P2D -> f64 -> i32
@trusted
extern \"sret_counts\" def sret_counts :: i64 -> i64 -> i64 -> i64 -> Rect -> L3
@trusted
extern \"four_then_rect\" def four_then_rect :: i64 -> i64 -> i64 -> i64 -> Rect -> i32
export def run :: own i64 -> i64
fn run n =
    let rect = Rect { left: 1i32, top: 2i32, right: 3i32, bottom: 4i32 }
    let a = five_then_rect n n n n n rect n
    let b = seven_doubles_then_p2d 1.0 2.0 3.0 4.0 5.0 6.0 7.0 (P2D { x: 0.5, y: 1.5 }) 8.0
    let c = sret_counts n n n n rect
    let d = four_then_rect n n n n rect
    (a as i64) + (b as i64) + c.a + (d as i64)
";

#[test]
fn system_v_counts_registers_over_the_call() {
    for triple in LINUX_X64 {
        let ir = emit(COUNTING, triple);
        let declares = declares(&ir);
        // One integer register is left for a record of two: the record goes to memory as a whole,
        // and the scalar after it still gets the register.
        assert_eq!(
            declares["five_then_rect"],
            "i32 @five_then_rect(i64, i64, i64, i64, i64, ptr byval([16 x i8]) align 8, i64)"
        );
        assert_eq!(
            declares["seven_doubles_then_p2d"],
            "i32 @seven_doubles_then_p2d(double, double, double, double, double, double, double, ptr byval([16 x i8]) align 8, double)"
        );
        // The `sret` pointer of the result uses an integer register.
        assert_eq!(
            declares["sret_counts"],
            "void @sret_counts(ptr sret([24 x i8]), i64, i64, i64, i64, ptr byval([16 x i8]) align 8)"
        );
        assert_eq!(
            declares["four_then_rect"],
            "i32 @four_then_rect(i64, i64, i64, i64, i64, i64)"
        );
        let call = ir
            .lines()
            .find(|line| line.contains("call i32 @five_then_rect("))
            .unwrap();
        assert!(call.contains("ptr byval([16 x i8]) align 8 %"), "{call}");
    }
    // The other targets classify each position alone.
    for (triple, rect) in [
        (Triple::X86_64WindowsMsvc, "ptr"),
        (Triple::Aarch64WindowsMsvc, "[2 x i64]"),
        (Triple::Aarch64AppleDarwin, "[2 x i64]"),
        (Triple::Aarch64LinuxGnu, "[2 x i64]"),
    ] {
        let declares = declares(&emit(COUNTING, triple));
        assert_eq!(
            declares["five_then_rect"],
            format!("i32 @five_then_rect(i64, i64, i64, i64, i64, {rect}, i64)"),
            "{triple:?}"
        );
        assert_eq!(
            declares["sret_counts"],
            format!("void @sret_counts(ptr sret([24 x i8]), i64, i64, i64, i64, {rect})"),
            "{triple:?}"
        );
    }
}

const COPIES: &str = "\
@c_layout
record L3 { a: i64, b: i64, c: i64 }
@trusted
extern \"scribble\" def scribble :: L3 -> L3 -> i32
@trusted
extern \"rotate\" def rotate :: L3 -> L3
export def run :: own i64 -> i64
fn run n =
    let value = L3 { a: n, b: n, c: n }
    let first = scribble value value
    let second = scribble value value
    let rotated = rotate value
    (first as i64) + (second as i64) + rotated.a
";

/// The operands of the call of `symbol`, as written.
fn call_operands(ir: &str, symbol: &str) -> Vec<Vec<String>> {
    ir.lines()
        .filter(|line| line.contains("call ") && line.contains(&format!("@{symbol}(")))
        .map(|line| {
            let inside = line.split_once(&format!("@{symbol}(")).unwrap().1;
            let inside = inside.strip_suffix(')').unwrap_or(inside);
            inside.split(", ").map(str::to_owned).collect()
        })
        .collect()
}

#[test]
fn memory_copies_are_fresh() {
    for triple in [
        Triple::X86_64WindowsMsvc,
        Triple::Aarch64WindowsMsvc,
        Triple::Aarch64AppleDarwin,
        Triple::X86_64LinuxGnu,
    ] {
        let ir = emit(COPIES, triple);
        // An extern is one function that makes the C call, so each call of it has a frame of its
        // own. In it, one value passed twice is two copies, each in a temporary of its own: a
        // callee that writes to an argument changes neither the other argument nor the value.
        let calls = call_operands(&ir, "scribble");
        assert_eq!(calls.len(), 1, "{triple:?}\n{ir}");
        let slots: Vec<&str> = calls[0]
            .iter()
            .map(|operand| operand.rsplit(' ').next().unwrap())
            .collect();
        assert_eq!(slots.len(), 2, "{triple:?}: {calls:?}");
        assert_ne!(slots[0], slots[1], "{triple:?}: a temporary is shared");
        for slot in &slots {
            assert!(
                ir.contains(&format!("{slot} = alloca [24 x i8], align 16")),
                "{triple:?}: {slot} is not a by-value temporary\n{ir}"
            );
            assert!(
                ir.contains(&format!("store [24 x i8] zeroinitializer, ptr {slot}")),
                "{triple:?}: {slot} is not cleared"
            );
        }
        // A result in memory is the first parameter with `sret`, in a temporary of its own.
        let rotate = &call_operands(&ir, "rotate")[0];
        assert!(
            rotate[0].starts_with("ptr sret([24 x i8]) %"),
            "{triple:?}: {rotate:?}"
        );
        let result = rotate[0].rsplit(' ').next().unwrap();
        let argument = rotate[1].rsplit(' ').next().unwrap();
        assert_ne!(result, argument, "{triple:?}");
        assert!(
            ir.contains(&format!("{result} = alloca [24 x i8], align 16"))
                && ir.contains(&format!("{argument} = alloca [24 x i8], align 16")),
            "{triple:?}: {rotate:?}"
        );
        // Attributes that only newer LLVM versions read are not part of the ABI and are left out.
        assert!(!ir.contains("dead_on_return") && !ir.contains("dead_on_unwind"));
    }
}

#[test]
fn slots_cover_the_coerced_size() {
    let source = take_and_return(SIZES, TAG);
    let ir = emit(&source, Triple::Aarch64WindowsMsvc);
    // A 3-byte record passes as `i64`: the load must stay inside the temporary.
    let call = ir
        .lines()
        .find(|line| line.contains("@take_B3(i64 %"))
        .expect("take_B3 takes an i64");
    let value = call
        .split_once("@take_B3(i64 ")
        .unwrap()
        .1
        .trim_end_matches(')');
    let load = ir
        .lines()
        .find(|line| {
            line.trim_start()
                .starts_with(&format!("{value} = load i64, ptr "))
        })
        .unwrap_or_else(|| panic!("no load of {value}\n{ir}"));
    let slot = load.rsplit(' ').next().unwrap();
    assert!(
        ir.contains(&format!("{slot} = alloca [8 x i8], align 16")),
        "{load}\n{ir}"
    );
    // A 9-byte record passes as `[2 x i64]`: 16 bytes.
    let call = ir
        .lines()
        .find(|line| line.contains("@take_B9([2 x i64] %"))
        .expect("take_B9 takes [2 x i64]");
    let value = call
        .split_once("@take_B9([2 x i64] ")
        .unwrap()
        .1
        .trim_end_matches(')');
    let load = ir
        .lines()
        .find(|line| {
            line.trim_start()
                .starts_with(&format!("{value} = load [2 x i64], ptr "))
        })
        .unwrap();
    let slot = load.rsplit(' ').next().unwrap();
    assert!(
        ir.contains(&format!("{slot} = alloca [16 x i8], align 16")),
        "{load}"
    );
    // The 3-byte result is `i24`, stored into a temporary of at least 3 bytes.
    assert!(ir.contains("call i24 @ret_B3(i32 "), "{ir}");
}

#[test]
fn wasm_rejects_and_every_native_target_accepts() {
    let source = "\
@c_layout
record Point { x: i32, y: i32 }
@trusted
extern \"take_point\" def take_point :: Point -> i32
@trusted
extern \"ret_point\" def ret_point :: i32 -> Point
@trusted
extern \"unused_point\" def unused_point :: Point -> Point
export def by_argument :: own i32 -> i32 = \\n -> take_point (Point { x: n, y: n })
";
    // `check` does not know the target: it accepts.
    let module = analyze(source).unwrap();
    for triple in Triple::ALL {
        let _scope = PlatformScope::enter(Platform::new(Some(triple)));
        llvm::emit(&module, llvm::Entry::Library)
            .unwrap_or_else(|e| panic!("{triple:?}: {}", e.message));
    }
    let error = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap_err();
    assert_eq!(error.code, "E1008");
    assert_eq!(error.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
    assert!(
        error
            .message
            .contains("'take_point' passes the '@c_layout' record 'Point' by value"),
        "{}",
        error.message
    );
    // A result by value is refused too, and an extern that the program does not reach is not.
    let by_result = source.replace(
        "export def by_argument :: own i32 -> i32 = \\n -> take_point (Point { x: n, y: n })",
        "export def by_result :: own i32 -> i32 = \\n -> (ret_point n).x",
    );
    let error =
        llvm::emit_target(&analyze(&by_result).unwrap(), llvm::Entry::Library, true).unwrap_err();
    assert_eq!(error.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
    assert!(
        error.message.contains("'ret_point' returns"),
        "{}",
        error.message
    );
    let unreached = source.replace(
        "export def by_argument :: own i32 -> i32 = \\n -> take_point (Point { x: n, y: n })",
        "export def plain :: own i32 -> i32 = \\n -> n",
    );
    llvm::emit_target(&analyze(&unreached).unwrap(), llvm::Entry::Library, true).unwrap();
    // `ref Point` stays a pointer and stays available on WebAssembly.
    let by_reference = "\
@c_layout
record Point { x: i32, y: i32 }
extern def take_point :: ref Point -> i32
export def run :: own i32 -> i32 = \\n ->
    let point = Point { x: n, y: n }
    take_point (ref point)
";
    llvm::emit_target(&analyze(by_reference).unwrap(), llvm::Entry::Library, true).unwrap();
}

#[test]
fn unchanged_forms() {
    // An unmarked record, `ref R`, `ref mut R` and `export def` keep the pointer forms (E05, W13,
    // W21): `const T *`, `T *`, and the leading out parameter, which is not `sret`.
    let source = "\
record Plain { x: i32, y: i32 }
@c_layout
record Point { x: i32, y: i32 }
@trusted
extern \"take_plain\" def take_plain :: Plain -> i32
@trusted
extern \"ret_plain\" def ret_plain :: i32 -> Plain
@trusted
extern \"take_ref\" def take_ref :: ref Point -> i32
@trusted
extern \"take_mut\" def take_mut :: ref mut Point -> i32
export def use_plain :: i32 -> i32 = \\n ->
    let p = ret_plain n
    take_plain p
export def use_point :: i32 -> i32 = \\n ->
    let mut p = Point { x: n, y: n }
    let read = take_ref (ref p)
    let written = take_mut (ref mut p)
    read + written
export def echo :: Point -> Point = \\p -> p
";
    for triple in [
        Triple::X86_64WindowsMsvc,
        Triple::Aarch64WindowsMsvc,
        Triple::X86_64LinuxGnu,
        Triple::Aarch64AppleDarwin,
    ] {
        let ir = emit(source, triple);
        let declares = declares(&ir);
        assert_eq!(declares["take_plain"], "i32 @take_plain(ptr)", "{triple:?}");
        assert_eq!(
            declares["ret_plain"], "void @ret_plain(ptr, i32)",
            "{triple:?}"
        );
        assert_eq!(declares["take_ref"], "i32 @take_ref(ptr)", "{triple:?}");
        assert_eq!(declares["take_mut"], "i32 @take_mut(ptr)", "{triple:?}");
        assert!(
            ir.contains("void @tz_echo(ptr %out, ptr %arg0)"),
            "{triple:?}\n{ir}"
        );
        assert!(!ir.contains("sret") && !ir.contains("byval"), "{triple:?}");
    }
    let header = llvm::header(&analyze(source).unwrap());
    assert!(
        header.contains(
            "void tz_echo(tz_record_4Main_5Point *out, const tz_record_4Main_5Point *arg0);"
        ),
        "{header}"
    );
}

#[test]
fn header_prints_values() {
    // An extern without a link name is a prototype of the generated header. A C prototype does not
    // depend on the target: the host's C compiler classifies the struct the same way.
    let source = "\
record Plain { x: i32, y: i32 }
@c_layout
record Point { x: i32, y: i32 }
@c_layout
record L3 { a: i64, b: i64, c: i64 }
extern def move_point :: Point -> i32 -> Point
extern def sum :: L3 -> ref Point -> i64
extern def plain :: Plain -> Plain
export def run :: own i32 -> i64 = \\n ->
    let moved = move_point (Point { x: n, y: n }) n
    let kept = plain (Plain { x: n, y: n })
    sum (L3 { a: 1, b: 2, c: 3 }) (ref moved) + (kept.x as i64)
";
    let module = analyze(source).unwrap();
    let header = llvm::header(&module);
    assert!(
        header.contains(
            "tz_record_4Main_5Point tsuzuri_host_Main_move_point(tz_record_4Main_5Point arg0, int32_t arg1);"
        ),
        "{header}"
    );
    assert!(
        header.contains(
            "int64_t tsuzuri_host_Main_sum(tz_record_4Main_2L3 arg0, const tz_record_4Main_5Point *arg1);"
        ),
        "{header}"
    );
    assert!(
        header.contains(
            "void tsuzuri_host_Main_plain(tz_record_4Main_5Plain *out, const tz_record_4Main_5Plain *arg0);"
        ),
        "{header}"
    );
    // The IR calls the same symbol with the lowered struct.
    let ir = emit(source, Triple::X86_64WindowsMsvc);
    assert_eq!(
        declares(&ir)["tsuzuri_host_Main_move_point"],
        "i64 @tsuzuri_host_Main_move_point(i64, i32)"
    );
}

#[test]
fn blocking_wrap_finds_the_callee() {
    // In a program that spawns, every extern call is a blocking call (TZ08). The wrapper finds the
    // callee of a call with an `sret` operand and of a call that returns an aggregate.
    let source = "\
@c_layout
record PointF { x: f32, y: f32 }
@c_layout
record L3 { a: i64, b: i64, c: i64 }
@trusted
extern \"ret_pointf\" def ret_pointf :: i32 -> PointF
@trusted
extern \"ret_l3\" def ret_l3 :: L3 -> L3
export def run :: own i32 -> i64
fn run n =
    tzroutine {
        let _point = ret_pointf n
    }
    let point = ret_pointf n
    let triple = ret_l3 (L3 { a: 1, b: 2, c: 3 })
    triple.a + (point.x as i64)
";
    for triple in [Triple::Aarch64AppleDarwin, Triple::X86_64LinuxGnu] {
        let ir = emit(source, triple);
        let lines: Vec<&str> = ir.lines().map(str::trim).collect();
        let mut found = 0;
        for (at, line) in lines.iter().enumerate() {
            if line.contains("call ")
                && (line.contains("@ret_pointf(") || line.contains("@ret_l3("))
            {
                assert_eq!(
                    lines[at - 1],
                    "call void @tsuzuri_tz_blocking_enter()",
                    "{line}"
                );
                assert_eq!(
                    lines[at + 1],
                    "call void @tsuzuri_tz_blocking_exit()",
                    "{line}"
                );
                found += 1;
            }
        }
        // One C call for each extern: the function of the extern makes it.
        assert_eq!(found, 2, "{triple:?}\n{ir}");
        assert!(
            ir.contains("call void @ret_l3(ptr sret([24 x i8]) %"),
            "{ir}"
        );
    }
    let ir = emit(source, Triple::Aarch64AppleDarwin);
    assert!(
        ir.contains("= call { float, float } @ret_pointf(i32 "),
        "{ir}"
    );
}

#[test]
fn input_only_records_pass_by_value_and_do_not_return() {
    // W21 Phase 2: a record with `FnPtr` or `ref ZString` is input only. By value it is an argument;
    // as a result it keeps the diagnostics of W21.
    let source = "\
extern type copy Hwnd
@c_layout
record Class { callback: FnPtr<(Hwnd -> i32)>, name: ref ZString }
@trusted
extern \"register\" def register :: Class -> i32
def wnd_proc :: Hwnd -> i32 = \\_ -> 1i32
export def run :: i32
fn run = register (Class { callback: FnPtr.of wnd_proc, name: ZString.literal \"W\" })
";
    let declares_x64 = declares(&emit(source, Triple::X86_64WindowsMsvc));
    assert_eq!(declares_x64["register"], "i32 @register(ptr)");
    let declares_arm64 = declares(&emit(source, Triple::Aarch64AppleDarwin));
    assert_eq!(declares_arm64["register"], "i32 @register([2 x i64])");
    for (result, kind) in [
        (
            "@c_layout\nrecord R { cb: FnPtr<(i32 -> i32)> }",
            DiagnosticKind::FnptrFromHost,
        ),
        (
            "@c_layout\nrecord R { title: ref ZString }",
            DiagnosticKind::CLayoutInputOnly,
        ),
    ] {
        let error = analyze(&format!(
            "{result}\n@trusted\nextern \"make\" def make :: unit -> R\n0\n"
        ))
        .unwrap_err();
        assert_eq!(error.code, "E1008");
        assert_eq!(error.kind, Some(kind.id()), "{}", error.message);
    }
}

#[test]
fn declarations_of_one_symbol_agree_on_the_struct() {
    // W14: two modules may declare one C symbol. A by-value position needs a record of the same C
    // shape in each declaration; a pointer there is another C ABI.
    let a = "\
@c_layout
record Point { x: i32, y: i32 }
extern type copy Hwnd
@trusted
extern \"WindowFromPoint\" def window_from_point :: Point -> Maybe<Hwnd>
@trusted
extern \"Offset\" def offset :: Hwnd -> Point -> Point
def from_a :: own i32 -> i32 = \\n ->
    match window_from_point (Point { x: n, y: n }) with
    | Maybe.Some _ -> 1i32
    | Maybe.None -> 0i32
";
    let same_shape = "\
@c_layout
record Pt { h: i32, v: i32 }
@trusted
extern \"WindowFromPoint\" def window_at :: Pt -> i64
@trusted
extern \"Offset\" def offset :: i64 -> Pt -> Pt
export def run :: own i32 -> i64 = \\n ->
    let moved = offset 1 (Pt { h: n, v: n })
    window_at moved + (A.from_a n as i64)
";
    let module = analyze_modules(&[("A.tz", a), ("Main.tz", same_shape)])
        .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    for (triple, point) in [
        (Triple::X86_64WindowsMsvc, "i64"),
        (Triple::Aarch64AppleDarwin, "i64"),
        (Triple::X86_64LinuxGnu, "i64"),
    ] {
        let _scope = PlatformScope::enter(Platform::new(Some(triple)));
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        let declares = declares(&ir);
        // The handle and the `i64` of the two declarations meet in `i64` (W14); the struct is
        // lowered once, for the target.
        assert_eq!(
            declares["WindowFromPoint"],
            format!("i64 @WindowFromPoint({point})"),
            "{triple:?}"
        );
        assert_eq!(
            ir.lines()
                .filter(|line| line.starts_with("declare ") && line.contains("@WindowFromPoint("))
                .count(),
            1
        );
    }
    // Records in memory: `sret` and `byval` name the bytes, not the record, so two records of one
    // shape still give one `declare`.
    let large_a = "\
@c_layout
record Matrix { a: f64, b: f64, c: f64 }
@trusted
extern \"Transpose\" def transpose :: Matrix -> Matrix
def from_a :: own f64 -> f64 = \\n -> (transpose (Matrix { a: n, b: n, c: n })).a
";
    let large_main = "\
@c_layout
record M3 { x: f64, y: f64, z: f64 }
@trusted
extern \"Transpose\" def flip :: M3 -> M3
export def run :: own f64 -> f64 = \\n -> (flip (M3 { x: n, y: n, z: n })).x + A.from_a n
";
    let large = analyze_modules(&[("A.tz", large_a), ("Main.tz", large_main)])
        .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    for (triple, expected) in [
        (
            Triple::X86_64WindowsMsvc,
            "void @Transpose(ptr sret([24 x i8]), ptr)",
        ),
        (
            Triple::Aarch64AppleDarwin,
            "{ double, double, double } @Transpose([3 x double])",
        ),
        (
            Triple::X86_64LinuxGnu,
            "void @Transpose(ptr sret([24 x i8]), ptr byval([24 x i8]) align 8)",
        ),
    ] {
        let _scope = PlatformScope::enter(Platform::new(Some(triple)));
        let ir = llvm::emit(&large, llvm::Entry::Library).unwrap();
        let lines: Vec<&str> = ir
            .lines()
            .filter(|line| line.starts_with("declare ") && line.contains("@Transpose("))
            .collect();
        assert_eq!(lines, vec![format!("declare {expected}")], "{triple:?}");
    }
    for (other, reason) in [
        (
            "@c_layout\nrecord Pt { h: i32, v: i32 }\n@trusted\nextern \"WindowFromPoint\" def window_at :: ref Pt -> i64\n0\n",
            "by value",
        ),
        (
            "@c_layout\nrecord Pt { h: i64 }\n@trusted\nextern \"WindowFromPoint\" def window_at :: Pt -> i64\n0\n",
            "by value",
        ),
        (
            "@trusted\nextern \"WindowFromPoint\" def window_at :: i64 -> i64\n0\n",
            "by value",
        ),
    ] {
        let error = analyze_modules(&[("A.tz", a), ("Main.tz", other)]).unwrap_err();
        assert_eq!(error.code, "E1008", "{}", error.message);
        assert_eq!(
            error.kind,
            Some(DiagnosticKind::ConflictingExternDeclaration.id())
        );
        assert!(error.message.contains(reason), "{}", error.message);
    }
}

#[test]
fn arguments_are_evaluated_left_to_right_and_copied_at_once() {
    // The arguments are evaluated left to right by the caller. The function of the extern then
    // writes each by-value record to its temporary in parameter order, and loads the registers
    // after the last write, immediately before the call.
    let source = "\
@c_layout
record Point { x: i32, y: i32 }
extern def mark :: i32 -> i32
@trusted
extern \"two\" def two :: Point -> i32 -> Point -> i32
export def run :: own i32 -> i32 = \\n ->
    two (Point { x: mark 1i32, y: n }) (mark 2i32) (Point { x: mark 3i32, y: n })
";
    let ir = emit(source, Triple::Aarch64AppleDarwin);
    let lines: Vec<&str> = ir.lines().map(str::trim).collect();
    let position = |needle: &str, from: usize| {
        lines
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, line)| line.contains(needle))
            .map(|(at, _)| at)
            .unwrap_or_else(|| panic!("no '{needle}' after line {from}\n{ir}"))
    };
    let first = position("call i32 @tz.fn.Main.mark(i32 1)", 0);
    let second = position("call i32 @tz.fn.Main.mark(i32 2)", first);
    let third = position("call i32 @tz.fn.Main.mark(i32 3)", second);
    position("call i32 @tz.fn.Main.two(", third);
    // The function of the extern, up to its closing brace.
    let begin = position("@tz.fn.Main.two(%tz.record.Main.Point %arg0", 0);
    let body: Vec<&str> = lines[begin..]
        .iter()
        .copied()
        .take_while(|line| *line != "}")
        .collect();
    let within = |needle: &str| {
        body.iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no '{needle}' in the extern\n{}", body.join("\n")))
    };
    let call = within("call i32 @two(i64 ");
    let operands = &call_operands(&ir, "two")[0];
    assert_eq!(operands.len(), 3, "{operands:?}");
    let loaded = |operand: &str| {
        let value = operand.rsplit(' ').next().unwrap();
        let at = within(&format!("{value} = load i64, ptr "));
        (at, body[at].rsplit(' ').next().unwrap().to_owned())
    };
    let (first_load, first_slot) = loaded(&operands[0]);
    let (second_load, second_slot) = loaded(&operands[2]);
    let cleared_first = within(&format!("store [8 x i8] zeroinitializer, ptr {first_slot}"));
    let cleared_second = within(&format!(
        "store [8 x i8] zeroinitializer, ptr {second_slot}"
    ));
    let last_write = body
        .iter()
        .rposition(|line| line.starts_with("store i32 ") && line.ends_with("align 4"))
        .expect("the fields are written");
    assert!(
        cleared_first < cleared_second,
        "the copies follow the parameter order"
    );
    assert!(
        cleared_second < last_write && last_write < first_load,
        "the registers are loaded after the last copy"
    );
    assert!(first_load < second_load && second_load < call);
    assert_eq!(declares(&ir)["two"], "i32 @two(i64, i32, i64)");
}
