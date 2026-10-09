use tsuzuri::{analyze, gpu};

fn kernel_id(module: &tsuzuri::check::CheckedModule) -> usize {
    module
        .functions
        .iter()
        .position(|function| function.qualified_name() == "Main.kernel")
        .unwrap()
}

#[test]
fn extracts_scalar_kernels_with_normal_cpu_lowering() {
    for ty in ["i32", "i32u", "i64", "i64u", "f32", "f64"] {
        let source = format!(
            "export def kernel :: {ty} -> {ty}\nfn kernel value = {{ let doubled = value + value; if doubled < value then value else doubled }}"
        );
        let module = analyze(&source).unwrap();
        let kernel = gpu::extract_kernel(&module, kernel_id(&module)).unwrap();
        assert_eq!(kernel.name(), "Main.kernel");
        for wasm in [false, true] {
            let ir = kernel.cpu_reference(wasm).unwrap();
            assert!(ir.contains("@tz_kernel"));
            assert_eq!(ir, kernel.cpu_reference(wasm).unwrap());
        }
    }
}

#[test]
fn rejects_kernel_effects_allocations_captures_and_recursion() {
    for source in [
        "export def kernel :: i32 -> i32\nfn kernel value = { let values = new [value]; values[0] }",
        "export def kernel :: i32 -> i32\nfn kernel value = { assert (value > 0); value }",
        "export def kernel :: i32 -> i32\nfn kernel value = { let mut total = value; while total > 0 do total = total - 1; total }",
        "export def kernel :: i32 -> i32\nfn kernel value = Task.run task { value }",
        "extern def external :: i32 -> i32\nexport def kernel :: i32 -> i32\nfn kernel value = external value",
        "def rec recur :: i32 -> i32\nfn rec recur value = if value > 0 then recur (value - 1) else value\nexport def kernel :: i32 -> i32\nfn kernel value = recur value",
    ] {
        let module = analyze(source).unwrap();
        let error = gpu::extract_kernel(&module, kernel_id(&module)).unwrap_err();
        assert_eq!(error.code, "E1018", "{source}: {error:?}");
    }
    let module = analyze("fn kernel(value: i32) -> i32 { value }").unwrap();
    assert_eq!(
        gpu::extract_kernel(&module, usize::MAX).unwrap_err().code,
        "E1018"
    );
}

#[test]
fn generates_deterministic_strict_integer_wgsl() {
    let module = analyze("def square :: i32 -> i32\nfn square value = value * value\nexport def kernel :: i32 -> i32\nfn kernel value = { let mut current = value; current = current + 1; if value < 0 then square current else ((Bits.ushr current 3) as i32u) as i32 }").unwrap();
    let kernel = gpu::extract_kernel(&module, kernel_id(&module)).unwrap();
    let wgsl = kernel.wgsl().unwrap();
    assert_eq!(wgsl, kernel.wgsl().unwrap());
    assert!(wgsl.contains("@workgroup_size(256)"));
    assert!(wgsl.contains("fn map_main("));
    assert!(wgsl.contains("fn init_main("));
    assert!(wgsl.contains("bitcast<i32>(bitcast<u32>"));
    assert!(wgsl.contains(" & 31u)"));
    assert!(wgsl.contains("bitcast<u32>("));
    assert!(wgsl.contains("bitcast<i32>("));
    assert!(kernel.function_count() > 1);
    for source in [
        "export def kernel :: f32 -> f32\nfn kernel value = value * value + value",
        "export def kernel :: i64 -> i64\nfn kernel value = value + 1",
        "export def kernel :: i32 -> i32\nfn kernel value = 100 / value",
    ] {
        let module = analyze(source).unwrap();
        assert_eq!(
            gpu::extract_kernel(&module, kernel_id(&module))
                .unwrap()
                .wgsl()
                .unwrap_err()
                .code,
            "E1018"
        );
    }
}

