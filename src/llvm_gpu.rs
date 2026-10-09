use super::*;

/// The kernel table of a program that runs kernels on a non-CPU backend (F09 Phase 2). It holds
/// one descriptor per kernel, which `Gpu.__run` indexes with the number that
/// `gpu::lower_kernels` wrote into the call. A descriptor is `{ flags, lanes, features, wgsl,
/// wgsl_length, spirv, spirv_length }`; it has no name, because LLVM needs a named type to be
/// defined before the functions that use it.
const KERNEL: &str = "{ i32, i32, i32, ptr, i32, ptr, i32 }";
const TABLE: &str = "@tz.gpu.kernels = private constant";

const OPEN_DECLARATION: &str = "declare i32 @tsuzuri_gpu_open(i32, i32)";
const RUN_DECLARATION: &str =
    "declare i32 @tsuzuri_gpu_run(i32, i32, i32, i32, ptr, i32, ptr, i32, ptr, i64, ptr)";

/// Defines `Gpu.__open`, `Gpu.__features`, and `Gpu.__run` (F09 Phase 2). The runtime functions
/// are declared plainly for every target; `with_gpu_host` and the driver decide who defines them.
pub(super) fn emit(
    instance: &BuiltinInstance,
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    symbol: &str,
) -> String {
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
        _ => unreachable!("only the GPU builtins are emitted here"),
    }
}

fn run(
    instance: &BuiltinInstance,
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    symbol: &str,
) -> String {
    intrinsics.insert(RUN_DECLARATION.into());
    if !globals
        .definitions
        .iter()
        .any(|definition| definition.contains(TABLE))
    {
        globals.definitions.push(kernel_table(module));
    }
    let output = llvm_type(&instance.types[1], module);
    let kernels = module.gpu.kernels.len();
    // The runtime reads the descriptor's fields as scalars, so no struct layout crosses to the host.
    // A call without a kernel (number -1: a strict call whose lanes are not 32-bit integers) passes
    // no source, and the runtime says so before this traps.
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
         br label %call\n\
         call:\n  \
         %flags = phi i32 [ %found_flags, %lookup ], [ 0, %prepare ]\n  \
         %lanes = phi i32 [ %found_lanes, %lookup ], [ 0, %prepare ]\n  \
         %wgsl = phi ptr [ %found_wgsl, %lookup ], [ null, %prepare ]\n  \
         %wgsl_length = phi i32 [ %found_wgsl_length, %lookup ], [ 0, %prepare ]\n  \
         %spirv = phi ptr [ %found_spirv, %lookup ], [ null, %prepare ]\n  \
         %spirv_length = phi i32 [ %found_spirv_length, %lookup ], [ 0, %prepare ]\n  \
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

/// The descriptors `{ flags, lanes, features, wgsl, wgsl_length, spirv, spirv_length }` of the
/// program's kernels, and their source texts, in kernel order.
fn kernel_table(module: &CheckedModule) -> String {
    let mut text = String::new();
    let mut entries = Vec::new();
    for (index, kernel) in module.gpu.kernels.iter().enumerate() {
        let mut constant = |suffix: &str, bytes: Option<&[u8]>| match bytes {
            Some(bytes) if !bytes.is_empty() => {
                let name = format!("@tz.gpu.kernel.{index}.{suffix}");
                let _ = writeln!(
                    text,
                    "{name} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                    bytes.len() + 1,
                    escape(bytes)
                );
                (name, bytes.len())
            }
            _ => ("null".to_owned(), 0),
        };
        let (wgsl, wgsl_length) = constant("wgsl", kernel.wgsl.as_deref().map(str::as_bytes));
        let (spirv, spirv_length) = constant("spirv", kernel.spirv.as_deref());
        entries.push(format!(
            "{KERNEL} {{ i32 {}, i32 {}, i32 {}, ptr {wgsl}, i32 {wgsl_length}, ptr {spirv}, i32 {spirv_length} }}",
            kernel.flags, kernel.lanes, kernel.features
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
/// makes them the JSPI imports `tsuzuri_gpu.open` and `tsuzuri_gpu.run`; without it they are
/// local functions that report the status 1 (unavailable), so the default module keeps no
/// import and `Gpu.request Gpu.WebGpu` fails explicitly.
pub(crate) fn with_wasm_gpu_host(ir: String, webgpu: bool) -> String {
    if !ir.contains(OPEN_DECLARATION) && !ir.contains(RUN_DECLARATION) {
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
    ir.replace(OPEN_DECLARATION, &open)
        .replace(RUN_DECLARATION, &run)
}
