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