#[test]
fn gpu_reference_api_is_explicit_opaque_and_owned() {
    let prefix = "let device = Result.get (Gpu.request Gpu.CpuReference)\n";
    let module = analyze(&format!("{prefix}let buffer = Gpu.init (&device) 8 (\\index -> index * index)\nlet mapped = Gpu.map (&device) (\\value -> value + 1) buffer\nlet values = Gpu.to_array mapped\nArray.sum (&values)")).unwrap();
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, wasm).unwrap();
        let enum_line = ir
            .lines()
            .position(|line| line.contains("Gpu.Backend") && line.ends_with("= type i32"))
            .unwrap();
        let record_line = ir
            .lines()
            .position(|line| line.contains("Gpu.Device") && line.contains("= type {"))
            .unwrap();
        assert!(enum_line < record_line);
    }
    for (body, code) in [
        (
            "Gpu.init (&device) 2 (\\index -> { assert (index >= 0); index })",
            "E1018",
        ),
        (
            "let initialize = Gpu.init\ninitialize (&device) 2 (\\index -> { assert (index >= 0); index })",
            "E1018",
        ),
        (
            "let offset = 1i32\nGpu.init (&device) 2 (\\index -> index + offset)",
            "E1018",
        ),
        (
            "let buffer = Gpu.init (&device) 2 (\\index -> index)\nlet first = Gpu.to_array buffer\nGpu.to_array buffer",
            "E1012",
        ),
        ("device.backend", "E1022"),
    ] {
        assert_eq!(
            analyze(&format!("{prefix}{body}")).unwrap_err().code,
            code,
            "{body}"
        );
    }
    let device = analyze("fn consume(value: Gpu.Device) -> unit { () }\nlet device = Result.get (Gpu.request Gpu.CpuReference)\nconsume device\nconsume device").unwrap_err();
    assert_eq!(device.code, "E1012");
    assert_eq!(
        tsuzuri::analyze_modules(&[("Gpu.tz", "")])
            .unwrap_err()
            .code,
        "E1011"
    );
}

fn relaxed_wgsl(source: &str) -> Result<String, tsuzuri::diagnostic::Diagnostic> {
    let module = analyze(source).unwrap();
    gpu::extract_kernel(&module, kernel_id(&module))?.wgsl_relaxed()
}

#[test]
fn generates_relaxed_f32_wgsl_with_explicit_header() {
    let module =
        analyze("export def kernel :: f32 -> f32\nfn kernel value = value * value + value")
            .unwrap();
    let id = kernel_id(&module);
    let local = module.functions[id].parameters[0].id;
    let kernel = gpu::extract_kernel(&module, id).unwrap();
    let wgsl = kernel.wgsl_relaxed().unwrap();
    assert_eq!(wgsl, kernel.wgsl_relaxed().unwrap());
    let expected = format!(
        "// tsuzuri-gpu float=relaxed input=f32 output=f32\nstruct Params {{ length: u32 }}\n@group(0) @binding(0) var<storage, read> input_values: array<f32>;\n@group(0) @binding(1) var<storage, read_write> output_values: array<f32>;\n@group(0) @binding(2) var<uniform> params: Params;\nfn kernel_{id}(local_{local}: f32) -> f32 {{\nlet value_0: f32 = local_{local};\nlet value_1: f32 = local_{local};\nlet value_2: f32 = (value_0 * value_1);\nlet value_3: f32 = local_{local};\nlet value_4: f32 = (value_2 + value_3);\nreturn value_4;\n}}\n@compute @workgroup_size(256)\nfn map_main(@builtin(global_invocation_id) invocation: vec3<u32>) {{\nif (invocation.x < params.length) {{ output_values[invocation.x] = kernel_{id}(input_values[invocation.x]); }}\n}}\n@compute @workgroup_size(256)\nfn init_main(@builtin(global_invocation_id) invocation: vec3<u32>) {{\nif (invocation.x < params.length) {{ output_values[invocation.x] = kernel_{id}(f32(invocation.x)); }}\n}}\n"
    );
    assert_eq!(wgsl, expected);
    let strict = kernel.wgsl().unwrap_err();
    assert_eq!(strict.code, "E1018");
    assert!(strict.message.contains("--emit wgsl-relaxed"));

    let horner = relaxed_wgsl(
        "export def kernel :: f32 -> f32\nfn kernel value = ((value * 0.5 + 0.25) * value + 0.125) * value + 1.0",
    )
    .unwrap();
    for bits in [1056964608u32, 1048576000, 1040187392, 1065353216] {
        assert!(horner.contains(&format!("bitcast<f32>({bits}u)")), "{bits}");
    }
    let ratio =
        relaxed_wgsl("export def kernel :: f32 -> f32\nfn kernel value = -value / 0.1").unwrap();
    assert!(ratio.contains("(-value_0)"));
    assert!(ratio.contains("(value_1 / value_2)"));
    assert!(ratio.contains("bitcast<f32>(1036831949u)"));
    let index = relaxed_wgsl(
        "export def kernel :: i32 -> f32\nfn kernel index = (index as f32) * 0.5 + 0.25",
    )
    .unwrap();
    assert!(index.starts_with("// tsuzuri-gpu float=relaxed input=i32 output=f32\n"));
    assert!(index.contains("array<i32>") && index.contains("f32(value_0)"));
    let threshold = relaxed_wgsl(
        "export def kernel :: f32 -> i32\nfn kernel value = if value * value > 2.0 then 1 else 0",
    )
    .unwrap();
    assert!(threshold.starts_with("// tsuzuri-gpu float=relaxed input=f32 output=i32\n"));
    let integer = relaxed_wgsl(
        "export def kernel :: i32u -> i32u\nfn kernel value = (value * 3i32u) ^ (value >>> 3)",
    )
    .unwrap();
    assert!(integer.starts_with("// tsuzuri-gpu float=relaxed input=u32 output=u32\n"));
}

