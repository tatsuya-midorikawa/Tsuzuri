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
