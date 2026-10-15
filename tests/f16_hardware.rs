//! The f16 lowering (D10): which f16 operations are LLVM `half` instructions and which stay the
//! bundled integer runtime, per `llvm::F16Path`. The bit-exact results are the job of
//! `tests/f16_hardware.mjs`; these tests pin the generated IR and the assembly it lowers to.

use std::process::Command;

use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry, F16Path};

const PATHS: [F16Path; 3] = [F16Path::Soft, F16Path::Conversions, F16Path::Hardware];

fn emit_f16(source: &str, f16: F16Path, wasm: bool) -> String {
    let module = tsuzuri::analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    llvm::emit_with_options(
        &module,
        EmitOptions {
            entry: Entry::Library,
            wasm,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: Allocator::System,
            f16,
        },
    )
    .unwrap()
}

/// The text of the function `@tz.fn.Main.<name>`, from its `define` line to its closing brace.
fn body<'a>(ir: &'a str, name: &str) -> &'a str {
    let marker = format!("@tz.fn.Main.{name}(");
    let start = ir
        .lines()
        .scan(0, |offset, line| {
            let at = *offset;
            *offset += line.len() + 1;
            Some((at, line))
        })
        .find(|(_, line)| line.starts_with("define ") && line.contains(&marker))
        .unwrap_or_else(|| panic!("no function {name}"))
        .0;
    let end = ir[start..]
        .find("\n}")
        .expect("a function ends with a brace");
    &ir[start..start + end]
}

/// Whether `text` uses LLVM's `half` type. A word of its own: `half_add` is a name.
fn has_half(text: &str) -> bool {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .any(|word| word == "half")
}

fn binary(operator: &str) -> String {
    format!("def f :: f16 -> f16 -> f16\nfn f x y = x {operator} y")
}

fn comparison(operator: &str) -> String {
    format!("def f :: f16 -> f16 -> bool\nfn f x y = x {operator} y")
}

fn lines_equal(ir: &str, line: &str) -> usize {
    ir.lines().filter(|candidate| *candidate == line).count()
}

#[test]
fn default_emission_and_wasm_stay_soft() {
    let source = binary("+");
    let module = tsuzuri::analyze(&source).unwrap();
    let library = llvm::emit(&module, Entry::Library).unwrap();
    assert!(library.contains("call void @tz_soft_op"));
    assert!(!has_half(&library));
    assert_eq!(library, emit_f16(&source, F16Path::Soft, false));
    assert_eq!(
        library,
        llvm::emit_target(&module, Entry::Library, false).unwrap()
    );
    // WASM has no `half` instructions: LLVM would call `__extendhfsf2` and `__truncsfhf2`.
    for path in PATHS {
        let wasm = emit_f16(&source, path, true);
        assert!(wasm.contains("call void @tz_soft_op"), "{path:?}");
        assert!(!has_half(&wasm), "{path:?}");
    }
}

#[test]
fn wasm_output_is_identical_for_every_path() {
    let source = format!(
        "{}\n{}\n{}\n{}\n{}",
        binary("*"),
        "def g :: f16 -> f32\nfn g x = x as f32",
        "def h :: f64 -> f16\nfn h x = x as f16",
        "def r :: f16 -> f16\nfn r x = Math.sqrt x",
        "def c :: f16 -> f16 -> bool\nfn c x y = x <= y"
    );
    let soft = emit_f16(&source, F16Path::Soft, true);
    for path in PATHS {
        assert_eq!(soft, emit_f16(&source, path, true), "{path:?}");
    }
}

#[test]
fn f16_free_programs_are_unchanged() {
    let source = "def mix :: f32 -> f64 -> i64 -> f64\nfn mix x y n = (x as f64) * y + (n as f64) + Math.sqrt y\n\
                  def compare :: f32 -> f32 -> bool\nfn compare x y = x < y || x == y\n\
                  def narrow :: f64 -> f32\nfn narrow x = x as f32\n\
                  def wide :: f128 -> f128 -> f128\nfn wide x y = x / y";
    for wasm in [false, true] {
        let soft = emit_f16(source, F16Path::Soft, wasm);
        for path in PATHS {
            assert_eq!(soft, emit_f16(source, path, wasm), "{path:?} wasm {wasm}");
        }
        assert!(!has_half(&soft));
    }
}