#[test]
fn rejects_relaxed_wgsl_outside_f32_i32_lanes() {
    for (source, fragment) in [
        (
            "export def kernel :: f64 -> f64\nfn kernel value = value + 1.0",
            "no f64 or 64-bit integers",
        ),
        (
            "export def kernel :: i64 -> i64\nfn kernel value = value + 1",
            "no f64 or 64-bit integers",
        ),
        (
            "export def kernel :: f32 -> bool\nfn kernel value = value > 1.0",
            "bool lanes",
        ),
        (
            "export def kernel :: f32 -> i32\nfn kernel value = if value * value > 2.0 then 1 else (value as i32)",
            "float-to-integer",
        ),
        (
            "export def kernel :: i32 -> i32\nfn kernel value = 100 / value",
            "division/remainder",
        ),
        (
            "export def kernel :: f32 -> f32\nfn kernel value = { let wide = (value as f64) + 1.0; wide as f32 }",
            "float-to-integer or f64 casts",
        ),
        (
            "export def kernel :: f32 -> f32\nfn kernel value = { let wide = (value as i64) + 1; wide as f32 }",
            "float-to-integer or f64 casts",
        ),
    ] {
        let error = relaxed_wgsl(source).unwrap_err();
        assert_eq!(error.code, "E1018", "{source}: {error:?}");
        assert!(
            error.message.contains(fragment),
            "{source}: {}",
            error.message
        );
    }
    for source in [
        "export def kernel :: f32 -> f32\nfn kernel value = value * value + value",
        "export def kernel :: i32 -> i32\nfn kernel value = if (value as f32) < 1.5 then 1 else 0",
        "export def kernel :: i32 -> i32\nfn kernel value = { let half = 0.5; if half < 1.0 then value else 0 }",
    ] {
        let module = analyze(source).unwrap();
        let error = gpu::extract_kernel(&module, kernel_id(&module))
            .unwrap()
            .wgsl()
            .unwrap_err();
        assert_eq!(error.code, "E1018", "{source}");
    }
}

#[test]
fn relaxed_gpu_api_validates_kernels_on_the_cpu_reference() {
    let prefix = "let device = Result.get (Gpu.request Gpu.CpuReference)\nlet floats: [f32] = [1.0, 2.0]\nlet buffer = Gpu.from_array (&device) (&floats)\n";
    for body in [
        "let mapped = Gpu.map_relaxed (&device) (\\item -> item * item + item) buffer\nlet result = Gpu.to_array mapped\nresult[0]",
        "let generated = Gpu.init_relaxed (&device) 8 (\\index -> (index as f32) * 0.5 + 0.25)\nlet values = Gpu.to_array generated\nArray.sum (&values)",
        "let mapped = Gpu.map (&device) (\\item -> item as i32) buffer\nlet result = Gpu.to_array mapped\nresult[0]",
    ] {
        let module = analyze(&format!("{prefix}{body}"))
            .unwrap_or_else(|error| panic!("{body}\n{}: {}", error.code, error.message));
        for wasm in [false, true] {
            let ir =
                tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, wasm).unwrap();
            assert_eq!(
                ir,
                tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, wasm).unwrap()
            );
        }
    }
    for (body, fragment) in [
        (
            "let wide: [f64] = [1.0]\nlet wide_buffer = Gpu.from_array (&device) (&wide)\nGpu.map_relaxed (&device) (\\item -> item + 1.0) wide_buffer",
            "no f64 or 64-bit integers",
        ),
        (
            "Gpu.map_relaxed (&device) (\\item -> item as i32) buffer",
            "float-to-integer",
        ),
        (
            "let escaped = Gpu.map_relaxed\nescaped (&device) (\\item -> item + 1.0) buffer",
            "cannot escape",
        ),
        (
            "let partial = Gpu.map_relaxed (&device)\npartial (\\item -> item + 1.0) buffer",
            "direct full application",
        ),
        (
            "let offset = 1.0f32\nGpu.map_relaxed (&device) (\\item -> item + offset) buffer",
            "known function or capture-free lambda",
        ),
        (
            "Gpu.init_relaxed (&device) 2 (\\index -> index as f64)",
            "no f64 or 64-bit integers",
        ),
    ] {
        let error = analyze(&format!("{prefix}{body}")).unwrap_err();
        assert_eq!(error.code, "E1018", "{body}: {error:?}");
        assert!(
            error.message.contains(fragment),
            "{body}: {}",
            error.message
        );
    }
}

