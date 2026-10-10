//! F09 Phase 3: what the compiler embeds for `Gpu.Vulkan` and `Gpu.Auto` programs. The SPIR-V emitter has its own
//! tests (tests/gpu_spirv.rs); these cover the kernel table, the descriptor, the feature masks, the marker that adds
//! the Vulkan runtime, and the programs that must stay as they were.

use tsuzuri::{
    analyze,
    check::CheckedModule,
    gpu::{self, FEATURE_INT64, FEATURE_STRICT_FLOAT},
    gpu_devices::{FEATURE_F16, FLAG_RELAXED, LANE_32, LANE_64, LANE_F32},
    llvm::{Entry, emit_target},
};

const KERNELS: &str = "def mix :: i32 -> i32\nfn mix value = value * 3 + 1\n\
def wide :: i64 -> i64\nfn wide value = value * 3l + 1l\n\
def poly :: f32 -> f32\nfn poly value = value * value + value\n\
def quotient :: f32 -> f32\nfn quotient value = value / 3.0\n\
def divide :: i32 -> i32\nfn divide value = value / 3\n";

fn program(backends: &[&str], calls: &str) -> String {
    let requests: String = backends
        .iter()
        .enumerate()
        .map(|(index, backend)| {
            format!("let device{index} = Result.get (Gpu.request Gpu.{backend})\n")
        })
        .collect();
    format!("{KERNELS}{requests}{calls}")
}

fn lanes(input: u32, output: u32) -> u32 {
    input | output << 8
}

fn callback_name(module: &CheckedModule, id: usize) -> String {
    module.functions[id].qualified_name()
}

const VULKAN_CALLS: &str = "let a = Gpu.to_array (Gpu.map (&device0) mix (Gpu.init (&device0) 8 (\\index -> index * 2)))\n\
let wides: [i64] = [1l, 2l]\n\
let b = Gpu.to_array (Gpu.map (&device0) wide (Gpu.from_array (&device0) (&wides)))\n\
let floats: [f32] = [1.0, 2.0]\n\
let c = Gpu.to_array (Gpu.map (&device0) poly (Gpu.from_array (&device0) (&floats)))\n\
let d = Gpu.to_array (Gpu.map_relaxed (&device0) quotient (Gpu.from_array (&device0) (&floats)))\n\
let e = Gpu.to_array (Gpu.map (&device0) quotient (Gpu.from_array (&device0) (&floats)))\n\
Array.sum (&a)";

#[test]
fn vulkan_programs_embed_a_spirv_module_where_the_emitter_can_reproduce_the_cpu() {
    let module = analyze(&program(&["Vulkan"], VULKAN_CALLS)).unwrap();
    let gpu = &module.gpu;
    assert!(gpu.vulkan);
    // The init lambda, mix, wide, strict poly, and relaxed quotient; the strict quotient has no module because
    // OpFDiv is only within 2.5 ULP, so its call site keeps the "no kernel" number.
    assert_eq!(gpu.kernels.len(), 5, "{:?}", gpu.kernels);
    assert!(gpu.kernels.iter().all(|kernel| kernel.wgsl.is_none()));
    assert!(gpu.kernels.iter().all(|kernel| kernel.spirv.is_some()));
    assert!(gpu.kernels.iter().all(|kernel| kernel.weight >= 1));
    let by_name = |name: &str, relaxed: bool| {
        gpu.kernels.iter().find(|kernel| {
            callback_name(&module, kernel.callback) == name && kernel.relaxed == relaxed
        })
    };
    let mix = by_name("Main.mix", false).unwrap();
    assert_eq!(
        (mix.lanes, mix.features, mix.flags),
        (lanes(LANE_32, LANE_32), 0, 0)
    );
    let wide = by_name("Main.wide", false).unwrap();
    assert_eq!(
        (wide.lanes, wide.features),
        (lanes(LANE_64, LANE_64), FEATURE_INT64)
    );
    let poly = by_name("Main.poly", false).unwrap();
    assert_eq!(
        (poly.lanes, poly.features),
        (lanes(LANE_F32, LANE_F32), FEATURE_STRICT_FLOAT)
    );
    let quotient = by_name("Main.quotient", true).unwrap();
    assert_eq!(
        (quotient.lanes, quotient.features, quotient.flags),
        (lanes(LANE_F32, LANE_F32), 0, FLAG_RELAXED)
    );
    assert!(by_name("Main.quotient", false).is_none());
    assert_eq!(gpu.features, FEATURE_INT64 | FEATURE_STRICT_FLOAT);
    assert_eq!(gpu.features & FEATURE_F16, 0);

    // The embedded bytes are what the emitter returns for the kernel, which is what `--emit spirv` writes.
    for kernel in &gpu.kernels {
        let extracted = gpu::extract_kernel(&module, kernel.callback).unwrap();
        let emitted = if kernel.relaxed {
            extracted.spirv_relaxed()
        } else {
            extracted.spirv()
        }
        .unwrap();
        assert_eq!(kernel.spirv.as_deref(), Some(emitted.bytes().as_slice()));
        assert_eq!(kernel.weight, emitted.weight);
        assert_eq!(kernel.features, emitted.features);
        assert_eq!(
            kernel.lanes,
            lanes(emitted.input.kind(), emitted.output.kind())
        );
    }
}