#[test]
fn emission_is_deterministic() {
    let source = format!(
        "{}\ndef g :: f16 -> f64\nfn g x = Math.sqrt x as f64\ndef h :: f64 -> f16\nfn h x = x as f16",
        binary("/")
    );
    for path in PATHS {
        assert_eq!(
            emit_f16(&source, path, false),
            emit_f16(&source, path, false),
            "{path:?}"
        );
    }
}

#[test]
fn emits_half_arithmetic_when_enabled() {
    for (operator, instruction, product) in [
        ("+", "fadd half", false),
        ("-", "fsub half", false),
        ("*", "fmul half", true),
        ("/", "fdiv half", true),
    ] {
        let ir = emit_f16(&binary(operator), F16Path::Hardware, false);
        let function = body(&ir, "f");
        assert!(function.contains(instruction), "{operator}\n{function}");
        assert!(!function.contains("tz_soft_"), "{operator}");
        // A NaN result is replaced by 0x7E00 with a sign taken from the operands, not the result.
        assert!(function.contains("icmp ugt i16"), "{operator}");
        assert!(function.contains(", 32256"), "{operator}");
        assert_eq!(function.contains("xor i16"), product, "{operator}");
        // The runtime is not even linked when nothing else needs it.
        assert!(!ir.contains("@tz_soft_op"), "{operator}");
    }
}

#[test]
fn emits_half_comparisons_with_ordered_predicates() {
    for (operator, predicate) in [
        ("==", "fcmp oeq half"),
        ("!=", "fcmp une half"),
        ("<", "fcmp olt half"),
        ("<=", "fcmp ole half"),
        (">", "fcmp ogt half"),
        (">=", "fcmp oge half"),
    ] {
        let ir = emit_f16(&comparison(operator), F16Path::Hardware, false);
        let function = body(&ir, "f");
        assert!(function.contains(predicate), "{operator}\n{function}");
        assert!(!function.contains("tz_soft_cmp"), "{operator}");
    }
}

#[test]
fn emits_half_sqrt_and_declares_intrinsic_once() {
    // `Math.sin` links `math.ll`, which dedupes declarations, and the f16 floor links `numeric.ll`.
    let source = "def a :: f16 -> f16\nfn a x = Math.sqrt x\n\
                  def b :: f16 -> f16\nfn b x = Math.sqrt (x + x)\n\
                  def c :: f32 -> f32\nfn c x = Math.sin x\n\
                  def d :: f16 -> f16\nfn d x = Math.floor x";
    let ir = emit_f16(source, F16Path::Hardware, false);
    assert_eq!(lines_equal(&ir, "declare half @llvm.sqrt.f16(half)"), 1);
    assert!(
        llvm::duplicate_declarations(&ir).is_empty(),
        "no duplicate declaration"
    );
    for name in ["Math.sqrt.f16", "tz.fn.$builtin.Math.sqrt"] {
        let defines: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("define ") && line.contains(name))
            .collect();
        assert!(!defines.is_empty(), "{name}");
    }
    let builtin = ir
        .split("define internal i16 @tz.builtin.Math.sqrt.f16(")
        .nth(1)
        .and_then(|rest| rest.split("\n}").next())
        .expect("the f16 sqrt builtin is defined");
    assert!(
        builtin.contains("call half @llvm.sqrt.f16(half"),
        "{builtin}"
    );
    assert!(!builtin.contains("tz_soft_math_unary"), "{builtin}");
    // A NaN operand comes first: a signaling NaN stays as it is; a NaN result is 0x7E00.
    assert!(builtin.contains("i16 32256"), "{builtin}");
    assert!(builtin.contains("select i1"), "{builtin}");
    // Floor is not an operation of this path: it stays in the runtime.
    assert!(ir.contains("call void @tz_soft_math_unary"));
    // The path that does not use `half` declares nothing of the sort.
    let soft = emit_f16(source, F16Path::Soft, false);
    assert!(!soft.contains("@llvm.sqrt.f16"));
}

