use super::*;

/// The kernel table of a program that runs kernels on a non-CPU backend (F09 Phase 2). It holds
/// one descriptor per kernel, which `Gpu.__run` and `Gpu.__select` index with the number that
/// `gpu::lower_kernels` wrote into the call. A descriptor is `{ flags, lanes, features, wgsl,
/// wgsl_length, spirv, spirv_length, weight }`, where the weight (F09 Phase 3) is a lower bound of
/// the operations one lane of the SPIR-V kernel executes, which `Gpu.Auto` prices the CPU run with
/// (`SpirvKernel::weight`); the type has no name, because LLVM
/// needs a named type to be defined before the functions that use it.
const KERNEL: &str = "{ i32, i32, i32, ptr, i32, ptr, i32, i32 }";
const TABLE: &str = "@tz.gpu.kernels = private constant";
/// The backend tag of the most recent device call (F09 Phase 3): `Gpu.__select` writes it and
/// `Gpu.__last` reads it. Accesses are atomic, so concurrent calls never race on it; their order is
/// unspecified.
const LAST: &str = "@tz.gpu.last = internal global i32 0, align 4";
/// What makes the driver add the Vulkan runtime to the GPU runtime: the program names `Gpu.Vulkan`
/// or `Gpu.Auto` (F09 Phase 3).
const VULKAN_MARKER: &str = "; tsuzuri-gpu: vulkan";

const OPEN_DECLARATION: &str = "declare i32 @tsuzuri_gpu_open(i32, i32)";
const RUN_DECLARATION: &str =
    "declare i32 @tsuzuri_gpu_run(i32, i32, i32, i32, ptr, i32, ptr, i32, ptr, i64, ptr)";
const SELECT_DECLARATION: &str =
    "declare i32 @tsuzuri_gpu_select(i32, i32, i32, ptr, i32, i32, i64)";

/// Defines `Gpu.__open`, `Gpu.__features`, `Gpu.__run`, `Gpu.__select`, and `Gpu.__last` (F09
/// Phases 2 and 3). The runtime functions are declared plainly for every target;
/// `with_wasm_gpu_host` and the driver decide who defines them.
pub(super) fn emit(
    instance: &BuiltinInstance,
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    symbol: &str,
) -> String {
    if module.gpu.vulkan
        && !globals
            .definitions
            .iter()
            .any(|definition| definition.contains(VULKAN_MARKER))
    {
        globals.definitions.push(format!("{VULKAN_MARKER}\n"));
    }
    match instance.builtin {
        Builtin::GpuFeatures => format!(
            "define internal i32 {symbol}() nounwind {{\nentry:\n  ret i32 {}\n}}\n\n",
            module.gpu.features
        ),
        Builtin::GpuOpen => {
            intrinsics.insert(OPEN_DECLARATION.into());
            format!(
                "define internal i32 {symbol}(i32 %backend, i32 %features) nounwind {{\nentry:\n  \
                 %status = call i32 @tsuzuri_gpu_open(i32 %backend, i32 %features)\n  ret i32 %status\n}}\n\n"
            )
        }
        Builtin::GpuRun => run(instance, module, intrinsics, globals, symbol),
        Builtin::GpuSelect => select(module, intrinsics, globals, symbol),
        Builtin::GpuLast => {
            define_last(globals);
            format!(
                "define internal i32 {symbol}() nounwind {{\nentry:\n  \
                 %value = load atomic i32, ptr @tz.gpu.last monotonic, align 4\n  ret i32 %value\n}}\n\n"
            )
        }
        _ => unreachable!("only the GPU builtins are emitted here"),
    }
}

fn define_table(module: &CheckedModule, globals: &mut Globals) {
    if !globals
        .definitions
        .iter()
        .any(|definition| definition.contains(TABLE))
    {
        globals.definitions.push(kernel_table(module));
    }
}

fn define_last(globals: &mut Globals) {
    if !globals
        .definitions
        .iter()
        .any(|definition| definition.contains(LAST))
    {
        globals.definitions.push(format!("{LAST}\n"));
    }
}