#[test]
fn the_descriptor_carries_the_weight_and_the_vulkan_marker_selects_the_runtime() {
    let module = analyze(&program(&["Vulkan"], VULKAN_CALLS)).unwrap();
    for wasm in [false, true] {
        let ir = emit_target(&module, Entry::Console, wasm).unwrap();
        assert_eq!(ir, emit_target(&module, Entry::Console, wasm).unwrap());
        assert_eq!(ir.matches("; tsuzuri-gpu: vulkan\n").count(), 1);
        assert_eq!(
            ir.matches("@tz.gpu.kernels = private constant [5 x { i32, i32, i32, ptr, i32, ptr, i32, i32 }]")
                .count(),
            1
        );
        assert_eq!(
            ir.matches("@tz.gpu.last = internal global i32 0").count(),
            1
        );
        // The words are read as 32-bit values; no kernel has WGSL, so no WGSL text is embedded.
        assert!(ir.contains(".spirv = private unnamed_addr constant ["));
        assert!(
            ir.contains("c\"\\03\\02#\\07"),
            "the SPIR-V magic number leads the module"
        );
        assert!(ir.contains(", align 4\n"));
        assert!(!ir.contains(".wgsl"));
        for declaration in [
            "declare i32 @tsuzuri_gpu_open(i32, i32)\n",
            "declare i32 @tsuzuri_gpu_select(i32, i32, i32, ptr, i32, i32, i64)\n",
        ] {
            assert_eq!(ir.matches(declaration).count(), 1, "{declaration}");
        }
        // The strict quotient call has no module: it passes the number that means "no kernel".
        assert!(ir.contains("i64 18446744073709551615)"));
    }
    for (index, kernel) in module.gpu.kernels.iter().enumerate() {
        assert!(kernel.weight >= 1, "{index}");
    }
}

/// The weights written in the kernel table of the IR (the last field of every descriptor), as the runtime reads them: an `i32`.
fn descriptor_weights(ir: &str) -> Vec<i32> {
    let table = ir
        .lines()
        .find(|line| line.starts_with("@tz.gpu.kernels = "))
        .expect("the kernel table");
    table
        .split("{ i32 ")
        .skip(1)
        .map(|entry| {
            // The fields of the value end at the first closing brace; what follows is the type of the next element.
            let fields = entry.split(" }").next().unwrap();
            let weight = fields.rsplit("i32 ").next().unwrap();
            weight
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("a weight in {entry:?}"))
        })
        .collect()
}