#[test]
fn emits_half_conversions_with_nan_canonicalization() {
    let widen32 = emit_f16(
        "def f :: f16 -> f32\nfn f x = x as f32",
        F16Path::Hardware,
        false,
    );
    let function = body(&widen32, "f");
    assert!(function.contains("fpext half"), "{function}");
    assert!(function.contains("to float"), "{function}");
    assert!(function.contains("2143289344"), "{function}");
    assert!(!function.contains("tz_soft_cast"), "{function}");
    let widen64 = emit_f16(
        "def f :: f16 -> f64\nfn f x = x as f64",
        F16Path::Hardware,
        false,
    );
    let function = body(&widen64, "f");
    assert!(function.contains("fpext half"), "{function}");
    assert!(function.contains("to double"), "{function}");
    assert!(function.contains("9221120237041090560"), "{function}");
    assert!(!function.contains("tz_soft_cast"), "{function}");
    let narrow32 = emit_f16(
        "def f :: f32 -> f16\nfn f x = x as f16",
        F16Path::Hardware,
        false,
    );
    let function = body(&narrow32, "f");
    assert!(function.contains("fptrunc float"), "{function}");
    assert!(function.contains("fcmp uno float"), "{function}");
    assert!(function.contains("32256"), "{function}");
    assert!(!function.contains("tz_soft_cast"), "{function}");
    // One instruction for a narrowing from f64: a detour through f32 would round twice.
    let narrow64 = emit_f16(
        "def f :: f64 -> f16\nfn f x = x as f16",
        F16Path::Hardware,
        false,
    );
    let function = body(&narrow64, "f");
    assert!(function.contains("fptrunc double"), "{function}");
    assert!(function.contains("to half"), "{function}");
    assert!(!function.contains("to float"), "{function}");
    assert!(!function.contains("tz_soft_cast"), "{function}");
}

#[test]
fn keeps_other_operations_in_the_runtime() {
    let source = "def fused :: f16 -> f16 -> f16 -> f16\nfn fused x y z = Math.fma x y z\n\
                  def least :: f16 -> f16 -> f16\nfn least x y = Math.min x y\n\
                  def within :: f16 -> f16 -> f16 -> f16\nfn within x y z = Math.clamp x y z\n\
                  def whole :: f16 -> i32\nfn whole x = x as i32\n\
                  def sized :: i64 -> f16\nfn sized n = n as f16\n\
                  def unsigned :: i16u -> f16\nfn unsigned n = n as f16\n\
                  def quad :: f16 -> f128\nfn quad x = x as f128\n\
                  def back :: f128 -> f16\nfn back x = x as f16\n\
                  def decimal :: f16 -> d64\nfn decimal x = x as d64";
    for path in [F16Path::Conversions, F16Path::Hardware] {
        let ir = emit_f16(source, path, false);
        // Nothing here has a `half` form on any path: the program has no `half` at all.
        assert!(!has_half(&ir), "{path:?}");
        // The builtins are functions of their own, which the user functions call.
        for symbol in [
            "call void @tz_soft_fma(",
            "call void @tz_soft_math_binary(",
            "call i32 @tz_soft_cmp(",
        ] {
            assert!(ir.contains(symbol), "{path:?} {symbol}");
        }
        for name in ["whole", "sized", "unsigned", "quad", "back", "decimal"] {
            let function = body(&ir, name);
            assert!(
                function.contains("call void @tz_soft_cast("),
                "{path:?} {name}\n{function}"
            );
        }
        for forbidden in [
            "llvm.fma.f16",
            "fmuladd",
            " fast ",
            " contract ",
            " reassoc ",
            " nnan ",
            " ninf ",
            " nsz ",
            " arcp ",
            " afn ",
        ] {
            assert!(!ir.contains(forbidden), "{path:?} {forbidden}");
        }
        // An integer converts to f16 only in the runtime: LLVM may go through binary32.
        assert!(
            ir.lines()
                .all(|line| !(line.contains("itofp") && line.contains("to half"))),
            "{path:?}"
        );
        assert!(
            ir.lines()
                .all(|line| !(line.contains("fpto") && line.contains("half "))),
            "{path:?}"
        );
    }
}