/// `Gpu.__select backend kernel mode count`: the backend tag that serves a call, which it also
/// records for `Gpu.__last`. A named backend serves the call itself. `Gpu.Auto` (5) asks the
/// runtime, with the kernel's descriptor, to choose between the CPU reference (0) and a device by
/// the measured cost rule, and a call without a kernel runs on the CPU.
fn select(
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    symbol: &str,
) -> String {
    intrinsics.insert(SELECT_DECLARATION.into());
    define_table(module, globals);
    define_last(globals);
    let kernels = module.gpu.kernels.len();
    let field = |name: &str, index: usize, ty: &str| {
        format!(
            "  %{name}_slot = getelementptr inbounds {KERNEL}, ptr %descriptor, i32 0, i32 {index}\n  \
             %{name} = load {ty}, ptr %{name}_slot{}\n",
            if ty == "ptr" { "" } else { ", align 4" }
        )
    };
    format!(
        "define internal i32 {symbol}(i32 %backend, i64 %kernel, i32 %mode, i64 %count) nounwind {{\n\
         entry:\n  \
         %auto = icmp eq i32 %backend, 5\n  \
         br i1 %auto, label %check, label %done\n\
         check:\n  \
         %known = icmp ult i64 %kernel, {kernels}\n  \
         br i1 %known, label %lookup, label %cpu\n\
         lookup:\n  \
         %descriptor = getelementptr inbounds {KERNEL}, ptr @tz.gpu.kernels, i64 %kernel\n\
         {lanes}{features}{spirv}{spirv_length}{weight}  \
         %chosen = call i32 @tsuzuri_gpu_select(i32 %mode, i32 %lanes, i32 %features, ptr %spirv, i32 %spirv_length, i32 %weight, i64 %count)\n  \
         br label %done\n\
         cpu:\n  \
         br label %done\n\
         done:\n  \
         %result = phi i32 [ %backend, %entry ], [ %chosen, %lookup ], [ 0, %cpu ]\n  \
         store atomic i32 %result, ptr @tz.gpu.last monotonic, align 4\n  \
         ret i32 %result\n}}\n\n",
        lanes = field("lanes", 1, "i32"),
        features = field("features", 2, "i32"),
        spirv = field("spirv", 5, "ptr"),
        spirv_length = field("spirv_length", 6, "i32"),
        weight = field("weight", 7, "i32"),
    )
}