const DEVICE_PROGRAM: &str = "def mix :: i32 -> i32\nfn mix value = value * 3 + 1\ndef poly :: f32 -> f32\nfn poly value = value * value + value\nlet device = Result.get (Gpu.request Gpu.WebGpu)\nlet a = Gpu.to_array (Gpu.init (&device) 8 mix)\nlet b = Gpu.to_array (Gpu.map (&device) mix (Gpu.init (&device) 8 mix))\nlet floats: [f32] = [1.0, 2.0]\nlet c = Gpu.to_array (Gpu.map_relaxed (&device) poly (Gpu.from_array (&device) (&floats)))\nlet d = Gpu.to_array (Gpu.map (&device) (\\item -> item + 1.0) (Gpu.from_array (&device) (&floats)))\nArray.sum (&a) + Array.sum (&b)";

#[test]
fn device_aware_programs_embed_one_kernel_per_call_site() {
    use tsuzuri::gpu_devices::{FLAG_RELAXED, LANE_32, LANE_F32};
    let module = analyze(DEVICE_PROGRAM).unwrap();
    // The strict i32 kernel serves both of its calls, the relaxed f32 kernel is a second one, and the strict
    // f32 lambda has none: a strict float call never runs on a device.
    let kernels = &module.gpu.kernels;
    assert_eq!(kernels.len(), 2, "{kernels:?}");
    assert_eq!(
        (kernels[0].relaxed, kernels[0].flags, kernels[0].lanes),
        (false, 0, LANE_32 | LANE_32 << 8)
    );
    assert_eq!(
        (kernels[1].relaxed, kernels[1].flags, kernels[1].lanes),
        (true, FLAG_RELAXED, LANE_F32 | LANE_F32 << 8)
    );
    assert_eq!(module.gpu.features, 0);
    let strict = kernels[0].wgsl.as_deref().unwrap();
    assert!(strict.starts_with("struct Params") && strict.contains("array<i32>"));
    let relaxed = kernels[1].wgsl.as_deref().unwrap();
    assert!(relaxed.starts_with("// tsuzuri-gpu float=relaxed input=f32 output=f32\n"));
    assert!(kernels.iter().all(|kernel| kernel.spirv.is_none()));
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, wasm).unwrap();
        assert_eq!(
            ir,
            tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, wasm).unwrap()
        );
        // The runtime functions are declared once, without import attributes: the driver gives them a host.
        assert_eq!(
            ir.matches("declare i32 @tsuzuri_gpu_open(i32, i32)\n")
                .count(),
            1
        );
        assert_eq!(ir.matches("declare i32 @tsuzuri_gpu_run(").count(), 1);
        assert!(!ir.contains("wasm-import-module\"=\"tsuzuri_gpu"));
        assert_eq!(
            ir.matches("@tz.gpu.kernels = private constant [2 x ")
                .count(),
            1
        );
        assert!(ir.contains("c\"// tsuzuri-gpu float=relaxed input=f32 output=f32\\0A"));
        // Every device-aware call passes its kernel number last: 0 and 1, and -1 for the strict f32 call.
        assert!(ir.contains("i64 0)") && ir.contains("i64 1)"));
        assert!(ir.contains("i64 18446744073709551615)"));
        assert!(ir.contains("@tz.fn.Gpu.request_on("));
        assert!(!ir.contains("@tz.fn.Gpu.request("));
    }
}