#[test]
fn conversions_path_converts_in_half_and_keeps_the_rest_soft() {
    let ir = emit_f16(
        "def widen32 :: f16 -> f32\nfn widen32 x = x as f32\n\
         def widen64 :: f16 -> f64\nfn widen64 x = x as f64\n\
         def narrow32 :: f32 -> f16\nfn narrow32 x = x as f16\n\
         def narrow64 :: f64 -> f16\nfn narrow64 x = x as f16\n\
         def add :: f16 -> f16 -> f16\nfn add x y = x + y\n\
         def less :: f16 -> f16 -> bool\nfn less x y = x < y\n\
         def root :: f16 -> f16\nfn root x = Math.sqrt x",
        F16Path::Conversions,
        false,
    );
    assert!(body(&ir, "widen32").contains("fpext half"));
    assert!(body(&ir, "widen64").contains("fpext half"));
    assert!(body(&ir, "narrow32").contains("fptrunc float"));
    // F16C has no instruction for binary64: LLVM would call `__truncdfhf2`, and a detour through
    // binary32 would round twice, so the runtime narrows it.
    let narrow64 = body(&ir, "narrow64");
    assert!(narrow64.contains("tz_soft_cast"), "{narrow64}");
    assert!(!narrow64.contains("fptrunc"), "{narrow64}");
    assert!(body(&ir, "add").contains("tz_soft_op"));
    assert!(body(&ir, "less").contains("tz_soft_cmp"));
    assert!(ir.contains("call void @tz_soft_math_unary"));
    assert!(!ir.contains("@llvm.sqrt.f16"));
    assert!(!ir.contains("fadd half") && !ir.contains("fcmp olt half"));
}

#[test]
fn builtins_and_operators_choose_the_same_path() {
    // A builtin called through a function value is emitted apart from its callers, with its own
    // `Globals`; it must still follow the path (`Math.sqrt` was the one to forget).
    let source = "def apply :: (f16 -> f16) -> f16 -> f16\nfn apply f x = f x\n\
                  def f :: f16 -> f16\nfn f x = apply Math.sqrt x";
    let hardware = emit_f16(source, F16Path::Hardware, false);
    assert!(hardware.contains("call half @llvm.sqrt.f16(half"));
    assert!(!hardware.contains("tz_soft_math_unary"));
    let soft = emit_f16(source, F16Path::Soft, false);
    assert!(soft.contains("tz_soft_math_unary") && !has_half(&soft));
}

/// Compiles `ir` for `target` with `-O<optimization>` and the extra `flags`, and returns the assembly.
fn assembly(ir: &str, target: &str, optimization: u8, flags: &[&str]) -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "tsuzuri-f16-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("module.ll");
    std::fs::write(&path, ir).unwrap();
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let output = Command::new(clang)
        .args(["-Wno-override-module", "-S", "-o", "-", "-target", target])
        .arg(format!("-O{optimization}"))
        .args(flags)
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    assert!(
        output.status.success(),
        "{target} {flags:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// The compiler-rt and libgcc functions that LLVM calls for `half` where the target has no
/// instruction for it: they would be unresolved symbols or a runtime dependency.
fn half_libcalls(assembly: &str) -> Vec<&str> {
    assembly
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| {
            let name = word.trim_start_matches('_');
            name.starts_with("extendhf")
                || (name.starts_with("trunc") && name.ends_with("hf2"))
                || (name.starts_with("gnu_") && name.ends_with("_ieee"))
        })
        .collect()
}

const OPERATIONS: &str = "\
export def add :: f32 -> f32 -> f32\nfn add x y = ((x as f16) + (y as f16)) as f32\n\
export def subtract :: f32 -> f32 -> f32\nfn subtract x y = ((x as f16) - (y as f16)) as f32\n\
export def multiply :: f32 -> f32 -> f32\nfn multiply x y = ((x as f16) * (y as f16)) as f32\n\
export def divide :: f32 -> f32 -> f32\nfn divide x y = ((x as f16) / (y as f16)) as f32\n\
export def less :: f32 -> f32 -> bool\nfn less x y = (x as f16) < (y as f16)\n\
export def root :: f32 -> f32\nfn root x = Math.sqrt (x as f16) as f32\n\
export def widen :: f32 -> f64\nfn widen x = (x as f16) as f64\n\
export def narrow :: f64 -> f32\nfn narrow x = (x as f16) as f32\n\
export def fused :: f32 -> f32\nfn fused x = Math.floor (x as f16) as f32\n";

