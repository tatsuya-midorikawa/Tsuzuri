use std::process::Command;

use tsuzuri::{analyze, llvm, trap::TrapSource};

const X86: u8 = 0b1110;
const ARM: u8 = 0b11_0000;

const DOT: &str = "@cpu [\"avx2\", \"sse4.2\", \"avx512\", \"sve\"]
export def dot :: ref [f64] -> ref [f64] -> f64
fn dot left right =
    let mut total: f64x4 = Simd.splat 0.0
    let mut index = 0
    while index + 4 <= left.length do
        let first: f64x4 = Simd.load left index
        let second: f64x4 = Simd.load right index
        total = total + first * second
        index = index + 4
    let mut sum = Simd.sum_lanes total
    while index < left.length do
        sum = sum + left[index] * right[index]
        index = index + 1
    sum
";

fn options(entry: llvm::Entry) -> llvm::EmitOptions {
    llvm::EmitOptions {
        entry,
        wasm: false,
        debug_output: false,
        allocator: llvm::Allocator::System,
    }
}

fn sources(source: &str) -> Vec<TrapSource<'_>> {
    std::iter::once(TrapSource {
        path: "Main.tz",
        text: source,
    })
    .chain(
        tsuzuri::stdlib::SOURCES
            .iter()
            .map(|(path, text)| TrapSource { path, text }),
    )
    .collect()
}

fn build(source: &str, debug: Option<bool>, trap_info: bool, levels: u8) -> String {
    build_entry(source, llvm::Entry::Library, debug, trap_info, levels)
}

fn build_entry(
    source: &str,
    entry: llvm::Entry,
    debug: Option<bool>,
    trap_info: bool,
    levels: u8,
) -> String {
    let module = analyze(source).unwrap_or_else(|error| panic!("{error:?}"));
    llvm::emit_native_build_for(
        &module,
        options(entry),
        &sources(source),
        debug,
        trap_info,
        levels,
    )
    .unwrap()
    .ir
}

/// The definition of `name` in `ir`, from its define line through its closing brace.
fn definition<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .match_indices("define ")
        .map(|(at, _)| at)
        .find(|&at| {
            ir[at..]
                .lines()
                .next()
                .is_some_and(|line| line.contains(&format!(" {name}(")))
        })
        .unwrap_or_else(|| panic!("{name} is not defined"));
    let end = start + ir[start..].find("\n}\n").unwrap() + 3;
    &ir[start..end]
}

#[test]
fn cpu_functions_get_a_version_per_detected_level_and_a_stub() {
    let ir = build(DOT, None, false, X86);
    let stub = definition(&ir, "@tz.fn.Main.dot");
    assert!(
        stub.contains("call i32 @tsuzuri_cpu_pick(i64 14)"),
        "{stub}"
    );
    for level in ["sse4.2", "avx2", "avx512"] {
        assert!(stub.contains(&format!(
            "tail call double @\"tz.fn.Main.dot.cpu.{level}\"("
        )));
    }
    assert!(stub.contains("tail call double @\"tz.fn.Main.dot.cpu.baseline\"("));
    assert!(ir.contains("@\"tz.fn.Main.dot.cpu\" = internal global i32 -1"));
    let avx2 = definition(&ir, "@\"tz.fn.Main.dot.cpu.avx2\"");
    assert!(
        avx2.lines()
            .next()
            .unwrap()
            .ends_with("nounwind \"target-features\"=\"+avx2\" {")
    );
    // The builtins that pass `<4 x double>` run with the same instruction set.
    assert!(avx2.contains(".cpu.avx2\"(%tz.array"), "{avx2}");
    assert!(ir.lines().any(|line| {
        line.starts_with("define internal <4 x double> @\"tz.fn.$builtin.Simd.load.")
            && line.contains(".cpu.avx2\"(")
            && line.ends_with("\"target-features\"=\"+avx2\" {")
    }));
    assert!(!ir.contains("cpu.sve") && !ir.contains("tz-cpu"));
    assert_eq!(
        ir.lines()
            .filter(|line| *line == "declare i32 @tsuzuri_cpu_pick(i64)")
            .count(),
        1
    );
    assert_eq!(ir, build(DOT, None, false, X86));

    let arm = build(DOT, None, false, ARM);
    assert!(arm.contains("call i32 @tsuzuri_cpu_pick(i64 16)"));
    assert!(
        arm.contains("@\"tz.fn.Main.dot.cpu.sve\"(")
            && arm.contains("\"target-features\"=\"+sve\"")
    );
    assert!(!arm.contains("avx") && !arm.contains("tz-cpu"));

    // Without a detected level the function stays as it is.
    let portable = build(DOT, None, false, 0);
    assert!(
        !portable.contains("tsuzuri_cpu_pick")
            && !portable.contains(".cpu.")
            && !portable.contains("tz-cpu")
    );
    assert!(portable.contains("define internal double @tz.fn.Main.dot("));
    let module = analyze(DOT).unwrap();
    for wasm in [false, true] {
        assert!(
            !llvm::emit_target(&module, llvm::Entry::Library, wasm)
                .unwrap()
                .contains("tz-cpu")
        );
    }
}