#[test]
fn relaxed_f16_kernels_enable_f16_and_need_the_shader_f16_feature() {
    use tsuzuri::gpu_devices::{FEATURE_F16, FLAG_RELAXED, LANE_32, LANE_F16, LANE_F32};
    let module = analyze(
        "def half :: f16 -> f16\nfn half value = (value * 0.5f16 + 0.25f16) * value - (-value)\ndef widen :: f16 -> f32\nfn widen value = (value as f32) * 3.0\ndef narrow :: f32 -> f16\nfn narrow value = value as f16\ndef index :: i32 -> f16\nfn index value = value as f16\nlet device = Result.get (Gpu.request Gpu.WebGpu)\nlet halves: [f16] = [1.0f16, 2.0f16]\nlet floats: [f32] = [1.0, 2.0]\nlet a = Gpu.to_array (Gpu.map_relaxed (&device) half (Gpu.from_array (&device) (&halves)))\nlet b = Gpu.to_array (Gpu.map_relaxed (&device) widen (Gpu.from_array (&device) (&halves)))\nlet c = Gpu.to_array (Gpu.map_relaxed (&device) narrow (Gpu.from_array (&device) (&floats)))\nlet d = Gpu.to_array (Gpu.init_relaxed (&device) 4 index)\nlet e = Gpu.to_array (Gpu.map (&device) half (Gpu.from_array (&device) (&halves)))\n0",
    )
    .unwrap();
    // The strict call of `half` has no kernel: strict f16 never runs on a device.
    let kernels = &module.gpu.kernels;
    assert_eq!(kernels.len(), 4, "{kernels:?}");
    assert_eq!(module.gpu.features, FEATURE_F16);
    let lanes = |input: u32, output: u32| input | output << 8;
    assert_eq!(
        kernels
            .iter()
            .map(|kernel| kernel.lanes)
            .collect::<Vec<_>>(),
        [
            lanes(LANE_F16, LANE_F16),
            lanes(LANE_F16, LANE_F32),
            lanes(LANE_F32, LANE_F16),
            lanes(LANE_32, LANE_F16)
        ]
    );
    assert!(kernels.iter().all(|kernel| kernel.relaxed
        && kernel.flags == FLAG_RELAXED
        && kernel.features == FEATURE_F16));
    let texts: Vec<_> = kernels
        .iter()
        .map(|kernel| kernel.wgsl.as_deref().unwrap())
        .collect();
    for (text, header) in texts.iter().zip(["f16", "f16", "f32", "i32"]) {
        assert!(
            text.starts_with("// tsuzuri-gpu float=relaxed input="),
            "{text}"
        );
        assert!(text.contains("enable f16;\nstruct Params"), "{text}");
        assert_eq!(text.matches("enable f16;").count(), 1);
        assert!(text.lines().next().unwrap().contains(header), "{text}");
    }
    assert!(texts[0].starts_with(
        "// tsuzuri-gpu float=relaxed input=f16 output=f16\nenable f16;\nstruct Params"
    ));
    assert!(texts[0].contains("var<storage, read> input_values: array<f16>"));
    assert!(texts[0].contains("var<storage, read_write> output_values: array<f16>"));
    for bits in [1056964608u32, 1048576000] {
        assert!(
            texts[0].contains(&format!("f16(bitcast<f32>({bits}u))")),
            "{bits}"
        );
    }
    assert!(texts[0].contains("(-value_"));
    assert!(texts[0].contains("kernel_") && texts[0].contains("(f16(invocation.x))"));
    assert!(texts[1].contains("input=f16 output=f32") && texts[1].contains("f32(value_"));
    assert!(texts[2].contains("input=f32 output=f16") && texts[2].contains("f16(value_"));
    assert!(texts[3].contains("input=i32 output=f16"));
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, wasm).unwrap();
        assert!(ir.contains("@tz.fn.$builtin.Gpu.__features()"));
        assert!(ir.contains("ret i32 1\n"));
        assert!(ir.contains("i64 18446744073709551615)"));
    }
    // A program without an f16 kernel does not ask the device for shader-f16.
    let plain = analyze(DEVICE_PROGRAM).unwrap();
    assert_eq!(plain.gpu.features, 0);
    // Strict f16 on the CPU reference is accepted; casts from f16 to an integer are not reproduced on the GPU.
    analyze("let device = Result.get (Gpu.request Gpu.CpuReference)\nlet halves: [f16] = [1.0f16]\nlet mapped = Gpu.map (&device) (\\item -> item + 1.0f16) (Gpu.from_array (&device) (&halves))\n0").unwrap();
    let error =
        relaxed_wgsl("export def kernel :: f32 -> i32\nfn kernel value = (value as f16) as i32")
            .unwrap_err();
    assert!(
        error.message.contains("float-to-integer"),
        "{}",
        error.message
    );
    let internal = relaxed_wgsl(
        "export def kernel :: f32 -> f32\nfn kernel value = ((value as f16) * 0.5f16) as f32",
    )
    .unwrap();
    assert!(internal.starts_with(
        "// tsuzuri-gpu float=relaxed input=f32 output=f32\nenable f16;\nstruct Params"
    ));
}

#[test]
fn programs_without_a_non_cpu_backend_keep_the_cpu_only_gpu_module() {
    let module = analyze("let device = Result.get (Gpu.request Gpu.CpuReference)\nlet buffer = Gpu.init (&device) 4 (\\index -> index * 2)\nlet mapped = Gpu.map (&device) (\\value -> value + 1) buffer\nlet values = Gpu.to_array mapped\nArray.sum (&values)").unwrap();
    assert!(module.gpu.kernels.is_empty());
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, wasm).unwrap();
        for absent in ["tsuzuri_gpu", "tz.gpu", "request_on", "init_on", "map_on"] {
            assert!(!ir.contains(absent), "{absent}");
        }
        assert!(ir.contains("@tz.fn.Gpu.request("));
    }
    // Naming any non-CPU backend makes a program device aware, even without a kernel.
    for backend in ["Vulkan", "Cuda", "Metal", "Auto", "WebGpu"] {
        let source = format!("let outcome = Gpu.request Gpu.{backend}\nResult.is_error (&outcome)");
        let module = analyze(&source).unwrap();
        assert!(module.gpu.kernels.is_empty(), "{backend}");
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Console, false).unwrap();
        assert!(ir.contains("@tz.fn.Gpu.request_on("), "{backend}");
    }
}