#[test]
fn half_lowers_without_libcalls_where_the_path_applies() {
    let hardware = emit_f16(OPERATIONS, F16Path::Hardware, false);
    let conversions = emit_f16(OPERATIONS, F16Path::Conversions, false);
    for optimization in [0, 3] {
        // AArch64 without FEAT_FP16 promotes each `half` operation to binary32 around exact
        // conversions, and with it computes in half precision; neither calls the runtime library.
        for target in [
            "aarch64-unknown-linux-gnu",
            "aarch64-unknown-linux-musl",
            "aarch64-pc-windows-msvc",
            "arm64-apple-macos",
        ] {
            let text = assembly(&hardware, target, optimization, &[]);
            assert!(
                half_libcalls(&text).is_empty(),
                "{target} -O{optimization}: {:?}",
                half_libcalls(&text)
            );
            assert!(text.contains("fcvt"), "{target} -O{optimization}");
        }
        // x86-64 with the instructions the driver names for the path in a `--cpu native` build.
        let sapphire = assembly(
            &hardware,
            "x86_64-unknown-linux-gnu",
            optimization,
            &["-mavx512fp16", "-mf16c"],
        );
        assert!(
            half_libcalls(&sapphire).is_empty(),
            "{:?}",
            half_libcalls(&sapphire)
        );
        assert!(sapphire.contains("vaddsh") && sapphire.contains("vcvtsd2sh"));
        for target in [
            "x86_64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
            "x86_64-apple-macos",
        ] {
            let text = assembly(&conversions, target, optimization, &["-mf16c"]);
            assert!(
                half_libcalls(&text).is_empty(),
                "{target} -O{optimization}: {:?}",
                half_libcalls(&text)
            );
            assert!(
                text.contains("vcvtph2ps") && text.contains("vcvtps2ph"),
                "{target}"
            );
        }
    }
}

#[test]
fn half_without_the_instructions_calls_the_runtime_library() {
    // The control of the test above: without `-mf16c`, LLVM calls compiler-rt for the same IR,
    // and a path that the driver picks must come with the flag that prevents it.
    let conversions = emit_f16(OPERATIONS, F16Path::Conversions, false);
    let text = assembly(&conversions, "x86_64-unknown-linux-gnu", 3, &[]);
    let calls = half_libcalls(&text);
    assert!(
        calls.iter().any(|call| call.contains("extendhfsf2")),
        "{calls:?}"
    );
    assert!(
        calls.iter().any(|call| call.contains("truncsfhf2")),
        "{calls:?}"
    );
    // Even with F16C, a direct narrowing from binary64 is a library call; the Hardware path may
    // use it only with AVX512-FP16.
    let hardware = emit_f16(OPERATIONS, F16Path::Hardware, false);
    let text = assembly(&hardware, "x86_64-unknown-linux-gnu", 3, &["-mf16c"]);
    assert!(
        half_libcalls(&text)
            .iter()
            .any(|call| call.contains("truncdfhf2")),
        "{:?}",
        half_libcalls(&text)
    );
}

/// The assembly of the function labelled `name`, through its `.size` line.
fn function_assembly<'a>(assembly: &'a str, name: &str) -> &'a str {
    let begin = assembly
        .match_indices(&format!("\n{name}:"))
        .map(|(at, _)| at + 1)
        .next()
        .unwrap_or_else(|| panic!("{name} has no label"));
    let end = assembly[begin..]
        .find(&format!(".size\t{name},"))
        .map_or(assembly.len(), |end| begin + end);
    &assembly[begin..end]
}