#[test]
fn cpu_versions_drop_debug_information_and_keep_trap_sites() {
    let source = "@cpu [\"avx2\"]\nexport def scale :: i64 -> i64\nfn scale value =\n    let wide: i32x8 = Simd.splat (value as i32)\n    let doubled = wide + wide\n    (Simd.sum_lanes doubled) as i64 + value / (value - 3)\n";
    let ir = build(source, Some(false), true, X86);
    let portable = definition(&ir, "@\"tz.fn.Main.scale.cpu.baseline\"");
    let subprogram = |text: &str| {
        let define = text.lines().next().unwrap();
        define[define.find(" !dbg !").unwrap()..].to_owned()
    };
    let avx2 = definition(&ir, "@\"tz.fn.Main.scale.cpu.avx2\"");
    assert!(
        !avx2.contains("!dbg") && !avx2.contains("llvm.dbg."),
        "{avx2}"
    );
    // The stub has a subprogram of its own and locates its calls in it.
    let stub = definition(&ir, "@tz.fn.Main.scale");
    assert_ne!(subprogram(stub), subprogram(portable));
    assert!(
        stub.lines()
            .filter(|line| line.contains("call "))
            .all(|line| line.contains(", !dbg !"))
    );
    // Both versions report the division by zero at the same sites.
    let reports = |text: &str| {
        text.lines()
            .filter_map(|line| line.trim().strip_prefix("call void @tz.trap.report("))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert!(!reports(avx2).is_empty());
    assert_eq!(reports(portable), reports(avx2));
}

/// Compiles `ir` for `target` and returns the assembly. LLVM must keep the debug information.
fn assembly(ir: &str, target: &str) -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "tsuzuri-multiversion-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("module.ll");
    std::fs::write(&path, ir).unwrap();
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let output = Command::new(clang)
        .args([
            "-Wno-override-module",
            "-O2",
            "-S",
            "-o",
            "-",
            "-target",
            target,
        ])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{errors}");
    assert!(!errors.contains("invalid debug info"), "{errors}");
    String::from_utf8(output.stdout).unwrap()
}

/// The assembly of the function labelled `name`.
fn function_assembly<'a>(assembly: &'a str, name: &str) -> &'a str {
    let begin = assembly
        .match_indices(&format!("\n{name}:"))
        .map(|(at, _)| at + 1)
        .next()
        .unwrap_or_else(|| panic!("{name} has no label"));
    let end = assembly[begin..]
        .find(".cfi_endproc")
        .map_or(assembly.len(), |end| begin + end);
    &assembly[begin..end]
}