#[test]
fn the_weight_of_a_very_heavy_kernel_is_the_largest_number_of_the_descriptor_field() {
    // Each of these 41 functions calls the next one twice: the sum of the instructions that a lane runs is far above 2^32,
    // and the descriptor field is an i32, in which 4294967295 is -1, which the runtime prices as the weight 1.
    let depth = 40;
    let mut source = format!("def f{depth} :: i32 -> i32\nfn f{depth} value = value + 1\n");
    for level in (0..depth).rev() {
        let next = level + 1;
        source.push_str(&format!(
            "def f{level} :: i32 -> i32\nfn f{level} value = f{next} (f{next} value)\n"
        ));
    }
    source.push_str("let device = Result.get (Gpu.request Gpu.Auto)\nlet a = Gpu.to_array (Gpu.init (&device) 8 f0)\nArray.sum (&a)");
    let module = analyze(&source).unwrap();
    let kernel = module
        .gpu
        .kernels
        .iter()
        .find(|kernel| callback_name(&module, kernel.callback) == "Main.f0")
        .unwrap();
    assert_eq!(kernel.weight, i32::MAX as u32);
    for wasm in [false, true] {
        let ir = emit_target(&module, Entry::Console, wasm).unwrap();
        assert_eq!(descriptor_weights(&ir), [i32::MAX], "wasm: {wasm}");
        assert!(!ir.contains(", i32 4294967295 }"), "wasm: {wasm}");
    }
    // The ordinary kernels of the other programs keep their exact (small, positive) weights in the same field.
    let ordinary = analyze(&program(&["Vulkan"], VULKAN_CALLS)).unwrap();
    let ir = emit_target(&ordinary, Entry::Console, false).unwrap();
    let written = descriptor_weights(&ir);
    let expected: Vec<i32> = ordinary
        .gpu
        .kernels
        .iter()
        .map(|kernel| i32::try_from(kernel.weight).unwrap())
        .collect();
    assert_eq!(written, expected);
    assert!(written.iter().all(|weight| *weight >= 1));
}

#[test]
fn only_the_backends_that_a_program_names_get_their_source() {
    let calls = "let a = Gpu.to_array (Gpu.map (&device0) mix (Gpu.init (&device0) 8 (\\index -> index * 2)))\nArray.sum (&a)";
    // WebGpu alone: WGSL, no SPIR-V, no Vulkan runtime.
    let webgpu = analyze(&program(&["WebGpu"], calls)).unwrap();
    assert!(!webgpu.gpu.vulkan);
    assert!(
        webgpu
            .gpu
            .kernels
            .iter()
            .all(|kernel| kernel.wgsl.is_some() && kernel.spirv.is_none() && kernel.weight == 0)
    );
    let ir = emit_target(&webgpu, Entry::Console, false).unwrap();
    assert!(!ir.contains("tsuzuri-gpu: vulkan"));
    // Vulkan alone: SPIR-V, no WGSL.
    let vulkan = analyze(&program(&["Vulkan"], calls)).unwrap();
    assert!(vulkan.gpu.vulkan);
    assert!(
        vulkan
            .gpu
            .kernels
            .iter()
            .all(|kernel| kernel.wgsl.is_none() && kernel.spirv.is_some())
    );
    // Auto reads SPIR-V too, but WebGPU is not one of its candidates.
    let auto = analyze(&program(&["Auto"], calls)).unwrap();
    assert!(auto.gpu.vulkan);
    assert!(
        auto.gpu
            .kernels
            .iter()
            .all(|kernel| kernel.wgsl.is_none() && kernel.spirv.is_some())
    );
    // Both backends named: every kernel carries both sources, and the lanes agree.
    let both = analyze(&program(&["WebGpu", "Vulkan"], calls)).unwrap();
    assert!(both.gpu.vulkan);
    assert!(
        both.gpu
            .kernels
            .iter()
            .all(|kernel| kernel.wgsl.is_some() && kernel.spirv.is_some())
    );
    // A backend with no implementation embeds nothing.
    for backend in ["Cuda", "Metal"] {
        let module = analyze(&program(&[backend], calls)).unwrap();
        assert!(!module.gpu.vulkan);
        assert!(module.gpu.kernels.is_empty(), "{backend}");
    }
}