#[test]
fn device_aware_programs_validate_relaxed_kernels_and_hide_the_runtime_primitives() {
    let prefix = "let device = Result.get (Gpu.request Gpu.WebGpu)\nlet wide: [f64] = [1.0]\nlet floats: [f32] = [1.0]\n";
    for (body, code, fragment) in [
        (
            "Gpu.map_relaxed (&device) (\\item -> item + 1.0) (Gpu.from_array (&device) (&wide))",
            "E1018",
            "no f64 or 64-bit integers",
        ),
        (
            "Gpu.map_relaxed (&device) (\\item -> item as i32) (Gpu.from_array (&device) (&floats))",
            "E1018",
            "float-to-integer",
        ),
        (
            "let escaped = Gpu.map\nescaped (&device) (\\item -> item + 1.0) (Gpu.from_array (&device) (&floats))",
            "E1018",
            "cannot escape",
        ),
        (
            "Gpu.__open 1 0",
            "E1022",
            "private to the standard Gpu module",
        ),
        (
            "let status = Gpu.__features()\nstatus",
            "E1022",
            "private to the standard Gpu module",
        ),
    ] {
        let error = analyze(&format!("{prefix}{body}")).unwrap_err();
        assert_eq!(error.code, code, "{body}: {error:?}");
        assert!(
            error.message.contains(fragment),
            "{body}: {}",
            error.message
        );
    }
    // A strict f32 call is not an error: it only has no kernel for a device.
    analyze(&format!(
        "{prefix}Gpu.to_array (Gpu.map (&device) (\\item -> item + 1.0) (Gpu.from_array (&device) (&floats)))"
    ))
    .unwrap();
}

/// The parameters that a function of the WGSL text assigns; WGSL parameters are immutable.
fn assigned_parameters(wgsl: &str) -> Vec<String> {
    let mut parameters: Vec<String> = Vec::new();
    let mut assigned = Vec::new();
    for line in wgsl.lines() {
        if let Some(rest) = line.strip_prefix("fn kernel_") {
            let list = rest.split_once('(').unwrap().1.split_once(')').unwrap().0;
            parameters = list
                .split(", ")
                .filter(|parameter| !parameter.is_empty())
                .map(|parameter| parameter.split(':').next().unwrap().to_owned())
                .collect();
        } else if line.starts_with("fn ") {
            parameters.clear();
        } else if let Some((target, _)) = line.split_once(" = ")
            && parameters.iter().any(|parameter| parameter == target)
        {
            assigned.push(line.to_owned());
        }
    }
    assigned
}

/// The deepest statement of the kernel functions in the WGSL text, counted as Tint counts: a function
/// body is depth 1, a statement is one deeper than its block, and the blocks of an `if` are one deeper
/// than the `if`.
fn statement_depth(wgsl: &str) -> usize {
    let (mut open, mut deepest, mut inside) = (0usize, 0usize, false);
    for line in wgsl.lines() {
        if line.starts_with("fn kernel_") {
            (inside, open, deepest) = (true, 0, deepest.max(1));
        } else if inside && line == "}" && open == 0 {
            inside = false;
        } else if inside {
            match line {
                "}" => open -= 1,
                "} else {" => deepest = deepest.max(1 + 2 * open),
                _ => {
                    deepest = deepest.max(2 + 2 * open);
                    if line.ends_with('{') {
                        open += 1;
                        deepest = deepest.max(1 + 2 * open);
                    }
                }
            }
        }
    }
    deepest
}