fn run(
    instance: &BuiltinInstance,
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    symbol: &str,
) -> String {
    intrinsics.insert(RUN_DECLARATION.into());
    define_table(module, globals);
    let output = llvm_type(&instance.types[1], module);
    let input_kind = lane_kind(&instance.types[0]);
    let output_kind = lane_kind(&instance.types[1]);
    let kernels = module.gpu.kernels.len();
    // The runtime reads the descriptor's fields as scalars, so no struct layout crosses to the host.
    // A call without a kernel (number -1: a strict call that none of the program's backends can
    // reproduce) passes no source, and the runtime says so before this traps. A kernel whose lane kinds
    // are not those of this instance's element types passes no source either: the host sizes its copies
    // of the input and the output by the kinds, so any other kernel would read or write past the arrays.
    // The compiler numbers every call's kernel itself (the std functions that carry the number are
    // private), so this is only a second guard: it keeps the arrays safe but cannot tell kernels with
    // equal lanes apart.
    format!(
        "define internal %tz.array {symbol}(i32 %backend, i64 %kernel, i32 %mode, %tz.array %input, i64 %count) nounwind {{\n\
         entry:\n  \
         %fits = icmp ule i64 %count, 2147483647\n  \
         br i1 %fits, label %prepare, label %fail\n\
         prepare:\n  \
         %known = icmp ult i64 %kernel, {kernels}\n  \
         br i1 %known, label %lookup, label %call\n\
         lookup:\n  \
         %descriptor = getelementptr inbounds {KERNEL}, ptr @tz.gpu.kernels, i64 %kernel\n  \
         %flags_slot = getelementptr inbounds {KERNEL}, ptr %descriptor, i32 0, i32 0\n  \
         %found_flags = load i32, ptr %flags_slot, align 4\n  \
         %lanes_slot = getelementptr inbounds {KERNEL}, ptr %descriptor, i32 0, i32 1\n  \
         %found_lanes = load i32, ptr %lanes_slot, align 4\n  \
         %wgsl_slot = getelementptr inbounds {KERNEL}, ptr %descriptor, i32 0, i32 3\n  \
         %found_wgsl = load ptr, ptr %wgsl_slot\n  \
         %wgsl_length_slot = getelementptr inbounds {KERNEL}, ptr %descriptor, i32 0, i32 4\n  \
         %found_wgsl_length = load i32, ptr %wgsl_length_slot, align 4\n  \
         %spirv_slot = getelementptr inbounds {KERNEL}, ptr %descriptor, i32 0, i32 5\n  \
         %found_spirv = load ptr, ptr %spirv_slot\n  \
         %spirv_length_slot = getelementptr inbounds {KERNEL}, ptr %descriptor, i32 0, i32 6\n  \
         %found_spirv_length = load i32, ptr %spirv_length_slot, align 4\n  \
         %found_input_kind = and i32 %found_lanes, 255\n  \
         %found_shifted = lshr i32 %found_lanes, 8\n  \
         %found_output_kind = and i32 %found_shifted, 255\n  \
         %input_matches = icmp eq i32 %found_input_kind, {input_kind}\n  \
         %output_matches = icmp eq i32 %found_output_kind, {output_kind}\n  \
         %matches = and i1 %input_matches, %output_matches\n  \
         %kept_flags = select i1 %matches, i32 %found_flags, i32 0\n  \
         %kept_lanes = select i1 %matches, i32 %found_lanes, i32 0\n  \
         %kept_wgsl = select i1 %matches, ptr %found_wgsl, ptr null\n  \
         %kept_wgsl_length = select i1 %matches, i32 %found_wgsl_length, i32 0\n  \
         %kept_spirv = select i1 %matches, ptr %found_spirv, ptr null\n  \
         %kept_spirv_length = select i1 %matches, i32 %found_spirv_length, i32 0\n  \
         br label %call\n\
         call:\n  \
         %flags = phi i32 [ %kept_flags, %lookup ], [ 0, %prepare ]\n  \
         %lanes = phi i32 [ %kept_lanes, %lookup ], [ 0, %prepare ]\n  \
         %wgsl = phi ptr [ %kept_wgsl, %lookup ], [ null, %prepare ]\n  \
         %wgsl_length = phi i32 [ %kept_wgsl_length, %lookup ], [ 0, %prepare ]\n  \
         %spirv = phi ptr [ %kept_spirv, %lookup ], [ null, %prepare ]\n  \
         %spirv_length = phi i32 [ %kept_spirv_length, %lookup ], [ 0, %prepare ]\n  \
         %size_end = getelementptr {output}, ptr null, i32 1\n  \
         %size = ptrtoint ptr %size_end to i64\n  \
         %total = mul i64 %count, %size\n  \
         %empty = icmp eq i64 %count, 0\n  \
         %bytes = select i1 %empty, i64 1, i64 %total\n  \
         %data = call ptr @tz.alloc(i64 %bytes)\n  \
         %input_data = extractvalue %tz.array %input, 0\n  \
         %status = call i32 @tsuzuri_gpu_run(i32 %backend, i32 %mode, i32 %flags, i32 %lanes, ptr %wgsl, i32 %wgsl_length, ptr %spirv, i32 %spirv_length, ptr %input_data, i64 %count, ptr %data)\n  \
         %done = icmp eq i32 %status, 0\n  \
         br i1 %done, label %finish, label %release\n\
         release:\n  \
         call void @tz.free(ptr %data)\n  \
         call void @llvm.trap()\n  \
         unreachable\n\
         finish:\n  \
         %partial = insertvalue %tz.array zeroinitializer, ptr %data, 0\n  \
         %array = insertvalue %tz.array %partial, i64 %count, 1\n  \
         ret %tz.array %array\n\
         fail:\n  \
         call void @llvm.trap()\n  \
         unreachable\n}}\n\n"
    )
}

/// The lane kind that a kernel descriptor needs for buffers of `ty`; -1, which no descriptor has, for any other type.
fn lane_kind(ty: &Type) -> i64 {
    crate::gpu_devices::lane(ty).map_or(-1, i64::from)
}