#[test]
fn a_kernel_with_both_sources_keeps_the_webgpu_semantics_and_adds_the_vulkan_features() {
    let calls = "let wides: [i64] = [1l]\nlet a = Gpu.to_array (Gpu.map (&device0) wide (Gpu.from_array (&device0) (&wides)))\nlet floats: [f32] = [1.0]\nlet b = Gpu.to_array (Gpu.map_relaxed (&device0) poly (Gpu.from_array (&device0) (&floats)))\n0";
    let module = analyze(&program(&["WebGpu", "Vulkan"], calls)).unwrap();
    // i64 has no WGSL, so its kernel has only a module; the relaxed f32 kernel has both.
    let wide = module
        .gpu
        .kernels
        .iter()
        .find(|kernel| callback_name(&module, kernel.callback) == "Main.wide")
        .unwrap();
    assert!(wide.wgsl.is_none() && wide.spirv.is_some());
    assert_eq!(wide.features, FEATURE_INT64);
    let poly = module
        .gpu
        .kernels
        .iter()
        .find(|kernel| callback_name(&module, kernel.callback) == "Main.poly")
        .unwrap();
    assert!(poly.wgsl.is_some() && poly.spirv.is_some() && poly.relaxed);
    assert_eq!(poly.features, 0);
    assert_eq!(module.gpu.features, FEATURE_INT64);
}

#[test]
fn auto_programs_need_no_device_and_do_not_change_cpu_programs() {
    let calls = "let a = Gpu.to_array (Gpu.map (&device0) mix (Gpu.init (&device0) 8 (\\index -> index * 2)))\nArray.sum (&a)";
    let module = analyze(&program(&["Auto"], calls)).unwrap();
    let ir = emit_target(&module, Entry::Console, false).unwrap();
    assert_eq!(
        ir.matches("define internal i32 @tz.builtin.Gpu.__select(")
            .count(),
        1
    );
    assert!(ir.contains("call i32 @tsuzuri_gpu_select("));
    assert!(ir.contains("store atomic i32 %result, ptr @tz.gpu.last monotonic, align 4"));
    // A program that only reads the last backend has the register and no runtime to call.
    let reader = analyze(
        "let backend = Gpu.last_backend ()\nResult.is_ok (&(Gpu.request Gpu.CpuReference))",
    )
    .unwrap();
    let ir = emit_target(&reader, Entry::Console, false).unwrap();
    assert!(ir.contains("load atomic i32, ptr @tz.gpu.last monotonic, align 4"));
    for absent in ["tsuzuri_gpu", "tz.gpu.kernels", "request_on", "map_on"] {
        assert!(!ir.contains(absent), "{absent}");
    }
    assert!(reader.gpu.kernels.is_empty() && !reader.gpu.vulkan);
}

#[test]
fn the_runtime_primitives_stay_private_to_the_standard_module() {
    for primitive in ["Gpu.__select 0 0 0 0", "Gpu.__last()"] {
        let source = format!(
            "let device = Result.get (Gpu.request Gpu.Vulkan)\nlet status = {primitive}\nstatus"
        );
        let error = analyze(&source).unwrap_err();
        assert_eq!(error.code, "E1022", "{primitive}: {error:?}");
        assert!(error.message.contains("private to the standard Gpu module"));
    }
}

/// A user call, or a first-class use, of anything that takes a number that only the compiler writes (a kernel index, a
/// backend tag) is E1022 in a program that names Vulkan or Auto too, so no user code can aim a kernel of another
/// shape at the arrays of a call.
#[test]
fn nothing_that_takes_a_compiler_written_index_is_callable_from_user_code() {
    for backend in ["Vulkan", "Auto"] {
        let prefix = format!(
            "{KERNELS}let device = Result.get (Gpu.request Gpu.{backend})\nlet values = [1, 2, 3]\n"
        );
        for body in [
            "let escaped = Gpu.__select\n0",
            "let escaped = Gpu.__last\n0",
            "let escaped = Gpu.__run\n0",
            "let escaped = Gpu.__open\n0",
            "let escaped = Gpu.__features\n0",
            "let escaped = Gpu.map_on\n0",
            "let escaped = Gpu.init_on\n0",
            "let escaped = Gpu.request_on\n0",
            "let escaped = Gpu.backend_code\n0",
            "let status = Gpu.__run 2 0 0 (ref values) 3\nstatus",
            "Gpu.to_array (Gpu.map_on (&device) mix (Gpu.from_array (&device) (&values)) 0)",
            "Gpu.to_array (Gpu.init_on (&device) 3 (\\index -> index) 0)",
        ] {
            let error = analyze(&format!("{prefix}{body}")).unwrap_err();
            assert_eq!(error.code, "E1022", "{backend}: {body}: {error:?}");
        }
    }
    // The compiler's own retargeting still reaches them, and the public observer needs no index.
    let module = analyze(&program(
        &["Auto"],
        "let a = Gpu.to_array (Gpu.map (&device0) mix (Gpu.init (&device0) 4 (\\index -> index)))\nlet seen = Gpu.last_backend ()\n0",
    ))
    .unwrap();
    assert_eq!(module.gpu.kernels.len(), 2);
}