#[test]
fn mutable_parameters_become_variables_in_wgsl() {
    let module = analyze(
        "export def kernel :: f32 -> f32\nfn kernel mut x = { x = x * 2.0f32; x + 1.0f32 }",
    )
    .unwrap();
    let id = kernel_id(&module);
    let local = module.functions[id].parameters[0].id;
    let wgsl = gpu::extract_kernel(&module, id)
        .unwrap()
        .wgsl_relaxed()
        .unwrap();
    let expected = format!(
        "fn kernel_{id}(param_{local}: f32) -> f32 {{\nvar local_{local}: f32 = param_{local};\nlet value_0: f32 = local_{local};\nlet value_1: f32 = bitcast<f32>(1073741824u);\nlet value_2: f32 = (value_0 * value_1);\nlocal_{local} = value_2;\nlet value_3: f32 = local_{local};\nlet value_4: f32 = bitcast<f32>(1065353216u);\nlet value_5: f32 = (value_3 + value_4);\nlet value_6: f32 = value_5;\nreturn value_6;\n}}\n"
    );
    assert!(wgsl.contains(&expected), "{wgsl}");
    assert!(assigned_parameters(&wgsl).is_empty(), "{wgsl}");

    // Strict kernels: mutable parameters of helpers (integer, unsigned, bool) are copied too, and the
    // parameters that no one assigns keep their name.
    let module = analyze(
        "def bump :: i32 -> i32\nfn bump mut x = { x = x + 1; x * 2 }\ndef toggle :: bool -> bool\nfn toggle mut flag = { flag = !flag; flag }\ndef wrap :: i32u -> i32u\nfn wrap mut x = { x = x * 3i32u; x ^ 7i32u }\nexport def kernel :: i32 -> i32\nfn kernel value = if toggle (value > 0) then bump value else (wrap (value as i32u)) as i32",
    )
    .unwrap();
    let id = kernel_id(&module);
    let local = module.functions[id].parameters[0].id;
    let wgsl = gpu::extract_kernel(&module, id).unwrap().wgsl().unwrap();
    assert_eq!(wgsl.matches(" = param_").count(), 3, "{wgsl}");
    for ty in ["i32", "bool", "u32"] {
        assert!(wgsl.contains(&format!(": {ty} = param_")), "{ty}: {wgsl}");
    }
    assert!(
        wgsl.contains(&format!("fn kernel_{id}(local_{local}: i32) -> i32 {{\n")),
        "{wgsl}"
    );
    assert!(assigned_parameters(&wgsl).is_empty(), "{wgsl}");

    // The kernels that a device-aware program embeds come from the same emitter.
    let module = analyze(
        "def bump :: f32 -> f32\nfn bump mut x = { x = x * 2.0f32; x + 1.0f32 }\ndef count :: i32 -> i32\nfn count mut n = { n = n + 1; n }\nlet device = Result.get (Gpu.request Gpu.WebGpu)\nlet floats: [f32] = [1.0, 2.0]\nlet a = Gpu.to_array (Gpu.map_relaxed (&device) bump (Gpu.from_array (&device) (&floats)))\nlet b = Gpu.to_array (Gpu.init (&device) 4 count)\n0",
    )
    .unwrap();
    assert_eq!(module.gpu.kernels.len(), 2);
    for kernel in &module.gpu.kernels {
        let wgsl = kernel.wgsl.as_deref().unwrap();
        assert!(wgsl.contains(" = param_"), "{wgsl}");
        assert!(assigned_parameters(wgsl).is_empty(), "{wgsl}");
    }
}

/// The debug build of the checker needs about 100 KB of stack for each else-if arm, so the tests with
/// deep sources run on a thread with a large stack. The compiler's own limits are not involved.
fn on_large_stack(test: impl FnOnce() + Send + 'static) {
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(test)
        .unwrap();
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
}

/// `if value == 0 then 100 else if value == 1 then 101 else ... else <tail>` with `arms` conditions.
fn chain_expression(arms: usize, ty: &str, tail: &str) -> String {
    let literal = |number: usize| {
        if ty == "f32" {
            format!("{number}.0")
        } else {
            number.to_string()
        }
    };
    let mut body = String::new();
    for arm in 0..arms {
        body += &format!(
            "if value == {} then {} else ",
            literal(arm),
            literal(100 + arm)
        );
    }
    body + tail
}

fn else_if_chain(arms: usize, ty: &str) -> String {
    let zero = if ty == "f32" { "0.0" } else { "0" };
    format!(
        "export def kernel :: {ty} -> {ty}\nfn kernel value = {}",
        chain_expression(arms, ty, zero)
    )
}

/// A kernel that nests `shape(levels)` below `arms` else-if arms.
fn nested_kernel(arms: usize, shape: &str, levels: usize) -> String {
    let mut tail = match shape {
        // if value == 0 then (if value == 1 then (... 1 else 0) else 0) else 0
        "if" => "1".to_owned(),
        // if value == 0 && (value == 1 && (... && value == N)) then 1 else 0
        "&&" | "||" => format!("value == {levels}"),
        // { if value == 0 then (if value == 1 then (... () else ()) else ()) else (); value }, which is
        // empty inside
        "empty" => "()".to_owned(),
        _ => unreachable!(),
    };
    for level in (0..levels).rev() {
        tail = match shape {
            "if" => format!("if value == {level} then ({tail}) else 0"),
            "empty" => format!("if value == {level} then ({tail}) else ()"),
            operator => format!("value == {level} {operator} ({tail})"),
        };
    }
    let tail = match shape {
        "if" => tail,
        "empty" => format!("{{ {tail}; value }}"),
        _ => format!("if {tail} then 1 else 0"),
    };
    format!(
        "export def kernel :: i32 -> i32\nfn kernel value = {}",
        chain_expression(arms, "i32", &format!("({tail})"))
    )
}