/// The descriptors `{ flags, lanes, features, wgsl, wgsl_length, spirv, spirv_length, weight }` of the
/// program's kernels, and their source texts, in kernel order.
fn kernel_table(module: &CheckedModule) -> String {
    let mut text = String::new();
    let mut entries = Vec::new();
    for (index, kernel) in module.gpu.kernels.iter().enumerate() {
        // The SPIR-V words are read as 32-bit values, so their text is aligned to 4 bytes.
        let mut constant = |suffix: &str, bytes: Option<&[u8]>, align: u32| match bytes {
            Some(bytes) if !bytes.is_empty() => {
                let name = format!("@tz.gpu.kernel.{index}.{suffix}");
                let _ = writeln!(
                    text,
                    "{name} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align {align}",
                    bytes.len() + 1,
                    escape(bytes)
                );
                (name, bytes.len())
            }
            _ => ("null".to_owned(), 0),
        };
        let (wgsl, wgsl_length) = constant("wgsl", kernel.wgsl.as_deref().map(str::as_bytes), 1);
        let (spirv, spirv_length) = constant("spirv", kernel.spirv.as_deref(), 4);
        entries.push(format!(
            "{KERNEL} {{ i32 {}, i32 {}, i32 {}, ptr {wgsl}, i32 {wgsl_length}, ptr {spirv}, i32 {spirv_length}, i32 {} }}",
            kernel.flags, kernel.lanes, kernel.features, kernel.weight
        ));
    }
    if entries.is_empty() {
        let _ = writeln!(text, "{TABLE} [0 x {KERNEL}] zeroinitializer, align 8");
    } else {
        let _ = writeln!(
            text,
            "{TABLE} [{} x {KERNEL}] [{}], align 8",
            entries.len(),
            entries.join(", ")
        );
    }
    text
}

fn escape(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            b' '..=b'~' if *byte != b'"' && *byte != b'\\' => text.push(*byte as char),
            _ => {
                let _ = write!(text, "\\{byte:02X}");
            }
        }
    }
    text
}

/// Gives the GPU runtime functions that a program declares their host (F09 Phase 2). Natively
/// the driver links `src/runtime/gpu.c`, which defines them. On WASM, `--wasm-feature webgpu`
/// makes `open` and `run` the JSPI imports `tsuzuri_gpu.open` and `tsuzuri_gpu.run`; without it
/// they are local functions that report the status 1 (unavailable), so the default module keeps no
/// import and `Gpu.request Gpu.WebGpu` and `Gpu.request Gpu.Vulkan` fail explicitly. `select`
/// (F09 Phase 3) is always a local function that chooses the CPU reference: no WASM host has a
/// measured cost rule, so `Gpu.Auto` never leaves the CPU there, and no import is added for it.
pub(crate) fn with_wasm_gpu_host(ir: String, webgpu: bool) -> String {
    if !ir.contains(OPEN_DECLARATION)
        && !ir.contains(RUN_DECLARATION)
        && !ir.contains(SELECT_DECLARATION)
    {
        return ir;
    }
    let (open, run) = if webgpu {
        (
            format!(
                "{OPEN_DECLARATION} \"wasm-import-module\"=\"tsuzuri_gpu\" \"wasm-import-name\"=\"open\""
            ),
            format!(
                "{RUN_DECLARATION} \"wasm-import-module\"=\"tsuzuri_gpu\" \"wasm-import-name\"=\"run\""
            ),
        )
    } else {
        (
            "define internal i32 @tsuzuri_gpu_open(i32 %backend, i32 %features) nounwind {\nentry:\n  ret i32 1\n}"
                .to_owned(),
            "define internal i32 @tsuzuri_gpu_run(i32 %backend, i32 %mode, i32 %flags, i32 %lanes, ptr %wgsl, i32 %wgsl_length, ptr %spirv, i32 %spirv_length, ptr %input, i64 %count, ptr %output) nounwind {\nentry:\n  ret i32 1\n}"
                .to_owned(),
        )
    };
    let select = "define internal i32 @tsuzuri_gpu_select(i32 %mode, i32 %lanes, i32 %features, ptr %spirv, i32 %spirv_length, i32 %weight, i64 %count) nounwind {\nentry:\n  ret i32 0\n}";
    ir.replace(OPEN_DECLARATION, &open)
        .replace(RUN_DECLARATION, &run)
        .replace(SELECT_DECLARATION, select)
}

/// The C source of the native GPU runtime of a program (F09 Phases 2 and 3): the WebGPU backend,
/// preceded by the Vulkan backend when the program names `Gpu.Vulkan` or `Gpu.Auto`. Both go into
/// the one translation unit of the driver's runtime, which `TZ_GPU_VULKAN` tells that Vulkan is
/// there.
pub(crate) fn runtime_source(ir: &str) -> String {
    let webgpu = include_str!("runtime/gpu.c");
    if ir.contains(VULKAN_MARKER) {
        format!(
            "#define TZ_GPU_VULKAN 1\n{}\n{webgpu}",
            include_str!("runtime/gpu-vulkan.c")
        )
    } else {
        webgpu.to_owned()
    }
}