#[test]
fn cpu_versions_keep_the_features_of_their_half_operations() {
    // A `@cpu` version has a `target-features` attribute of its own, which replaces the build's
    // features: without `+f16c` in it, LLVM would call compiler-rt for the version's `half`.
    const SOURCE: &str = "@cpu [\"avx2\", \"sve\"]\nexport def scale :: f32 -> f32\nfn scale x = ((x as f16) as f32) * 2.0f32\n";
    // No `half` operation, only a name with `half` in it: the level's features alone.
    const PLAIN: &str =
        "@cpu [\"avx2\"]\nexport def half_add :: f32 -> f32\nfn half_add x = x * 2.0f32\n";
    let build = |source: &str, f16, levels| {
        let module = tsuzuri::analyze(source).unwrap();
        let sources = [tsuzuri::trap::TrapSource {
            path: "Main.tz",
            text: source,
        }];
        llvm::emit_native_build_for(
            &module,
            EmitOptions {
                entry: Entry::Library,
                wasm: false,
                debug_output: false,
                runtime_objects: false,
                wasm_simd128: false,
                allocator: Allocator::System,
                f16,
            },
            &sources,
            None,
            false,
            levels,
        )
        .unwrap()
        .ir
    };
    let attribute = |ir: &str, suffix: &str| {
        let define = ir
            .lines()
            .find(|line| line.starts_with("define ") && line.contains(suffix))
            .unwrap_or_else(|| panic!("no version {suffix}"))
            .to_owned();
        define
            .split_once("\"target-features\"=\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(features, _)| features.to_owned())
            .unwrap_or_default()
    };
    const X86: u8 = 0b1110;
    const ARM: u8 = 0b11_0000;
    for (path, expected) in [
        (F16Path::Soft, "+avx2"),
        (F16Path::Conversions, "+avx2,+f16c"),
        (F16Path::Hardware, "+avx2,+avx512fp16,+f16c"),
    ] {
        let ir = build(SOURCE, path, X86);
        assert_eq!(attribute(&ir, "Main.scale.cpu.avx2"), expected, "{path:?}");
        // The AArch64 versions need nothing: `half` lowers to base instructions.
        assert_eq!(
            attribute(&build(SOURCE, path, ARM), "Main.scale.cpu.sve"),
            "+sve"
        );
        let plain = build(PLAIN, path, X86);
        assert!(!has_half(&plain), "{path:?}");
        assert_eq!(
            attribute(&plain, "Main.half_add.cpu.avx2"),
            "+avx2",
            "{path:?}"
        );
    }
    // Compiled for plain x86-64, the version still converts with F16C and calls no library.
    let ir = build(SOURCE, F16Path::Conversions, X86);
    let text = assembly(&ir, "x86_64-unknown-linux-gnu", 3, &[]);
    let version = function_assembly(&text, "tz.fn.Main.scale.cpu.avx2");
    assert!(
        version.contains("vcvtph2ps") && version.contains("vcvtps2ph"),
        "{version}"
    );
    assert!(half_libcalls(version).is_empty(), "{version}");
}

#[test]
fn driver_builds_follow_the_hosts_path() {
    // The driver's choice reaches the IR of a build: AArch64 hosts emit `half`, and WASM never
    // does. The decision itself is unit-tested in `src/driver.rs`.
    use tsuzuri::driver::{BuildOptions, Emit, Project, Target};
    let directory = std::env::temp_dir().join(format!("tsuzuri-f16-build-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("Main.tz"), OPERATIONS).unwrap();
    let project = Project::load(&directory).unwrap();
    let module = project.analyze().unwrap();
    let mut results = Vec::new();
    for (target, name) in [(Target::Native, "native"), (Target::Wasm32, "wasm32")] {
        let output = directory.join(format!("{name}.ll"));
        tsuzuri::driver::build(
            &module,
            &project,
            &output,
            BuildOptions {
                target,
                emit: Emit::Llvm,
                optimization: 0,
                cache: false,
                ..BuildOptions::default()
            },
        )
        .unwrap();
        results.push(std::fs::read_to_string(output).unwrap());
    }
    std::fs::remove_dir_all(&directory).unwrap();
    // x86-64 builds are `Soft` unless `--cpu native` detects F16C, and every Apple Silicon CPU has
    // FEAT_FP16; the other AArch64 hosts follow `AARCH64_HALF_WITHOUT_FP16` in `src/driver.rs`.
    if cfg!(target_arch = "x86_64") {
        assert!(!has_half(&results[0]));
    }
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        assert!(has_half(&results[0]));
    }
    assert!(!has_half(&results[1]));
    assert!(results[1].contains("@tz_soft_op"));
}