fn strict_wgsl(source: &str) -> Result<String, tsuzuri::diagnostic::Diagnostic> {
    let module = analyze(source).unwrap();
    gpu::extract_kernel(&module, kernel_id(&module))?.wgsl()
}

#[test]
fn rejects_statements_nested_deeper_than_wgsl_allows() {
    on_large_stack(|| {
        // Tint rejects 128 levels ("statement nesting depth / chaining length exceeds limit of 127"). An
        // else-if chain of 62 arms is the longest that it accepts: each arm nests an if in the else block,
        // and everything else that nests (an if in a then branch, the right operand of && or ||) costs two
        // levels as well. The depth is counted again from the emitted text.
        for ty in ["i32", "f32"] {
            let wgsl = |source: &str| {
                if ty == "f32" {
                    relaxed_wgsl(source)
                } else {
                    strict_wgsl(source)
                }
            };
            let accepted = wgsl(&else_if_chain(62, ty)).unwrap();
            assert_eq!(statement_depth(&accepted), 126, "{ty}");
            let error = wgsl(&else_if_chain(63, ty)).unwrap_err();
            assert_eq!(error.code, "E1017", "{ty}");
            assert!(
                error
                    .message
                    .contains("127 levels of WGSL statement nesting"),
                "{}",
                error.message
            );
        }
        // 40 arms, then `levels` of one shape: (shape, deepest accepted levels, the depth there). The sum of
        // arms and levels is at most 62, or 63 when the innermost blocks are empty and hold no statement (a
        // block of depth 127 is accepted).
        for (shape, levels, depth) in [
            ("if", 22, 126),
            ("&&", 22, 126),
            ("||", 22, 126),
            ("empty", 23, 127),
        ] {
            let accepted = strict_wgsl(&nested_kernel(40, shape, levels)).unwrap();
            assert_eq!(statement_depth(&accepted), depth, "{shape}");
            let error = strict_wgsl(&nested_kernel(40, shape, levels + 1)).unwrap_err();
            assert_eq!(error.code, "E1017", "{shape}");
        }
        // A helper function starts a new body, so a long chain can continue in it.
        let chain = else_if_chain(62, "i32");
        let chain = chain.split_once("= ").unwrap().1;
        let wgsl = strict_wgsl(&format!(
            "def tail :: i32 -> i32\nfn tail value = {chain}\nexport def kernel :: i32 -> i32\nfn kernel value = {}",
            chain.replace(" else 0", " else tail value")
        ))
        .unwrap();
        assert_eq!(wgsl.matches("fn kernel_").count(), 2);
        assert_eq!(statement_depth(&wgsl), 126);
    });
}

#[test]
fn the_nesting_limit_reaches_relaxed_calls_and_embedded_kernels() {
    on_large_stack(|| {
        // A program that calls a callback with `arms` else-if arms (of `ty`) on a backend.
        let program = |arms: usize, ty: &str, backend: &str, call: &str| {
            let chain = else_if_chain(arms, ty)
                .replace("export def kernel", "def chain")
                .replace("fn kernel", "fn chain");
            let values = if ty == "f32" { "[1.0, 2.0]" } else { "[1, 2]" };
            format!(
                "{chain}\nlet device = Result.get (Gpu.request Gpu.{backend})\nlet values: [{ty}] = {values}\nGpu.to_array (Gpu.{call} (&device) chain (Gpu.from_array (&device) (&values)))"
            )
        };
        // The CPU reference does not need WGSL: a strict call is fine, a relaxed call is validated.
        analyze(&program(63, "i32", "CpuReference", "map")).unwrap();
        for backend in ["CpuReference", "WebGpu"] {
            let error = analyze(&program(63, "f32", backend, "map_relaxed")).unwrap_err();
            assert_eq!(error.code, "E1017", "{backend}: {error:?}");
            assert!(error.message.contains("127 levels"), "{}", error.message);
        }
        // On a device a strict call has no kernel, like any strict call whose WGSL cannot be written.
        let module = analyze(&program(63, "i32", "WebGpu", "map")).unwrap();
        assert!(module.gpu.kernels.is_empty());
        // 62 arms embed, strict (i32) and relaxed (f32).
        for (ty, call) in [("i32", "map"), ("f32", "map_relaxed")] {
            let module = analyze(&program(62, ty, "WebGpu", call)).unwrap();
            assert_eq!(module.gpu.kernels.len(), 1, "{call}");
            let wgsl = module.gpu.kernels[0].wgsl.as_deref().unwrap();
            assert_eq!(statement_depth(wgsl), 126, "{call}");
        }
    });
}