/// `Gpu.__run` passes a kernel's source only when the lane kinds of its descriptor are those of the element types of the
/// call, because the host sizes its copies of the arrays by the descriptor. That holds for the Vulkan and Auto arms and
/// for 64-bit lanes (kind 4) as for the WebGPU ones, and the kernel numbers of `__run` and `__select` are compared as
/// unsigned values (a number that is out of the table, including -1, is a call without a kernel).
#[test]
fn the_lane_kind_guard_covers_the_vulkan_and_auto_arms_and_64_bit_lanes() {
    for backend in ["Vulkan", "Auto"] {
        let module = analyze(&program(&[backend], VULKAN_CALLS)).unwrap();
        let tables = module.gpu.kernels.len();
        for wasm in [false, true] {
            let ir = emit_target(&module, Entry::Console, wasm).unwrap();
            let instances: Vec<&str> = ir
                .split("define internal %tz.array @tz.builtin.Gpu.__run.")
                .skip(1)
                .map(|text| text.split("\n}\n").next().unwrap())
                .collect();
            assert_eq!(instances.len(), 3, "{backend} {wasm}");
            for (suffix, kind) in [
                ("i32$Ci32", LANE_32),
                ("i64$Ci64", LANE_64),
                ("f32$Cf32", LANE_F32),
            ] {
                let body = instances
                    .iter()
                    .find(|body| body.starts_with(&format!("{suffix}(")))
                    .unwrap_or_else(|| panic!("{backend}: {suffix}"));
                for name in ["input", "output"] {
                    assert!(
                        body.contains(&format!(
                            "%{name}_matches = icmp eq i32 %found_{name}_kind, {kind}\n"
                        )),
                        "{backend} {suffix} {name}"
                    );
                }
                assert!(body.contains("%matches = and i1 %input_matches, %output_matches\n"));
                assert!(
                    body.contains("%kept_spirv = select i1 %matches, ptr %found_spirv, ptr null\n"),
                    "{backend} {suffix}: a kernel of other lanes keeps no SPIR-V"
                );
                assert!(
                    body.contains(&format!("%known = icmp ult i64 %kernel, {tables}\n")),
                    "{backend} {suffix}: the index is compared unsigned"
                );
            }
            let select = ir
                .split("define internal i32 @tz.builtin.Gpu.__select")
                .nth(1)
                .and_then(|text| text.split("\n}\n").next())
                .unwrap_or_else(|| panic!("{backend}: Gpu.__select"));
            assert!(
                select.contains(&format!("%known = icmp ult i64 %kernel, {tables}\n")),
                "{backend}: Gpu.__select compares the index unsigned"
            );
        }
    }
}

#[test]
fn strict_kernels_that_the_emitter_rejects_have_no_module_and_relaxed_ones_must_still_parse() {
    // A strict f32 division and a strict integer division: the CPU reference runs them (the integer one traps on
    // zero); no device can reproduce them.
    let calls = "let floats: [f32] = [1.0]\nlet a = Gpu.to_array (Gpu.map (&device0) quotient (Gpu.from_array (&device0) (&floats)))\nlet b = Gpu.to_array (Gpu.init (&device0) 4 divide)\n0";
    let module = analyze(&program(&["Vulkan"], calls)).unwrap();
    assert!(module.gpu.kernels.is_empty(), "{:?}", module.gpu.kernels);
    let ir = emit_target(&module, Entry::Console, false).unwrap();
    assert!(ir.contains("i64 18446744073709551615)"));
    // Relaxed calls are still validated like relaxed WGSL: f64 is a compile error whatever the backend.
    let error = analyze(
        "let device = Result.get (Gpu.request Gpu.Vulkan)\nlet wide: [f64] = [1.0]\nGpu.map_relaxed (&device) (\\item -> item + 1.0) (Gpu.from_array (&device) (&wide))",
    )
    .unwrap_err();
    assert_eq!(error.code, "E1018");
}
