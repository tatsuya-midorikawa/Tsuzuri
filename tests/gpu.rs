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
    let module = analyze("def square :: i32 -> i32\nfn square value = value * value\nexport def kernel :: i32 -> i32\nfn kernel value = { let mut current = value; current = current + 1; if value < 0 then square current else ((current >>> 3) as i32u) as i32 }").unwrap();
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