#[test]
fn cpu_versions_compile_for_their_instruction_sets() {
    let x86 = assembly(&build(DOT, None, false, X86), "x86_64-unknown-linux-gnu");
    assert!(function_assembly(&x86, "tz.fn.Main.dot.cpu.avx2").contains("ymm"));
    assert!(function_assembly(&x86, "tz.fn.Main.dot.cpu.avx512").contains("mm"));
    assert!(function_assembly(&x86, "tz.fn.Main.dot.cpu.sse4.2").contains("xmm"));
    // The export inlines the stub and the portable version, which use no AVX register.
    assert!(!function_assembly(&x86, "tz_dot").contains("ymm"));
    let arm = assembly(&build(DOT, None, false, ARM), "aarch64-unknown-linux-gnu");
    assert!(arm.contains("\ntz.fn.Main.dot.cpu.sve:"));
    // A program with debug information and trap sites keeps both through the versions.
    let program = format!(
        "{}\nlet values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]\n(dot (ref values) (ref values)) as i64\n",
        DOT.replace("export def", "def")
    );
    for (levels, target) in [
        (X86, "x86_64-unknown-linux-gnu"),
        (ARM, "aarch64-unknown-linux-gnu"),
    ] {
        let ir = build_entry(&program, llvm::Entry::Console, Some(true), true, levels);
        assert!(ir.contains("call i32 @tsuzuri_cpu_pick("));
        assert!(assembly(&ir, target).contains(".debug_info"));
    }
}

#[test]
fn cpu_attributes_are_validated() {
    for (source, code) in [
        ("@cpu [\"avx3\"]\ndef f :: i64 -> i64\nfn f x = x", "E0002"),
        (
            "@cpu [\"avx2\", \"avx2\"]\ndef f :: i64 -> i64\nfn f x = x",
            "E0002",
        ),
        ("@cpu []\ndef f :: i64 -> i64\nfn f x = x", "E0002"),
        ("@cpu [avx2]\ndef f :: i64 -> i64\nfn f x = x", "E0002"),
        ("@cpu [\"avx2\"]\nrecord P { x: i64 }\n0", "E0002"),
        ("@cpu [\"avx2\"]\nfn f (x: i64) -> i64 { x }\n0", "E0002"),
        (
            "@cpu [\"avx2\"]\ndef f :: i32x8 -> i32\nfn f vector = Simd.sum_lanes vector",
            "E1005",
        ),
        (
            "record Wide { lanes: f64x4 }\n@cpu [\"avx2\"]\ndef f :: i64 -> Wide\nfn f x = Wide { lanes: Simd.splat (x as f64) }",
            "E1005",
        ),
        (
            "@cpu [\"avx2\"]\ndef same :: 'a -> 'a\nfn same x = x\nlet value: i32x8 = same (Simd.splat 1i32)\nSimd.sum_lanes value",
            "E1005",
        ),
        (
            "@cpu [\"avx2\"]\ndef apply :: (i32x8 -> i32x8) -> i32 -> i32\nfn apply f x = Simd.sum_lanes (f (Simd.splat x))",
            "E1005",
        ),
    ] {
        match analyze(source) {
            Ok(_) => panic!("accepted:\n{source}"),
            Err(error) => assert_eq!(error.code, code, "{source}\n{error:?}"),
        }
    }
    // A function value that passes 128-bit vectors, a generic function, and an exported
    // function with a private signature are fine.
    for source in [
        "@cpu [\"avx2\"]\ndef apply :: (i32x4 -> i32x4) -> i32 -> i32\nfn apply f x = Simd.sum_lanes (f (Simd.splat x))",
        "@cpu [\"sse4.2\", \"sve2\"]\ndef same :: 'a -> 'a\nfn same x = x\nsame 1",
        "/// Doubles.\n@cpu [\"avx2\"]\nprivate def twice :: i64 -> i64\nfn twice x = x * 2\ntwice 21",
    ] {
        analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
}
