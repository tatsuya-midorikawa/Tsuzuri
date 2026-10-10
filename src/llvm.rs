use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::check::{
    Builtin, BuiltinInstance, CheckedFunction, CheckedModule, FunctionRef, Local, ModuleOrigin,
    SharedKind, Type, TypedExpr, TypedExprKind,
};
use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::{BinaryOp, StringLiteral, UnaryOp};
use crate::trap::{TrapKind, TrapSource};

#[path = "llvm_traps.rs"]
mod traps;
pub use traps::EmitOutput;

#[path = "call_specialization.rs"]
pub(crate) mod call_specialization;
#[path = "llvm_control.rs"]
mod control;
#[path = "llvm_debug.rs"]
mod debug;
pub use debug::with_codeview;
#[path = "llvm_frame.rs"]
mod frame;
use call_specialization::{ClosureTarget, Specialization, Specializations};
use frame::Frame;

/// The builtin instances that emitted code calls, with their concrete
/// callee types.
type Builtins = BTreeMap<BuiltinInstance, Type>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Library,
    Console,
    TestRunner,
    /// `tsuzuri bench`: `@tsuzuri_bench_count` and `@tsuzuri_bench_sample` over the benches (G18).
    BenchRunner,
}

#[derive(Clone, Copy, Debug)]
pub struct EmitOptions {
    pub entry: Entry,
    pub wasm: bool,
    pub debug_output: bool,
    /// Where the program's heap blocks come from (F13).
    pub allocator: Allocator,
}

/// The heap runtime behind `@tz.alloc`, `@tz.free`, and `@tz.realloc` (F13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Allocator {
    /// The target's allocator: the C library's on native targets, the module's free list on WASM.
    #[default]
    System,
    /// The host defines `tsuzuri_host_alloc`, `tsuzuri_host_free`, and `tsuzuri_host_realloc`;
    /// WASM modules import them from `tsuzuri_heap`.
    Host,
    /// The system allocator with counts that `tsuzuri_alloc_stats` reports (Phase 2).
    Counting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportStyle {
    Default,
    Dllexport,
}

pub fn emit_with_export_style(
    module: &CheckedModule,
    options: EmitOptions,
    style: ExportStyle,
) -> Result<String, Diagnostic> {
    if options.wasm && style == ExportStyle::Dllexport {
        return Err(Diagnostic::new(
            "E2000",
            "dllexport requires a native target",
            Span::default(),
        ));
    }
    let ir = emit_with_options(module, options)?;
    Ok(if style == ExportStyle::Dllexport {
        windows_abi(ir, module)
    } else {
        ir
    })
}

pub(crate) fn windows_abi(ir: String, module: &CheckedModule) -> String {
    let mut output = String::with_capacity(ir.len());
    let mut exports: BTreeSet<_> = module
        .functions
        .iter()
        .filter(|function| function.exported)
        .map(|function| format!("tz_{}", function.name))
        .collect();
    if io_entry(module) || main_entry(module) {
        exports.insert("tsuzuri_main".into());
    }
    let has_write = ir.contains("declare i64 @write(i32, ptr, i64)");
    let has_console = ir.contains("define internal i32 @tz.console.write(");
    let mut console = false;
    for line in ir.lines() {
        if line == "declare i64 @write(i32, ptr, i64)" {
            continue;
        }
        if line.starts_with("define ")
            && line
                .split_once('@')
                .and_then(|(_, suffix)| suffix.split_once('('))
                .is_some_and(|(name, _)| exports.contains(name))
        {
            let _ = writeln!(output, "define dllexport {}", &line[7..]);
        } else {
            let _ = writeln!(output, "{line}");
        }
        if line.starts_with("define internal i32 @tz.console.write(") {
            console = true;
        }
        if console && line == "entry:" {
            output.push_str("  %binary_mode = call i32 @_setmode(i32 1, i32 32768)\n");
            console = false;
        }
    }
    if has_console || has_write {
        output.push_str("declare i32 @_setmode(i32, i32)\n");
    }
    if has_write {
        output.push_str("declare i32 @_write(i32, ptr, i32)\ndefine internal i64 @write(i32 %fd, ptr %buffer, i64 %length) {\nentry:\n  %mode = call i32 @_setmode(i32 %fd, i32 32768)\n  %large = icmp ugt i64 %length, 2147483647\n  %chunk = select i1 %large, i64 2147483647, i64 %length\n  %count = trunc i64 %chunk to i32\n  %written = call i32 @_write(i32 %fd, ptr %buffer, i32 %count)\n  %result = sext i32 %written to i64\n  ret i64 %result\n}\n");
    }
    output.push_str(include_str!("runtime/wasm.ll"));
    output
}

/// Lowers the standard IO to the WASI host: its `declare`s lose the `tsuzuri_io` import attributes,
/// so `src/runtime/os-wasi.c` defines the functions instead of the embedder.
pub(crate) fn with_wasi_host(ir: &str) -> String {
    let mut output = ir.to_owned();
    for name in ["read_line", "write"] {
        output = output.replace(
            &format!(" \"wasm-import-module\"=\"tsuzuri_io\" \"wasm-import-name\"=\"{name}\""),
            "",
        );
    }
    output
}

/// Makes the declarations of the Net primitives imports of `tsuzuri_net` (`--wasm-feature net`, E09 Phase 3): only the
/// functions that the program reaches are declared, so only those are imported. The generated JavaScript bindings
/// implement them.
pub(crate) fn with_net_imports(ir: &str) -> String {
    let mut output = String::with_capacity(ir.len() + 256);
    for line in ir.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        match body
            .strip_prefix("declare ")
            .and_then(|declaration| declaration.split_once(" @tsuzuri_net_"))
            .filter(|_| body.ends_with(')'))
        {
            Some((_, rest)) => {
                let name = rest.split('(').next().unwrap_or(rest);
                output.push_str(body);
                output.push_str(&format!(
                    " \"wasm-import-module\"=\"tsuzuri_net\" \"wasm-import-name\"=\"{name}\"{}",
                    &line[body.len()..]
                ));
            }
            None => output.push_str(line),
        }
    }
    output
}

/// Sets the WASM heap limit in the heap runtime's three comparisons. Only whole
/// lines match, so string constants containing the same text stay unchanged.
pub(crate) fn with_wasm_heap_limit(ir: String, limit: u64) -> String {
    if limit == crate::driver::DEFAULT_WASM_MAX_MEMORY {
        return ir;
    }
    let mut output = String::with_capacity(ir.len());
    for line in ir.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let ending = &line[body.len()..];
        let _ = match body {
            "  %fits = icmp ule i64 %size, 16777184"
            | "  %fits = icmp ule i64 %new_size, 16777184" => {
                let (prefix, _) = body
                    .rsplit_once(' ')
                    .expect("heap limit lines end with a constant");
                write!(output, "{prefix} {}{ending}", limit - 32)
            }
            "  %within = icmp ule i64 %end, 16777216" => {
                write!(output, "  %within = icmp ule i64 %end, {limit}{ending}")
            }
            // Above 2 GiB `%begin + %needed` can wrap in i32; `%begin` never exceeds the limit.
            "  %within = icmp ule i32 %end, 16777216" if limit > 1 << 31 => write!(
                output,
                "  %room = sub i32 {limit}, %begin{ending}  %within = icmp ule i32 %needed, %room{ending}"
            ),
            "  %within = icmp ule i32 %end, 16777216" => {
                write!(output, "  %within = icmp ule i32 %end, {limit}{ending}")
            }
            _ => write!(output, "{line}"),
        };
    }
    output
}

/// Bytes kept below every checked frame for unchecked code: the threads runtime
/// C and the appended i128 helpers, whose frames total under 1 KiB.
const STACK_CHECK_MARGIN: u32 = 4096;

/// Checks every function's stack pointer, after its static frame is allocated,
/// against its stack, so an overflow traps before the frame is used instead of
/// wrapping below address 0 into grown memory or running into a worker stack's neighbor.
pub(crate) fn with_stack_checks(ir: String, workers: bool) -> String {
    let mut output = String::with_capacity(ir.len() + ir.len() / 8);
    let mut lines = ir.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        output.push_str(line);
        let header = line.trim_end();
        if !header.starts_with("define ") || !header.ends_with('{') {
            continue;
        }
        if let Some(label) = lines.next_if(|next| {
            let label = next.split(';').next().unwrap_or_default().trim_end();
            !label.starts_with([' ', '\t']) && label.ends_with(':')
        }) {
            output.push_str(label);
        }
        // Inlining the check splits the entry block, so its static allocas move
        // ahead of the call to stay static allocations that SROA can promote.
        let mut allocas = String::new();
        let mut rest = String::new();
        for next in lines.by_ref() {
            let body = next.trim_start();
            if body.split_once(" = ").is_some_and(|(_, value)| {
                value.starts_with("alloca ")
                    && !value.contains(", i32 %")
                    && !value.contains(", i64 %")
            }) {
                allocas.push_str(next);
                continue;
            }
            rest.push_str(next);
            if ["br ", "ret", "switch ", "unreachable", "indirectbr "]
                .iter()
                .any(|terminator| body.starts_with(terminator))
            {
                break;
            }
        }
        output.push_str(&allocas);
        output.push_str("  call void @tz.stack.check()\n");
        output.push_str(&rest);
    }
    // The frame address is the stack pointer after the static frame. Unlike
    // stacksave it has no side effects, and hosts set worker bounds before any
    // call, so LLVM can merge inlined checks and hoist them out of loops.
    output.push_str(
        "declare ptr @llvm.frameaddress.p0(i32 immarg)\n@__stack_low = external global i8\n@__stack_high = external global i8\n",
    );
    let bounds = if workers {
        "@tsuzuri_stack_base = addrspace(1) global i32 0\n@tsuzuri_stack_top = addrspace(1) global i32 0\n\
         ; Zero bounds mean the main stack, so a worker whose host never set its\n\
         ; bounds traps on its first call instead of running unchecked.\n\
         define internal void @tz.stack.check() nounwind {\nentry:\n\
         \x20 %pointer = call ptr @llvm.frameaddress.p0(i32 0)\n\
         \x20 %sp = ptrtoint ptr %pointer to i32\n\
         \x20 %worker_top = load i32, ptr addrspace(1) @tsuzuri_stack_top, !invariant.load !{}\n\
         \x20 %main = icmp eq i32 %worker_top, 0\n\
         \x20 %main_top = ptrtoint ptr @__stack_high to i32\n\
         \x20 %top = select i1 %main, i32 %main_top, i32 %worker_top\n\
         \x20 %worker_base = load i32, ptr addrspace(1) @tsuzuri_stack_base, !invariant.load !{}\n\
         \x20 %main_base = ptrtoint ptr @__stack_low to i32\n\
         \x20 %base = select i1 %main, i32 %main_base, i32 %worker_base\n"
    } else {
        "define internal void @tz.stack.check() nounwind {\nentry:\n\
         \x20 %pointer = call ptr @llvm.frameaddress.p0(i32 0)\n\
         \x20 %sp = ptrtoint ptr %pointer to i32\n\
         \x20 %top = ptrtoint ptr @__stack_high to i32\n\
         \x20 %base = ptrtoint ptr @__stack_low to i32\n"
    };
    output.push_str(bounds);
    let _ = write!(
        output,
        "  %limit = add i32 %base, {STACK_CHECK_MARGIN}\n  %offset = sub i32 %sp, %limit\n  %range = sub i32 %top, %limit\n  %inside = icmp ule i32 %offset, %range\n  br i1 %inside, label %done, label %overflow\noverflow:\n  call void @llvm.trap()\n  unreachable\ndone:\n  ret void\n}}\n"
    );
    output
}

#[path = "llvm_bulk.rs"]
mod bulk;
#[path = "llvm_compare.rs"]
mod compare;
#[path = "llvm_cpu.rs"]
mod cpu;
#[path = "llvm_display.rs"]
mod display;
#[path = "llvm_exception.rs"]
mod exception;
#[path = "llvm_hash.rs"]
mod hash;
#[path = "llvm_abi.rs"]
pub(crate) mod host_abi;
#[path = "llvm_imports.rs"]
mod imports;
#[path = "llvm_io.rs"]
mod io;
#[path = "llvm_math.rs"]
mod math;
#[path = "llvm_parallel.rs"]
mod parallel;
#[path = "llvm_recursive.rs"]
mod recursive;
#[path = "llvm_shared.rs"]
mod shared;
#[path = "llvm_simd.rs"]
mod simd;
#[path = "llvm_task.rs"]
mod task;
pub(crate) use host_abi::uses_host_abi;

pub fn emit(module: &CheckedModule, entry: Entry) -> Result<String, Diagnostic> {
    emit_target(module, entry, false)
}

/// Every test or bench of a runner entry; `None` for the other entries.
fn runner_selection(module: &CheckedModule, entry: Entry) -> Option<Vec<usize>> {
    match entry {
        Entry::TestRunner => Some((0..module.tests.len()).collect()),
        Entry::BenchRunner => Some((0..module.benches.len()).collect()),
        Entry::Library | Entry::Console => None,
    }
}

pub fn emit_target(module: &CheckedModule, entry: Entry, wasm: bool) -> Result<String, Diagnostic> {
    emit_with_options(
        module,
        EmitOptions {
            entry,
            wasm,
            debug_output: false,
            allocator: Allocator::System,
        },
    )
}

pub fn emit_with_options(
    module: &CheckedModule,
    options: EmitOptions,
) -> Result<String, Diagnostic> {
    let tests = runner_selection(module, options.entry);
    emit_program(
        module,
        options.entry,
        options.wasm,
        tests.as_deref(),
        options.debug_output,
        Instrumentation {
            allocator: options.allocator,
            ..Instrumentation::default()
        },
    )
    .map(|(ir, _)| ir)
}

pub fn emit_with_trap_info(
    module: &CheckedModule,
    options: EmitOptions,
    sources: &[TrapSource<'_>],
) -> Result<EmitOutput, Diagnostic> {
    let tests = runner_selection(module, options.entry);
    let (ir, marks) = emit_program(
        module,
        options.entry,
        options.wasm,
        tests.as_deref(),
        options.debug_output,
        Instrumentation {
            traps: true,
            allocator: options.allocator,
            ..Instrumentation::default()
        },
    )?;
    traps::instrument(ir, module, marks.unwrap(), sources, options.wasm, false)
}

pub fn emit_with_debug_info(
    module: &CheckedModule,
    options: EmitOptions,
    sources: &[TrapSource<'_>],
    optimized: bool,
    trap_info: bool,
) -> Result<EmitOutput, Diagnostic> {
    if sources.is_empty() {
        return Err(Diagnostic::new(
            "E2000",
            "debug information requires a source map",
            Span::default(),
        ));
    }
    let tests = runner_selection(module, options.entry);
    let (ir, marks) = emit_program(
        module,
        options.entry,
        options.wasm,
        tests.as_deref(),
        options.debug_output,
        Instrumentation {
            traps: trap_info,
            debug: Some((sources, optimized)),
            allocator: options.allocator,
            ..Instrumentation::default()
        },
    )?;
    if let Some(marks) = marks {
        traps::instrument(ir, module, marks, sources, options.wasm, false)
    } else {
        Ok(EmitOutput {
            ir,
            trap_sites: Vec::new(),
        })
    }
}

pub fn emit_native_build(
    module: &CheckedModule,
    options: EmitOptions,
    sources: &[TrapSource<'_>],
    debug: Option<bool>,
    trap_info: bool,
) -> Result<EmitOutput, Diagnostic> {
    emit_native_build_for(
        module,
        options,
        sources,
        debug,
        trap_info,
        cpu::host_levels(),
    )
}

/// `emit_native_build` for a CPU whose runtime detects `levels`, bit `L` for level `L` of
/// `syntax::CPU_TARGETS`: `@cpu` functions get versions for these levels (F08 Phase 3).
pub fn emit_native_build_for(
    module: &CheckedModule,
    options: EmitOptions,
    sources: &[TrapSource<'_>],
    debug: Option<bool>,
    trap_info: bool,
    levels: u8,
) -> Result<EmitOutput, Diagnostic> {
    if options.wasm {
        return Err(Diagnostic::new(
            "E2000",
            "CPU dispatch is native-only",
            Span::default(),
        ));
    }
    let trusted_array = sources.iter().any(|source| {
        source.path == "std/Array.tz" && source.text == include_str!("../std/Array.tz")
    });
    let (ir, marks) = emit_program(
        module,
        options.entry,
        false,
        None,
        options.debug_output,
        Instrumentation {
            traps: trap_info,
            debug: debug.map(|optimized| (sources, optimized)),
            cpu_dispatch: trusted_array,
            allocator: options.allocator,
            multiversion: true,
            ..Instrumentation::default()
        },
    )?;
    let mut output = if let Some(marks) = marks {
        traps::instrument(ir, module, marks, sources, false, false)?
    } else {
        EmitOutput {
            ir,
            trap_sites: Vec::new(),
        }
    };
    output.ir = cpu::multiversion(output.ir, module, levels)?;
    Ok(output)
}

/// Emits a native build whose exports also come as `tsuzuri_try_<name>`: a trap inside one returns
/// to the host with a status instead of ending the process (E14 Phase 2).
pub fn emit_trap_return(
    module: &CheckedModule,
    options: EmitOptions,
    sources: &[TrapSource<'_>],
    debug: Option<bool>,
    cpu_dispatch: bool,
) -> Result<EmitOutput, Diagnostic> {
    if options.wasm {
        return Err(Diagnostic::new(
            "E2000",
            "--trap-mode return is native-only",
            Span::default(),
        ));
    }
    let trusted_array = cpu_dispatch
        && sources.iter().any(|source| {
            source.path == "std/Array.tz" && source.text == include_str!("../std/Array.tz")
        });
    let (ir, marks) = emit_program(
        module,
        options.entry,
        false,
        None,
        options.debug_output,
        Instrumentation {
            traps: true,
            debug: debug.map(|optimized| (sources, optimized)),
            cpu_dispatch: trusted_array,
            trap_return: true,
            allocator: options.allocator,
            multiversion: cpu_dispatch,
            ..Instrumentation::default()
        },
    )?;
    let mut output = traps::instrument(ir, module, marks.unwrap(), sources, false, true)?;
    output.ir = cpu::multiversion(output.ir, module, cpu::host_levels())?;
    let wrappers = host_abi::try_wrappers(&output.ir, module);
    output.ir.push_str(&wrappers);
    Ok(output)
}

/// The CPU kernels of `src/runtime/cpu.c` that std functions call in native builds (F08), as
/// (symbol, LLVM result type), in the order of their declarations.
pub const CPU_KERNELS: [(&str, &str); 20] = [
    ("tsuzuri_cpu_sum_i8", "i8"),
    ("tsuzuri_cpu_sum_i16", "i16"),
    ("tsuzuri_cpu_sum_i32", "i32"),
    ("tsuzuri_cpu_sum_i64", "i64"),
    ("tsuzuri_cpu_min_i8", "i64"),
    ("tsuzuri_cpu_min_i16", "i64"),
    ("tsuzuri_cpu_min_i32", "i64"),
    ("tsuzuri_cpu_min_i64", "i64"),
    ("tsuzuri_cpu_min_i8u", "i64"),
    ("tsuzuri_cpu_min_i16u", "i64"),
    ("tsuzuri_cpu_min_i32u", "i64"),
    ("tsuzuri_cpu_min_i64u", "i64"),
    ("tsuzuri_cpu_max_i8", "i64"),
    ("tsuzuri_cpu_max_i16", "i64"),
    ("tsuzuri_cpu_max_i32", "i64"),
    ("tsuzuri_cpu_max_i64", "i64"),
    ("tsuzuri_cpu_max_i8u", "i64"),
    ("tsuzuri_cpu_max_i16u", "i64"),
    ("tsuzuri_cpu_max_i32u", "i64"),
    ("tsuzuri_cpu_max_i64u", "i64"),
];

/// The kernel that replaces the body of a specialized std function (F08): `Array.sum` and the
/// `min_index`/`max_index` helpers of `Array.min`/`Array.max` over 8- to 64-bit integers. A sum
/// is the same bits for both signs, so it shares the signed kernel.
fn cpu_kernel(function: &CheckedFunction) -> Option<&'static str> {
    if function.origin.module != ModuleOrigin::Std || function.module != "Array" {
        return None;
    }
    let [Type::Reference(array, false)] = function.signature.parameters.as_slice() else {
        return None;
    };
    let Type::Array(element) = array.as_ref() else {
        return None;
    };
    let Type::Integer(bits @ (8 | 16 | 32 | 64), signed) = **element else {
        return None;
    };
    let (operation, result) = match function.name.split(".$mono.").next()? {
        "sum" => ("sum", Type::Integer(bits, signed)),
        "min_index" => ("min", Type::I64),
        "max_index" => ("max", Type::I64),
        _ => return None,
    };
    if function.signature.result != result {
        return None;
    }
    let unsigned = if operation != "sum" && !signed {
        "u"
    } else {
        ""
    };
    let symbol = format!("tsuzuri_cpu_{operation}_i{bits}{unsigned}");
    CPU_KERNELS
        .iter()
        .find(|(name, _)| *name == symbol)
        .map(|(name, _)| *name)
}

/// `heap-native.ll` over the tracked allocator of the trap runtime.
fn heap_native_tracked() -> &'static str {
    static HEAP: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HEAP.get_or_init(|| {
        include_str!("runtime/heap-native.ll")
            .replace("@malloc", "@tsuzuri_tracked_malloc")
            .replace("@realloc", "@tsuzuri_tracked_realloc")
            .replace("@free", "@tsuzuri_tracked_free")
    })
}

/// A heap runtime renamed to the base under `heap-counting.ll` (F13 Phase 2). The trap
/// sites of the renamed functions stay allocation failures (`traps::runtime_kind`).
fn counted_base(heap: &str) -> String {
    heap.replace("@tz.alloc(", "@tz.alloc.base(")
        .replace("@tz.free(", "@tz.free.base(")
        .replace("@tz.realloc(", "@tz.realloc.base(")
}

/// `heap-host.ll` for a WASM module, which imports the three functions from `tsuzuri_heap`.
fn heap_host_wasm() -> String {
    let mut heap = include_str!("runtime/heap-host.ll").to_owned();
    for (declaration, name) in [
        ("declare ptr @tsuzuri_host_alloc(i64, i64)", "alloc"),
        ("declare void @tsuzuri_host_free(ptr, i64, i64)", "free"),
        (
            "declare ptr @tsuzuri_host_realloc(ptr, i64, i64, i64)",
            "realloc",
        ),
    ] {
        heap = heap.replace(
            declaration,
            &format!(
                "{declaration} \"wasm-import-module\"=\"tsuzuri_heap\" \"wasm-import-name\"=\"{name}\""
            ),
        );
    }
    heap
}

/// Emits a WASM build with shared-memory threads, 64-bit memory, or stack checks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_wasm_build(
    module: &CheckedModule,
    options: EmitOptions,
    sources: &[TrapSource<'_>],
    debug: Option<bool>,
    trap_info: bool,
    threads: bool,
    memory64: bool,
    stack_checks: bool,
) -> Result<EmitOutput, Diagnostic> {
    let (ir, marks) = emit_program(
        module,
        options.entry,
        true,
        None,
        options.debug_output,
        Instrumentation {
            traps: trap_info,
            debug: debug.map(|optimized| (sources, optimized)),
            cpu_dispatch: false,
            wasm_threads: threads,
            memory64,
            trap_return: false,
            allocator: options.allocator,
            multiversion: false,
            coverage: None,
            seed: None,
        },
    )?;
    // Before trap instrumentation, so an overflow reports the site of the checked function.
    let ir = if stack_checks {
        with_stack_checks(ir, threads)
    } else {
        ir
    };
    if let Some(marks) = marks {
        traps::instrument(ir, module, marks, sources, true, false)
    } else {
        Ok(EmitOutput {
            ir,
            trap_sites: Vec::new(),
        })
    }
}

pub fn emit_test_runner(
    module: &CheckedModule,
    selected: &[usize],
    wasm: bool,
) -> Result<String, Diagnostic> {
    emit_test_runner_for(module, selected, wasm, false, None)
}

/// The test runner of the `selected` tests. `debug` holds the source map and whether the code
/// is optimized when the runner carries debug information for a debugger (G16 Phase 2).
pub(crate) fn emit_test_runner_for(
    module: &CheckedModule,
    selected: &[usize],
    wasm: bool,
    memory64: bool,
    debug: Option<(&[TrapSource<'_>], bool)>,
) -> Result<String, Diagnostic> {
    emit_test_runner_with(
        module,
        selected,
        TestRunnerOptions {
            wasm,
            memory64,
            debug,
            ..TestRunnerOptions::default()
        },
    )
}

/// The native test runner of `emit_test_runner`, counting the regions of `plan` in
/// `@tsuzuri_coverage_counters` (`tsuzuri test --coverage`, G18 Phase 2).
pub fn emit_test_runner_covered(
    module: &CheckedModule,
    selected: &[usize],
    plan: &crate::coverage::CoveragePlan,
) -> Result<String, Diagnostic> {
    emit_test_runner_with(
        module,
        selected,
        TestRunnerOptions {
            coverage: Some(plan),
            ..TestRunnerOptions::default()
        },
    )
}

/// The seed of property tests (`Gen.for_all`) without `tsuzuri test --seed` (G18 Phase 3).
pub const DEFAULT_PROPERTY_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// How `emit_test_runner_with` builds a test runner.
#[derive(Clone, Copy, Debug, Default)]
pub struct TestRunnerOptions<'a> {
    pub wasm: bool,
    pub memory64: bool,
    /// Count coverage regions (native only).
    pub coverage: Option<&'a crate::coverage::CoveragePlan>,
    /// The property-test seed instead of `DEFAULT_PROPERTY_SEED`.
    pub seed: Option<u64>,
    /// WASM only: `Debug.print` writes through the `tsuzuri_debug.write` import, which the Node
    /// test runner provides.
    pub debug_output: bool,
    /// The source map and whether the code is optimized when the runner carries debug
    /// information for a debugger (G16 Phase 2).
    pub debug: Option<(&'a [TrapSource<'a>], bool)>,
}

pub fn emit_test_runner_with(
    module: &CheckedModule,
    selected: &[usize],
    options: TestRunnerOptions<'_>,
) -> Result<String, Diagnostic> {
    if selected.iter().any(|index| *index >= module.tests.len()) {
        return Err(Diagnostic::new(
            "E2000",
            "invalid test index",
            Span::default(),
        ));
    }
    emit_program(
        module,
        Entry::TestRunner,
        options.wasm,
        Some(selected),
        options.debug_output && options.wasm,
        Instrumentation {
            memory64: options.memory64,
            debug: options.debug,
            coverage: options.coverage.filter(|_| !options.wasm),
            seed: options.seed,
            ..Instrumentation::default()
        },
    )
    .map(|(ir, _)| ir)
}

/// The native runner of the selected benches for `tsuzuri bench` (G18). `@tsuzuri_bench_sample`
/// calls bench `index` with an iteration count and returns its nanoseconds, or -1 for a bad index.
pub fn emit_bench_runner(module: &CheckedModule, selected: &[usize]) -> Result<String, Diagnostic> {
    if selected.iter().any(|index| *index >= module.benches.len()) {
        return Err(Diagnostic::new(
            "E2000",
            "invalid bench index",
            Span::default(),
        ));
    }
    emit_program(
        module,
        Entry::BenchRunner,
        false,
        Some(selected),
        false,
        Instrumentation::default(),
    )
    .map(|(ir, _)| ir)
}

#[derive(Default)]
struct Instrumentation<'a> {
    traps: bool,
    debug: Option<(&'a [TrapSource<'a>], bool)>,
    cpu_dispatch: bool,
    wasm_threads: bool,
    memory64: bool,
    /// Native `--trap-mode return`: a tracked heap, so a trap can free everything a call allocated.
    trap_return: bool,
    allocator: Allocator,
    /// A native build that compiles `@cpu` functions for further CPU levels (F08 Phase 3).
    multiversion: bool,
    /// The regions that `tsuzuri test --coverage` counts (G18 Phase 2).
    coverage: Option<&'a crate::coverage::CoveragePlan>,
    /// The property-test seed of `tsuzuri test --seed` (G18 Phase 3).
    seed: Option<u64>,
}

fn emit_program(
    module: &CheckedModule,
    entry: Entry,
    wasm: bool,
    tests: Option<&[usize]>,
    debug_output: bool,
    instrumentation: Instrumentation<'_>,
) -> Result<(String, Option<traps::Marks>), Diagnostic> {
    validate_lowering(module)?;
    if entry == Entry::Console {
        validate_main(module)?;
    }
    let allocator = instrumentation.allocator;
    if allocator == Allocator::Host && instrumentation.wasm_threads {
        return Err(Diagnostic::new(
            "E2000",
            "the host allocator cannot be combined with WASM threads",
            Span::default(),
        ));
    }
    if allocator != Allocator::System && instrumentation.trap_return {
        return Err(Diagnostic::new(
            "E2000",
            "--trap-mode return keeps its own tracked allocator",
            Span::default(),
        ));
    }
    let mut output = String::from(
        "; Tsuzuri - deterministic LLVM IR\nsource_filename = \"tsuzuri\"\n%tz.string = type { ptr, i64 }\n%tz.utf8string = type { ptr, i64 }\n%tz.array = type { ptr, i64 }\n%tz.list = type { ptr, i64 }\n%tz.vec = type { ptr, i64, i64 }\n%tz.closure = type { ptr, ptr, ptr, ptr }\n%tz.abi.buffer = type { ptr, i64 }\n",
    );
    let types = module.types();
    // A14: only a program that writes `dyn` defines the dyn value type.
    if module.uses_dyn {
        output.push_str("%tz.dyn = type { ptr, ptr }\n");
    }
    output.push_str(&host_abi::type_definitions(module));
    let roots = tests.map(|selected| {
        selected
            .iter()
            .map(|index| {
                if entry == Entry::BenchRunner {
                    module.benches[*index].function
                } else {
                    module.tests[*index].function
                }
            })
            .collect::<Vec<_>>()
    });
    let (roots, reachable) = program_reach(module, roots.as_deref(), entry == Entry::BenchRunner);
    let emitted: Vec<bool> = (0..module.functions.len())
        .map(|id| reachable.contains(&id))
        .collect();
    let named = named_types(module, &emitted);
    let generic = |ty: &&Type| matches!(ty, Type::Record(_, arguments) | Type::Union(_, arguments) if !arguments.is_empty());
    let record_types = (0..module.records.len())
        .filter(|id| module.records[*id].parameters.is_empty())
        .map(|id| Type::Record(id, Box::default()))
        .filter(|ty| {
            let Type::Record(id, _) = ty else {
                unreachable!()
            };
            module.records[*id].origin == ModuleOrigin::User || named.contains(ty)
        })
        .chain(
            named
                .iter()
                .filter(generic)
                .filter(|ty| matches!(ty, Type::Record(..)))
                .cloned(),
        );
    let mut record_definitions = String::new();
    for ty in record_types {
        let Type::Record(id, arguments) = &ty else {
            unreachable!("only record types are collected")
        };
        let _ = writeln!(
            record_definitions,
            "{} = type {{ {} }}",
            llvm_type(&ty, module),
            types
                .record_fields(*id, arguments)
                .iter()
                .map(|ty| llvm_type(ty, module))
                .chain(drop_flag(&ty, module).map(|_| "i8".to_owned()))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let union_types = (0..module.unions.len())
        .filter(|id| module.unions[*id].parameters.is_empty())
        .map(|id| Type::Union(id, Box::default()))
        .filter(|ty| {
            let Type::Union(id, _) = ty else {
                unreachable!()
            };
            module.unions[*id].origin == ModuleOrigin::User || named.contains(ty)
        })
        .chain(
            named
                .iter()
                .filter(generic)
                .filter(|ty| matches!(ty, Type::Union(..)))
                .cloned(),
        );
    let mut union_definitions = String::new();
    for ty in union_types {
        let Type::Union(id, arguments) = &ty else {
            unreachable!("only union types are collected")
        };
        let shape = union_layout(*id, arguments, module);
        let is_enum = matches!(shape, UnionLayout::Enum);
        let flag = if drop_flag(&ty, module).is_some() {
            ", i8"
        } else {
            ""
        };
        let layout = match shape {
            UnionLayout::Enum => "i32".into(),
            UnionLayout::Common(payload) => {
                format!("{{ i32, {}{flag} }}", llvm_type(&payload, module))
            }
            UnionLayout::General(count) => format!("{{ i32, [{count} x i128]{flag} }}"),
        };
        if module.types().recursive(&ty) {
            let fields = match union_layout(*id, arguments, module) {
                UnionLayout::Enum => "i8".into(),
                UnionLayout::Common(payload) => llvm_type(&payload, module),
                UnionLayout::General(count) => format!("[{count} x i128]"),
            };
            let _ = writeln!(
                union_definitions,
                "{} = type {{ ptr, ptr, ptr, i32, {fields} }}",
                recursive::node_type(&ty, module)
            );
        } else {
            let destination = if is_enum {
                &mut output
            } else {
                &mut union_definitions
            };
            let _ = writeln!(destination, "{} = type {layout}", llvm_type(&ty, module));
        }
    }
    output.push_str(&record_definitions);
    output.push_str(&union_definitions);
    output.push_str("\ndeclare void @llvm.trap()\n\n");
    let mut builtins = Builtins::new();
    let mut intrinsics = BTreeSet::new();
    let mut globals = Globals {
        wasm,
        async_reactor: module
            .functions
            .iter()
            .enumerate()
            .any(|(id, function)| emitted[id] && async_reactor_function(function)),
        memory64: instrumentation.memory64,
        traps: instrumentation.traps.then(traps::Marks::default),
        cpu_dispatch: instrumentation.cpu_dispatch && !wasm,
        multiversion: instrumentation.multiversion && !wasm,
        coverage: instrumentation.coverage.cloned(),
        seed: instrumentation.seed,
        ..Globals::default()
    };
    if let Some((sources, optimized)) = instrumentation.debug {
        globals.debug = Some(debug::DebugContext::new(
            sources,
            optimized,
            wasm && !instrumentation.memory64,
            &mut globals.next_metadata,
            &mut globals.definitions,
        ));
    }
    let mut specializations = Specializations::new(module);
    for (id, function) in module.functions.iter().enumerate() {
        // A callback extern has no function body: its calls pass the host the callback wrappers directly.
        if !emitted[id]
            || matches!(&function.body.kind, TypedExprKind::HostCall(import, _) if import.callbacks)
        {
            continue;
        }
        let start = output.len();
        let emitter = FunctionEmitter::new(
            module,
            function,
            id,
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        );
        output.push_str(&emitter.emit());
        output.push_str(&closure_wrappers(
            module,
            function,
            id,
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        ));
        if let Some(symbol) = host_entry(function) {
            output.push_str(&named_wrapper(function, module, symbol));
        }
        if function.exported {
            let wrapper = if host_abi::extended(function) {
                host_abi::wrapper(
                    module,
                    function,
                    id,
                    host_abi::WrapperKind::Export,
                    &mut builtins,
                    &mut intrinsics,
                    &mut globals,
                    &mut specializations,
                )
            } else {
                export_wrapper(function, module)
            };
            output.push_str(&debug::wrapper(
                wrapper,
                module,
                function,
                &format!("@tz_{}", function.name),
                &mut globals,
            ));
        }
        if let Some(marks) = &mut globals.traps {
            marks.source(&output[start..], function);
        }
    }
    output.push_str(&emit_dyn_tables(
        module,
        &emitted,
        &mut builtins,
        &mut intrinsics,
        &mut globals,
        &mut specializations,
    ));
    let mut next = 0;
    while let Some(key) = specializations.requests.get(next).cloned() {
        let start = output.len();
        let emitter = FunctionEmitter::new(
            module,
            &module.functions[key.function],
            key.function,
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        )
        .specialized(next, &key);
        output.push_str(&emitter.emit());
        if let Some(marks) = &mut globals.traps {
            marks.source(&output[start..], &module.functions[key.function]);
        }
        next += 1;
    }
    // The functions passed to the host as callbacks get C-ABI wrappers, in id order.
    for id in std::mem::take(&mut globals.callbacks) {
        let function = &module.functions[id];
        let wrapper = host_abi::wrapper(
            module,
            function,
            id,
            host_abi::WrapperKind::Callback,
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        );
        output.push_str(&debug::wrapper(
            wrapper,
            module,
            function,
            &format!("@tz.callback.{}", function.qualified_name()),
            &mut globals,
        ));
    }
    for (instance, ty) in &builtins {
        output.push_str(&emit_builtin(
            instance,
            ty,
            module,
            &mut intrinsics,
            wasm,
            debug_output,
            &mut globals,
        ));
    }
    let io_wrapper = if matches!(entry, Entry::Library | Entry::Console) && io_entry(module) {
        Some(io::entry(FunctionEmitter::new(
            module,
            &module.functions[module.entry.unwrap()],
            module.entry.unwrap(),
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        )))
    } else {
        None
    };
    output.push_str(&recursive::emit_helpers(
        module,
        &mut builtins,
        &mut intrinsics,
        &mut globals,
        &mut specializations,
    ));
    // Only a program that reads `Env.args` receives argc and argv; other entries keep `@main()`.
    let uses_args = intrinsics.contains("declare void @tsuzuri_os_set_args(i32, ptr)");
    if entry != Entry::BenchRunner && intrinsics.contains("declare i64 @tsuzuri_bench_now()") {
        return Err(clock_error(module, &roots));
    }
    for intrinsic in intrinsics {
        let _ = writeln!(output, "{intrinsic}");
    }
    if let Some(io_wrapper) = io_wrapper {
        output.push_str(&debug::wrapper(
            io_wrapper,
            module,
            &module.functions[module.entry.unwrap()],
            "@tsuzuri_main",
            &mut globals,
        ));
    }
    if entry == Entry::Library && main_entry(module) {
        output.push_str(&debug::wrapper(
            library_def_main(module, wasm),
            module,
            &module.functions[module.entry.unwrap()],
            "@tsuzuri_main",
            &mut globals,
        ));
    }
    if entry == Entry::Console {
        output.push_str(&debug::wrapper(
            console_main(module, uses_args),
            module,
            &module.functions[module.entry.unwrap()],
            "@main",
            &mut globals,
        ));
    }
    if let Some(selected) = tests.filter(|_| entry == Entry::BenchRunner) {
        let _ = writeln!(
            output,
            "define i32 @tsuzuri_bench_count() {{\nentry:\n  ret i32 {}\n}}",
            selected.len()
        );
        output.push_str("define i64 @tsuzuri_bench_sample(i32 %index, i64 %iterations) {\nentry:\n  switch i32 %index, label %bad [\n");
        for index in 0..selected.len() {
            let _ = writeln!(output, "    i32 {index}, label %bench{index}");
        }
        output.push_str("  ]\nbad:\n  ret i64 -1\n");
        for (index, selected) in selected.iter().enumerate() {
            let function = &module.functions[module.benches[*selected].function];
            let _ = writeln!(
                output,
                "bench{index}:\n  %result{index} = call i64 @tz.fn.{}(i64 %iterations)\n  ret i64 %result{index}",
                function.qualified_name()
            );
        }
        output.push_str("}\n");
    } else if let Some(selected) = tests {
        let _ = writeln!(
            output,
            "define i32 @tsuzuri_test_count() {{\nentry:\n  ret i32 {}\n}}",
            selected.len()
        );
        let mut runner = String::from(
            "define i32 @tsuzuri_test_run(i32 %index) {\nentry:\n  switch i32 %index, label %bad [\n",
        );
        for index in 0..selected.len() {
            let _ = writeln!(runner, "    i32 {index}, label %test{index}");
        }
        runner.push_str("  ]\nbad:\n  ret i32 2\n");
        for (index, selected) in selected.iter().enumerate() {
            let function = &module.functions[module.tests[*selected].function];
            let _ = writeln!(
                runner,
                "test{index}:\n  %result{index} = call i8 @tz.fn.{}()\n  ret i32 0",
                function.qualified_name()
            );
        }
        runner.push_str("}\n");
        // With debug information the dispatch is an artificial subprogram whose calls have
        // locations, so a test that optimization inlines into it keeps its line table (G16).
        output.push_str(&match selected.first() {
            Some(first) => debug::wrapper(
                runner,
                module,
                &module.functions[module.tests[*first].function],
                "@tsuzuri_test_run",
                &mut globals,
            ),
            None => runner,
        });
    }
    if let Some(plan) = instrumentation.coverage {
        let _ = writeln!(
            output,
            "@tsuzuri_coverage_counters = global [{0} x i64] zeroinitializer\n@tsuzuri_coverage_count = constant i64 {0}",
            plan.len()
        );
    }
    for global in globals.definitions {
        output.push_str(&global);
        output.push('\n');
    }
    if output.contains("@tz.rec.") {
        output.push_str(include_str!("runtime/recursive.ll"));
    }
    // A counting build also lets a WASM host allocate the buffer that `tsuzuri_alloc_stats` fills.
    if uses_host_abi(module)
        || (!wasm && uses_reactor(&output))
        || output.contains("@tsuzuri_io_")
        || output.contains("@tsuzuri_os_")
        || output.contains("@tsuzuri_net_")
        || output.contains("@tsuzuri_arguments(")
        || instrumentation.allocator == Allocator::Counting
    {
        if wasm || cfg!(windows) || instrumentation.trap_return {
            output.push_str(host_abi::allocator());
        } else {
            output.push_str(&host_abi::native_allocator());
        }
    }
    // The driver links `cpu.c` when a line declares a `tsuzuri_cpu_` function (F08), and the
    // runtime text above may end without a newline.
    for (symbol, result) in CPU_KERNELS {
        if output.contains(&format!("@{symbol}(")) {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            let _ = writeln!(output, "declare {result} @{symbol}(ptr, i64)");
        }
    }
    if output.contains("@tsuzuri_task_parallel(")
        || output.contains("@tsuzuri_task_parallel_results(")
    {
        output.push_str(if wasm && !instrumentation.wasm_threads {
            include_str!("runtime/task-wasm.ll")
        } else {
            "declare void @tsuzuri_task_parallel(ptr, ptr, i64)\ndeclare i64 @tsuzuri_task_parallel_results(ptr, ptr, i64)\n"
        });
    }
    if output.contains("@tz.debug.write") {
        output.push_str(include_str!("runtime/debug.ll"));
    }
    if output.contains("@tz.display.") {
        output.push_str(include_str!("runtime/display.ll"));
    }
    // Interpolation padding calls string functions, so append it before the string runtime below.
    if output.contains("@tz.format.") {
        output.push_str(include_str!("runtime/format.ll"));
    }
    if output.contains("@tz_soft_") {
        output = output.replace("declare void @llvm.trap()\n", "");
        output.push_str(include_str!("runtime/numeric.ll"));
    }
    if output.contains("@tz_math_") {
        output.push_str(include_str!("runtime/math.ll"));
        let mut declarations = BTreeSet::new();
        output = output
            .lines()
            .filter(|line| {
                !line.starts_with("declare ")
                    || declarations.insert(
                        line.split_once('@')
                            .unwrap()
                            .1
                            .split('(')
                            .next()
                            .unwrap()
                            .to_owned(),
                    )
            })
            .collect::<Vec<_>>()
            .join("\n");
        output.push('\n');
    }
    if output.contains("@tz.closure.") {
        output.push_str(include_str!("runtime/closure.ll"));
    }
    if output.contains("@tz.dyn.drop(") {
        output.push_str(DYN_DROP);
    }
    if output.contains("@tz.dyn.clone(") {
        output.push_str(DYN_CLONE);
    }
    if output.contains("@tz.character.") {
        output.push_str(include_str!("runtime/character.ll"));
    }
    if output.contains("@tz.unicode.") {
        output.push_str(include_str!("runtime/unicode.ll"));
    }
    if instrumentation.wasm_threads {
        let threads = include_str!("runtime/heap-wasm-threads.ll");
        if allocator == Allocator::Counting {
            output.push_str(&counted_base(threads));
        } else {
            output.push_str(threads);
        }
    }
    if output.contains("@tz.string.")
        || output.contains("@tz.utf8string.")
        || output.contains("@tz.free")
        || output.contains("@tz.alloc")
        || output.contains("@tz.realloc")
    {
        output.push_str(include_str!("runtime/string.ll"));
        output.push_str(include_str!("runtime/utf8string.ll"));
        let heap: std::borrow::Cow<'static, str> = if instrumentation.wasm_threads {
            include_str!("runtime/heap-wasm.ll")
                .replace("@tz.alloc(", "@tz.heap.alloc.unlocked(")
                .replace("@tz.free(", "@tz.heap.free.unlocked(")
                .replace("@tz.realloc(", "@tz.heap.realloc.unlocked(")
                .into()
        } else if instrumentation.memory64 {
            include_str!("runtime/heap-wasm64.ll").into()
        } else if wasm {
            include_str!("runtime/heap-wasm.ll").into()
        } else if instrumentation.trap_return {
            heap_native_tracked().into()
        } else {
            include_str!("runtime/heap-native.ll").into()
        };
        match allocator {
            Allocator::System => output.push_str(&heap),
            Allocator::Host if wasm => output.push_str(&heap_host_wasm()),
            Allocator::Host => output.push_str(include_str!("runtime/heap-host.ll")),
            Allocator::Counting => {
                // The threads wrappers above already call the unlocked heap; they are the base.
                if !instrumentation.wasm_threads {
                    output.push_str(&counted_base(&heap));
                } else {
                    output.push_str(&heap);
                }
                output.push_str(include_str!("runtime/heap-counting.ll"));
            }
        }
    }
    #[cfg(debug_assertions)]
    check_copy_inventory(module, &globals.emitted_copies);
    Ok((simd::with_vector_alignment(output), globals.traps))
}

/// Every implicit copy that the code makes is in `copies::sites`, which W1006 and PM07 rely on.
#[cfg(debug_assertions)]
fn check_copy_inventory(
    module: &CheckedModule,
    emitted: &BTreeSet<(usize, Option<usize>, usize, usize)>,
) {
    if emitted.is_empty() {
        return;
    }
    let listed: BTreeSet<_> = crate::copies::sites(module)
        .into_iter()
        .map(|site| {
            (
                site.function,
                site.span.source,
                site.span.start,
                site.span.end,
            )
        })
        .collect();
    let missing: Vec<_> = emitted
        .difference(&listed)
        .map(|(function, source, start, end)| {
            format!(
                "implicit clone at {source:?}:{start}..{end} in {} is missing from copies::sites",
                module.functions[*function].qualified_name()
            )
        })
        .collect();
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

fn validate_lowering(module: &CheckedModule) -> Result<(), Diagnostic> {
    let check = |ty: &Type, span| {
        if ty.contains_error() || ty.contains_constructor() {
            Err(Diagnostic::new(
                "E1015",
                "internal compiler error: erroneous or unsaturated typed IR cannot be lowered",
                span,
            ))
        } else {
            Ok(())
        }
    };
    for record in &module.records {
        for (_, ty) in &record.fields {
            check(ty, record.span)?;
        }
    }
    for union in &module.unions {
        for ty in union.cases.iter().filter_map(|(_, ty)| ty.as_ref()) {
            check(ty, union.span)?;
        }
    }
    for function in &module.functions {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
        {
            check(ty, function.span)?;
        }
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            check(&expression.ty, expression.span)?;
            if matches!(expression.kind, TypedExprKind::Error) {
                check(&Type::Error, expression.span)?;
            }
            pending.extend(expression.children());
        }
    }
    Ok(())
}

pub fn header(module: &CheckedModule) -> String {
    header_full(module, false, Allocator::System)
}

/// Whether a function of the program calls itself again through direct calls, so its stack can overflow.
pub fn has_recursion(ir: &str) -> bool {
    host_abi::has_recursion(ir)
}

/// The C header; with `trap_return` also the `tsuzuri_try_<name>` prototypes of `--trap-mode return`.
pub fn header_with(module: &CheckedModule, trap_return: bool) -> String {
    header_full(module, trap_return, Allocator::System)
}

/// The C header of a build with `allocator`: `--allocator host` adds the prototypes of the
/// functions the host defines, and `--allocator counting` the statistics function (F13).
pub fn header_with_allocator(module: &CheckedModule, allocator: Allocator) -> String {
    header_full(module, false, allocator)
}

fn header_full(module: &CheckedModule, trap_return: bool, allocator: Allocator) -> String {
    let mut output = String::from(
        "/* Generated by Tsuzuri. bool uses int32_t. Narrow integers use normalized 32-bit ABI values. */\n\
         #pragma once\n\
         #include <stdint.h>\n\n\
         #ifdef __cplusplus\n\
         extern \"C\" {\n\
         #endif\n\n",
    );
    if trap_return {
        output.push_str(
            "/* The trap of a tsuzuri_try_<name> call: `site` is an id of <output>.trap.json, `kind` a trap kind. */\n\
             #ifndef TSUZURI_TRAP_INFO_DEFINED\n\
             #define TSUZURI_TRAP_INFO_DEFINED\n\
             typedef struct {\n    uint32_t site;\n    uint32_t kind;\n} tsuzuri_trap_info;\n\
             #endif\n\n",
        );
    }
    output.push_str(&host_abi::handle_typedefs(module));
    output.push_str(&host_abi::header_types(module));
    if io_entry(module) || main_entry(module) {
        output.push_str("int32_t tsuzuri_main(void);\n");
    }
    output.push_str(&imports::header(module));
    for (symbol, _) in host_entries(module) {
        output.push_str(match symbol {
            "tsuzuri_async_poll" => "int64_t tsuzuri_async_poll(int64_t now);\n",
            _ => "void tsuzuri_async_complete(int64_t operation, int64_t value);\n",
        });
    }
    if reaches_reactor(module) {
        output.push_str("int32_t tsuzuri_async_post(int64_t operation, int64_t value);\n");
    }
    for function in &module.functions {
        if !function.exported {
            continue;
        }
        let parameters = host_abi::c_parameters(function, module);
        let _ = writeln!(
            output,
            "{} tz_{}({parameters});",
            if crate::abi::out_result(&function.signature.result) {
                "void".into()
            } else {
                c_type(&function.signature.result)
            },
            function.name
        );
        if trap_return {
            let _ = writeln!(output, "{}", host_abi::try_prototype(function, module));
        }
    }
    match allocator {
        Allocator::System => {}
        Allocator::Host => output.push_str(host_abi::HOST_ALLOCATOR_PROTOTYPES),
        Allocator::Counting => output.push_str(host_abi::ALLOCATION_STATS_PROTOTYPE),
    }
    output.push_str("\n#ifdef __cplusplus\n}\n#endif\n");
    output
}

struct Globals {
    definitions: Vec<String>,
    next_metadata: usize,
    wasm: bool,
    async_reactor: bool,
    memory64: bool,
    traps: Option<traps::Marks>,
    debug: Option<debug::DebugContext>,
    cpu_dispatch: bool,
    multiversion: bool,
    parallel_kernels: usize,
    recursive_types: BTreeSet<Type>,
    /// The values of the deferred shared blocks that the program drops, with whether they
    /// count atomically (C10).
    shared_types: BTreeSet<(Type, bool)>,
    /// User functions the program hands to the host as C function pointers.
    callbacks: BTreeSet<usize>,
    /// The regions that `tsuzuri test --coverage` counts (G18 Phase 2).
    coverage: Option<crate::coverage::CoveragePlan>,
    /// The property-test seed of `tsuzuri test --seed` (G18 Phase 3).
    seed: Option<u64>,
    /// The implicit copies emitted in function bodies, as (function, source, start, end) (A15).
    #[cfg(debug_assertions)]
    emitted_copies: BTreeSet<(usize, Option<usize>, usize, usize)>,
}

impl Default for Globals {
    fn default() -> Self {
        static FIRST_METADATA: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let next_metadata = *FIRST_METADATA.get_or_init(|| {
            include_str!("runtime/numeric.ll")
                .lines()
                .chain(include_str!("runtime/math.ll").lines())
                .filter_map(|line| {
                    line.strip_prefix('!')?
                        .split_once('=')?
                        .0
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .max()
                .map_or(0, |id| id + 1)
        });
        Self {
            definitions: Vec::new(),
            next_metadata,
            wasm: false,
            async_reactor: false,
            memory64: false,
            traps: None,
            debug: None,
            cpu_dispatch: false,
            multiversion: false,
            parallel_kernels: 0,
            recursive_types: BTreeSet::new(),
            shared_types: BTreeSet::new(),
            callbacks: BTreeSet::new(),
            coverage: None,
            seed: None,
            #[cfg(debug_assertions)]
            emitted_copies: BTreeSet::new(),
        }
    }
}

fn environment_type(function: &CheckedFunction, count: usize, module: &CheckedModule) -> String {
    format!(
        "{{ {} }}",
        function.signature.parameters[..count]
            .iter()
            .map(|ty| llvm_type(ty, module))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn immediate_capture(function: &CheckedFunction, count: usize, globals: &Globals) -> bool {
    if function.is_task || count != 1 {
        return false;
    }
    let width = match function.signature.parameters[0] {
        Type::Integer(bits, _) | Type::Binary(bits @ (32 | 64)) => u32::from(bits),
        Type::Bool => 1,
        _ => return false,
    };
    width
        <= if globals.memory64 {
            64
        } else if globals.wasm {
            32
        } else {
            usize::BITS
        }
}

fn closure_wrappers(
    module: &CheckedModule,
    function: &CheckedFunction,
    id: usize,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let mut output = String::new();
    let name = function.qualified_name();
    let arity = function.parameters.len();
    for count in function.capture_count..arity.max(function.capture_count + 1) {
        let environment = environment_type(function, count, module);
        let immediate = immediate_capture(function, count, globals);
        if count != 0 && !immediate {
            if !function.is_task
                && !function.owned_captures
                && function.signature.parameters[..count]
                    .iter()
                    .all(|ty| ty.can_capture(&module.types()))
            {
                let mut clone = FunctionEmitter::new(
                    module,
                    function,
                    id,
                    builtins,
                    intrinsics,
                    globals,
                    specializations,
                );
                let allocation = clone.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({environment}, ptr null, i32 1) to i64))"));
                for (index, ty) in function.signature.parameters[..count].iter().enumerate() {
                    let pointer = clone.value(format!(
                        "getelementptr inbounds {environment}, ptr %env, i32 0, i32 {index}"
                    ));
                    let value = clone.value(format!("load {}, ptr {pointer}", clone.ty(ty)));
                    let value = clone.clone_value(ty, &value);
                    let target = clone.value(format!(
                        "getelementptr inbounds {environment}, ptr {allocation}, i32 0, i32 {index}"
                    ));
                    clone.instruction(format!("store {} {value}, ptr {target}", clone.ty(ty)));
                }
                clone.instruction(format!("ret ptr {allocation}"));
                output.push_str(
                    &clone.auxiliary(&format!("ptr @tz.env.clone.{name}.{count}(ptr %env)")),
                );
            }

            let mut drop = FunctionEmitter::new(
                module,
                function,
                id,
                builtins,
                intrinsics,
                globals,
                specializations,
            );
            for (index, ty) in function.signature.parameters[..count].iter().enumerate() {
                if ty.needs_drop(&module.types()) {
                    let pointer = drop.value(format!(
                        "getelementptr inbounds {environment}, ptr %env, i32 0, i32 {index}"
                    ));
                    let value = drop.value(format!("load {}, ptr {pointer}", drop.ty(ty)));
                    drop.drop_value(ty, &value);
                }
            }
            drop.instruction("call void @tz.free(ptr %env)");
            drop.instruction("ret void");
            output
                .push_str(&drop.auxiliary(&format!("void @tz.env.drop.{name}.{count}(ptr %env)")));
        }
        if function.owned_captures && count != 0 && !immediate {
            // The body borrows the captures; only a consuming call drops the environment.
            let mut apply = FunctionEmitter::new(
                module,
                function,
                id,
                builtins,
                intrinsics,
                globals,
                specializations,
            );
            let mut values = Vec::new();
            for index in 0..count {
                values.push(apply.closure_capture_value(function, count, index, "%env"));
            }
            values.push("%argument".into());
            let arguments = values
                .iter()
                .zip(&function.signature.parameters)
                .map(|(value, ty)| format!("{} {value}", apply.ty(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let result = apply.ty(&function.signature.result);
            let value = apply.value(format!("call {result} @tz.fn.{name}({arguments})"));
            let release = apply.label();
            let done = apply.label();
            apply.branch("%borrow", &done, &release);
            apply.begin(&release);
            apply.instruction(format!("call void @tz.env.drop.{name}.{count}(ptr %env)"));
            apply.jump(&done);
            apply.begin(&done);
            apply.instruction(format!("ret {result} {value}"));
            let argument = apply.ty(&function.signature.parameters[count]);
            output.push_str(&apply.auxiliary(&format!(
                "{result} @tz.apply.{name}.{count}({argument} %argument, ptr %env, i1 %borrow)"
            )));
            continue;
        }
        let mut apply = FunctionEmitter::new(
            module,
            function,
            id,
            builtins,
            intrinsics,
            globals,
            specializations,
        );
        let mut env = "%env".to_owned();
        if !immediate
            && !function.is_task
            && count != 0
            && function.signature.parameters[..count]
                .iter()
                .all(|ty| ty.can_capture(&module.types()))
        {
            let target = ClosureTarget {
                function: id,
                bound: count,
            };
            let worker = if count + 1 >= arity && apply.specializations.can_borrow(target, module) {
                if function.signature.parameters[..count]
                    .iter()
                    .all(|ty| !ty.needs_drop(&module.types()))
                {
                    Some(format!("@tz.fn.{name}"))
                } else {
                    apply
                        .specializations
                        .request(Specialization {
                            function: id,
                            callbacks: Vec::new(),
                            borrowed: count,
                        })
                        .map(|worker| format!("@tz.specialized.{worker}"))
                }
            } else {
                None
            };
            let entry = apply.block.clone();
            let borrowed = apply.label();
            let owned = apply.label();
            apply.branch("%borrow", &borrowed, &owned);
            apply.begin(&borrowed);
            if let Some(worker) = worker {
                let mut values = Vec::new();
                for (index, ty) in function.signature.parameters[..count].iter().enumerate() {
                    let pointer = apply.value(format!(
                        "getelementptr inbounds {environment}, ptr %env, i32 0, i32 {index}"
                    ));
                    let value = apply.value(format!("load {}, ptr {pointer}", apply.ty(ty)));
                    values.push(format!("{} {value}", apply.ty(ty)));
                }
                if count < arity {
                    values.push(format!(
                        "{} %argument",
                        apply.ty(&function.signature.parameters[count])
                    ));
                }
                let result = apply.ty(&function.signature.result);
                let value = apply.value(format!("call {result} {worker}({})", values.join(", ")));
                apply.instruction(format!("ret {result} {value}"));
                apply.begin(&owned);
            } else {
                let cloned =
                    apply.value(format!("call ptr @tz.env.clone.{name}.{count}(ptr %env)"));
                let end = apply.block.clone();
                apply.jump(&owned);
                apply.begin(&owned);
                env = apply.value(format!("phi ptr [ %env, %{entry} ], [ {cloned}, %{end} ]"));
            }
        }
        let mut values = Vec::new();
        for index in 0..count {
            values.push(apply.closure_capture_value(function, count, index, &env));
        }
        if count < arity {
            values.push("%argument".into());
        }
        if count != 0 && !immediate {
            apply.instruction(format!("call void @tz.free(ptr {env})"));
        }
        let (value, result) = if values.len() == arity {
            let arguments = values
                .iter()
                .zip(&function.signature.parameters)
                .map(|(value, ty)| format!("{} {value}", apply.ty(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let value = apply.value(format!(
                "call {} @tz.fn.{name}({arguments})",
                apply.ty(&function.signature.result)
            ));
            (value, function.signature.result.clone())
        } else {
            let value = apply.make_closure(id, &values);
            let result = Type::function(
                function.signature.parameters[values.len()..].to_vec(),
                function.signature.result.clone(),
            );
            (value, result)
        };
        apply.instruction(format!("ret {} {value}", apply.ty(&result)));
        let argument = if count < arity {
            format!(
                "{} %argument, ",
                apply.ty(&function.signature.parameters[count])
            )
        } else {
            String::new()
        };
        let borrow = if function.is_task { "" } else { ", i1 %borrow" };
        output.push_str(&apply.auxiliary(&format!(
            "{} @tz.apply.{name}.{count}({argument}ptr %env{borrow})",
            llvm_type(&result, module)
        )));
    }
    output
}

fn c_type(ty: &Type) -> String {
    match ty {
        Type::Integer(bits, signed) => {
            format!("{}int{}_t", if *signed { "" } else { "u" }, (*bits).max(32))
        }
        Type::Binary(32) => "float".into(),
        Type::Binary(64) => "double".into(),
        Type::Bool => "int32_t".into(),
        Type::Unit => "void".into(),
        Type::Handle(name) => host_abi::handle_c_name(name),
        Type::Reference(inner, false) if matches!(inner.as_ref(), Type::Handle(_)) => c_type(inner),
        _ => unreachable!("the type checker enforces scalar exports"),
    }
}

/// The length of a concrete fixed-length array type `[T; N]` (A16).
pub(crate) fn fixed_length(ty: &Type) -> usize {
    let length = ty
        .fixed_length()
        .expect("specialization substitutes every length parameter");
    usize::try_from(length).expect("lengths are at most 1024")
}

fn llvm_type(ty: &Type, module: &CheckedModule) -> String {
    match ty {
        Type::Simd(vector) => format!(
            "<{} x {}>",
            vector.lanes(),
            llvm_type(&vector.element(), module)
        ),
        Type::Integer(bits, _) | Type::Decimal(bits) | Type::Binary(bits @ (16 | 128)) => {
            format!("i{bits}")
        }
        Type::Binary(32) => "float".into(),
        Type::Binary(64) => "double".into(),
        Type::Binary(_) => unreachable!("binary widths checked"),
        Type::Bool => "i1".into(),
        Type::Char => "i16".into(),
        Type::Utf8Char => "i32".into(),
        Type::Unit => "i8".into(),
        Type::String => "%tz.string".into(),
        Type::Utf8String => "%tz.utf8string".into(),
        Type::Record(id, arguments) if arguments.is_empty() => {
            format!("%tz.record.{}", module.records[*id].name)
        }
        Type::Record(..) => format!("%\"tz.record.{}\"", canonical_type(ty, module)),
        Type::Union(..) if module.types().recursive(ty) => "ptr".into(),
        Type::Union(..) => format!("%\"tz.union.{}\"", canonical_type(ty, module)),
        Type::Array(_) => "%tz.array".into(),
        Type::List(_) => "%tz.list".into(),
        Type::Vec(_) => "%tz.vec".into(),
        Type::Tuple(elements) => format!(
            "{{ {} }}",
            elements
                .iter()
                .map(|ty| llvm_type(ty, module))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Function(..) | Type::Task(_) => "%tz.closure".into(),
        Type::Dyn(_) => "%tz.dyn".into(),
        Type::Reference(..) if ty.slice_element().is_some() => "%tz.array".into(),
        Type::Reference(..) | Type::Handle(_) | Type::Shared(..) => "ptr".into(),
        Type::FixedArray(element, _) => {
            format!("[{} x {}]", fixed_length(ty), llvm_type(element, module))
        }
        Type::ArrayView(_) => unreachable!("an exclusive slice target is not a value"),
        Type::Length(_) => unreachable!("a length is not a value"),
        Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => {
            unreachable!("erroneous and polymorphic types cannot reach LLVM")
        }
    }
}

/// The spelling of a `dyn` type in LLVM names (A14): `dyn[Shapes.Shape]`, with `+copy`, `+send`,
/// and `+ref` for its markers and region, which no class name contains.
fn dyn_name(dyn_type: &crate::check::DynType) -> String {
    let mut name = format!("dyn[{}", dyn_type.classes.join(","));
    for (written, marker) in [
        (dyn_type.copy, "+copy"),
        (dyn_type.send, "+send"),
        (dyn_type.borrowed, "+ref"),
    ] {
        if written {
            name.push_str(marker);
        }
    }
    name.push(']');
    name
}

/// The field of a vtable's method slots (A14 D4). A vtable holds the drop slot, the clone slot
/// (null without `Copy`), the stored value's size and alignment, the method slots, and, when the
/// program upcasts values of its key, the vtables of the upcast targets (Phase 2).
const DYN_SLOTS: usize = 4;

/// The fields of a vtable up to its method slots, which address any slot.
fn dyn_vtable_prefix(slots: usize) -> String {
    format!("{{ ptr, ptr, i64, i64, [{slots} x ptr] }}")
}

/// The whole type of the vtables of a vtable key.
fn dyn_vtable_type(layout: &crate::check::DynLayout) -> String {
    if layout.upcasts.is_empty() {
        dyn_vtable_prefix(layout.slots)
    } else {
        format!(
            "{{ ptr, ptr, i64, i64, [{} x ptr], [{} x ptr] }}",
            layout.slots,
            layout.upcasts.len()
        )
    }
}

/// The vtable of a vtable key for a stored type, as `@"tz.vtable.Shapes.Shape[Main.Square]"`.
fn dyn_vtable_symbol(key: &crate::check::DynType, ty: &Type, module: &CheckedModule) -> String {
    let marker = if key.copy { "+copy" } else { "" };
    format!(
        "@\"tz.vtable.{}{marker}[{}]\"",
        key.classes.join(","),
        canonical_type(ty, module)
    )
}

/// Drops a dyn value through its vtable; moved-out storage is zero and drops nothing.
const DYN_DROP: &str = "define internal void @tz.dyn.drop(%tz.dyn %value) nounwind {\nentry:\n  %vtable = extractvalue %tz.dyn %value, 1\n  %empty = icmp eq ptr %vtable, null\n  br i1 %empty, label %exit, label %drop\ndrop:\n  %data = extractvalue %tz.dyn %value, 0\n  %destroy = load ptr, ptr %vtable\n  call void %destroy(ptr %data)\n  br label %exit\nexit:\n  ret void\n}\n\n";

/// Clones a `Copy` dyn value through its vtable's clone slot into new data.
const DYN_CLONE: &str = "define internal %tz.dyn @tz.dyn.clone(%tz.dyn %value) nounwind {\nentry:\n  %vtable = extractvalue %tz.dyn %value, 1\n  %empty = icmp eq ptr %vtable, null\n  br i1 %empty, label %exit, label %copy\ncopy:\n  %data = extractvalue %tz.dyn %value, 0\n  %slot = getelementptr inbounds { ptr, ptr }, ptr %vtable, i32 0, i32 1\n  %clone = load ptr, ptr %slot\n  %new = call ptr %clone(ptr %data)\n  %result = insertvalue %tz.dyn %value, ptr %new, 0\n  ret %tz.dyn %result\nexit:\n  ret %tz.dyn %value\n}\n\n";

/// The vtables whose slot functions the program emits, after their slot adapters and the drop
/// and clone functions of their stored types (A14 D4), in `CheckedModule::vtables` order.
fn emit_dyn_tables(
    module: &CheckedModule,
    emitted: &[bool],
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let mut output = String::new();
    let mut adapters = BTreeSet::new();
    let mut drops = BTreeSet::new();
    let mut clones = BTreeSet::new();
    for ((key, ty), functions) in &module.vtables {
        if !functions.iter().all(|id| emitted[*id]) {
            continue;
        }
        let name = canonical_type(ty, module);
        // Any function of the module gives the helpers' emitters their context.
        let context = functions[0];
        macro_rules! emitter {
            () => {
                FunctionEmitter::new(
                    module,
                    &module.functions[context],
                    context,
                    builtins,
                    intrinsics,
                    globals,
                    specializations,
                )
            };
        }
        let (size, align) = storage_layout(ty, module);
        let size = size.max(1);
        if drops.insert(ty.clone()) {
            let mut drop = emitter!();
            if ty.needs_drop(&module.types()) {
                let value = drop.value(format!("load {}, ptr %data", drop.ty(ty)));
                drop.drop_value(ty, &value);
            }
            drop.instruction("call void @tz.free(ptr %data)");
            drop.instruction("ret void");
            output.push_str(&drop.auxiliary(&format!("void @\"tz.dyn.drop[{name}]\"(ptr %data)")));
        }
        if key.copy && clones.insert(ty.clone()) {
            let mut clone = emitter!();
            let copy = clone.value(format!("call ptr @tz.alloc(i64 {size})"));
            let value = clone.value(format!("load {}, ptr %data", clone.ty(ty)));
            let value = if ty.needs_drop(&module.types()) {
                clone.clone_value(ty, &value)
            } else {
                value
            };
            clone.instruction(format!("store {} {value}, ptr {copy}", clone.ty(ty)));
            clone.instruction(format!("ret ptr {copy}"));
            output.push_str(&clone.auxiliary(&format!("ptr @\"tz.dyn.clone[{name}]\"(ptr %data)")));
        }
        for &id in functions {
            if adapters.insert(id) {
                output.push_str(&dyn_adapter(
                    module,
                    id,
                    builtins,
                    intrinsics,
                    globals,
                    specializations,
                ));
            }
        }
        let layout = &module.dyn_layouts[key];
        let slots = functions
            .iter()
            .map(|id| {
                format!(
                    "ptr @\"tz.dyn.slot.{}\"",
                    module.functions[*id].qualified_name()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let clone = if key.copy {
            format!("ptr @\"tz.dyn.clone[{name}]\"")
        } else {
            "ptr null".into()
        };
        let mut fields = format!(
            "ptr @\"tz.dyn.drop[{name}]\", {clone}, i64 {size}, i64 {align}, [{} x ptr] [{slots}]",
            functions.len()
        );
        if !layout.upcasts.is_empty() {
            let targets = layout
                .upcasts
                .iter()
                .map(|target| format!("ptr {}", dyn_vtable_symbol(target, ty, module)))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = write!(fields, ", [{} x ptr] [{targets}]", layout.upcasts.len());
        }
        let _ = writeln!(
            output,
            "{} = internal unnamed_addr constant {} {{ {fields} }}",
            dyn_vtable_symbol(key, ty, module),
            dyn_vtable_type(layout)
        );
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

/// The slot adapter of a method function (A14 D4): `R (ptr data, A1, ..., An)` turns the data of
/// a dyn value into the method's receiver, the reference itself, the slice it holds, or the
/// value moved out of its freed allocation, and calls the method directly. A method defined
/// with fewer parameters returns a function that takes the rest.
fn dyn_adapter(
    module: &CheckedModule,
    id: usize,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let function = &module.functions[id];
    let mut adapter = FunctionEmitter::new(
        module,
        function,
        id,
        builtins,
        intrinsics,
        globals,
        specializations,
    );
    let receiver_type = &function.signature.parameters[0];
    let receiver = match receiver_type {
        Type::Reference(..) if receiver_type.slice_element().is_some() => {
            adapter.value("load %tz.array, ptr %data")
        }
        Type::Reference(..) => "%data".to_owned(),
        ty => {
            let value = adapter.value(format!("load {}, ptr %data", adapter.ty(ty)));
            adapter.instruction("call void @tz.free(ptr %data)");
            value
        }
    };
    let mut parameters = vec!["ptr %data".to_owned()];
    let mut values = vec![(receiver_type.clone(), receiver)];
    for (index, ty) in function.signature.parameters.iter().enumerate().skip(1) {
        parameters.push(format!("{} %a{index}", adapter.ty(ty)));
        values.push((ty.clone(), format!("%a{index}")));
    }
    let count = function.parameters.len();
    let direct = values[..count]
        .iter()
        .map(|(ty, value)| format!("{} {value}", adapter.ty(ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut ty = function.signature.as_type().after_arguments(count);
    let mut value = adapter.value(format!(
        "call {} @tz.fn.{}({direct})",
        adapter.ty(&ty),
        function.qualified_name()
    ));
    for (argument_type, argument) in &values[count..] {
        (value, ty) = adapter.apply_value(&value, &ty, Some((argument_type, argument)), false);
    }
    let result = adapter.ty(&ty);
    adapter.instruction(format!("ret {result} {value}"));
    adapter.auxiliary(&format!(
        "{result} @\"tz.dyn.slot.{}\"({})",
        function.qualified_name(),
        parameters.join(", ")
    ))
}

/// The injective Tsuzuri spelling of a concrete type used in LLVM type names.
fn canonical_type(ty: &Type, module: &CheckedModule) -> String {
    let list = |types: &[Type]| {
        types
            .iter()
            .map(|ty| canonical_type(ty, module))
            .collect::<Vec<_>>()
            .join(",")
    };
    match ty {
        Type::Record(id, arguments) if arguments.is_empty() => module.records[*id].name.clone(),
        Type::Record(id, arguments) => {
            format!("{}[{}]", module.records[*id].name, list(arguments))
        }
        Type::Union(id, arguments) if arguments.is_empty() => module.unions[*id].name.clone(),
        Type::Union(id, arguments) => {
            format!("{}[{}]", module.unions[*id].name, list(arguments))
        }
        Type::Array(element) => format!("array[{}]", canonical_type(element, module)),
        Type::ArrayView(element) => format!("view[{}]", canonical_type(element, module)),
        Type::FixedArray(element, length) => format!(
            "fixed[{},{}]",
            canonical_type(length, module),
            canonical_type(element, module)
        ),
        // Digits never start a type's spelling, so a length argument stays injective.
        Type::Length(length) => length.to_string(),
        Type::List(element) => format!("list[{}]", canonical_type(element, module)),
        Type::Vec(element) => format!("vec[{}]", canonical_type(element, module)),
        Type::Tuple(elements) => format!("tuple[{}]", list(elements)),
        Type::Function(parameters, result) => format!(
            "fn[{}->{}]",
            list(parameters),
            canonical_type(result, module)
        ),
        Type::Reference(value, false) => format!("ref[{}]", canonical_type(value, module)),
        Type::Reference(value, true) => format!("refmut[{}]", canonical_type(value, module)),
        Type::Task(result) => format!("task[{}]", canonical_type(result, module)),
        Type::Shared(value, kind) => format!(
            "{}[{}]",
            match kind {
                SharedKind::Rc => "rc",
                SharedKind::RcWeak => "rc.weak",
                SharedKind::Arc => "arc",
                SharedKind::ArcWeak => "arc.weak",
            },
            canonical_type(value, module)
        ),
        Type::Handle(name) => format!("extern.{name}"),
        Type::Dyn(dyn_type) => dyn_name(dyn_type),
        Type::Simd(_)
        | Type::Integer(..)
        | Type::Binary(_)
        | Type::Decimal(_)
        | Type::Bool
        | Type::Unit
        | Type::Char
        | Type::Utf8Char
        | Type::String
        | Type::Utf8String => ty.display(&module.types()),
        Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => {
            unreachable!("erroneous and polymorphic types cannot reach LLVM")
        }
    }
}

/// How a union instance stores its tag and payload.
enum UnionLayout {
    /// Every case is nullary, so the value is its `i32` tag.
    Enum,
    /// Every payload has the LLVM type of this one: `{ i32, T }`.
    Common(Type),
    /// Payloads of different LLVM types share `{ i32, [K x i128] }` storage
    /// that holds the largest payload.
    General(usize),
}

/// Size and alignment in bytes of a type's LLVM representation on 64-bit
/// targets with 16-byte `i128`, which bound those of wasm32 and older layouts.
fn storage_layout(ty: &Type, module: &CheckedModule) -> (usize, usize) {
    let aggregate = |fields: &mut dyn Iterator<Item = (usize, usize)>| {
        let (size, align) = fields.fold((0usize, 1usize), |(size, align), (field, field_align)| {
            (
                size.next_multiple_of(field_align) + field,
                align.max(field_align),
            )
        });
        (size.next_multiple_of(align), align)
    };
    match ty {
        Type::Simd(vector) => (vector.bytes(), vector.bytes()),
        Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => {
            let bytes = usize::from(*bits).div_ceil(8);
            (bytes, bytes)
        }
        Type::Bool | Type::Unit => (1, 1),
        Type::Char => (2, 2),
        Type::Utf8Char => (4, 4),
        Type::String | Type::Utf8String | Type::Array(_) | Type::List(_) => (16, 8),
        Type::Function(..) | Type::Task(_) => (32, 8),
        Type::Vec(_) => (24, 8),
        Type::Dyn(_) => (16, 8),
        Type::Reference(..) if ty.slice_element().is_some() => (16, 8),
        Type::Reference(..) | Type::Handle(_) | Type::Shared(..) => (8, 8),
        Type::ArrayView(_) => unreachable!("an exclusive slice target is not a value"),
        Type::Length(_) => unreachable!("a length is not a value"),
        // The stride of `[N x T]` is the element's size, already a multiple of its alignment.
        Type::FixedArray(element, _) => {
            let (size, align) = storage_layout(element, module);
            (size * fixed_length(ty), align)
        }
        Type::Tuple(elements) => {
            aggregate(&mut elements.iter().map(|ty| storage_layout(ty, module)))
        }
        Type::Record(id, arguments) => aggregate(
            &mut module
                .types()
                .record_fields(*id, arguments)
                .iter()
                .map(|ty| storage_layout(ty, module))
                .chain(drop_flag(ty, module).map(|_| (1, 1))),
        ),
        Type::Union(..) if module.types().recursive(ty) => (8, 8),
        Type::Union(id, arguments) => match union_layout(*id, arguments, module) {
            UnionLayout::Enum => (4, 4),
            UnionLayout::Common(payload) => aggregate(
                &mut [(4, 4), storage_layout(&payload, module)]
                    .into_iter()
                    .chain(drop_flag(ty, module).map(|_| (1, 1))),
            ),
            UnionLayout::General(count) if drop_flag(ty, module).is_some() => {
                aggregate(&mut [(4, 4), (16 * count, 16), (1, 1)].into_iter())
            }
            UnionLayout::General(count) => (16 + 16 * count, 16),
        },
        Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => {
            unreachable!("erroneous and polymorphic types cannot reach LLVM")
        }
    }
}

fn union_layout(id: usize, arguments: &[Type], module: &CheckedModule) -> UnionLayout {
    let payloads: Vec<_> = module
        .types()
        .union_payloads(id, arguments)
        .into_iter()
        .flatten()
        .collect();
    let Some(first) = payloads.first() else {
        // A Drop union keeps its live flag after the tag, so it is never a bare tag.
        return if module.unions[id].user_drop {
            UnionLayout::General(0)
        } else {
            UnionLayout::Enum
        };
    };
    let llvm = llvm_type(first, module);
    if payloads.iter().all(|ty| llvm_type(ty, module) == llvm) {
        return UnionLayout::Common(first.clone());
    }
    let bytes = payloads
        .iter()
        .map(|ty| storage_layout(ty, module).0)
        .max()
        .unwrap_or(0);
    UnionLayout::General(bytes.div_ceil(16))
}

/// The index of the live flag (`i8 1`) that follows the fields of a Drop record, or the tag and
/// payload of a non-recursive Drop union. Moved-out storage is zero, which clears the flag, so
/// the drop glue skips the user drop there (B07). Recursive Drop unions use a null node instead.
fn drop_flag(ty: &Type, module: &CheckedModule) -> Option<usize> {
    match ty {
        Type::Record(id, _) if module.records[*id].user_drop => {
            Some(module.records[*id].fields.len())
        }
        Type::Union(id, _) if module.unions[*id].user_drop && !module.types().recursive(ty) => {
            Some(2)
        }
        _ => None,
    }
}

/// The functions that the program needs: user functions, exports, and the
/// entry point, and every function they refer to (GUIDE D-22). Unused std
/// functions and the helpers generated for them are left out.
pub(crate) fn reachable_functions(
    module: &CheckedModule,
    roots: Option<&[usize]>,
    benches: bool,
) -> BTreeSet<usize> {
    program_reach(module, roots, benches).1
}

/// Pushes the functions that `expression` refers to onto `pending`, and sets `clock` if it reads
/// the clock `Bench.now`.
fn references(
    expression: &TypedExpr,
    module: &CheckedModule,
    pending: &mut Vec<usize>,
    clock: &mut bool,
) {
    direct_references(expression, module, pending, clock);
    for child in expression.children() {
        references(child, module, pending, clock);
    }
}

/// `references` of the expression itself, without its children.
fn direct_references(
    expression: &TypedExpr,
    module: &CheckedModule,
    pending: &mut Vec<usize>,
    clock: &mut bool,
) {
    match &expression.kind {
        TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::Closure(id, _) => {
            pending.push(*id);
        }
        // A14: a stored value's vtable calls its slot functions; an upcast reuses those.
        TypedExprKind::Function(FunctionRef::Builtin(instance))
            if instance.builtin == Builtin::DynOf =>
        {
            if let [ty, Type::Dyn(target)] = instance.types.as_slice()
                && let Some(functions) = module.vtables.get(&(target.vtable_key(), ty.clone()))
            {
                pending.extend(functions);
            }
        }
        TypedExprKind::Function(FunctionRef::Builtin(instance))
            if instance.builtin == Builtin::BenchNow =>
        {
            *clock = true;
        }
        _ => {}
    }
}

/// The functions reachable from `pending`, and those of them that read the clock themselves.
fn close(module: &CheckedModule, mut pending: Vec<usize>) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let mut reachable = BTreeSet::new();
    let mut clocks = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if reachable.insert(id) {
            let mut clock = false;
            references(&module.functions[id].body, module, &mut pending, &mut clock);
            if clock {
                clocks.insert(id);
            }
        }
    }
    (reachable, clocks)
}

/// `reachable_functions` with its roots. Without explicit roots, the roots are the public user
/// functions, the exports, and the entry, but a public function that reaches the clock
/// `Bench.now`, which only the bench runner defines, is a root only if it is exported or the
/// entry: helpers of `bench` declarations do not make other builds fail (G18 D3). Such a program
/// would otherwise be rejected, so the output of every program that builds stays the same.
fn program_reach(
    module: &CheckedModule,
    roots: Option<&[usize]>,
    benches: bool,
) -> (Vec<usize>, BTreeSet<usize>) {
    // Drop glue calls the user drops without a reference in any body.
    let drops: Vec<usize> = module
        .user_drops
        .values()
        .copied()
        .filter(|drop| benches || !module.bench_drops.contains(drop))
        .collect();
    if let Some(roots) = roots {
        let roots: Vec<usize> = roots.iter().chain(&drops).copied().collect();
        return (roots.clone(), close(module, roots).0);
    }
    let candidates: Vec<usize> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(id, function)| {
            (function.origin.module == ModuleOrigin::User
                && function.origin.test.is_none()
                && function.origin.parent.is_none()
                && function.visibility == crate::syntax::Visibility::Public)
                && !matches!(function.body.kind, TypedExprKind::HostCall(..))
                || function.exported
                || module.entry == Some(*id)
        })
        .map(|(id, _)| id)
        .collect();
    let roots: Vec<usize> = candidates.iter().chain(&drops).copied().collect();
    let (reachable, clocks) = close(module, roots.clone());
    if clocks.is_empty() {
        return (roots, reachable);
    }
    let reaching = reaching(module, &reachable, &clocks);
    let roots: Vec<usize> = candidates
        .into_iter()
        .filter(|id| {
            !reaching.contains(id) || module.functions[*id].exported || module.entry == Some(*id)
        })
        .chain(drops)
        .collect();
    (roots.clone(), close(module, roots).0)
}

/// The functions of `reachable` from which one of `targets` is reachable, `targets` included.
fn reaching(
    module: &CheckedModule,
    reachable: &BTreeSet<usize>,
    targets: &BTreeSet<usize>,
) -> BTreeSet<usize> {
    let mut callers: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &id in reachable {
        let mut callees = Vec::new();
        references(&module.functions[id].body, module, &mut callees, &mut false);
        for callee in callees {
            callers.entry(callee).or_default().push(id);
        }
    }
    let mut reaching = targets.clone();
    let mut pending: Vec<usize> = targets.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for &caller in callers.get(&id).into_iter().flatten() {
            if reaching.insert(caller) {
                pending.push(caller);
            }
        }
    }
    reaching
}

/// The `E1018` of an output other than the bench runner's that reaches `Bench.now`. It names the
/// root that reaches the clock and the last function written in user code on the shortest path
/// from the roots (in order) to a body that reads it, at the expression of that function that
/// leads there.
fn clock_error(module: &CheckedModule, roots: &[usize]) -> Diagnostic {
    const RULE: &str = "Bench.now runs only under tsuzuri bench";
    const ADVICE: &str =
        "call it only from bench declarations and from functions that only benchmarks use";
    let mut previous: BTreeMap<usize, Option<usize>> = BTreeMap::new();
    let mut queue = std::collections::VecDeque::new();
    let mut visit = |id: usize, from: Option<usize>, queue: &mut std::collections::VecDeque<_>| {
        if let std::collections::btree_map::Entry::Vacant(slot) = previous.entry(id) {
            slot.insert(from);
            queue.push_back(id);
        }
    };
    for &root in roots {
        visit(root, None, &mut queue);
    }
    let mut found = None;
    while let Some(id) = queue.pop_front() {
        let mut callees = Vec::new();
        let mut clock = false;
        references(&module.functions[id].body, module, &mut callees, &mut clock);
        if clock {
            found = Some(id);
            break;
        }
        for callee in callees {
            visit(callee, Some(id), &mut queue);
        }
    }
    let Some(found) = found else {
        return Diagnostic::new("E1018", format!("{RULE}; {ADVICE}"), Span::default());
    };
    let mut path = vec![found];
    while let Some(Some(id)) = previous.get(path.last().unwrap()) {
        path.push(*id);
    }
    path.reverse();
    let root = path[0];
    let root_name = if let Some(test) = module.tests.iter().find(|test| test.function == root) {
        format!("test {:?}", test.name)
    } else if let Some((ty, _)) = module.user_drops.iter().find(|(_, drop)| **drop == root) {
        format!("the Drop instance of `{}`", ty.display(&module.types()))
    } else if let Some(name) = source_function_name(&module.functions[root]) {
        let export = if module.functions[root].exported {
            "export "
        } else {
            ""
        };
        format!("{export}`{name}`")
    } else if module.entry == Some(root) {
        "the top-level code".to_owned()
    } else {
        format!("`{}`", module.functions[root].qualified_name())
    };
    let named = (1..path.len())
        .rev()
        .find(|&index| source_function_name(&module.functions[path[index]]).is_some())
        .unwrap_or(0);
    let next = path.get(named + 1).copied();
    let function = &module.functions[path[named]];
    let span = clock_use(&function.body, module, next)
        .filter(|span| *span != Span::default())
        .unwrap_or(function.span);
    let message = match source_function_name(function) {
        Some(name) if named > 0 => {
            format!("{RULE}, but {root_name} reaches it through `{name}`; {ADVICE}")
        }
        _ => format!("{RULE}, but {root_name} reads it; {ADVICE}"),
    };
    Diagnostic::new("E1018", message, span)
}

/// `Module.name` of a function written in user code, for diagnostics; `None` for tests, lambdas,
/// and generated functions.
fn source_function_name(function: &CheckedFunction) -> Option<String> {
    let name = function
        .name
        .split_once(".$mono.")
        .map_or(function.name.as_str(), |(name, _)| name);
    (function.origin.module == ModuleOrigin::User
        && function.origin.test.is_none()
        && function.origin.bench.is_none()
        && !function.module.starts_with('$')
        && !name.contains('$'))
    .then(|| format!("{}.{name}", function.module))
}

/// The span of the first expression of `expression`, in tree order, that refers to function
/// `target`, or that reads the clock if `target` is `None`.
fn clock_use(
    expression: &TypedExpr,
    module: &CheckedModule,
    target: Option<usize>,
) -> Option<Span> {
    let mut callees = Vec::new();
    let mut clock = false;
    direct_references(expression, module, &mut callees, &mut clock);
    let refers = match target {
        Some(target) => callees.contains(&target),
        None => clock,
    };
    if refers {
        return Some(expression.span);
    }
    expression
        .children()
        .into_iter()
        .find_map(|child| clock_use(child, module, target))
}

/// Record and union types, generic instances included, reachable from
/// user-origin type declarations and emitted functions, including those
/// nested in other types' fields and payloads, in deterministic order.
fn named_types(module: &CheckedModule, emitted: &[bool]) -> BTreeSet<Type> {
    fn visit(ty: &Type, pending: &mut Vec<Type>) {
        match ty {
            Type::Record(..) | Type::Union(..) => pending.push(ty.clone()),
            // A shared block stores its value, whose named types need definitions (C10).
            Type::Array(ty)
            | Type::List(ty)
            | Type::Vec(ty)
            | Type::Task(ty)
            | Type::FixedArray(ty, _)
            | Type::Shared(ty, _)
            | Type::Reference(ty, _) => visit(ty, pending),
            Type::Tuple(types) => types.iter().for_each(|ty| visit(ty, pending)),
            Type::Function(parameters, result) => {
                parameters.iter().for_each(|ty| visit(ty, pending));
                visit(result, pending);
            }
            _ => {}
        }
    }
    fn walk(expression: &TypedExpr, pending: &mut Vec<Type>) {
        visit(&expression.ty, pending);
        let mut local = |local: &Local| visit(&local.ty, pending);
        match &expression.kind {
            TypedExprKind::Block { bindings, .. } => {
                bindings.iter().for_each(|(binding, _)| local(binding))
            }
            TypedExprKind::ForRange { local: bound, .. } => local(bound),
            TypedExprKind::ForEach {
                owner,
                local: bound,
                ..
            } => {
                local(owner);
                local(bound);
            }
            TypedExprKind::Match {
                local: subject,
                arms,
                ..
            } => {
                local(subject);
                for alternative in arms.iter().flat_map(|arm| &arm.alternatives) {
                    for step in &alternative.steps {
                        if let crate::check::PatternStep::Bind(bound, _) = step {
                            local(bound);
                        }
                    }
                    alternative
                        .bindings
                        .iter()
                        .for_each(|(binding, _)| local(binding));
                }
            }
            TypedExprKind::Lambda {
                parameters,
                captures,
                ..
            } => parameters.iter().chain(captures).for_each(local),
            _ => {}
        }
        for child in expression.children() {
            walk(child, pending);
        }
    }
    let mut pending = Vec::new();
    for (id, record) in module.records.iter().enumerate() {
        if record.parameters.is_empty() && record.origin == ModuleOrigin::User {
            for ty in module.types().record_fields(id, &[]) {
                visit(&ty, &mut pending);
            }
        }
    }
    for (id, union) in module.unions.iter().enumerate() {
        if union.parameters.is_empty() && union.origin == ModuleOrigin::User {
            for ty in module.types().union_payloads(id, &[]).into_iter().flatten() {
                visit(&ty, &mut pending);
            }
        }
    }
    for (function, _) in module
        .functions
        .iter()
        .zip(emitted)
        .filter(|(_, emitted)| **emitted)
    {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
            .chain(function.parameters.iter().map(|parameter| &parameter.ty))
        {
            visit(ty, &mut pending);
        }
        walk(&function.body, &mut pending);
    }
    let mut instances = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        let nested: Vec<_> = match &ty {
            Type::Record(id, arguments) => module.types().record_fields(*id, arguments),
            Type::Union(id, arguments) => module
                .types()
                .union_payloads(*id, arguments)
                .into_iter()
                .flatten()
                .collect(),
            _ => unreachable!("only record and union types are pending"),
        };
        if instances.insert(ty) {
            nested.iter().for_each(|ty| visit(ty, &mut pending));
        }
    }
    instances
}

fn abi_type(ty: &Type) -> String {
    match ty {
        Type::Integer(8 | 16 | 32, _) | Type::Bool => "i32".into(),
        Type::Integer(64, _) => "i64".into(),
        Type::Binary(32) => "float".into(),
        Type::Binary(64) => "double".into(),
        Type::Unit => "void".into(),
        Type::Handle(_) => "ptr".into(),
        Type::Reference(inner, false) if matches!(inner.as_ref(), Type::Handle(_)) => "ptr".into(),
        _ => unreachable!("the type checker enforces scalar exports"),
    }
}

pub fn io_entry(module: &CheckedModule) -> bool {
    let Some(main) = module.entry.map(|id| &module.functions[id]) else {
        return false;
    };
    let Type::Record(id, arguments) = &main.signature.result else {
        return false;
    };
    main.parameters.is_empty()
        && module.records[*id].origin == ModuleOrigin::Std
        && module.records[*id].name == "IO.IO"
        && arguments.len() == 1
        && module.types().record_fields(*id, arguments)
            == [Type::function(vec![Type::Unit], arguments[0].clone())]
}

/// Whether IR waits in the reactor of `Async.block_on` (B08 Phase 3).
pub fn uses_reactor(ir: &str) -> bool {
    ir.contains("declare void @tsuzuri_async_wait(")
}

/// Whether IR calls the socket runtime of the standard Net module, `src/runtime/net.c` (E09).
pub fn uses_net(ir: &str) -> bool {
    ir.contains("@tsuzuri_net_")
}

/// Whether IR starts an async Net operation, whose completion the runtime posts to the reactor with
/// `tsuzuri_async_post` (E09 Phase 2).
pub fn uses_net_async(ir: &str) -> bool {
    ["@tsuzuri_net_watch(", "@tsuzuri_net_connect("]
        .iter()
        .any(|symbol| ir.contains(symbol))
}

/// Whether the program can reach a socket primitive of the standard Net module: the functions that become `@tsuzuri_net_`
/// declarations in its IR, and imports of `tsuzuri_net` under `--wasm-feature net` (E09 Phase 3). It looks where the
/// IR does, so that the bindings, which are made without IR, agree with the `.wasm` on whether the module has them.
pub fn reaches_net(module: &CheckedModule) -> bool {
    fn mentions(expression: &TypedExpr) -> bool {
        matches!(
            &expression.kind,
            TypedExprKind::Function(FunctionRef::Builtin(instance))
                if matches!(
                    instance.builtin,
                    Builtin::NetResolve
                        | Builtin::NetOpen
                        | Builtin::NetAccept
                        | Builtin::NetRead
                        | Builtin::NetWrite
                        | Builtin::NetClose
                        | Builtin::NetClassify
                        | Builtin::NetWatch
                        | Builtin::NetUnwatch
                        | Builtin::NetConnect
                        | Builtin::NetNames
                        | Builtin::NetSend
                )
        ) || expression.children().into_iter().any(mentions)
    }
    reachable_functions(module, None, false)
        .into_iter()
        .any(|id| mentions(&module.functions[id].body))
}

/// Whether the program can reach `Async.block_on`, whose native runtime defines
/// `tsuzuri_async_post`.
pub fn reaches_reactor(module: &CheckedModule) -> bool {
    let reachable = reachable_functions(module, None, false);
    module
        .functions
        .iter()
        .enumerate()
        .any(|(id, function)| reachable.contains(&id) && async_reactor_function(function))
}

fn async_reactor_function(function: &CheckedFunction) -> bool {
    function.origin.module == ModuleOrigin::Std
        && function.module == "Async"
        && function.name.split('.').next() == Some("reactor")
}

/// The entry points of the executor that the host drives, `(symbol, function)`, when the
/// program can reach that executor (B08): `Async.start` and `Async.block_on` refer to them.
pub fn host_entries(module: &CheckedModule) -> Vec<(&'static str, usize)> {
    let reachable = reachable_functions(module, None, false);
    module
        .functions
        .iter()
        .enumerate()
        .filter(|(id, _)| reachable.contains(id))
        .filter_map(|(id, function)| host_entry(function).map(|symbol| (symbol, id)))
        .collect()
}

/// The symbol under which the host calls a std function of the host-driven executor.
fn host_entry(function: &CheckedFunction) -> Option<&'static str> {
    if function.origin.module != ModuleOrigin::Std || function.module != "Async" {
        return None;
    }
    match function.name.as_str() {
        "__poll" => Some("tsuzuri_async_poll"),
        "__complete" => Some("tsuzuri_async_complete"),
        _ => None,
    }
}

/// Whether the entry is a `def main`, which the checker allows only as `unit -> i32` or
/// `Array<string> -> i32`; its result is the process exit code.
pub fn main_entry(module: &CheckedModule) -> bool {
    module
        .entry
        .is_some_and(|id| module.functions[id].name == "main")
}

/// Whether `main` takes the command-line arguments.
fn main_arguments(module: &CheckedModule) -> bool {
    main_entry(module)
        && module.entry.is_some_and(|id| {
            matches!(
                module.functions[id].signature.parameters.as_slice(),
                [Type::Array(_)]
            )
        })
}

/// Whether the entry returns the process exit code: a `def main`, or an `IO<i32>` entry (E08 D9).
pub fn exit_code_entry(module: &CheckedModule) -> bool {
    main_entry(module)
        || io_entry(module)
            && module
                .entry
                .is_some_and(|id| match &module.functions[id].signature.result {
                    Type::Record(_, arguments) => arguments[0] == Type::Integer(32, true),
                    _ => false,
                })
}

fn validate_main(module: &CheckedModule) -> Result<(), Diagnostic> {
    let main = module
        .entry
        .map(|id| &module.functions[id])
        .ok_or_else(|| {
            Diagnostic::new(
                "E2004",
                "an executable requires top-level entry-point code or 'def main' in Main.tz; use '--emit object' for a library",
                Span::default(),
            )
        })?;
    if !main_entry(module)
        && !io_entry(module)
        && !main.signature.result.is_scalar()
        && !matches!(
            main.signature.result,
            Type::Unit | Type::String | Type::Utf8String
        )
    {
        return Err(Diagnostic::new(
            "E2004",
            "top-level entry-point code must end with IO<T>, a number, bool, char, utf8char, unit, string, or utf8string",
            main.span,
        ));
    }
    Ok(())
}

struct LoopTargets {
    exit: String,
    advance: String,
    scope_base: usize,
    temporary_base: usize,
}

/// Where a raised exception goes: the slot that receives its code, the block
/// that handles it, and the scopes and temporaries it leaves.
struct TryTarget {
    handler: String,
    slot: String,
    scope_base: usize,
    temporary_base: usize,
}

struct FunctionEmitter<'a, 'b> {
    module: &'a CheckedModule,
    function: &'a CheckedFunction,
    function_id: usize,
    symbol: String,
    specializations: &'b mut Specializations,
    known_closures: BTreeMap<usize, ClosureTarget>,
    borrowed_locals: BTreeSet<usize>,
    single_use: BTreeSet<usize>,
    ranges: crate::ranges::RangeFacts,
    /// Array reads emitted without a bounds check so far.
    proven_reads: usize,
    borrowed_worker: bool,
    builtins: &'b mut Builtins,
    intrinsics: &'b mut BTreeSet<String>,
    lines: Vec<String>,
    allocas: Vec<String>,
    debug_declarations: Vec<String>,
    debug_scope: Option<usize>,
    debug_switch: bool,
    locals: BTreeMap<usize, String>,
    next_value: usize,
    next_block: usize,
    block: String,
    back_edges: Vec<(String, Vec<String>)>,
    scopes: Vec<Vec<(String, Type)>>,
    loop_targets: Vec<LoopTargets>,
    try_targets: Vec<TryTarget>,
    /// The code slots of the exceptions that enclosing handlers are handling.
    caught: Vec<String>,
    temporaries: Vec<(Type, String, Vec<Frame>)>,
    /// Stack parts that each slot and local (including match aliases) may hold.
    frame_slots: BTreeMap<String, Vec<Frame>>,
    frame_locals: BTreeMap<usize, Vec<Frame>>,
    globals: &'b mut Globals,
    current_span: Span,
    /// The span that debug locations use instead of `current_span` (G16 D7).
    debug_span: Option<Span>,
    trap_kind: Option<TrapKind>,
    drop_pending: Option<String>,
    clone_pending: Option<String>,
    /// Whether this body counts coverage regions: a counted user function of a covered runner.
    counted: bool,
    /// Whether implicit copies count for the debug inventory check: only in bodies that `emit` writes.
    #[cfg(debug_assertions)]
    note_copies: bool,
}

struct BorrowedCall {
    target: ClosureTarget,
    symbol: String,
    captures: Vec<String>,
    cleanup: Vec<(Type, String, Vec<Frame>)>,
}

impl<'a, 'b> FunctionEmitter<'a, 'b> {
    fn new(
        module: &'a CheckedModule,
        function: &'a CheckedFunction,
        function_id: usize,
        builtins: &'b mut Builtins,
        intrinsics: &'b mut BTreeSet<String>,
        globals: &'b mut Globals,
        specializations: &'b mut Specializations,
    ) -> Self {
        let counted = globals.coverage.is_some() && crate::coverage::counted(function);
        Self {
            module,
            function,
            function_id,
            symbol: format!("@tz.fn.{}", function.qualified_name()),
            specializations,
            known_closures: BTreeMap::new(),
            // An owned function's environment outlives each call (B07).
            borrowed_locals: if function.owned_captures {
                function.parameters[..function.capture_count]
                    .iter()
                    .map(|parameter| parameter.id)
                    .collect()
            } else {
                BTreeSet::new()
            },
            single_use: BTreeSet::new(),
            ranges: crate::ranges::RangeFacts::default(),
            proven_reads: 0,
            borrowed_worker: false,
            builtins,
            intrinsics,
            globals,
            current_span: function.body.span,
            debug_span: None,
            trap_kind: None,
            drop_pending: None,
            clone_pending: None,
            lines: Vec::new(),
            allocas: Vec::new(),
            debug_declarations: Vec::new(),
            debug_scope: None,
            debug_switch: false,
            locals: BTreeMap::new(),
            next_value: 0,
            next_block: 0,
            block: "entry".into(),
            back_edges: Vec::new(),
            scopes: vec![Vec::new()],
            loop_targets: Vec::new(),
            try_targets: Vec::new(),
            caught: Vec::new(),
            temporaries: Vec::new(),
            frame_slots: BTreeMap::new(),
            frame_locals: BTreeMap::new(),
            counted,
            #[cfg(debug_assertions)]
            note_copies: false,
        }
    }

    fn specialized(mut self, id: usize, key: &Specialization) -> Self {
        self.symbol = format!("@tz.specialized.{id}");
        self.borrowed_worker = key.borrowed != 0;
        // The caller retains these immutable payloads; owning temporaries still use normal drops.
        for parameter in &self.function.parameters[..key.borrowed] {
            self.borrowed_locals.insert(parameter.id);
        }
        for (index, target) in &key.callbacks {
            let local = self.function.parameters[*index].id;
            self.borrowed_locals.insert(local);
            self.known_closures.insert(local, *target);
        }
        self
    }

    fn auxiliary(self, signature: &str) -> String {
        let mut output = format!("define internal {signature} nounwind {{\nentry:\n");
        for line in self.allocas {
            let _ = writeln!(output, "  {line}");
        }
        for line in self.lines {
            let _ = writeln!(output, "{line}");
        }
        output.push_str("}\n\n");
        output
    }

    fn emit(mut self) -> String {
        // Workers clone their borrowed parameters, which `copies::sites` does not list.
        #[cfg(debug_assertions)]
        {
            self.note_copies = !self.symbol.starts_with("@tz.specialized.");
        }
        self.debug_scope =
            self.globals
                .debug_subprogram(self.module, self.function, &self.symbol, false);
        self.single_use = call_specialization::single_use_locals(&self.function.body);
        self.ranges = crate::ranges::analyze(self.module, self.function);
        self.block = "loop".into();
        if let TypedExprKind::DynDispatch { slot, slots } = self.function.body.kind {
            // A14: the parameters pass on to the slot, which owns them; nothing drops here.
            self.dyn_dispatch(slot, slots);
        } else {
            for (index, parameter) in self.function.parameters.iter().enumerate() {
                self.bind_local(parameter, &format!("%p{index}"));
            }
            self.cover(crate::coverage::RegionKind::Body, self.function.body.span);
            self.emit_body();
        }
        let parameters = self
            .function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| format!("{} %arg{index}", self.ty(&parameter.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let debug = self
            .debug_scope
            .map_or_else(String::new, |scope| format!(" !dbg !{scope}"));
        // `cpu::multiversion` replaces the marked function with versions and a stub (F08 Phase 3).
        let cpu = if self.globals.multiversion && self.function.origin.cpu != 0 {
            format!(
                " {}\"{}:{}\"",
                cpu::MARK,
                self.function.origin.cpu,
                self.function_id
            )
        } else {
            String::new()
        };
        // Unicode table reads inline into every caller, so that LLVM resolves the table switch where
        // the table number is constant and keeps only the tables that a program reads (D09).
        // Resuming a continuation inlines too, so even at -O0 a chain of suspended computations
        // costs no extra frames per level (B08).
        let inline = if self.function.module == "$builtin"
            && (self.function.name.starts_with("Unicode.__table_")
                || self.function.name.starts_with("Async.__resume"))
        {
            " alwaysinline"
        } else {
            ""
        };
        let mut output = format!(
            "define internal {} {}({parameters}) nounwind{inline}{cpu}{debug} {{\nentry:\n",
            self.ty(&self.function.signature.result),
            self.symbol
        );
        for alloca in self.allocas {
            let _ = writeln!(output, "  {alloca}");
        }
        for declaration in self.debug_declarations {
            let _ = writeln!(output, "  {declaration}");
        }
        output.push_str("  br label %loop\nloop:\n");
        for (index, parameter) in self.function.parameters.iter().enumerate() {
            let mut incoming = format!("[ %arg{index}, %entry ]");
            for (block, values) in &self.back_edges {
                let _ = write!(incoming, ", [ {}, %{block} ]", values[index]);
            }
            let _ = writeln!(
                output,
                "  %p{index} = phi {} {incoming}",
                llvm_type(&parameter.ty, self.module)
            );
        }
        for line in self.lines {
            output.push_str(&line);
            output.push('\n');
        }
        output.push_str("}\n\n");
        output
    }

    /// The body of an ordinary function, whose parameters are bound. A native build with CPU
    /// dispatch replaces the body of a std kernel function with a call to its kernel (F08).
    fn emit_body(&mut self) {
        if let Some(symbol) = self
            .globals
            .cpu_dispatch
            .then(|| cpu_kernel(self.function))
            .flatten()
        {
            let result = self.ty(&self.function.signature.result);
            let data = self.value("extractvalue %tz.array %p0, 0");
            let length = self.value("extractvalue %tz.array %p0, 1");
            let value = self.value(format!("call {result} @{symbol}(ptr {data}, i64 {length})"));
            self.instruction(format!("ret {result} {value}"));
        } else {
            self.tail(&self.function.body);
        }
    }

    /// The body of a method of a generated `dyn` instance (A14 D4): reads slot `slot` of the
    /// receiver's vtable and calls it with the receiver's data and the other parameters. The
    /// slot's adapter converts the data to the method's receiver, so the call has the same
    /// type for every stored type, as WASM's indirect calls require.
    fn dyn_dispatch(&mut self, slot: u32, slots: u32) {
        let pair = match &self.function.signature.parameters[0] {
            Type::Reference(..) => self.value("load %tz.dyn, ptr %p0"),
            _ => "%p0".to_owned(),
        };
        let data = self.value(format!("extractvalue %tz.dyn {pair}, 0"));
        let vtable = self.value(format!("extractvalue %tz.dyn {pair}, 1"));
        let pointer = self.value(format!(
            "getelementptr inbounds {}, ptr {vtable}, i32 0, i32 {DYN_SLOTS}, i64 {slot}",
            dyn_vtable_prefix(slots as usize)
        ));
        let method = self.value(format!("load ptr, ptr {pointer}"));
        let mut arguments = vec![format!("ptr {data}")];
        for (index, ty) in self
            .function
            .signature
            .parameters
            .iter()
            .enumerate()
            .skip(1)
        {
            arguments.push(format!("{} %p{index}", self.ty(ty)));
        }
        let result = self.ty(&self.function.signature.result);
        let value = self.value(format!("call {result} {method}({})", arguments.join(", ")));
        self.instruction(format!("ret {result} {value}"));
    }

    fn ty(&self, ty: &Type) -> String {
        llvm_type(ty, self.module)
    }

    fn fresh(&mut self) -> String {
        let value = format!("%v{}", self.next_value);
        self.next_value += 1;
        value
    }

    fn instruction(&mut self, text: impl Into<String>) {
        let mut text = text.into();
        if text.trim_start().starts_with("switch ") && text.trim_end().ends_with('[') {
            self.debug_switch = true;
        } else if text.trim() == "]" {
            self.debug_switch = false;
        }
        if let Some(scope) = self.debug_scope
            && !self.debug_switch
            && let Some(location) = self
                .globals
                .debug_location(scope, self.debug_span.unwrap_or(self.current_span))
        {
            let _ = write!(text, ", !dbg !{location}");
        }
        if text.contains("call ") {
            if let Some(marks) = &mut self.globals.traps {
                let id = self.globals.next_metadata;
                self.globals.next_metadata += 1;
                marks
                    .instructions
                    .insert(id, (self.current_span, self.trap_kind));
                self.globals.definitions.push(format!("!{id} = !{{i32 0}}"));
                let _ = write!(text, ", !tz.site !{id}");
            }
        }
        self.lines.push(format!("  {text}"));
    }

    fn value(&mut self, instruction: impl Into<String>) -> String {
        let value = self.fresh();
        self.instruction(format!("{value} = {}", instruction.into()));
        value
    }

    fn label(&mut self) -> String {
        let block = format!("b{}", self.next_block);
        self.next_block += 1;
        block
    }

    fn begin(&mut self, block: &str) {
        self.lines.push(format!("{block}:"));
        self.block = block.to_owned();
    }

    /// Counts one entry into the coverage region of `kind` at `span`, if the plan has it.
    fn cover(&mut self, kind: crate::coverage::RegionKind, span: Span) {
        if self.counted {
            self.count_region(kind, span);
        }
    }

    /// Counts one entry into the body of user function `id` where this body replaced a call of it
    /// by the call's effect (an identity call, an inline `x + y` helper), whether or not this
    /// body is counted: the callee ran, as far as the program can tell.
    fn cover_call(&mut self, id: usize) {
        let function = &self.module.functions[id];
        if self.globals.coverage.is_some() && crate::coverage::counted(function) {
            self.count_region(crate::coverage::RegionKind::Body, function.body.span);
        }
    }

    fn count_region(&mut self, kind: crate::coverage::RegionKind, span: Span) {
        let Some(plan) = &self.globals.coverage else {
            return;
        };
        let Some(region) = plan.region(kind, span) else {
            return;
        };
        let count = plan.len();
        self.value(format!(
            "atomicrmw add ptr getelementptr inbounds ([{count} x i64], ptr @tsuzuri_coverage_counters, i64 0, i64 {region}), i64 1 monotonic"
        ));
    }

    /// `call_specialization::transparent`, counting each identity function that it looks through
    /// because the generated code uses the argument instead of calling the function.
    fn skip_identity_calls<'e>(&mut self, expression: &'e TypedExpr) -> &'e TypedExpr {
        if self.globals.coverage.is_none() {
            return call_specialization::transparent(expression, self.module);
        }
        let mut skipped = Vec::new();
        let expression =
            call_specialization::transparent_calls(expression, self.module, |id| skipped.push(id));
        for id in skipped {
            self.cover_call(id);
        }
        expression
    }

    fn jump(&mut self, block: &str) {
        self.instruction(format!("br label %{block}"));
    }

    fn branch(&mut self, condition: &str, yes: &str, no: &str) {
        self.instruction(format!("br i1 {condition}, label %{yes}, label %{no}"));
    }

    fn hint_loop(&mut self, body: &TypedExpr, reads: usize) {
        fn uses(expression: &TypedExpr, id: usize) -> bool {
            matches!(expression.kind, TypedExprKind::Local(local) if local == id)
                || expression
                    .children()
                    .into_iter()
                    .any(|child| uses(child, id))
        }
        fn reduction(expression: &TypedExpr, module: &CheckedModule) -> bool {
            if let TypedExprKind::Assign(place, value) = &expression.kind {
                if let (TypedExprKind::Local(id), Some((operator, left, right, _))) = (
                    &place.kind,
                    call_specialization::binary_operation(value, module),
                ) {
                    if matches!(
                        operator,
                        BinaryOp::Add
                            | BinaryOp::Multiply
                            | BinaryOp::BitAnd
                            | BinaryOp::BitOr
                            | BinaryOp::BitXor
                    ) && value.ty.is_integer()
                        && ((matches!(left.kind, TypedExprKind::Local(local) if local == *id)
                            && !uses(right, *id))
                            || (matches!(right.kind, TypedExprKind::Local(local) if local == *id)
                                && !uses(left, *id)))
                    {
                        return true;
                    }
                }
            }
            expression
                .children()
                .into_iter()
                .any(|child| reduction(child, module))
        }
        fn assignments(expression: &TypedExpr) -> usize {
            usize::from(matches!(expression.kind, TypedExprKind::Assign(..)))
                + expression
                    .children()
                    .into_iter()
                    .map(assignments)
                    .sum::<usize>()
        }
        fn small(expression: &TypedExpr, remaining: &mut usize) -> bool {
            if *remaining == 0 {
                return false;
            }
            *remaining -= 1;
            expression
                .children()
                .into_iter()
                .all(|child| small(child, remaining))
        }
        // Without a bounds check, the hint makes LLVM unroll before it vectorizes; let the vectorizer go first.
        if self.proven_reads != reads
            || !small(body, &mut 64)
            || assignments(body) != 1
            || !reduction(body, self.module)
        {
            return;
        }
        let id = self.globals.next_metadata;
        self.globals.next_metadata += 2;
        self.globals.definitions.push(format!(
            "!{id} = distinct !{{!{id}, !{}}}\n!{} = !{{!\"llvm.loop.unroll.enable\"}}",
            id + 1,
            id + 1
        ));
        let branch = self.lines.last_mut().expect("loop back edge");
        debug_assert!(branch.trim_start().starts_with("br "));
        let _ = write!(branch, ", !llvm.loop !{id}");
    }

    fn guard(&mut self, valid: &str, kind: TrapKind) {
        let success = self.label();
        let failure = self.label();
        self.branch(valid, &success, &failure);
        self.begin(&failure);
        self.emit_trap(kind);
        self.instruction("unreachable");
        self.begin(&success);
    }

    fn emit_trap(&mut self, kind: TrapKind) {
        let previous = self.trap_kind.replace(kind);
        self.instruction("call void @llvm.trap()");
        self.trap_kind = previous;
    }

    fn slot(&mut self, ty: &Type) -> String {
        let slot = self.fresh();
        self.allocas
            .push(format!("{slot} = alloca {}, align 16", self.ty(ty)));
        slot
    }

    fn bind_local(&mut self, local: &Local, value: &str) {
        self.forget_temporary(value);
        let slot = self.slot(&local.ty);
        if let Some(scope) = self.debug_scope
            && local.provenance == crate::syntax::Provenance::User
            && local.name != "_"
        {
            let parameter = self
                .function
                .parameters
                .iter()
                .position(|parameter| parameter.id == local.id)
                .map(|index| index + 1);
            if let Some((variable, location)) =
                self.globals
                    .debug_variable(self.module, scope, local, parameter)
            {
                self.intrinsics
                    .insert("declare void @llvm.dbg.declare(metadata, metadata, metadata)".into());
                self.debug_declarations.push(format!("call void @llvm.dbg.declare(metadata ptr {slot}, metadata !{variable}, metadata !DIExpression()), !dbg !{location}"));
            }
        }
        let store = format!("store {} {value}, ptr {slot}", self.ty(&local.ty));
        if self.debug_scope.is_some()
            && self
                .function
                .parameters
                .iter()
                .any(|parameter| parameter.id == local.id)
        {
            // As in Clang, the parameters' stores have no location: they belong to the prologue,
            // so a breakpoint on the function stops where the parameters have their values (G16 D7).
            self.lines.push(format!("  {store}"));
        } else if local.span == Span::default() {
            self.instruction(store);
        } else {
            // A binding's store is located at its declaration, so stepping does not go back to
            // the start of the enclosing block (G16 D7).
            let previous = std::mem::replace(&mut self.current_span, local.span);
            self.instruction(store);
            self.current_span = previous;
        }
        self.locals.insert(local.id, slot.clone());
        if local.ty.needs_drop(&self.module.types()) && !self.borrowed_locals.contains(&local.id) {
            self.scopes
                .last_mut()
                .unwrap()
                .push((slot, local.ty.clone()));
        }
    }

    fn bind(&mut self, bindings: &[(Local, TypedExpr)]) {
        for (local, expression) in bindings {
            let target = (!local.mutable)
                .then(|| call_specialization::target(expression, &self.known_closures, self.module))
                .flatten();
            let (value, frames) = self.frame_value(expression);
            self.bind_local(local, &value);
            self.bind_frames(local, frames);
            if let Some(target) = target {
                self.known_closures.insert(local.id, target);
            }
        }
    }

    fn is_self(&self, callee: &TypedExpr) -> bool {
        matches!(callee.kind, TypedExprKind::Function(FunctionRef::User(id)) if id == self.function_id)
            && !self.borrowed_worker
            && !self
                .function
                .signature
                .parameters
                .iter()
                .any(|ty| ty.carries_loans(&self.module.types()))
    }

    fn tail_arguments(&mut self, arguments: &[TypedExpr]) -> Vec<String> {
        if self.globals.wasm {
            return arguments
                .iter()
                .map(|argument| self.expression(argument))
                .collect();
        }
        enum Argument {
            Value(String),
            Arithmetic(String),
        }
        let mut pending = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let operation = call_specialization::binary_operation(argument, self.module).filter(
                |(operator, _, _, _)| {
                    argument.ty.is_integer()
                        && matches!(operator, BinaryOp::Add | BinaryOp::Subtract)
                },
            );
            if let Some((operator, left, right, helper)) = operation {
                // Snapshot operands now; only the nontrapping wrapping operation moves to the latch.
                let left = self.expression(left);
                let right = self.expression(right);
                if let Some(helper) = helper {
                    self.cover_call(helper);
                }
                let opcode = if operator == BinaryOp::Add {
                    "add"
                } else {
                    "sub"
                };
                pending.push(Argument::Arithmetic(format!(
                    "{opcode} {} {left}, {right}",
                    self.ty(&argument.ty)
                )));
            } else {
                pending.push(Argument::Value(self.expression(argument)));
            }
        }
        pending
            .into_iter()
            .map(|argument| match argument {
                Argument::Value(value) => value,
                Argument::Arithmetic(instruction) => self.value(instruction),
            })
            .collect()
    }

    fn tail(&mut self, expression: &TypedExpr) {
        let previous = std::mem::replace(&mut self.current_span, expression.span);
        self.emit_tail(expression);
        self.current_span = previous;
    }

    fn emit_tail(&mut self, expression: &TypedExpr) {
        match &expression.kind {
            TypedExprKind::Match { local, value, arms } => {
                self.match_expression(local, value, arms, &expression.ty, true, None);
            }
            TypedExprKind::Block { bindings, result } => {
                self.scopes.push(Vec::new());
                self.bind(bindings);
                self.tail(result);
                self.scopes.pop();
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let test = self.expression(condition);
                let yes = self.label();
                let no = self.label();
                self.branch(&test, &yes, &no);
                self.begin(&yes);
                self.cover(crate::coverage::RegionKind::Then, then_branch.span);
                let facts = self.ranges.enter_condition(self.module, condition);
                self.tail(then_branch);
                self.ranges.leave_condition(facts);
                self.begin(&no);
                self.cover(crate::coverage::RegionKind::Else, else_branch.span);
                self.tail(else_branch);
            }
            TypedExprKind::Call(callee, arguments)
                if self.is_self(callee) && arguments.len() == self.function.parameters.len() =>
            {
                let values = self.tail_arguments(arguments);
                self.drop_all();
                self.back_edges.push((self.block.clone(), values));
                self.jump("loop");
                self.hint_loop(&self.function.body, 0);
            }
            TypedExprKind::Binary(BinaryOp::Pipe, argument, callee)
                if self.is_self(callee) && self.function.parameters.len() == 1 =>
            {
                let values = self.tail_arguments(std::slice::from_ref(argument.as_ref()));
                self.drop_all();
                self.back_edges.push((self.block.clone(), values));
                self.jump("loop");
                self.hint_loop(&self.function.body, 0);
            }
            _ => {
                let value = self.expression(expression);
                self.drop_all();
                self.instruction(format!("ret {} {value}", self.ty(&expression.ty)));
            }
        }
    }

    fn expression(&mut self, expression: &TypedExpr) -> String {
        self.expression_mode(expression, true)
    }

    /// Reads a place. A taken value is cloned (Copy) or moved; a moved value leaves this frame's
    /// storage unless `relocate` is false and the caller destroys it immediately.
    fn read_place(&mut self, expression: &TypedExpr, take: bool, relocate: bool) -> String {
        let slot = self.place(expression);
        let value = self.value(format!("load {}, ptr {slot}", self.ty(&expression.ty)));
        if take && expression.ty.needs_drop(&self.module.types()) {
            if self.clones_on_take(expression) {
                #[cfg(debug_assertions)]
                self.note_copy(expression);
                return self.clone_value(&expression.ty, &value);
            }
            self.instruction(format!(
                "store {} zeroinitializer, ptr {slot}",
                self.ty(&expression.ty)
            ));
            if relocate {
                let frames = self.frame_of_place(expression);
                return self.relocate(&expression.ty, &value, &frames);
            }
        }
        value
    }

    /// Taking a Copy place copies it, except at the only use of an owned local.
    fn clones_on_take(&self, expression: &TypedExpr) -> bool {
        let last_use = matches!(expression.kind, TypedExprKind::Local(id)
            if self.single_use.contains(&id) && !self.borrowed_locals.contains(&id));
        expression.ty.is_copy(&self.module.types()) && !last_use
    }

    #[cfg(debug_assertions)]
    fn note_copy(&mut self, expression: &TypedExpr) {
        if self.note_copies {
            let span = expression.span;
            self.globals.emitted_copies.insert((
                self.function_id,
                span.source,
                span.start,
                span.end,
            ));
        }
    }

    fn record_update(
        &mut self,
        expression: &TypedExpr,
        base: &TypedExpr,
        fields: &[(usize, TypedExpr)],
    ) -> String {
        let mut record = self.expression(base);
        let values: Vec<_> = fields
            .iter()
            .map(|(index, field)| (*index, field, self.expression(field)))
            .collect();
        for (index, field, value) in values {
            if field.ty.needs_drop(&self.module.types()) {
                let previous = self.value(format!(
                    "extractvalue {} {record}, {index}",
                    self.ty(&expression.ty),
                ));
                self.drop_value(&field.ty, &previous);
            }
            record = self.value(format!(
                "insertvalue {} {record}, {} {value}, {index}",
                self.ty(&expression.ty),
                self.ty(&field.ty),
            ));
        }
        record
    }

    fn string_constant(&mut self, text: &StringLiteral) -> String {
        let name = format!("@tz.literal.{}", self.globals.definitions.len());
        let constant = match text {
            StringLiteral::Utf16(units) => {
                let values = units
                    .iter()
                    .map(|unit| format!("i16 {unit}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{} x i16] [{values}]", units.len())
            }
            StringLiteral::Utf8(text) => {
                let escaped = text
                    .bytes()
                    .map(|byte| format!("\\{byte:02X}"))
                    .collect::<String>();
                format!("[{} x i8] c\"{escaped}\"", text.len())
            }
        };
        self.globals
            .definitions
            .push(format!("{name} = private unnamed_addr constant {constant}"));
        name
    }

    fn remember_temporary(&mut self, ty: &Type, value: &str, frames: &[Frame]) {
        if (!self.loop_targets.is_empty() || !self.try_targets.is_empty())
            && ty.needs_drop(&self.module.types())
        {
            self.temporaries
                .push((ty.clone(), value.to_owned(), frames.to_vec()));
        }
    }

    fn forget_temporary(&mut self, value: &str) {
        self.temporaries.retain(|(_, held, _)| held != value);
    }

    fn expression_mode(&mut self, expression: &TypedExpr, take: bool) -> String {
        let previous = std::mem::replace(&mut self.current_span, expression.span);
        let base = self.temporaries.len();
        let value = self.emit_expression_mode(expression, take);
        self.temporaries.truncate(base);
        if take {
            self.remember_temporary(&expression.ty, &value, &[]);
        }
        self.current_span = previous;
        value
    }

    fn shared_array_deref(expression: &TypedExpr) -> Option<&TypedExpr> {
        match &expression.kind {
            TypedExprKind::Dereference(reference) if reference.ty.slice_element().is_some() => {
                Some(reference)
            }
            _ => None,
        }
    }

    fn emit_expression_mode(&mut self, expression: &TypedExpr, take: bool) -> String {
        if let Some(reference) = Self::shared_array_deref(expression) {
            let view = self.expression_mode(reference, false);
            return if take {
                #[cfg(debug_assertions)]
                self.note_copy(expression);
                self.clone_value(&expression.ty, &view)
            } else {
                view
            };
        }
        if Self::is_place(expression) {
            return self.read_place(expression, take, true);
        }
        match &expression.kind {
            TypedExprKind::Parallel(operation, arguments) => {
                self.parallel_expression(*operation, arguments, &expression.ty)
            }
            TypedExprKind::Int(value) => value.to_string(),
            TypedExprKind::GenericFunction(..)
            | TypedExprKind::Method(..)
            | TypedExprKind::TypeFunction { .. }
            | TypedExprKind::GenericInteger(..)
            | TypedExprKind::GenericFloat(_)
            | TypedExprKind::Error
            | TypedExprKind::BorrowOperand(_)
            | TypedExprKind::Lambda { .. }
            | TypedExprKind::CaseConstructor { .. } => {
                unreachable!("polymorphism is resolved before LLVM")
            }
            // `emit` writes a dispatch body itself; it is never a subexpression (A14).
            TypedExprKind::DynDispatch { .. } => {
                unreachable!("a dyn dispatch is a whole function body")
            }
            TypedExprKind::Construct {
                case_id, payload, ..
            } => self.construct(&expression.ty, *case_id, payload.as_deref()),
            TypedExprKind::UnionTag(value) => self.union_tag(value),
            TypedExprKind::UnionPayload { value, .. } => {
                let union = self.expression(value);
                let payload = self.payload_value(&value.ty, &union, &expression.ty);
                if value.ty.has_user_drop(&self.module.types()) {
                    // The whole value meets its user drop; ownership allows only Copy payloads here.
                    let payload = if expression.ty.needs_drop(&self.module.types()) {
                        #[cfg(debug_assertions)]
                        self.note_copy(expression);
                        self.clone_value(&expression.ty, &payload)
                    } else {
                        payload
                    };
                    self.drop_value(&value.ty, &union);
                    return payload;
                }
                if self.module.types().recursive(&value.ty) {
                    self.forget_temporary(&union);
                    self.instruction(format!("call void @tz.free(ptr {union})"));
                }
                payload
            }
            TypedExprKind::Float(value) => value.clone(),
            TypedExprKind::HostCall(import, arguments) => {
                self.host_call(import, arguments, &expression.ty)
            }
            TypedExprKind::Bool(value) => if *value { "1" } else { "0" }.into(),
            TypedExprKind::Unit => "0".into(),
            TypedExprKind::Break | TypedExprKind::Continue => {
                self.emit_loop_jump(matches!(expression.kind, TypedExprKind::Break));
                "0".into()
            }
            TypedExprKind::While { condition, body } => {
                self.while_loop(condition, body);
                "0".into()
            }
            TypedExprKind::ForRange {
                local,
                start,
                step,
                finish,
                body,
            } => {
                self.range_loop(local, start, step, finish, body);
                "0".into()
            }
            TypedExprKind::ForEach {
                local,
                source,
                body,
                ..
            } => {
                self.for_each(local, source, body);
                "0".into()
            }
            TypedExprKind::Match { local, value, arms } => {
                self.match_expression(local, value, arms, &expression.ty, false, None)
            }
            TypedExprKind::String(text) => {
                let name = self.string_constant(text);
                let ty = self.ty(&expression.ty);
                self.value(format!(
                    "call {ty} @{}.new(ptr {name}, i64 {})",
                    &ty[1..],
                    text.len()
                ))
            }
            TypedExprKind::Local(_)
            | TypedExprKind::Dereference(_)
            | TypedExprKind::ListTail(..) => {
                unreachable!("places handled above")
            }
            TypedExprKind::Borrow(value, mutable) => {
                if !mutable && matches!(value.ty, Type::Array(_)) {
                    return self.expression_mode(value, false);
                }
                let slot = self.place(value);
                let frames = self.frame_of_place(value);
                if *mutable && !frames.is_empty() {
                    // The borrower may replace and drop the value, so it must own heap storage.
                    let ty = self.ty(&value.ty);
                    let current = self.value(format!("load {ty}, ptr {slot}"));
                    let moved = self.relocate(&value.ty, &current, &frames);
                    self.instruction(format!("store {ty} {moved}, ptr {slot}"));
                }
                slot
            }
            TypedExprKind::Assign(place, value) => {
                let value = self.expression(value);
                let slot = self.place(place);
                self.drop_slot(&slot, &place.ty);
                self.instruction(format!("store {} {value}, ptr {slot}", self.ty(&place.ty)));
                "0".into()
            }
            TypedExprKind::Cast(value) => self.cast(value, &expression.ty),
            TypedExprKind::Function(reference) => match reference {
                FunctionRef::User(id) => self.make_closure(*id, &[]),
                FunctionRef::Builtin(_) => unreachable!("builtin values are lifted before LLVM"),
            },
            TypedExprKind::Closure(id, captures) => {
                // A capture's span is its local's declaration, but the closure reads it here.
                let located = self.debug_span.replace(expression.span);
                let values: Vec<_> = captures
                    .iter()
                    .map(|capture| self.expression(capture))
                    .collect();
                self.debug_span = located;
                self.make_closure(*id, &values)
            }
            TypedExprKind::Unary(operator, operand) => {
                let value = self.expression(operand);
                if let Type::Simd(vector) = operand.ty {
                    return self.simd_unary(*operator, vector, &value);
                }
                let ty = self.ty(&operand.ty);
                self.value(match operator {
                    UnaryOp::Negate if matches!(operand.ty, Type::Binary(32 | 64)) => {
                        format!("fneg {ty} {value}")
                    }
                    UnaryOp::Negate if operand.ty.is_float() => {
                        let bits = match operand.ty {
                            Type::Binary(bits) | Type::Decimal(bits) => bits,
                            _ => unreachable!(),
                        };
                        format!("xor {ty} {value}, {}", 1u128 << (bits - 1))
                    }
                    UnaryOp::Negate => format!("sub {ty} 0, {value}"),
                    UnaryOp::Not => format!("xor i1 {value}, 1"),
                    UnaryOp::BitNot => format!("xor {ty} {value}, -1"),
                    UnaryOp::Plus => unreachable!("unary plus is checked to its operand"),
                })
            }
            TypedExprKind::Checked(value) => self.checked(value),
            TypedExprKind::Try(handled) => self.try_expression(handled, &expression.ty),
            TypedExprKind::RaisedException => {
                let slot = self.caught.last().expect("handlers read their exception");
                let slot = slot.clone();
                self.value(format!("load i32, ptr {slot}"))
            }
            TypedExprKind::Reraise => {
                let slot = self.caught.last().expect("re-raises are in handlers");
                let slot = slot.clone();
                let code = self.value(format!("load i32, ptr {slot}"));
                self.raise(&code);
                "poison".into()
            }
            TypedExprKind::Binary(BinaryOp::And | BinaryOp::Or, left, right) => {
                let TypedExprKind::Binary(operator, _, _) = expression.kind else {
                    unreachable!()
                };
                self.short_circuit(operator, left, right)
            }
            TypedExprKind::Binary(BinaryOp::Pipe, argument, callee) => {
                if matches!(callee.kind, TypedExprKind::Function(_)) {
                    return self.call(callee, std::slice::from_ref(argument.as_ref()));
                }
                if let TypedExprKind::BorrowOperand(operand) = &argument.kind {
                    // `temp |> f` borrows the temporary until `f` returns.
                    let mut cleanup = Vec::new();
                    let value = self.operand_borrow(argument, operand, &mut cleanup);
                    let borrowed = Self::is_place(callee);
                    let function = self.expression_mode(callee, !borrowed);
                    let result = self
                        .apply_value(
                            &function,
                            &callee.ty,
                            Some((&argument.ty, &value)),
                            borrowed,
                        )
                        .0;
                    self.release_operands(cleanup);
                    return result;
                }
                let value = self.expression(argument);
                if let Some(call) = self.prepare_known_call(callee, 1) {
                    let result = self.emit_borrowed_call(&call, &[value]);
                    self.finish_borrowed_call(&call);
                    return result;
                }
                let borrowed = Self::is_place(callee);
                let function = self.expression_mode(callee, !borrowed);
                self.apply_value(
                    &function,
                    &callee.ty,
                    Some((&argument.ty, &value)),
                    borrowed,
                )
                .0
            }
            TypedExprKind::Binary(operator, left, right) => self.binary(*operator, left, right),
            TypedExprKind::StructuralCompare(operator, arguments) => {
                self.structural_compare(*operator, arguments)
            }
            TypedExprKind::StructuralHash(arguments) => self.structural_hash(arguments),
            TypedExprKind::StructuralDisplay(arguments) => self.structural_display(arguments),
            TypedExprKind::Interpolated(interpolation) => {
                self.interpolation(interpolation, &expression.ty)
            }
            TypedExprKind::Call(callee, arguments) => self.call(callee, arguments),
            TypedExprKind::TaskRun(task) => {
                let task = self.expression(task);
                let code = self.value(format!("extractvalue %tz.closure {task}, 0"));
                let environment = self.value(format!("extractvalue %tz.closure {task}, 1"));
                self.value(format!(
                    "call {} {code}(ptr {environment})",
                    self.ty(&expression.ty)
                ))
            }
            TypedExprKind::TaskParallel(tasks) => self.parallel_tasks(tasks, &expression.ty),
            TypedExprKind::TaskParallelResults(tasks) => {
                self.parallel_result_tasks(tasks, &expression.ty)
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let test = self.expression(condition);
                let temporary_base = self.temporaries.len();
                let yes = self.label();
                let no = self.label();
                let merge = self.label();
                self.branch(&test, &yes, &no);
                self.begin(&yes);
                self.cover(crate::coverage::RegionKind::Then, then_branch.span);
                let facts = self.ranges.enter_condition(self.module, condition);
                let then_value = self.expression(then_branch);
                self.ranges.leave_condition(facts);
                let then_end = self.block.clone();
                self.jump(&merge);
                self.temporaries.truncate(temporary_base);
                self.begin(&no);
                self.cover(crate::coverage::RegionKind::Else, else_branch.span);
                let else_value = self.expression(else_branch);
                let else_end = self.block.clone();
                self.jump(&merge);
                self.temporaries.truncate(temporary_base);
                self.begin(&merge);
                self.value(format!(
                    "phi {} [ {then_value}, %{then_end} ], [ {else_value}, %{else_end} ]",
                    self.ty(&expression.ty),
                ))
            }
            TypedExprKind::Block { bindings, result } => {
                self.scopes.push(Vec::new());
                self.bind(bindings);
                let value = self.expression(result);
                let scope = self.scopes.pop().unwrap();
                self.drop_scope(&scope);
                value
            }
            TypedExprKind::RecordUpdate { base, fields } => {
                self.record_update(expression, base, fields)
            }
            TypedExprKind::Slice { value, start, end } => {
                if expression.ty.is_view() {
                    self.own_heap_storage(value);
                }
                self.array_slice(value, start.as_deref(), end.as_deref())
            }
            TypedExprKind::Record(fields) => {
                let mut record = "zeroinitializer".into();
                for (index, field) in fields {
                    let value = self.expression(field);
                    record = self.value(format!(
                        "insertvalue {} {record}, {} {value}, {index}",
                        self.ty(&expression.ty),
                        self.ty(&field.ty)
                    ));
                }
                self.mark_live(&expression.ty, record)
            }
            TypedExprKind::Tuple(elements) => {
                let mut tuple = "zeroinitializer".into();
                for (index, element) in elements.iter().enumerate() {
                    let value = self.expression(element);
                    tuple = self.value(format!(
                        "insertvalue {} {tuple}, {} {value}, {index}",
                        self.ty(&expression.ty),
                        self.ty(&element.ty)
                    ));
                }
                tuple
            }
            TypedExprKind::Array(elements) | TypedExprKind::List(elements)
                if elements.is_empty() =>
            {
                // An empty literal owns no storage; dropping its null buffer is a no-op.
                "zeroinitializer".into()
            }
            TypedExprKind::Array(_) | TypedExprKind::List(_) => self.heap_collection(expression),
            TypedExprKind::NewLiteral(literal) => self.heap_collection(literal),
            TypedExprKind::NewArray(length, initializer) => {
                let length = self.expression(length);
                let direct = self.prepare_known_call(initializer, 1);
                let callee = direct.is_none().then(|| self.expression(initializer));
                let Type::Array(element) = &expression.ty else {
                    unreachable!()
                };
                let (array, data) = self.allocate_array(element, &length);
                self.array_loop(&length, |emitter, index| {
                    let value = if let Some(call) = &direct {
                        emitter.emit_borrowed_call(call, &[index.to_owned()])
                    } else {
                        emitter
                            .apply_value(
                                callee.as_ref().unwrap(),
                                &initializer.ty,
                                Some((&Type::I64, index)),
                                true,
                            )
                            .0
                    };
                    let pointer = emitter.element_pointer(element, &data, index);
                    emitter.instruction(format!(
                        "store {} {value}, ptr {pointer}",
                        emitter.ty(element)
                    ));
                });
                if let Some(call) = &direct {
                    self.finish_borrowed_call(call);
                } else {
                    self.drop_value(&initializer.ty, callee.as_ref().unwrap());
                }
                array
            }
            TypedExprKind::NewList(length, initializer) => {
                let length = self.expression(length);
                let direct = self.prepare_known_call(initializer, 1);
                let callee = direct.is_none().then(|| self.expression(initializer));
                let Type::List(element) = &expression.ty else {
                    unreachable!()
                };
                self.allocation_size(&self.list_node_type(element), &length);
                let (head, tail) = self.list_builder();
                self.array_loop(&length, |emitter, index| {
                    let value = if let Some(call) = &direct {
                        emitter.emit_borrowed_call(call, &[index.to_owned()])
                    } else {
                        emitter
                            .apply_value(
                                callee.as_ref().unwrap(),
                                &initializer.ty,
                                Some((&Type::I64, index)),
                                true,
                            )
                            .0
                    };
                    emitter.append_list(element, &tail, &value);
                });
                if let Some(call) = &direct {
                    self.finish_borrowed_call(call);
                } else {
                    self.drop_value(&initializer.ty, callee.as_ref().unwrap());
                }
                self.finish_list(&head, &length)
            }
            TypedExprKind::Field(record, index) => {
                let value = self.expression(record);
                let field = self.value(format!(
                    "extractvalue {} {value}, {index}",
                    self.ty(&record.ty)
                ));
                if record.ty.has_user_drop(&self.module.types()) {
                    // The whole value meets its user drop; ownership allows only Copy fields here.
                    let field = if expression.ty.needs_drop(&self.module.types()) {
                        #[cfg(debug_assertions)]
                        self.note_copy(expression);
                        self.clone_value(&expression.ty, &field)
                    } else {
                        field
                    };
                    self.drop_value(&record.ty, &value);
                    return field;
                }
                if record.ty.needs_drop(&self.module.types()) {
                    let remainder = self.value(format!(
                        "insertvalue {} {value}, {} zeroinitializer, {index}",
                        self.ty(&record.ty),
                        self.ty(&expression.ty)
                    ));
                    self.drop_value(&record.ty, &remainder);
                }
                field
            }
            TypedExprKind::Index(string, index) if string.ty.is_string() => {
                let (value, frames) = self.read_operand(string);
                let index = self.expression(index);
                let ty = self.ty(&string.ty);
                let element = self.ty(&expression.ty);
                let data = self.value(format!("extractvalue {ty} {value}, 0"));
                let length = self.value(format!("extractvalue {ty} {value}, 1"));
                let valid = self.value(format!("icmp ult i64 {index}, {length}"));
                self.guard(&valid, TrapKind::BoundsCheck);
                let pointer = self.value(format!(
                    "getelementptr inbounds {element}, ptr {data}, i64 {index}"
                ));
                let unit = self.value(format!("load {element}, ptr {pointer}"));
                self.release_operand(string, &value, &frames);
                unit
            }
            TypedExprKind::Index(array, index) if matches!(array.ty, Type::FixedArray(..)) => {
                // A16: the element is read in place, without loading the whole array.
                let (base, operand) = self.fixed_base(array);
                let position = self.expression(index);
                let Type::FixedArray(element, _) = &array.ty else {
                    unreachable!()
                };
                let pointer = self.fixed_element_pointer(&array.ty, &base, index, &position);
                let extracted = self.value(format!("load {}, ptr {pointer}", self.ty(element)));
                #[cfg(debug_assertions)]
                if element.is_copy(&self.module.types()) && element.needs_drop(&self.module.types())
                {
                    self.note_copy(expression);
                }
                let result = self.clone_value(element, &extracted);
                if let Some((value, frames)) = operand {
                    self.release_operand(array, &value, &frames);
                }
                result
            }
            TypedExprKind::Index(array, index) => {
                let proven = self.ranges.index_in_bounds(array, index);
                let (value, frames) = self.read_operand(array);
                let index = self.expression(index);
                let (Type::Array(element) | Type::List(element) | Type::Vec(element)) = &array.ty
                else {
                    unreachable!()
                };
                let pointer = self.indexed_pointer(proven, &array.ty, &value, &index);
                let extracted = self.value(format!("load {}, ptr {pointer}", self.ty(element)));
                #[cfg(debug_assertions)]
                if element.is_copy(&self.module.types()) && element.needs_drop(&self.module.types())
                {
                    self.note_copy(expression);
                }
                let result = self.clone_value(element, &extracted);
                self.release_operand(array, &value, &frames);
                result
            }
            TypedExprKind::Length(array) if matches!(array.ty, Type::FixedArray(..)) => {
                // The length is the type's; the operand still runs for its effects and traps.
                let (_, operand) = self.fixed_base(array);
                if let Some((value, frames)) = operand {
                    self.release_operand(array, &value, &frames);
                }
                fixed_length(&array.ty).to_string()
            }
            TypedExprKind::Length(array) => {
                let (value, frames) = self.read_operand(array);
                let length = self.value(format!("extractvalue {} {value}, 1", self.ty(&array.ty)));
                self.release_operand(array, &value, &frames);
                length
            }
            TypedExprKind::StringLength(string) => {
                let (value, frames) = self.read_operand(string);
                let length = self.value(format!("extractvalue {} {value}, 1", self.ty(&string.ty)));
                self.release_operand(string, &value, &frames);
                length
            }
        }
    }

    /// Builds an array or list literal on the heap, as for `new [...]` and escaping temporaries.
    fn heap_collection(&mut self, expression: &TypedExpr) -> String {
        match (&expression.kind, &expression.ty) {
            (TypedExprKind::Array(elements), Type::Array(element)) => {
                let (array, data) = self.allocate_array(element, &elements.len().to_string());
                let temporary_base = self.temporaries.len();
                let tracking = !self.loop_targets.is_empty();
                for (index, value) in elements.iter().enumerate() {
                    if tracking {
                        let prefix =
                            self.value(format!("insertvalue %tz.array {array}, i64 {index}, 1"));
                        self.temporaries.truncate(temporary_base);
                        self.remember_temporary(&expression.ty, &prefix, &[]);
                    }
                    let value = self.expression(value);
                    let pointer = self.element_pointer(element, &data, &index.to_string());
                    self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
                    if tracking {
                        self.temporaries.truncate(temporary_base + 1);
                    }
                }
                array
            }
            (TypedExprKind::List(elements), Type::List(element)) => {
                let (head, tail) = self.list_builder();
                let temporary_base = self.temporaries.len();
                let tracking = !self.loop_targets.is_empty();
                for (index, value) in elements.iter().enumerate() {
                    if tracking {
                        let prefix = self.finish_list(&head, &index.to_string());
                        self.temporaries.truncate(temporary_base);
                        self.remember_temporary(&expression.ty, &prefix, &[]);
                    }
                    let value = self.expression(value);
                    self.append_list(element, &tail, &value);
                    if tracking {
                        self.temporaries.truncate(temporary_base + 1);
                    }
                }
                self.finish_list(&head, &elements.len().to_string())
            }
            _ => unreachable!("'new' literals are array or list literals"),
        }
    }

    fn is_place(expression: &TypedExpr) -> bool {
        expression.is_place()
    }

    fn place(&mut self, expression: &TypedExpr) -> String {
        let previous = std::mem::replace(&mut self.current_span, expression.span);
        let value = self.emit_place(expression);
        self.current_span = previous;
        value
    }

    fn emit_place(&mut self, expression: &TypedExpr) -> String {
        match &expression.kind {
            TypedExprKind::Local(id) => self.locals[id].clone(),
            TypedExprKind::Dereference(value) if value.ty.slice_element().is_some() => {
                let view = self.expression_mode(value, false);
                self.spill(&expression.ty, &view)
            }
            TypedExprKind::Dereference(value) => self.expression_mode(value, false),
            TypedExprKind::Field(value, index) => {
                let slot = self.place(value);
                self.value(format!(
                    "getelementptr inbounds {}, ptr {slot}, i32 0, i32 {index}",
                    self.ty(&value.ty)
                ))
            }
            TypedExprKind::UnionPayload { value, .. } => {
                let slot = self.place(value);
                self.payload_pointer(&value.ty, &slot)
            }
            TypedExprKind::ListTail(value, count) => {
                let slot = self.place(value);
                let list = self.value(format!("load %tz.list, ptr {slot}"));
                let length = self.value(format!("extractvalue %tz.list {list}, 1"));
                let valid = self.value(format!("icmp uge i64 {length}, {count}"));
                self.guard(&valid, TrapKind::PatternMismatch);
                let head = self.value(format!("extractvalue %tz.list {list}, 0"));
                let tail = self.list_loop(&head, &count.to_string(), |_, _| {});
                let length = self.value(format!("sub i64 {length}, {count}"));
                let descriptor = self.value(format!(
                    "insertvalue %tz.list zeroinitializer, ptr {tail}, 0"
                ));
                let descriptor = self.value(format!(
                    "insertvalue %tz.list {descriptor}, i64 {length}, 1"
                ));
                let slot = self.slot(&value.ty);
                self.instruction(format!("store %tz.list {descriptor}, ptr {slot}"));
                slot
            }
            TypedExprKind::Index(value, index) if matches!(value.ty, Type::FixedArray(..)) => {
                let base = self.place(value);
                let position = self.expression(index);
                self.fixed_element_pointer(&value.ty, &base, index, &position)
            }
            TypedExprKind::Index(value, index) => {
                let proven = self.ranges.index_in_bounds(value, index);
                if let Some(reference) = Self::shared_array_deref(value) {
                    let array = self.expression_mode(reference, false);
                    let index = self.expression(index);
                    return self.indexed_pointer(proven, &value.ty, &array, &index);
                }
                let slot = self.place(value);
                let array = self.value(format!("load {}, ptr {slot}", self.ty(&value.ty)));
                let index = self.expression(index);
                self.indexed_pointer(proven, &value.ty, &array, &index)
            }
            _ => unreachable!("borrow checker requires an addressable place"),
        }
    }

    fn drop_value(&mut self, ty: &Type, value: &str) {
        self.forget_temporary(value);
        let value = &self
            .user_drop(ty, value)
            .unwrap_or_else(|| value.to_owned());
        match ty {
            Type::Union(..) if self.module.types().recursive(ty) => {
                self.globals.recursive_types.insert(ty.clone());
                if let Some(pending) = &self.drop_pending {
                    self.instruction(format!(
                        "call void @tz.rec.enqueue(ptr {value}, ptr {pending})"
                    ));
                } else {
                    self.instruction(format!("call void @tz.rec.drop(ptr {value})"));
                }
            }
            Type::Function(..) | Type::Task(_) => {
                self.instruction(format!("call void @tz.closure.drop(%tz.closure {value})"))
            }
            // A14: the vtable's drop slot drops the stored value and frees its data.
            Type::Dyn(_) => self.instruction(format!("call void @tz.dyn.drop(%tz.dyn {value})")),
            Type::Shared(shared, kind) => self.drop_shared(shared, *kind, value),
            Type::String | Type::Utf8String => {
                let pointer = self.value(format!("extractvalue {} {value}, 0", self.ty(ty)));
                self.instruction(format!("call void @tz.free(ptr {pointer})"));
            }
            Type::Record(id, arguments) => {
                let fields = self.module.types().record_fields(*id, arguments);
                for (index, field) in fields.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let extracted =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        self.drop_value(field, &extracted);
                    }
                }
            }
            Type::Tuple(elements) => {
                for (index, field) in elements.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let extracted =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        self.drop_value(field, &extracted);
                    }
                }
            }
            Type::Union(..) => {
                let cases = self.owning_cases(ty);
                if cases.is_empty() {
                    return;
                }
                let spilled = matches!(self.union_layout(ty), UnionLayout::General(_))
                    .then(|| self.spill(ty, value));
                let (labels, done) = self.case_switch(ty, value, &cases);
                for ((_, payload), label) in cases.into_iter().zip(labels) {
                    self.begin(&label);
                    let extracted = match &spilled {
                        Some(slot) => {
                            let pointer = self.payload_pointer(ty, slot);
                            self.value(format!("load {}, ptr {pointer}", self.ty(&payload)))
                        }
                        None => self.value(format!("extractvalue {} {value}, 1", self.ty(ty))),
                    };
                    self.drop_value(&payload, &extracted);
                    self.jump(&done);
                }
                self.begin(&done);
            }
            Type::Array(element) | Type::Vec(element) => {
                let data = self.value(format!("extractvalue {} {value}, 0", self.ty(ty)));
                if element.needs_drop(&self.module.types()) {
                    let length = self.value(format!("extractvalue {} {value}, 1", self.ty(ty)));
                    self.array_loop(&length, |emitter, index| {
                        let pointer = emitter.element_pointer(element, &data, index);
                        let extracted =
                            emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                        emitter.drop_value(element, &extracted);
                    });
                }
                self.instruction(format!("call void @tz.free(ptr {data})"));
            }
            // A16: the elements are inline, so only their own resources drop, in a loop.
            Type::FixedArray(element, _) if element.needs_drop(&self.module.types()) => {
                let slot = self.spill(ty, value);
                self.array_loop(&fixed_length(ty).to_string(), |emitter, index| {
                    let pointer = emitter.fixed_pointer(ty, &slot, index);
                    let extracted =
                        emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                    emitter.drop_value(element, &extracted);
                });
            }
            Type::List(element) => {
                let head = self.value(format!("extractvalue %tz.list {value}, 0"));
                let length = self.value(format!("extractvalue %tz.list {value}, 1"));
                self.list_loop(&head, &length, |emitter, node| {
                    if element.needs_drop(&emitter.module.types()) {
                        let pointer = emitter.list_element_pointer(element, node);
                        let value =
                            emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                        emitter.drop_value(element, &value);
                    }
                    emitter.instruction(format!("call void @tz.free(ptr {node})"));
                });
            }
            _ => {}
        }
    }

    /// Sets the live flag of a new Drop record or union value.
    fn mark_live(&mut self, ty: &Type, value: String) -> String {
        match drop_flag(ty, self.module) {
            Some(flag) => self.value(format!("insertvalue {} {value}, i8 1, {flag}", self.ty(ty))),
            None => value,
        }
    }

    /// Calls the user drop of a live Drop record or union and returns the value whose fields
    /// drop next. Moved-out storage is zero, so its cleared flag skips the call.
    fn user_drop(&mut self, ty: &Type, value: &str) -> Option<String> {
        let flag = drop_flag(ty, self.module)?;
        let llvm = self.ty(ty);
        let stored = self.value(format!("extractvalue {llvm} {value}, {flag}"));
        let live = self.value(format!("icmp ne i8 {stored}, 0"));
        let entry = self.block.clone();
        let call = self.label();
        let join = self.label();
        self.branch(&live, &call, &join);
        self.begin(&call);
        let dropped = self.call_user_drop(ty, value);
        let end = self.block.clone();
        self.jump(&join);
        self.begin(&join);
        Some(self.value(format!(
            "phi {llvm} [ {value}, %{entry} ], [ {dropped}, %{end} ]"
        )))
    }

    /// Runs `Drop.drop` on `value` in a slot and reads the value back, since `drop` takes
    /// `ref mut` and its fields drop afterwards.
    pub(super) fn call_user_drop(&mut self, ty: &Type, value: &str) -> String {
        let module = self.module;
        let function = &module.functions[module.user_drops[ty]];
        let slot = self.spill(ty, value);
        self.instruction(format!(
            "call {} @tz.fn.{}(ptr {slot})",
            llvm_type(&function.signature.result, module),
            function.qualified_name()
        ));
        self.value(format!("load {}, ptr {slot}", self.ty(ty)))
    }

    fn clone_value(&mut self, ty: &Type, value: &str) -> String {
        assert!(
            !ty.has_user_drop(&self.module.types()) && !ty.is_owned_function(&self.module.types()),
            "Drop types and owned functions are never cloned"
        );
        match ty {
            Type::Union(..) if self.module.types().recursive(ty) => {
                self.globals.recursive_types.insert(ty.clone());
                if let Some(pending) = &self.clone_pending {
                    self.value(format!(
                        "call ptr @tz.rec.clone.enqueue(ptr {value}, ptr {pending})"
                    ))
                } else {
                    self.value(format!("call ptr @tz.rec.clone(ptr {value})"))
                }
            }
            Type::Vec(element) => self.clone_vector(element, value),
            Type::Shared(shared, kind) => self.clone_shared(shared, *kind, value),
            Type::Task(_) => unreachable!("single-use tasks cannot be cloned"),
            Type::Handle(_) => unreachable!("extern handles cannot be cloned"),
            // A14 Phase 2: only a `Copy` dyn value has a clone slot.
            Type::Dyn(dyn_type) => {
                assert!(dyn_type.copy, "dyn values without Copy cannot be cloned");
                self.value(format!("call %tz.dyn @tz.dyn.clone(%tz.dyn {value})"))
            }
            Type::String | Type::Utf8String => {
                let ty = self.ty(ty);
                let pointer = self.value(format!("extractvalue {ty} {value}, 0"));
                let length = self.value(format!("extractvalue {ty} {value}, 1"));
                self.value(format!(
                    "call {ty} @{}.new(ptr {pointer}, i64 {length})",
                    &ty[1..]
                ))
            }
            Type::Function(..) => self.value(format!(
                "call %tz.closure @tz.closure.clone(%tz.closure {value})"
            )),
            Type::Record(id, arguments) => {
                let mut result = value.to_owned();
                let fields = self.module.types().record_fields(*id, arguments);
                for (index, field) in fields.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let field_value =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        let copy = self.clone_value(field, &field_value);
                        result = self.value(format!(
                            "insertvalue {} {result}, {} {copy}, {index}",
                            self.ty(ty),
                            self.ty(field)
                        ));
                    }
                }
                result
            }
            Type::Tuple(elements) => {
                let mut result = value.to_owned();
                for (index, field) in elements.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let extracted =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        let copy = self.clone_value(field, &extracted);
                        result = self.value(format!(
                            "insertvalue {} {result}, {} {copy}, {index}",
                            self.ty(ty),
                            self.ty(field)
                        ));
                    }
                }
                result
            }
            Type::Union(..) => {
                let cases = self.owning_cases(ty);
                if cases.is_empty() {
                    return value.to_owned();
                }
                // The copy starts as the original and receives a cloned payload in place.
                let slot = self.spill(ty, value);
                let (labels, done) = self.case_switch(ty, value, &cases);
                for ((_, payload), label) in cases.into_iter().zip(labels) {
                    self.begin(&label);
                    let pointer = self.payload_pointer(ty, &slot);
                    let llvm = self.ty(&payload);
                    let original = self.value(format!("load {llvm}, ptr {pointer}"));
                    let copy = self.clone_value(&payload, &original);
                    self.instruction(format!("store {llvm} {copy}, ptr {pointer}"));
                    self.jump(&done);
                }
                self.begin(&done);
                self.value(format!("load {}, ptr {slot}", self.ty(ty)))
            }
            Type::Array(element) => {
                let data = self.value(format!("extractvalue %tz.array {value}, 0"));
                let length = self.value(format!("extractvalue %tz.array {value}, 1"));
                let (result, target) = self.allocate_array(element, &length);
                self.array_loop(&length, |emitter, index| {
                    let source = emitter.element_pointer(element, &data, index);
                    let element_value =
                        emitter.value(format!("load {}, ptr {source}", emitter.ty(element)));
                    let copy = emitter.clone_value(element, &element_value);
                    let destination = emitter.element_pointer(element, &target, index);
                    emitter.instruction(format!(
                        "store {} {copy}, ptr {destination}",
                        emitter.ty(element)
                    ));
                });
                result
            }
            // A16: a Copy fixed-length array is an SSA value copy; owned elements clone in a loop.
            Type::FixedArray(element, _) if element.needs_drop(&self.module.types()) => {
                let source = self.spill(ty, value);
                let target = self.slot(ty);
                self.array_loop(&fixed_length(ty).to_string(), |emitter, index| {
                    let from = emitter.fixed_pointer(ty, &source, index);
                    let llvm = emitter.ty(element);
                    let original = emitter.value(format!("load {llvm}, ptr {from}"));
                    let copy = emitter.clone_value(element, &original);
                    let to = emitter.fixed_pointer(ty, &target, index);
                    emitter.instruction(format!("store {llvm} {copy}, ptr {to}"));
                });
                self.value(format!("load {}, ptr {target}", self.ty(ty)))
            }
            Type::List(element) => {
                let source = self.value(format!("extractvalue %tz.list {value}, 0"));
                let length = self.value(format!("extractvalue %tz.list {value}, 1"));
                let (head, tail) = self.list_builder();
                self.list_loop(&source, &length, |emitter, node| {
                    let pointer = emitter.list_element_pointer(element, node);
                    let value =
                        emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                    let copy = emitter.clone_value(element, &value);
                    emitter.append_list(element, &tail, &copy);
                });
                self.finish_list(&head, &length)
            }
            _ => value.to_owned(),
        }
    }

    fn union_layout(&self, ty: &Type) -> UnionLayout {
        let Type::Union(id, arguments) = ty else {
            unreachable!("union layouts belong to union types")
        };
        union_layout(*id, arguments, self.module)
    }

    /// The payload storage of the union stored at `slot`, typed by each case's load or store.
    fn payload_pointer(&mut self, ty: &Type, slot: &str) -> String {
        if self.module.types().recursive(ty) {
            let node = self.value(format!("load ptr, ptr {slot}"));
            return self.recursive_payload(ty, &node);
        }
        self.value(format!(
            "getelementptr inbounds {}, ptr {slot}, i32 0, i32 1",
            self.ty(ty)
        ))
    }

    fn construct(&mut self, ty: &Type, case_id: usize, payload: Option<&TypedExpr>) -> String {
        let payload = payload.map(|payload| (self.expression(payload), self.ty(&payload.ty)));
        self.construct_value(ty, case_id, payload)
    }

    fn construct_value(
        &mut self,
        ty: &Type,
        case_id: usize,
        payload: Option<(String, String)>,
    ) -> String {
        if self.module.types().recursive(ty) {
            return self.recursive_construct(ty, case_id, payload);
        }
        let layout = self.union_layout(ty);
        if matches!(layout, UnionLayout::Enum) {
            return case_id.to_string();
        }
        let llvm = self.ty(ty);
        let tagged = self.value(format!(
            "insertvalue {llvm} zeroinitializer, i32 {case_id}, 0"
        ));
        let tagged = self.mark_live(ty, tagged);
        let Some((value, payload_type)) = payload else {
            return tagged;
        };
        if let UnionLayout::Common(_) = layout {
            return self.value(format!(
                "insertvalue {llvm} {tagged}, {payload_type} {value}, 1"
            ));
        }
        let slot = self.spill(ty, &tagged);
        let pointer = self.payload_pointer(ty, &slot);
        self.instruction(format!("store {payload_type} {value}, ptr {pointer}"));
        self.value(format!("load {llvm}, ptr {slot}"))
    }

    fn union_tag(&mut self, value: &TypedExpr) -> String {
        if self.module.types().recursive(&value.ty) {
            let (union, frames) = self.read_operand(value);
            let tag = self.recursive_tag(&value.ty, &union);
            self.release_operand(value, &union, &frames);
            return tag;
        }
        if Self::is_place(value) {
            // The tag is the first field, so it is also the value of an enum-like union.
            let slot = self.place(value);
            return self.value(format!("load i32, ptr {slot}"));
        }
        let (union, frames) = self.read_operand(value);
        let tag = if matches!(self.union_layout(&value.ty), UnionLayout::Enum) {
            union.clone()
        } else {
            self.value(format!("extractvalue {} {union}, 0", self.ty(&value.ty)))
        };
        self.release_operand(value, &union, &frames);
        tag
    }

    fn payload_value(&mut self, ty: &Type, union: &str, payload: &Type) -> String {
        if self.module.types().recursive(ty) {
            let pointer = self.recursive_payload(ty, union);
            return self.value(format!("load {}, ptr {pointer}", self.ty(payload)));
        }
        match self.union_layout(ty) {
            UnionLayout::Enum => unreachable!("nullary cases have no payload"),
            UnionLayout::Common(_) => {
                self.value(format!("extractvalue {} {union}, 1", self.ty(ty)))
            }
            UnionLayout::General(_) => {
                let slot = self.spill(ty, union);
                let pointer = self.payload_pointer(ty, &slot);
                self.value(format!("load {}, ptr {pointer}", self.ty(payload)))
            }
        }
    }

    /// Cases whose payload owns resources, by tag.
    fn owning_cases(&self, ty: &Type) -> Vec<(usize, Type)> {
        let Type::Union(id, arguments) = ty else {
            unreachable!("union cases belong to union types")
        };
        let types = self.module.types();
        types
            .union_payloads(*id, arguments)
            .into_iter()
            .enumerate()
            .filter_map(|(case, payload)| Some((case, payload.filter(|ty| ty.needs_drop(&types))?)))
            .collect()
    }

    /// Branches on the tag of `value` to one new block per case; other tags reach the returned
    /// join block.
    fn case_switch(
        &mut self,
        ty: &Type,
        value: &str,
        cases: &[(usize, Type)],
    ) -> (Vec<String>, String) {
        let tag = if self.module.types().recursive(ty) {
            self.recursive_tag(ty, value)
        } else {
            self.value(format!("extractvalue {} {value}, 0", self.ty(ty)))
        };
        let labels: Vec<_> = cases.iter().map(|_| self.label()).collect();
        let done = self.label();
        self.instruction(format!("switch i32 {tag}, label %{done} ["));
        for ((case, _), label) in cases.iter().zip(&labels) {
            self.instruction(format!("  i32 {case}, label %{label}"));
        }
        self.instruction("]");
        (labels, done)
    }

    fn allocate_array(&mut self, element: &Type, length: &str) -> (String, String) {
        let bytes = self.allocation_size(&self.ty(element), length);
        let empty = self.value(format!("icmp eq i64 {length}, 0"));
        let bytes = self.value(format!("select i1 {empty}, i64 1, i64 {bytes}"));
        let data = self.value(format!("call ptr @tz.alloc(i64 {bytes})"));
        let array = self.value(format!(
            "insertvalue %tz.array zeroinitializer, ptr {data}, 0"
        ));
        let array = self.value(format!("insertvalue %tz.array {array}, i64 {length}, 1"));
        (array, data)
    }

    fn allocation_size(&mut self, ty: &str, length: &str) -> String {
        let size = format!("ptrtoint (ptr getelementptr ({ty}, ptr null, i32 1) to i64)");
        let zero = self.value(format!("icmp eq i64 {size}, 0"));
        let stride = self.value(format!("select i1 {zero}, i64 1, i64 {size}"));
        let limit = self.value(format!("udiv i64 {}, {stride}", i64::MAX));
        // Unsigned comparison rejects negative lengths as well as byte-size overflow.
        let valid = self.value(format!("icmp ule i64 {length}, {limit}"));
        self.guard(&valid, TrapKind::AllocationSize);
        self.value(format!("mul i64 {length}, {stride}"))
    }

    fn element_pointer(&mut self, element: &Type, data: &str, index: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {data}, i64 {index}",
            self.ty(element)
        ))
    }

    fn array_slice(
        &mut self,
        source: &TypedExpr,
        start: Option<&TypedExpr>,
        end: Option<&TypedExpr>,
    ) -> String {
        let view = if matches!(source.ty, Type::FixedArray(..)) {
            // A16: a fixed-length array place is viewed in place as `{ ptr, N }`.
            let base = self.place(source);
            let view = self.value(format!(
                "insertvalue %tz.array zeroinitializer, ptr {base}, 0"
            ));
            self.value(format!(
                "insertvalue %tz.array {view}, i64 {}, 1",
                fixed_length(&source.ty)
            ))
        } else {
            self.expression_mode(source, false)
        };
        let length = self.value(format!("extractvalue %tz.array {view}, 1"));
        let start = start.map_or_else(|| "0".into(), |start| self.expression(start));
        let end = end.map_or_else(|| length.clone(), |end| self.expression(end));
        let ordered = self.value(format!("icmp ule i64 {start}, {end}"));
        let bounded = self.value(format!("icmp ule i64 {end}, {length}"));
        let valid = self.value(format!("and i1 {ordered}, {bounded}"));
        self.guard(&valid, TrapKind::BoundsCheck);
        let data = self.value(format!("extractvalue %tz.array {view}, 0"));
        let (Type::Array(element) | Type::FixedArray(element, _)) = &source.ty else {
            unreachable!("slice source type checked")
        };
        let pointer = self.element_pointer(element, &data, &start);
        let length = self.value(format!("sub i64 {end}, {start}"));
        let view = self.value(format!(
            "insertvalue %tz.array zeroinitializer, ptr {pointer}, 0"
        ));
        self.value(format!("insertvalue %tz.array {view}, i64 {length}, 1"))
    }

    /// `Dyn.of value` (A14 D3): moves the value into a new allocation and pairs it with the
    /// vtable of its type. A dyn value whose type specialization chose to upcast keeps its data
    /// and takes the target's vtable from its own vtable's upcast table (Phase 2).
    fn dyn_of(&mut self, instance: &BuiltinInstance, argument: &TypedExpr) -> String {
        let [ty, Type::Dyn(target)] = instance.types.as_slice() else {
            unreachable!("Dyn.of stores a value in a dyn type")
        };
        let key = target.vtable_key();
        let value = self.expression(argument);
        if self.module.vtables.contains_key(&(key.clone(), ty.clone())) {
            let size = storage_layout(ty, self.module).0.max(1);
            let cell = self.value(format!("call ptr @tz.alloc(i64 {size})"));
            self.instruction(format!("store {} {value}, ptr {cell}", self.ty(ty)));
            let pair = self.value(format!(
                "insertvalue %tz.dyn zeroinitializer, ptr {cell}, 0"
            ));
            let vtable = dyn_vtable_symbol(&key, ty, self.module);
            return self.value(format!("insertvalue %tz.dyn {pair}, ptr {vtable}, 1"));
        }
        let Type::Dyn(source) = ty else {
            unreachable!("only a dyn value is stored without a vtable of its type")
        };
        let source = source.vtable_key();
        if source == key {
            return value;
        }
        let layout = &self.module.dyn_layouts[&source];
        let index = layout
            .upcasts
            .iter()
            .position(|candidate| *candidate == key)
            .expect("an upcast is in its source's table");
        let table = dyn_vtable_type(layout);
        let vtable = self.value(format!("extractvalue %tz.dyn {value}, 1"));
        let pointer = self.value(format!(
            "getelementptr inbounds {table}, ptr {vtable}, i32 0, i32 {}, i64 {index}",
            DYN_SLOTS + 1
        ));
        let upcast = self.value(format!("load ptr, ptr {pointer}"));
        self.value(format!("insertvalue %tz.dyn {value}, ptr {upcast}, 1"))
    }

    /// `FixedArray.init` (A16): stores `element(index)` for index 0, 1, ..., N - 1 into a slot of
    /// the array type `ty` and reads the filled array back.
    fn fill_fixed_array(
        &mut self,
        ty: &Type,
        mut element: impl FnMut(&mut Self, &str) -> String,
    ) -> String {
        let Type::FixedArray(element_type, _) = ty else {
            unreachable!("FixedArray.init creates a fixed-length array")
        };
        let slot = self.slot(ty);
        self.array_loop(&fixed_length(ty).to_string(), |emitter, index| {
            let value = element(emitter, index);
            let pointer = emitter.fixed_pointer(ty, &slot, index);
            emitter.instruction(format!(
                "store {} {value}, ptr {pointer}",
                emitter.ty(element_type)
            ));
        });
        self.value(format!("load {}, ptr {slot}", self.ty(ty)))
    }

    /// The address of a fixed-length array operand (A16): a place stays where it is, and any other
    /// value is spilled, returned with its operand to release after use.
    fn fixed_base(&mut self, array: &TypedExpr) -> (String, Option<(String, Vec<Frame>)>) {
        if Self::is_place(array) {
            return (self.place(array), None);
        }
        let (value, frames) = self.read_operand(array);
        let slot = self.spill(&array.ty, &value);
        (slot, Some((value, frames)))
    }

    /// The address of element `index` of the `[N x T]` at `base`, without a bounds check.
    fn fixed_pointer(&mut self, ty: &Type, base: &str, index: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {base}, i64 0, i64 {index}",
            self.ty(ty)
        ))
    }

    /// The checked address of element `position` (the value of `index`) of the `[N x T]` at
    /// `base`. An integer literal index below N needs no check (A16 D6); any other index, even a
    /// literal past the end, traps at run time.
    fn fixed_element_pointer(
        &mut self,
        ty: &Type,
        base: &str,
        index: &TypedExpr,
        position: &str,
    ) -> String {
        let length = fixed_length(ty);
        let constant = matches!(index.kind, TypedExprKind::Int(value) if value < length as u128);
        if !constant {
            let valid = self.value(format!("icmp ult i64 {position}, {length}"));
            self.guard(&valid, TrapKind::BoundsCheck);
        }
        self.fixed_pointer(ty, base, position)
    }

    fn checked_element_pointer(&mut self, ty: &Type, collection: &str, index: &str) -> String {
        let length = self.value(format!("extractvalue {} {collection}, 1", self.ty(ty)));
        let valid = self.value(format!("icmp ult i64 {index}, {length}"));
        self.guard(&valid, TrapKind::BoundsCheck);
        let data = self.value(format!("extractvalue {} {collection}, 0", self.ty(ty)));
        match ty {
            Type::Array(element) | Type::Vec(element) => {
                self.element_pointer(element, &data, index)
            }
            Type::List(element) => {
                let node = self.list_loop(&data, index, |_, _| {});
                self.list_element_pointer(element, &node)
            }
            _ => unreachable!("indexing requires a collection"),
        }
    }

    /// The element address of an index that `ranges` proved in bounds, else the checked one.
    fn indexed_pointer(
        &mut self,
        proven: bool,
        ty: &Type,
        collection: &str,
        index: &str,
    ) -> String {
        if !proven {
            return self.checked_element_pointer(ty, collection, index);
        }
        let Type::Array(element) = ty else {
            unreachable!("range facts only prove arrays")
        };
        self.proven_reads += 1;
        let data = self.value(format!("extractvalue {} {collection}, 0", self.ty(ty)));
        self.element_pointer(element, &data, index)
    }

    fn list_node_type(&self, element: &Type) -> String {
        format!("{{ ptr, {} }}", self.ty(element))
    }

    fn list_element_pointer(&mut self, element: &Type, node: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {node}, i32 0, i32 1",
            self.list_node_type(element)
        ))
    }

    fn list_builder(&mut self) -> (String, String) {
        let pointer = Type::Reference(Box::new(Type::Unit), false);
        let head = self.slot(&pointer);
        let tail = self.slot(&pointer);
        self.instruction(format!("store ptr null, ptr {head}"));
        self.instruction(format!("store ptr {head}, ptr {tail}"));
        (head, tail)
    }

    fn append_list(&mut self, element: &Type, tail: &str, value: &str) {
        let node = self.value(format!(
            "call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64))",
            self.list_node_type(element)
        ));
        self.instruction(format!("store ptr null, ptr {node}"));
        let pointer = self.list_element_pointer(element, &node);
        self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
        let previous = self.value(format!("load ptr, ptr {tail}"));
        self.instruction(format!("store ptr {node}, ptr {previous}"));
        // The first field of each node is the next pointer, also used as the builder's tail slot.
        self.instruction(format!("store ptr {node}, ptr {tail}"));
    }

    fn finish_list(&mut self, head: &str, length: &str) -> String {
        let head = self.value(format!("load ptr, ptr {head}"));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {head}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    fn list_loop(
        &mut self,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str),
    ) -> String {
        self.list_loop_control(head, length, |emitter, node, _, _| body(emitter, node))
    }

    fn list_loop_control(
        &mut self,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str, &str, &str),
    ) -> String {
        let entry = self.block.clone();
        let condition = self.label();
        let element = self.label();
        let advance = self.label();
        let exit = self.label();
        let index = self.fresh();
        let next_index = self.fresh();
        let node = self.fresh();
        let next_node = self.fresh();
        self.jump(&condition);
        self.begin(&condition);
        self.instruction(format!(
            "{index} = phi i64 [ 0, %{entry} ], [ {next_index}, %{advance} ]"
        ));
        self.instruction(format!(
            "{node} = phi ptr [ {head}, %{entry} ], [ {next_node}, %{advance} ]"
        ));
        let more = self.value(format!("icmp ult i64 {index}, {length}"));
        self.branch(&more, &element, &exit);
        self.begin(&element);
        // Read the link before the callback, which may free this node.
        self.instruction(format!("{next_node} = load ptr, ptr {node}"));
        body(self, &node, &exit, &advance);
        self.jump(&advance);
        self.begin(&advance);
        self.instruction(format!("{next_index} = add i64 {index}, 1"));
        self.jump(&condition);
        self.begin(&exit);
        node
    }

    fn array_loop(&mut self, length: &str, body: impl FnOnce(&mut Self, &str)) {
        self.array_loop_control(length, |emitter, index, _, _| body(emitter, index));
    }

    fn array_loop_control(&mut self, length: &str, body: impl FnOnce(&mut Self, &str, &str, &str)) {
        let entry = self.block.clone();
        let condition = self.label();
        let element = self.label();
        let advance = self.label();
        let exit = self.label();
        let index = self.fresh();
        let next = self.fresh();
        self.jump(&condition);
        self.begin(&condition);
        self.instruction(format!(
            "{index} = phi i64 [ 0, %{entry} ], [ {next}, %{advance} ]"
        ));
        let more = self.value(format!("icmp ult i64 {index}, {length}"));
        self.branch(&more, &element, &exit);
        self.begin(&element);
        body(self, &index, &exit, &advance);
        self.jump(&advance);
        self.begin(&advance);
        self.instruction(format!("{next} = add i64 {index}, 1"));
        self.jump(&condition);
        self.begin(&exit);
    }

    fn closure_capture_value(
        &mut self,
        function: &CheckedFunction,
        count: usize,
        index: usize,
        environment: &str,
    ) -> String {
        let ty = &function.signature.parameters[index];
        if immediate_capture(function, count, self.globals) {
            let integer = match ty {
                Type::Binary(bits) => format!("i{bits}"),
                _ => self.ty(ty),
            };
            let value = self.value(format!("ptrtoint ptr {environment} to {integer}"));
            if matches!(ty, Type::Binary(_)) {
                self.value(format!("bitcast {integer} {value} to {}", self.ty(ty)))
            } else {
                value
            }
        } else {
            let environment_ty = environment_type(function, count, self.module);
            let pointer = self.value(format!(
                "getelementptr inbounds {environment_ty}, ptr {environment}, i32 0, i32 {index}"
            ));
            self.value(format!("load {}, ptr {pointer}", self.ty(ty)))
        }
    }

    fn make_closure(&mut self, id: usize, values: &[String]) -> String {
        let function = &self.module.functions[id];
        let count = values.len();
        let environment = if count == 0 {
            "null".to_owned()
        } else if immediate_capture(function, count, self.globals) {
            let ty = &function.signature.parameters[0];
            let (integer, value) = if let Type::Binary(bits) = ty {
                let integer = format!("i{bits}");
                let value = self.value(format!(
                    "bitcast {} {} to {integer}",
                    self.ty(ty),
                    values[0]
                ));
                (integer, value)
            } else {
                (self.ty(ty), values[0].clone())
            };
            self.value(format!("inttoptr {integer} {value} to ptr"))
        } else {
            let ty = environment_type(function, count, self.module);
            let environment = self.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({ty}, ptr null, i32 1) to i64))"));
            for (index, value) in values.iter().enumerate() {
                let pointer = self.value(format!(
                    "getelementptr inbounds {ty}, ptr {environment}, i32 0, i32 {index}"
                ));
                self.instruction(format!(
                    "store {} {value}, ptr {pointer}",
                    self.ty(&function.signature.parameters[index])
                ));
            }
            environment
        };
        self.closure_descriptor(id, count, &environment)
    }

    fn closure_descriptor(&mut self, id: usize, count: usize, environment: &str) -> String {
        let function = &self.module.functions[id];
        let name = function.qualified_name();
        let value = self.value(format!(
            "insertvalue %tz.closure zeroinitializer, ptr @tz.apply.{name}.{count}, 0"
        ));
        let value = self.value(format!(
            "insertvalue %tz.closure {value}, ptr {environment}, 1"
        ));
        if count == 0 {
            return value;
        }
        if immediate_capture(function, count, self.globals) {
            let value = self.value(format!(
                "insertvalue %tz.closure {value}, ptr @tz.closure.immediate.clone, 2"
            ));
            return self.value(format!(
                "insertvalue %tz.closure {value}, ptr @tz.closure.immediate.drop, 3"
            ));
        }
        let value = if !function.is_task
            && !function.owned_captures
            && function.signature.parameters[..count]
                .iter()
                .all(|ty| ty.can_capture(&self.module.types()))
        {
            self.value(format!(
                "insertvalue %tz.closure {value}, ptr @tz.env.clone.{name}.{count}, 2"
            ))
        } else {
            value
        };
        self.value(format!(
            "insertvalue %tz.closure {value}, ptr @tz.env.drop.{name}.{count}, 3"
        ))
    }

    fn stack_closure(&mut self, target: ClosureTarget, values: &[String]) -> String {
        if values.is_empty() {
            return self.closure_descriptor(target.function, 0, "null");
        }
        let function = &self.module.functions[target.function];
        if immediate_capture(function, target.bound, self.globals) {
            return self.make_closure(target.function, values);
        }
        let environment_ty = environment_type(function, target.bound, self.module);
        let environment = self.fresh();
        self.allocas
            .push(format!("{environment} = alloca {environment_ty}, align 16"));
        for (index, value) in values.iter().enumerate() {
            let pointer = self.value(format!(
                "getelementptr inbounds {environment_ty}, ptr {environment}, i32 0, i32 {index}"
            ));
            self.instruction(format!(
                "store {} {value}, ptr {pointer}",
                self.ty(&function.signature.parameters[index])
            ));
        }
        self.closure_descriptor(target.function, target.bound, &environment)
    }

    fn parallel_tasks(&mut self, tasks: &TypedExpr, result: &Type) -> String {
        let tasks = self.expression(tasks);
        let source = self.value(format!("extractvalue %tz.array {tasks}, 0"));
        let length = self.value(format!("extractvalue %tz.array {tasks}, 1"));
        let Type::Array(element) = result else {
            unreachable!()
        };
        let (array, target) = self.allocate_array(element, &length);
        let ty = self.ty(element);
        let callback = format!("@tz.task.item.{}", ty.trim_start_matches('%'));
        if let Some(marks) = &mut self.globals.traps {
            marks
                .sources
                .entry(callback.clone())
                .or_insert((self.current_span, false));
        }
        self.intrinsics.insert(format!(
            "define internal void {callback}(ptr %context, i64 %index) nounwind {{\n\
             entry:\n\
               %source = load ptr, ptr %context\n\
               %target.slot = getelementptr inbounds {{ ptr, ptr }}, ptr %context, i32 0, i32 1\n\
               %target = load ptr, ptr %target.slot\n\
               %slot = getelementptr inbounds %tz.closure, ptr %source, i64 %index\n\
               %task = load %tz.closure, ptr %slot\n\
               %code = extractvalue %tz.closure %task, 0\n\
               %env = extractvalue %tz.closure %task, 1\n\
               %result = call {ty} %code(ptr %env)\n\
               %destination = getelementptr inbounds {ty}, ptr %target, i64 %index\n\
               store {ty} %result, ptr %destination\n\
               ret void\n\
             }}\n"
        ));
        let context = self.fresh();
        self.allocas
            .push(format!("{context} = alloca {{ ptr, ptr }}, align 16"));
        self.instruction(format!("store ptr {source}, ptr {context}"));
        let slot = self.value(format!(
            "getelementptr inbounds {{ ptr, ptr }}, ptr {context}, i32 0, i32 1"
        ));
        self.instruction(format!("store ptr {target}, ptr {slot}"));
        self.instruction(format!(
            "call void @tsuzuri_task_parallel(ptr {callback}, ptr {context}, i64 {length})"
        ));
        self.instruction(format!("call void @tz.free(ptr {source})"));
        array
    }

    fn apply_value(
        &mut self,
        callee: &str,
        ty: &Type,
        argument: Option<(&Type, &str)>,
        borrowed: bool,
    ) -> (String, Type) {
        let code = self.value(format!("extractvalue %tz.closure {callee}, 0"));
        let environment = self.value(format!("extractvalue %tz.closure {callee}, 1"));
        let result = ty.after_arguments(usize::from(argument.is_some()));
        let argument = argument.map_or_else(String::new, |(ty, value)| {
            format!("{} {value}, ", self.ty(ty))
        });
        let value = self.value(format!(
            "call {} {code}({argument}ptr {environment}, i1 {borrowed})",
            self.ty(&result)
        ));
        (value, result)
    }

    fn comparison_call(&mut self, callee: &TypedExpr, arguments: &[TypedExpr]) -> String {
        let direct = match callee.kind {
            TypedExprKind::Function(FunctionRef::User(id)) => Some(id),
            _ => None,
        };
        let borrowed = Self::is_place(callee);
        let mut function = direct
            .is_none()
            .then(|| self.expression_mode(callee, !borrowed));
        let mut cleanup = Vec::new();
        let mut values = Vec::new();
        for argument in arguments {
            let value = if let TypedExprKind::BorrowOperand(operand) = &argument.kind {
                self.operand_borrow(argument, operand, &mut cleanup)
            } else {
                self.expression(argument)
            };
            values.push(value);
        }
        let result = if direct.is_some() {
            self.comparison_values(callee, &values)
        } else {
            let mut ty = callee.ty.clone();
            for (index, (argument, value)) in arguments.iter().zip(&values).enumerate() {
                let (result, result_type) = self.apply_value(
                    function.as_ref().unwrap(),
                    &ty,
                    Some((&argument.ty, value)),
                    borrowed && index == 0,
                );
                function = Some(result);
                ty = result_type;
            }
            function.unwrap()
        };
        self.release_operands(cleanup);
        result
    }

    /// The borrow of a `BorrowOperand`: a place is borrowed where it is and
    /// any other value is spilled to a slot that `release_operands` drops.
    fn operand_borrow(
        &mut self,
        argument: &TypedExpr,
        operand: &TypedExpr,
        cleanup: &mut Vec<(Type, String, String, Vec<Frame>)>,
    ) -> String {
        if argument.ty.slice_element().is_some() {
            if Self::is_place(operand) {
                self.expression_mode(operand, false)
            } else {
                let (value, frames) = self.frame_value(operand);
                let slot = self.spill(&operand.ty, &value);
                cleanup.push((operand.ty.clone(), slot, value.clone(), frames));
                value
            }
        } else if Self::is_place(operand) {
            self.place(operand)
        } else {
            let (value, frames) = self.frame_value(operand);
            let slot = self.spill(&operand.ty, &value);
            cleanup.push((operand.ty.clone(), slot.clone(), value, frames));
            slot
        }
    }

    fn release_operands(&mut self, cleanup: Vec<(Type, String, String, Vec<Frame>)>) {
        for (ty, slot, value, frames) in cleanup {
            self.drop_framed(&ty, &value, &frames);
            self.instruction(format!(
                "store {} zeroinitializer, ptr {slot}",
                self.ty(&ty)
            ));
        }
    }

    fn call(&mut self, callee: &TypedExpr, arguments: &[TypedExpr]) -> String {
        if arguments
            .iter()
            .any(|argument| matches!(argument.kind, TypedExprKind::BorrowOperand(_)))
        {
            return self.comparison_call(callee, arguments);
        }
        // A14: `Dyn.of value` stores the value with its vtable, or upcasts a dyn value.
        if let TypedExprKind::Function(FunctionRef::Builtin(instance)) = &callee.kind
            && instance.builtin == Builtin::DynOf
            && let [value] = arguments
        {
            return self.dyn_of(instance, value);
        }
        // A16: a direct `FixedArray.init f` fills the array in place, calling a known `f` directly.
        if let TypedExprKind::Function(FunctionRef::Builtin(instance)) = &callee.kind
            && instance.builtin == Builtin::FixedArrayInit
            && let [initializer] = arguments
        {
            let ty = callee.ty.after_arguments(1);
            let direct = self.prepare_known_call(initializer, 1);
            let function = direct.is_none().then(|| self.expression(initializer));
            let array = self.fill_fixed_array(&ty, |emitter, index| match &direct {
                Some(call) => emitter.emit_borrowed_call(call, &[index.to_owned()]),
                None => {
                    emitter
                        .apply_value(
                            function.as_ref().unwrap(),
                            &initializer.ty,
                            Some((&Type::I64, index)),
                            true,
                        )
                        .0
                }
            });
            match &direct {
                Some(call) => self.finish_borrowed_call(call),
                None => self.drop_value(&initializer.ty, function.as_ref().unwrap()),
            }
            return array;
        }
        if let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind {
            let function = &self.module.functions[id];
            if let TypedExprKind::HostCall(import, _) = &function.body.kind
                && import.callbacks
            {
                return self.host_call(import, arguments, &function.signature.result);
            }
            if arguments.len() == 1 && call_specialization::is_identity(function) {
                let value = self.expression(&arguments[0]);
                self.cover_call(id);
                return value;
            }
        }
        if !matches!(callee.kind, TypedExprKind::Function(_)) {
            if let Some(target) =
                call_specialization::target(callee, &self.known_closures, self.module)
            {
                if target.bound + arguments.len()
                    == self.module.functions[target.function].parameters.len()
                    && !arguments
                        .iter()
                        .any(|argument| call_specialization::may_mutate(argument, self.module))
                    && self.specializations.can_borrow(target, self.module)
                {
                    if let Some(value) = self.borrowed_call(callee, target, arguments) {
                        return value;
                    }
                }
            }
        }
        let known = match &callee.kind {
            &TypedExprKind::Function(FunctionRef::User(id)) => {
                let function = &self.module.functions[id];
                if arguments.len() < function.parameters.len() {
                    let values: Vec<_> = arguments
                        .iter()
                        .map(|argument| self.expression(argument))
                        .collect();
                    return self.make_closure(id, &values);
                }
                let mut callbacks = Vec::new();
                if arguments.len() == function.parameters.len() {
                    for index in self.specializations.eligible[id].clone() {
                        if arguments[index + 1..]
                            .iter()
                            .any(|argument| call_specialization::may_mutate(argument, self.module))
                        {
                            continue;
                        }
                        if let Some(target) = call_specialization::target(
                            &arguments[index],
                            &self.known_closures,
                            self.module,
                        ) {
                            if self.specializations.can_borrow(target, self.module) {
                                callbacks.push((index, target));
                            }
                        }
                    }
                }
                let symbol = if callbacks.is_empty() {
                    format!("@tz.fn.{}", function.qualified_name())
                } else if let Some(variant) = self.specializations.request(Specialization {
                    function: id,
                    callbacks: callbacks.clone(),
                    borrowed: 0,
                }) {
                    format!("@tz.specialized.{variant}")
                } else {
                    callbacks.clear();
                    format!("@tz.fn.{}", function.qualified_name())
                };
                Some((symbol, function.signature.clone(), callbacks))
            }
            TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                let count = instance.builtin.scheme().parameters.len();
                debug_assert!(
                    count <= arguments.len(),
                    "builtin wrappers apply every parameter"
                );
                let Type::Function(parameters, _) = &callee.ty else {
                    unreachable!("a builtin is a function")
                };
                let signature = crate::check::Signature {
                    parameters: parameters[..count].to_vec(),
                    result: callee.ty.after_arguments(count),
                };
                self.builtins
                    .entry(instance.clone())
                    .or_insert_with(|| callee.ty.clone());
                Some((builtin_symbol(instance, self.module), signature, Vec::new()))
            }
            _ => None,
        };
        let mut borrowed = false;
        let (mut value, mut ty, consumed) = if let Some((symbol, signature, callbacks)) = known {
            let temporary_base = self.temporaries.len();
            let count = signature.parameters.len();
            let mut values = Vec::new();
            let mut cleanup = Vec::new();
            for (index, argument) in arguments[..count].iter().enumerate() {
                let value =
                    if let Some((_, target)) = callbacks.iter().find(|(slot, _)| *slot == index) {
                        let argument = self.skip_identity_calls(argument);
                        if Self::is_place(argument) {
                            self.expression_mode(argument, false)
                        } else {
                            let captures = self.capture_values(argument);
                            self.capture_cleanup(*target, &captures, &mut cleanup);
                            let captures: Vec<_> =
                                captures.into_iter().map(|(value, _)| value).collect();
                            self.stack_closure(*target, &captures)
                        }
                    } else {
                        self.expression(argument)
                    };
                values.push(format!("{} {value}", self.ty(&argument.ty)));
            }
            let values = values.join(", ");
            let value = self.value(format!(
                "call {} {symbol}({values})",
                self.ty(&signature.result)
            ));
            for (ty, value, frames) in cleanup.iter().rev() {
                self.drop_framed(ty, value, frames);
            }
            self.temporaries.truncate(temporary_base);
            self.remember_temporary(&signature.result, &value, &[]);
            (value, signature.result, count)
        } else {
            borrowed = Self::is_place(callee)
                && !arguments
                    .iter()
                    .any(|argument| call_specialization::may_mutate(argument, self.module));
            let value = self.expression_mode(callee, !borrowed);
            if arguments.is_empty() {
                return self.apply_value(&value, &callee.ty, None, borrowed).0;
            }
            (value, callee.ty.clone(), 0)
        };
        for argument in &arguments[consumed..] {
            let next = self.expression(argument);
            let callee = value.clone();
            (value, ty) = self.apply_value(&value, &ty, Some((&argument.ty, &next)), borrowed);
            self.forget_temporary(&next);
            if !borrowed {
                self.forget_temporary(&callee);
            }
            self.remember_temporary(&ty, &value, &[]);
            borrowed = false;
        }
        value
    }

    fn capture_values(&mut self, expression: &TypedExpr) -> Vec<(String, Vec<Frame>)> {
        let expression = self.skip_identity_calls(expression);
        match &expression.kind {
            // Known temporary closures only lend their captures to a borrowing worker; the caller
            // drops them after the call, so stack values need not move to the heap.
            TypedExprKind::Closure(_, values) | TypedExprKind::Call(_, values) => values
                .iter()
                .map(|value| self.take_operand(value))
                .collect(),
            TypedExprKind::Function(_) => Vec::new(),
            _ => unreachable!("known temporary closures have explicit captures"),
        }
    }

    fn capture_cleanup(
        &self,
        target: ClosureTarget,
        captures: &[(String, Vec<Frame>)],
        cleanup: &mut Vec<(Type, String, Vec<Frame>)>,
    ) {
        for ((value, frames), ty) in captures
            .iter()
            .zip(&self.module.functions[target.function].signature.parameters[..target.bound])
        {
            if ty.needs_drop(&self.module.types()) {
                cleanup.push((ty.clone(), value.clone(), frames.clone()));
            }
        }
    }

    fn borrowed_call(
        &mut self,
        callee: &TypedExpr,
        target: ClosureTarget,
        arguments: &[TypedExpr],
    ) -> Option<String> {
        let call = self.prepare_borrowed_call(callee, target)?;
        let arguments: Vec<_> = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect();
        let result = self.emit_borrowed_call(&call, &arguments);
        self.finish_borrowed_call(&call);
        Some(result)
    }

    fn prepare_known_call(&mut self, expression: &TypedExpr, arity: usize) -> Option<BorrowedCall> {
        let target = call_specialization::target(expression, &self.known_closures, self.module)?;
        if target.bound + arity != self.module.functions[target.function].parameters.len()
            || !self.specializations.can_borrow(target, self.module)
        {
            return None;
        }
        self.prepare_borrowed_call(expression, target)
    }

    fn prepare_borrowed_call(
        &mut self,
        callee: &TypedExpr,
        target: ClosureTarget,
    ) -> Option<BorrowedCall> {
        let function = &self.module.functions[target.function];
        let symbol = if target.bound == 0 {
            format!("@tz.fn.{}", function.qualified_name())
        } else {
            let id = self.specializations.request(Specialization {
                function: target.function,
                callbacks: Vec::new(),
                borrowed: target.bound,
            })?;
            format!("@tz.specialized.{id}")
        };
        // After the last early return: a caller without this call evaluates `callee` itself.
        let callee = self.skip_identity_calls(callee);
        let mut cleanup = Vec::new();
        let captures = if Self::is_place(callee) {
            let value = self.expression_mode(callee, false);
            let environment = self.value(format!("extractvalue %tz.closure {value}, 1"));
            let mut values = Vec::new();
            for index in 0..target.bound {
                values.push(self.closure_capture_value(
                    function,
                    target.bound,
                    index,
                    &environment,
                ));
            }
            values
        } else {
            let values = self.capture_values(callee);
            self.capture_cleanup(target, &values, &mut cleanup);
            values.into_iter().map(|(value, _)| value).collect()
        };
        Some(BorrowedCall {
            target,
            symbol,
            captures,
            cleanup,
        })
    }

    fn emit_borrowed_call(&mut self, call: &BorrowedCall, arguments: &[String]) -> String {
        let function = &self.module.functions[call.target.function];
        let values = call
            .captures
            .iter()
            .chain(arguments)
            .zip(&function.signature.parameters)
            .map(|(value, ty)| format!("{} {value}", self.ty(ty)))
            .collect::<Vec<_>>()
            .join(", ");
        self.value(format!(
            "call {} {}({values})",
            self.ty(&function.signature.result),
            call.symbol
        ))
    }

    fn finish_borrowed_call(&mut self, call: &BorrowedCall) {
        for (ty, value, frames) in call.cleanup.iter().rev() {
            self.drop_framed(ty, value, frames);
        }
    }

    fn drop_slot(&mut self, slot: &str, ty: &Type) {
        if ty.needs_drop(&self.module.types()) {
            let value = self.value(format!("load {}, ptr {slot}", self.ty(ty)));
            let frames = self.frame_slots.get(slot).cloned().unwrap_or_default();
            self.drop_framed(ty, &value, &frames);
            self.instruction(format!("store {} zeroinitializer, ptr {slot}", self.ty(ty)));
        }
    }

    fn drop_scope(&mut self, scope: &[(String, Type)]) {
        for (slot, ty) in scope.iter().rev() {
            self.drop_slot(slot, ty);
        }
    }

    fn drop_all(&mut self) {
        for scope in self.scopes.clone().iter().rev() {
            self.drop_scope(scope);
        }
    }

    fn spill(&mut self, ty: &Type, value: &str) -> String {
        let slot = self.slot(ty);
        self.instruction(format!("store {} {value}, ptr {slot}", self.ty(ty)));
        slot
    }

    fn cast(&mut self, expression: &TypedExpr, to: &Type) -> String {
        let value = self.expression(expression);
        if expression.ty == *to {
            return value;
        }
        if let (Type::Integer(from, signed), Type::Integer(bits, _)) = (&expression.ty, to) {
            if from == bits {
                return value;
            }
            let instruction = if bits < from {
                "trunc"
            } else if *signed {
                "sext"
            } else {
                "zext"
            };
            return self.value(format!(
                "{instruction} {} {value} to {}",
                self.ty(&expression.ty),
                self.ty(to)
            ));
        }
        let instruction = match (&expression.ty, to) {
            (Type::Integer(8 | 16 | 32 | 64, true), Type::Binary(32 | 64)) => Some("sitofp"),
            (Type::Integer(8 | 16 | 32 | 64, false), Type::Binary(32 | 64)) => Some("uitofp"),
            (Type::Binary(32), Type::Binary(64)) => Some("fpext"),
            (Type::Binary(64), Type::Binary(32)) => Some("fptrunc"),
            _ => None,
        };
        if let Some(instruction) = instruction {
            return self.value(format!(
                "{instruction} {} {value} to {}",
                self.ty(&expression.ty),
                self.ty(to)
            ));
        }
        if let (Type::Binary(from @ (32 | 64)), Type::Integer(bits @ (8 | 16 | 32 | 64), signed)) =
            (&expression.ty, to)
        {
            let intrinsic = saturating_cast(*from, *bits, *signed, self.intrinsics);
            return self.value(format!(
                "call {} @{intrinsic}({} {value})",
                self.ty(to),
                self.ty(&expression.ty)
            ));
        }
        let input = self.spill(&expression.ty, &value);
        let output = self.slot(to);
        self.instruction(format!(
            "call void @tz_soft_cast(ptr {output}, ptr {input}, i32 {}, i32 {})",
            numeric_kind(&expression.ty),
            numeric_kind(to)
        ));
        self.value(format!("load {}, ptr {output}", self.ty(to)))
    }

    fn short_circuit(&mut self, operator: BinaryOp, left: &TypedExpr, right: &TypedExpr) -> String {
        let left = self.expression(left);
        let left_end = self.block.clone();
        let evaluate_right = self.label();
        let merge = self.label();
        let shortcut = if operator == BinaryOp::And {
            self.branch(&left, &evaluate_right, &merge);
            "0"
        } else {
            self.branch(&left, &merge, &evaluate_right);
            "1"
        };
        self.begin(&evaluate_right);
        self.cover(crate::coverage::RegionKind::ShortCircuit, right.span);
        let right = self.expression(right);
        let right_end = self.block.clone();
        self.jump(&merge);
        self.begin(&merge);
        self.value(format!(
            "phi i1 [ {shortcut}, %{left_end} ], [ {right}, %{right_end} ]"
        ))
    }

    fn binary(&mut self, operator: BinaryOp, left: &TypedExpr, right: &TypedExpr) -> String {
        use BinaryOp::*;
        if left.ty.is_string() {
            let take = operator == Add;
            // Concatenation reads both operands and then destroys them, so stack strings stay put.
            let (lhs, left_frames) = if take {
                self.take_operand(left)
            } else {
                self.read_operand(left)
            };
            let (rhs, right_frames) = if take {
                self.take_operand(right)
            } else {
                self.read_operand(right)
            };
            let ty = self.ty(&left.ty);
            let runtime = &ty[1..];
            let result = match operator {
                Add => self.value(format!(
                    "call {ty} @{runtime}.concat({ty} {lhs}, {ty} {rhs})"
                )),
                Equal | NotEqual => {
                    self.value(format!("call i1 @{runtime}.equal({ty} {lhs}, {ty} {rhs})"))
                }
                Less | LessEqual | Greater | GreaterEqual => {
                    let order = self.value(format!(
                        "call i32 @{runtime}.compare({ty} {lhs}, {ty} {rhs})"
                    ));
                    let predicate = match operator {
                        Less => "slt",
                        LessEqual => "sle",
                        Greater => "sgt",
                        GreaterEqual => "sge",
                        _ => unreachable!(),
                    };
                    self.value(format!("icmp {predicate} i32 {order}, 0"))
                }
                _ => unreachable!("string operator checked"),
            };
            if take {
                self.drop_framed(&left.ty, &lhs, &left_frames);
                self.drop_framed(&right.ty, &rhs, &right_frames);
            } else {
                self.release_operand(left, &lhs, &left_frames);
                self.release_operand(right, &rhs, &right_frames);
            }
            return if operator == NotEqual {
                self.value(format!("xor i1 {result}, 1"))
            } else {
                result
            };
        }
        let lhs = self.expression(left);
        let mut rhs = self.expression(right);
        if let Type::Simd(vector) = left.ty {
            return self.simd_binary(operator, vector, &lhs, &rhs);
        }
        if operator == Power {
            return self.power(&left.ty, &lhs, &rhs, false);
        }
        let ty = self.ty(&left.ty);
        if matches!(left.ty, Type::Decimal(_) | Type::Binary(16 | 128)) {
            let a = self.spill(&left.ty, &lhs);
            let b = self.spill(&right.ty, &rhs);
            let kind = numeric_kind(&left.ty);
            if matches!(operator, Add | Subtract | Multiply | Divide) {
                let op = match operator {
                    Add => 0,
                    Subtract => 1,
                    Multiply => 2,
                    _ => 3,
                };
                let output = self.slot(&left.ty);
                self.instruction(format!(
                    "call void @tz_soft_op(ptr {output}, ptr {a}, ptr {b}, i32 {kind}, i32 {op})"
                ));
                return self.value(format!("load {ty}, ptr {output}"));
            }
            let order = self.value(format!(
                "call i32 @tz_soft_cmp(ptr {a}, ptr {b}, i32 {kind})"
            ));
            let (predicate, value) = match operator {
                Equal => ("eq", 0),
                NotEqual => ("ne", 0),
                Less => ("slt", 0),
                LessEqual => ("sle", 0),
                Greater => ("eq", 1),
                GreaterEqual => ("ule", 1),
                _ => unreachable!("software float operator checked"),
            };
            return self.value(format!("icmp {predicate} i32 {order}, {value}"));
        }
        if matches!(operator, Divide | Remainder) && left.ty.is_integer() {
            let mut valid = self.value(format!("icmp ne {ty} {rhs}, 0"));
            let trap_info = self.globals.traps.is_some();
            if trap_info {
                self.guard(&valid, TrapKind::IntegerDivisionByZero);
            }
            if let Type::Integer(bits, true) = left.ty {
                let minimum = self.value(format!("icmp eq {ty} {lhs}, {}", 1u128 << (bits - 1)));
                let negative_one = self.value(format!("icmp eq {ty} {rhs}, -1"));
                let overflow = self.value(format!("and i1 {minimum}, {negative_one}"));
                let no_overflow = self.value(format!("xor i1 {overflow}, 1"));
                if trap_info {
                    self.guard(&no_overflow, TrapKind::IntegerDivisionOverflow);
                } else {
                    valid = self.value(format!("and i1 {valid}, {no_overflow}"));
                }
            }
            if !trap_info {
                self.guard(&valid, TrapKind::IntegerDivisionByZero);
            }
        }
        if matches!(operator, ShiftLeft | ShiftRight | ShiftRightUnsigned) {
            let Type::Integer(bits, _) = left.ty else {
                unreachable!()
            };
            rhs = self.value(format!("and {ty} {rhs}, {}", bits - 1));
        }
        let unsigned = matches!(
            left.ty,
            Type::Integer(_, false) | Type::Char | Type::Utf8Char
        );
        let instruction = if left.ty.is_float() {
            match operator {
                Add => "fadd",
                Subtract => "fsub",
                Multiply => "fmul",
                Divide => "fdiv",
                Equal => "fcmp oeq",
                NotEqual => "fcmp une",
                Less => "fcmp olt",
                LessEqual => "fcmp ole",
                Greater => "fcmp ogt",
                GreaterEqual => "fcmp oge",
                _ => unreachable!("float operator checked"),
            }
        } else {
            match operator {
                Add => "add",
                Subtract => "sub",
                Multiply => "mul",
                Divide => {
                    if unsigned {
                        "udiv"
                    } else {
                        "sdiv"
                    }
                }
                Remainder => {
                    if unsigned {
                        "urem"
                    } else {
                        "srem"
                    }
                }
                Equal => "icmp eq",
                NotEqual => "icmp ne",
                Less => {
                    if unsigned {
                        "icmp ult"
                    } else {
                        "icmp slt"
                    }
                }
                LessEqual => {
                    if unsigned {
                        "icmp ule"
                    } else {
                        "icmp sle"
                    }
                }
                Greater => {
                    if unsigned {
                        "icmp ugt"
                    } else {
                        "icmp sgt"
                    }
                }
                GreaterEqual => {
                    if unsigned {
                        "icmp uge"
                    } else {
                        "icmp sge"
                    }
                }
                BitAnd => "and",
                BitOr => "or",
                BitXor => "xor",
                ShiftLeft => "shl",
                ShiftRight => {
                    if unsigned {
                        "lshr"
                    } else {
                        "ashr"
                    }
                }
                ShiftRightUnsigned => "lshr",
                _ => unreachable!("operator checked or handled separately"),
            }
        };
        self.value(format!("{instruction} {} {lhs}, {rhs}", self.ty(&left.ty)))
    }
}

fn numeric_kind(ty: &Type) -> u16 {
    match ty {
        Type::Binary(16) => 0,
        Type::Binary(32) => 1,
        Type::Binary(64) => 2,
        Type::Binary(128) => 3,
        Type::Decimal(32) => 4,
        Type::Decimal(64) => 5,
        Type::Decimal(128) => 6,
        Type::Integer(bits, signed) => 16 + (bits / 8).ilog2() as u16 + if *signed { 0 } else { 8 },
        _ => unreachable!("numeric type checked"),
    }
}

fn export_wrapper(function: &CheckedFunction, module: &CheckedModule) -> String {
    named_wrapper(function, module, &format!("tz_{}", function.name))
}

/// The C-ABI wrapper `symbol` of a function with scalar parameters and result.
fn named_wrapper(function: &CheckedFunction, module: &CheckedModule, symbol: &str) -> String {
    let parameters = function
        .signature
        .parameters
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("{} %arg{index}", abi_type(ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let result = &function.signature.result;
    let mut output = format!(
        "define {} @{symbol}({parameters}) nounwind {{\nentry:\n",
        abi_type(result)
    );
    let mut arguments = Vec::new();
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        if *ty == Type::Bool {
            let _ = writeln!(output, "  %b{index} = icmp ne i32 %arg{index}, 0");
            arguments.push(format!("i1 %b{index}"));
        } else if let Type::Integer(bits @ (8 | 16), _) = ty {
            let _ = writeln!(output, "  %n{index} = trunc i32 %arg{index} to i{bits}");
            arguments.push(format!("i{bits} %n{index}"));
        } else if matches!(ty, Type::Reference(inner, false) if matches!(inner.as_ref(), Type::Handle(_)))
        {
            // The host passes the handle itself, and a borrow of it is the address of a slot.
            let _ = writeln!(
                output,
                "  %h{index} = alloca ptr, align 8\n  store ptr %arg{index}, ptr %h{index}"
            );
            arguments.push(format!("ptr %h{index}"));
        } else {
            arguments.push(format!("{} %arg{index}", llvm_type(ty, module)));
        }
    }
    let _ = writeln!(
        output,
        "  %result = call {} @tz.fn.{}({})",
        llvm_type(result, module),
        function.qualified_name(),
        arguments.join(", ")
    );
    match result {
        Type::Bool => output.push_str("  %bool = zext i1 %result to i32\n  ret i32 %bool\n"),
        Type::Unit => output.push_str("  ret void\n"),
        Type::Integer(bits @ (8 | 16), signed) => {
            let _ = writeln!(
                output,
                "  %wide = {} i{bits} %result to i32\n  ret i32 %wide",
                if *signed { "sext" } else { "zext" }
            );
        }
        _ => {
            let _ = writeln!(output, "  ret {} %result", llvm_type(result, module));
        }
    }
    output.push_str("}\n\n");
    output
}

fn saturating_cast(
    from: u16,
    bits: u16,
    signed: bool,
    intrinsics: &mut BTreeSet<String>,
) -> String {
    let sign = if signed { "s" } else { "u" };
    let intrinsic = format!("llvm.fpto{sign}i.sat.i{bits}.f{from}");
    let source = if from == 32 { "float" } else { "double" };
    intrinsics.insert(format!("declare i{bits} @{intrinsic}({source})"));
    intrinsic
}

/// The LLVM symbol of a builtin instance: `@tz.builtin.name`, followed for a
/// polymorphic builtin by its type arguments as unquoted, injective names.
fn builtin_symbol(instance: &BuiltinInstance, module: &CheckedModule) -> String {
    let name = instance.builtin.name();
    let mut symbol = format!(
        "@tz.builtin.{}",
        name.strip_prefix("$builtin.").unwrap_or(name)
    );
    if !instance.types.is_empty() {
        symbol.push('.');
        symbol.push_str(
            &instance
                .types
                .iter()
                .map(|ty| mangled_type(ty, module))
                .collect::<Vec<_>>()
                .join("$C"),
        );
    }
    symbol
}

/// `canonical_type` spelled with LLVM identifier characters only; `$` never
/// occurs in canonical types, so the escapes keep the spelling injective.
fn mangled_type(ty: &Type, module: &CheckedModule) -> String {
    canonical_type(ty, module)
        .replace("->", "$A")
        .replace('[', "$L")
        .replace(']', "$R")
        .replace(',', "$C")
}

/// Defines one builtin instance, whose callee type `ty` is concrete.
fn emit_builtin(
    instance: &BuiltinInstance,
    ty: &Type,
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
    wasm: bool,
    debug_output: bool,
    globals: &mut Globals,
) -> String {
    let builtin = instance.builtin;
    let name = builtin.name();
    let symbol = builtin_symbol(instance, module);
    let count = builtin.scheme().parameters.len();
    let result = llvm_type(&ty.after_arguments(count), module);
    if builtin == Builtin::Unreachable {
        return format!(
            "define internal {result} {symbol}(i8 %unit) noreturn nounwind {{\n\
             entry:\n  call void @llvm.trap()\n  unreachable\n}}\n\n"
        );
    }
    #[cfg(test)]
    if let Some(definition) = test_builtin(instance, ty, &symbol, &result, module) {
        return definition;
    }
    match builtin {
        builtin if builtin.name().starts_with("Simd.") => {
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        builtin if builtin.name().starts_with("Math.") => {
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        Builtin::Hash
        | Builtin::HashMix
        | Builtin::DisplayQuoted
        | Builtin::SeqNext
        | Builtin::OwnedDrop
        | Builtin::Ignore
        | Builtin::BenchConsume
        | Builtin::Not
        | Builtin::OwnedFunction
        | Builtin::OwnedCall
        | Builtin::AsyncResume
        | Builtin::IOReadLine
        | Builtin::IOWrite
        | Builtin::OsRead
        | Builtin::OsArgs
        | Builtin::OsWrite
        | Builtin::OsRandom
        | Builtin::OsClock
        | Builtin::OsSleep
        | Builtin::OsOpen
        | Builtin::OsHandle
        | Builtin::OsClose
        | Builtin::OsSpawn
        | Builtin::NetResolve
        | Builtin::NetOpen
        | Builtin::NetAccept
        | Builtin::NetRead
        | Builtin::NetWrite
        | Builtin::NetClose
        | Builtin::NetClassify
        | Builtin::NetWatch
        | Builtin::NetUnwatch
        | Builtin::NetConnect
        | Builtin::NetNames
        | Builtin::NetSend => emit_typed_builtin(instance, ty, module, intrinsics, globals),
        Builtin::Default => format!(
            "define internal {result} {symbol}() nounwind {{\nentry:\n  ret {result} zeroinitializer\n}}\n"
        ),
        // Arena ids only need to be unique, so a monotonic increment suffices; the counter
        // publishes no other memory. Default wasm32 lowers the atomic to a plain add.
        // The clock is defined only by the C entry of `tsuzuri bench` (`runtime/bench-runner.c`);
        // `emit_program` rejects other outputs that reach it (G18 D3).
        Builtin::BenchNow => {
            intrinsics.insert("declare i64 @tsuzuri_bench_now()".into());
            format!(
                "define internal i64 {symbol}() nounwind {{\nentry:\n  %now = call i64 @tsuzuri_bench_now()\n  ret i64 %now\n}}\n\n"
            )
        }
        Builtin::GenSeed => format!(
            "define internal i64 {symbol}() nounwind {{\nentry:\n  ret i64 {}\n}}\n\n",
            globals.seed.unwrap_or(DEFAULT_PROPERTY_SEED) as i64
        ),
        Builtin::ArenaNextId => format!(
            "@tz.arena.next_id = internal global i64 0, align 8\n\n\
             define internal i64 {symbol}() nounwind {{\n\
             entry:\n  %previous = atomicrmw add ptr @tz.arena.next_id, i64 1 monotonic, align 8\n  \
             %id = add i64 %previous, 1\n  %valid = icmp sgt i64 %id, 0\n  \
             br i1 %valid, label %done, label %exhausted\n\
             exhausted:\n  call void @llvm.trap()\n  unreachable\n\
             done:\n  ret i64 %id\n}}\n\n"
        ),
        // Operation ids are unique and never reused. The high bits name the thread that began the
        // operation, so `tsuzuri_async_post` can hand its completion to that thread (B08).
        Builtin::AsyncNextId => {
            async_thread_index(globals, wasm);
            let register = if !wasm && globals.async_reactor {
                intrinsics.insert("declare void @tsuzuri_async_register(i64)".into());
                "  call void @tsuzuri_async_register(i64 %id)\n"
            } else {
                ""
            };
            let storage = if wasm {
                "global"
            } else {
                "thread_local global"
            };
            format!(
                "@tz.async.next_id = internal {storage} i64 0, align 8\n\n\
                 define internal i64 {symbol}() nounwind {{\n\
                 entry:\n  %thread = call i64 @tz.async.thread_index()\n  \
                 %previous = load i64, ptr @tz.async.next_id, align 8\n  %count = add i64 %previous, 1\n  \
                 %valid = icmp ult i64 %count, {ASYNC_OPERATIONS_PER_THREAD}\n  \
                 br i1 %valid, label %done, label %exhausted\n\
                 exhausted:\n  call void @llvm.trap()\n  unreachable\n\
                 done:\n  store i64 %count, ptr @tz.async.next_id, align 8\n  \
                 %high = shl i64 %thread, {ASYNC_THREAD_SHIFT}\n  %id = or i64 %high, %count\n{register}  ret i64 %id\n}}\n\n"
            )
        }
        Builtin::AsyncRetire => {
            let retire = if !wasm && globals.async_reactor {
                intrinsics.insert("declare void @tsuzuri_async_retire(i64)".into());
                "  call void @tsuzuri_async_retire(i64 %operation)\n"
            } else {
                ""
            };
            format!(
                "define internal {result} {symbol}(i64 %operation) nounwind {{\nentry:\n{retire}  ret {result} 0\n}}\n\n"
            )
        }
        // The reactor of `Async.block_on` (B08 Phase 3): `src/runtime/async.c` natively, which the
        // driver links when the IR declares these functions, and JSPI imports on WASM, which the
        // driver accepts only with `--wasm-feature jspi`.
        Builtin::AsyncClock => {
            intrinsics.insert(if wasm {
                "declare i64 @tsuzuri_async_clock() \"wasm-import-module\"=\"tsuzuri_async\" \"wasm-import-name\"=\"clock\"".into()
            } else {
                "declare i64 @tsuzuri_async_clock()".into()
            });
            format!(
                "define internal i64 {symbol}() nounwind {{\nentry:\n  %now = call i64 @tsuzuri_async_clock()\n  ret i64 %now\n}}\n\n"
            )
        }
        Builtin::AsyncWait if wasm => {
            intrinsics.insert(
                "declare void @tsuzuri_async_wait(i64) \"wasm-import-module\"=\"tsuzuri_async\" \"wasm-import-name\"=\"wait\"".into(),
            );
            format!(
                "define internal {result} {symbol}(i64 %deadline) nounwind {{\nentry:\n  call void @tsuzuri_async_wait(i64 %deadline)\n  ret {result} 0\n}}\n\n"
            )
        }
        // Natively the thread waits for the completions posted to it.
        Builtin::AsyncWait => {
            async_thread_index(globals, wasm);
            intrinsics.insert("declare void @tsuzuri_async_wait(i64, i64)".into());
            format!(
                "define internal {result} {symbol}(i64 %deadline) nounwind {{\nentry:\n  %thread = call i64 @tz.async.thread_index()\n  \
                 call void @tsuzuri_async_wait(i64 %thread, i64 %deadline)\n  ret {result} 0\n}}\n\n"
            )
        }
        // JavaScript completes operations on its own thread with `tsuzuri_async_complete`.
        Builtin::AsyncPosted if wasm => format!(
            "define internal {result} {symbol}() nounwind {{\nentry:\n  ret {result} zeroinitializer\n}}\n\n"
        ),
        Builtin::AsyncPosted => {
            async_thread_index(globals, wasm);
            intrinsics.insert("declare i32 @tsuzuri_async_take(i64, ptr, ptr)".into());
            format!(
                "define internal {result} {symbol}() nounwind {{\nentry:\n  %thread = call i64 @tz.async.thread_index()\n  \
                 %operation = alloca i64, align 8\n  %value = alloca i64, align 8\n  \
                 %found = call i32 @tsuzuri_async_take(i64 %thread, ptr %operation, ptr %value)\n  %first = load i64, ptr %operation, align 8\n  \
                 %second = load i64, ptr %value, align 8\n  %pair = insertvalue {result} zeroinitializer, i64 %first, 0\n  \
                 %result = insertvalue {result} %pair, i64 %second, 1\n  ret {result} %result\n}}\n\n"
            )
        }
        // The two builtins of a slot share its globals, which `emit_target` writes once. Each
        // thread has its own executor, so the slots are thread-local (B08).
        Builtin::AsyncTake | Builtin::AsyncPut => {
            let (slot, lock) = async_slot(&canonical_type(&instance.types[0], module));
            let storage = if wasm {
                "global"
            } else {
                "thread_local global"
            };
            for global in [
                format!(
                    "{slot} = internal {storage} {} zeroinitializer, align 16",
                    llvm_type(&instance.types[0], module)
                ),
                format!("{lock} = internal {storage} i8 0, align 1"),
            ] {
                if !globals.definitions.contains(&global) {
                    globals.definitions.push(global);
                }
            }
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        // The generated tables live in `unicode.ll`, which `emit_target` appends on use (D09).
        Builtin::UnicodeTableLength => format!(
            "define internal i64 {symbol}(i64 %table) nounwind alwaysinline {{\nentry:\n  %r = call i64 @tz.unicode.length(i64 %table)\n  ret i64 %r\n}}\n\n"
        ),
        Builtin::UnicodeTableEntry => format!(
            "define internal i64 {symbol}(i64 %table, i64 %index) nounwind alwaysinline {{\nentry:\n  %r = call i64 @tz.unicode.entry(i64 %table, i64 %index)\n  ret i64 %r\n}}\n\n"
        ),
        Builtin::DebugPrintString => {
            let mut write = String::new();
            let import = if wasm && debug_output {
                "declare void @tsuzuri_debug_write(ptr, i64) \"wasm-import-module\"=\"tsuzuri_debug\" \"wasm-import-name\"=\"write\"\n"
            } else {
                ""
            };
            if !wasm || debug_output {
                write.push_str("  %units = extractvalue %tz.string %text, 1\n  %utf8 = call %tz.utf8string @tz.utf8string.from_string(ptr %data, i64 %units)\n");
                if wasm {
                    write.push_str("  %bytes = extractvalue %tz.utf8string %utf8, 0\n  %length = extractvalue %tz.utf8string %utf8, 1\n  call void @tsuzuri_debug_write(ptr %bytes, i64 %length)\n  call void @tz.free(ptr %bytes)\n");
                } else {
                    write.push_str("  call void @tz.debug.write(%tz.utf8string %utf8)\n");
                }
            }
            format!(
                "{import}define internal i8 {symbol}(%tz.string %text) nounwind {{\nentry:\n  %data = extractvalue %tz.string %text, 0\n{write}  call void @tz.free(ptr %data)\n  ret i8 0\n}}\n"
            )
        }
        Builtin::StringToCodeUnits
        | Builtin::StringFromCodeUnits
        | Builtin::Utf8StringToBytes
        | Builtin::Utf8StringFromBytes
        | Builtin::Utf8StringDecodeAt
        | Builtin::StringCompare
        | Builtin::Utf8StringCompare => {
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        Builtin::CharToU16
        | Builtin::CharOfU16
        | Builtin::Utf8CharToU32
        | Builtin::Utf8CharOfU32
        | Builtin::Utf8CharOfU32Unchecked => emit_character_conversion(instance, ty, module),
        Builtin::ArraySet
        | Builtin::ArrayUpdate
        | Builtin::ArraySwap
        | Builtin::ArrayWrite
        | Builtin::ArraySwapIn
        | Builtin::ArraySplitAtMut
        | Builtin::FixedArrayInit
        | Builtin::ListCons
        | Builtin::ListTail => emit_typed_builtin(instance, ty, module, intrinsics, globals),
        Builtin::ArrayConcat
        | Builtin::ArrayToList
        | Builtin::ArraySortBy
        | Builtin::ListMap
        | Builtin::ListMapRef
        | Builtin::ListReverse
        | Builtin::ListToArray
        | Builtin::ListFoldRef => emit_typed_builtin(instance, ty, module, intrinsics, globals),
        builtin if builtin.name().starts_with("Int.") => {
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        builtin if builtin.name().starts_with("Vec.") => {
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        builtin if builtin.shared_kind().is_some() => {
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        Builtin::Display | Builtin::ToString => emit_display(instance, module),
        Builtin::Parse => emit_parse(instance, ty, module),
        Builtin::ToFloat => format!(
            "define internal double @tz.builtin.{name}(i64 %x) nounwind {{\n\
             entry:\n  %r = sitofp i64 %x to double\n  ret double %r\n}}\n\n"
        ),
        Builtin::ToInt => {
            let intrinsic = saturating_cast(64, 64, true, intrinsics);
            format!(
                "define internal i64 @tz.builtin.{name}(double %x) nounwind {{\n\
                 entry:\n  %r = call i64 @{intrinsic}(double %x)\n  ret i64 %r\n}}\n\n"
            )
        }
        Builtin::Assert => format!(
            "define internal i8 @tz.builtin.{name}(i1 %x) nounwind {{\n\
             entry:\n  br i1 %x, label %ok, label %fail\n\
             fail:\n  call void @llvm.trap()\n  unreachable\n\
             ok:\n  ret i8 0\n}}\n\n"
        ),
        Builtin::CloneString
        | Builtin::CloneUtf8String
        | Builtin::StringFromUtf8
        | Builtin::Utf8StringFromString
        | Builtin::StringIsWellFormed
        | Builtin::StringToWellFormed => {
            let (input, runtime) = match builtin {
                Builtin::CloneString => ("%tz.string", "tz.string.new"),
                Builtin::CloneUtf8String => ("%tz.utf8string", "tz.utf8string.new"),
                Builtin::StringFromUtf8 => ("%tz.utf8string", "tz.string.from_utf8"),
                Builtin::Utf8StringFromString => ("%tz.string", "tz.utf8string.from_string"),
                Builtin::StringIsWellFormed => ("%tz.string", "tz.string.is_well_formed"),
                Builtin::StringToWellFormed => ("%tz.string", "tz.string.to_well_formed"),
                _ => unreachable!(),
            };
            format!(
                "define internal {result} {symbol}(ptr %x) nounwind {{\n\
                 entry:\n  %s = load {input}, ptr %x\n\
                 %p = extractvalue {input} %s, 0\n  %n = extractvalue {input} %s, 1\n\
                 %r = call {result} @{runtime}(ptr %p, i64 %n)\n  ret {result} %r\n}}\n\n"
            )
        }
        Builtin::Sqrt | Builtin::Floor | Builtin::Ceil | Builtin::Abs => {
            emit_typed_builtin(instance, ty, module, intrinsics, globals)
        }
        _ => unreachable!("all builtins have a lowering"),
    }
}

fn emit_typed_builtin(
    instance: &BuiltinInstance,
    ty: &Type,
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
    shared_globals: &mut Globals,
) -> String {
    let (id, function) = module.functions.iter().enumerate().find(|(_, function)| {
        matches!(&function.body.kind, TypedExprKind::Call(callee, _)
            if matches!(&callee.kind, TypedExprKind::Function(FunctionRef::Builtin(found)) if found == instance))
    }).expect("a builtin has its checked function wrapper");
    let mut builtins = Builtins::new();
    let mut local_globals = Globals {
        wasm: shared_globals.wasm,
        ..Globals::default()
    };
    let separate = shared_globals.traps.is_none();
    let globals = if separate {
        &mut local_globals
    } else {
        &mut *shared_globals
    };
    let mut specializations = Specializations::new(module);
    let mut emitter = FunctionEmitter::new(
        module,
        function,
        id,
        &mut builtins,
        intrinsics,
        globals,
        &mut specializations,
    );
    let element = instance.types.first().unwrap_or(&Type::Unit);
    let element_type = emitter.ty(element);
    let result = if matches!(
        instance.builtin,
        Builtin::Sqrt | Builtin::Floor | Builtin::Ceil | Builtin::Abs
    ) {
        let builtin = match instance.builtin {
            Builtin::Sqrt => Builtin::MathSqrt,
            Builtin::Floor => Builtin::MathFloor,
            Builtin::Ceil => Builtin::MathCeil,
            Builtin::Abs => Builtin::MathAbs,
            _ => unreachable!(),
        };
        emitter.math_builtin(&BuiltinInstance {
            builtin,
            types: vec![Type::F64],
        })
    } else if instance.builtin.name().starts_with("Simd.") {
        emitter.simd_builtin(instance)
    } else if instance.builtin == Builtin::SeqNext {
        emitter.sequence_next(ty)
    } else if matches!(instance.builtin, Builtin::OwnedDrop | Builtin::Ignore) {
        emitter.drop_value(element, "%arg0");
        "0".to_owned()
    } else if instance.builtin == Builtin::BenchConsume {
        // As Rust's `black_box`: the value goes to memory that an opaque asm with a memory
        // clobber reads, so LLVM keeps its computation and the memory it reaches (G18 D6).
        let slot = emitter.slot(element);
        emitter.instruction(format!("store {element_type} %arg0, ptr {slot}"));
        emitter.instruction(format!(
            "call void asm sideeffect \"\", \"r,~{{memory}}\"(ptr {slot})"
        ));
        emitter.drop_value(element, "%arg0");
        "0".to_owned()
    } else if instance.builtin == Builtin::Not {
        emitter.value("xor i1 %arg0, 1")
    } else if instance.builtin == Builtin::OwnedFunction {
        let owned = emitter.ty(&ty.after_arguments(1));
        emitter.value(format!(
            "insertvalue {owned} zeroinitializer, %tz.closure %arg0, 0"
        ))
    } else if matches!(instance.builtin, Builtin::AsyncTake | Builtin::AsyncPut) {
        emitter.async_slot_builtin(instance, ty)
    } else if instance.builtin == Builtin::AsyncResume {
        // The call takes the continuation's environment, which nothing else holds (B08).
        let Type::Function(parameters, _) = ty else {
            unreachable!("builtin has function type")
        };
        let run = Type::function(vec![parameters[1].clone()], ty.after_arguments(2));
        let next = emitter.ty(&parameters[0]);
        let closure = emitter.value(format!("extractvalue {next} %arg0, 0"));
        emitter
            .apply_value(&closure, &run, Some((&parameters[1], "%arg1")), false)
            .0
    } else if instance.builtin == Builtin::OwnedCall {
        // The call borrows the environment, which the owned function keeps.
        let Type::Function(parameters, _) = ty else {
            unreachable!("builtin has function type")
        };
        let Type::Reference(owned, false) = &parameters[0] else {
            unreachable!("Owned.call borrows the owned function")
        };
        let run = Type::function(vec![parameters[1].clone()], ty.after_arguments(2));
        let owned = emitter.ty(owned);
        let field = emitter.value(format!(
            "getelementptr inbounds {owned}, ptr %arg0, i32 0, i32 0"
        ));
        let closure = emitter.value(format!("load %tz.closure, ptr {field}"));
        emitter
            .apply_value(&closure, &run, Some((&parameters[1], "%arg1")), true)
            .0
    } else if matches!(instance.builtin, Builtin::IOReadLine | Builtin::IOWrite) {
        emitter.io_builtin(instance.builtin, ty)
    } else if matches!(
        instance.builtin,
        Builtin::OsRead
            | Builtin::OsArgs
            | Builtin::OsWrite
            | Builtin::OsRandom
            | Builtin::OsClock
            | Builtin::OsSleep
            | Builtin::OsOpen
            | Builtin::OsHandle
            | Builtin::OsClose
            | Builtin::OsSpawn
    ) {
        emitter.os_builtin(instance.builtin, ty)
    } else if matches!(
        instance.builtin,
        Builtin::NetResolve
            | Builtin::NetOpen
            | Builtin::NetAccept
            | Builtin::NetRead
            | Builtin::NetWrite
            | Builtin::NetClose
            | Builtin::NetClassify
            | Builtin::NetWatch
            | Builtin::NetUnwatch
            | Builtin::NetConnect
            | Builtin::NetNames
            | Builtin::NetSend
    ) {
        emitter.net_builtin(instance.builtin, ty)
    } else if instance.builtin.name().starts_with("Math.") {
        emitter.math_builtin(instance)
    } else if instance.builtin == Builtin::DisplayQuoted {
        emitter.display_quoted(element)
    } else if instance.builtin == Builtin::HashMix {
        emitter.hash_word("%arg0", "%arg1")
    } else if instance.builtin == Builtin::Hash {
        emitter.hash_primitive(element)
    } else if matches!(
        instance.builtin,
        Builtin::StringToCodeUnits
            | Builtin::StringFromCodeUnits
            | Builtin::Utf8StringToBytes
            | Builtin::Utf8StringFromBytes
            | Builtin::Utf8StringDecodeAt
            | Builtin::StringCompare
            | Builtin::Utf8StringCompare
    ) {
        emitter.string_buffer_builtin(instance, ty)
    } else if instance.builtin.name().starts_with("Int.") {
        emitter.integer_builtin(instance, ty)
    } else if instance.builtin.name().starts_with("Vec.") {
        emitter.vector_builtin(instance, ty)
    } else if instance.builtin.shared_kind().is_some() {
        emitter.shared_builtin(instance, ty)
    } else if instance.builtin == Builtin::FixedArrayInit {
        // A function value of `FixedArray.init`; its instance types include the array (A16).
        let Type::Function(parameters, _) = ty else {
            unreachable!("builtin has function type")
        };
        let initializer = parameters[0].clone();
        let array = emitter.fill_fixed_array(&ty.after_arguments(1), |emitter, index| {
            emitter
                .apply_value("%arg0", &initializer, Some((&Type::I64, index)), true)
                .0
        });
        emitter.drop_value(&initializer, "%arg0");
        array
    } else {
        match instance.builtin {
            Builtin::ArraySet | Builtin::ArrayUpdate | Builtin::ArraySwap => {
                let collection = Type::Array(Box::new(element.clone()));
                let first = emitter.checked_element_pointer(&collection, "%arg0", "%arg1");
                if instance.builtin == Builtin::ArraySwap {
                    let second = emitter.checked_element_pointer(&collection, "%arg0", "%arg2");
                    let same = emitter.value("icmp eq i64 %arg1, %arg2");
                    let done = emitter.label();
                    let exchange = emitter.label();
                    emitter.branch(&same, &done, &exchange);
                    emitter.begin(&exchange);
                    let left = emitter.value(format!("load {element_type}, ptr {first}"));
                    let right = emitter.value(format!("load {element_type}, ptr {second}"));
                    emitter.instruction(format!("store {element_type} {right}, ptr {first}"));
                    emitter.instruction(format!("store {element_type} {left}, ptr {second}"));
                    emitter.jump(&done);
                    emitter.begin(&done);
                } else {
                    let previous = emitter.value(format!("load {element_type}, ptr {first}"));
                    let replacement = if instance.builtin == Builtin::ArraySet {
                        emitter.drop_value(element, &previous);
                        "%arg2".to_owned()
                    } else {
                        let callback = Type::function(vec![element.clone()], element.clone());
                        emitter
                            .apply_value("%arg2", &callback, Some((element, &previous)), false)
                            .0
                    };
                    emitter.instruction(format!("store {element_type} {replacement}, ptr {first}"));
                }
                "%arg0".to_owned()
            }
            Builtin::ArrayWrite | Builtin::ArraySwapIn => {
                let collection = Type::Array(Box::new(element.clone()));
                let first = emitter.checked_element_pointer(&collection, "%arg0", "%arg1");
                if instance.builtin == Builtin::ArraySwapIn {
                    let second = emitter.checked_element_pointer(&collection, "%arg0", "%arg2");
                    let same = emitter.value("icmp eq i64 %arg1, %arg2");
                    let done = emitter.label();
                    let exchange = emitter.label();
                    emitter.branch(&same, &done, &exchange);
                    emitter.begin(&exchange);
                    let left = emitter.value(format!("load {element_type}, ptr {first}"));
                    let right = emitter.value(format!("load {element_type}, ptr {second}"));
                    emitter.instruction(format!("store {element_type} {right}, ptr {first}"));
                    emitter.instruction(format!("store {element_type} {left}, ptr {second}"));
                    emitter.jump(&done);
                    emitter.begin(&done);
                } else {
                    let previous = emitter.value(format!("load {element_type}, ptr {first}"));
                    emitter.drop_value(element, &previous);
                    emitter.instruction(format!("store {element_type} %arg2, ptr {first}"));
                }
                "0".to_owned()
            }
            Builtin::ArraySplitAtMut => {
                let length = emitter.value("extractvalue %tz.array %arg0, 1");
                let valid = emitter.value(format!("icmp ule i64 %arg1, {length}"));
                emitter.guard(&valid, TrapKind::BoundsCheck);
                let data = emitter.value("extractvalue %tz.array %arg0, 0");
                let left = emitter.value(format!(
                    "insertvalue %tz.array zeroinitializer, ptr {data}, 0"
                ));
                let left = emitter.value(format!("insertvalue %tz.array {left}, i64 %arg1, 1"));
                let rest = emitter.element_pointer(element, &data, "%arg1");
                let remaining = emitter.value(format!("sub i64 {length}, %arg1"));
                let right = emitter.value(format!(
                    "insertvalue %tz.array zeroinitializer, ptr {rest}, 0"
                ));
                let right =
                    emitter.value(format!("insertvalue %tz.array {right}, i64 {remaining}, 1"));
                let pair = emitter.value(format!(
                    "insertvalue {{ %tz.array, %tz.array }} zeroinitializer, %tz.array {left}, 0"
                ));
                emitter.value(format!(
                    "insertvalue {{ %tz.array, %tz.array }} {pair}, %tz.array {right}, 1"
                ))
            }
            Builtin::ListCons => {
                let length = emitter.value("extractvalue %tz.list %arg1, 1");
                let valid = emitter.value(format!("icmp ult i64 {length}, 9223372036854775807"));
                emitter.guard(&valid, TrapKind::AllocationSize);
                let head = emitter.value("extractvalue %tz.list %arg1, 0");
                let node_type = emitter.list_node_type(element);
                let node = emitter.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({node_type}, ptr null, i32 1) to i64))"));
                emitter.instruction(format!("store ptr {head}, ptr {node}"));
                let slot = emitter.list_element_pointer(element, &node);
                emitter.instruction(format!("store {element_type} %arg0, ptr {slot}"));
                let length = emitter.value(format!("add i64 {length}, 1"));
                let value = emitter.value(format!(
                    "insertvalue %tz.list zeroinitializer, ptr {node}, 0"
                ));
                emitter.value(format!("insertvalue %tz.list {value}, i64 {length}, 1"))
            }
            Builtin::ListTail => {
                let length = emitter.value("extractvalue %tz.list %arg0, 1");
                let valid = emitter.value(format!("icmp ugt i64 {length}, 0"));
                emitter.guard(&valid, TrapKind::BoundsCheck);
                let head = emitter.value("extractvalue %tz.list %arg0, 0");
                let next = emitter.value(format!("load ptr, ptr {head}"));
                let slot = emitter.list_element_pointer(element, &head);
                let previous = emitter.value(format!("load {element_type}, ptr {slot}"));
                emitter.drop_value(element, &previous);
                emitter.instruction(format!("call void @tz.free(ptr {head})"));
                let length = emitter.value(format!("sub i64 {length}, 1"));
                let value = emitter.value(format!(
                    "insertvalue %tz.list zeroinitializer, ptr {next}, 0"
                ));
                emitter.value(format!("insertvalue %tz.list {value}, i64 {length}, 1"))
            }
            _ => emitter.bulk_collection_builtin(instance, ty),
        }
    };
    let count = instance.builtin.scheme().parameters.len();
    let Type::Function(parameters, _) = ty else {
        unreachable!("builtin has function type")
    };
    let arguments = parameters[..count]
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("{} %arg{index}", emitter.ty(ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let result_type = emitter.ty(&ty.after_arguments(count));
    emitter.instruction(format!("ret {result_type} {result}"));
    let mut definition = emitter.auxiliary(&format!(
        "{result_type} {}({arguments})",
        builtin_symbol(instance, module)
    ));
    if instance.builtin == Builtin::AsyncResume {
        definition = definition.replacen(" nounwind {", " nounwind alwaysinline {", 1);
    }
    // A builtin body that drops recursive values or deferred shared blocks needs their helpers.
    if separate {
        shared_globals
            .recursive_types
            .append(&mut local_globals.recursive_types);
        shared_globals
            .shared_types
            .append(&mut local_globals.shared_types);
    }
    definition
}

/// The globals of the host executor's state slot for the canonical type name `ty`, one per
/// type: the value, and its lock (0 empty, 1 full, 2 taken).
fn async_slot(ty: &str) -> (String, String) {
    (
        format!("@\"tz.async.slot.{ty}\""),
        format!("@\"tz.async.lock.{ty}\""),
    )
}

/// The bit at which an operation id of `Async.__next_id` keeps the index of the thread that began
/// the operation; `src/runtime/async.c` reads it the same way to route a posted completion (B08).
const ASYNC_THREAD_SHIFT: u32 = 39;
/// The operations that one thread can begin, so that the count stays below the thread's bits.
const ASYNC_OPERATIONS_PER_THREAD: u64 = 1 << ASYNC_THREAD_SHIFT;

/// Defines `@tz.async.thread_index`: the calling thread's index for the executor of `Async.start`
/// and `Async.block_on`, assigned on first use and never reused. Indexes stay below 2^24, so an
/// operation id that carries one stays positive.
fn async_thread_index(globals: &mut Globals, wasm: bool) {
    let definition = if wasm {
        "@tz.async.thread = internal global i64 1, align 8\n\
        define internal i64 @tz.async.thread_index() nounwind {\n\
        entry:\n  %epoch = load i64, ptr @tz.async.thread, align 8\n  ret i64 %epoch\n\
}\n\
        define void @tsuzuri_async_set_epoch(i64 %epoch) nounwind {\n\
        entry:\n  %positive = icmp sgt i64 %epoch, 0\n  %bounded = icmp ult i64 %epoch, 16777216\n  \
        %count = load i64, ptr @tz.async.next_id, align 8\n  %unused = icmp eq i64 %count, 0\n  \
        %range = and i1 %positive, %bounded\n  %valid = and i1 %range, %unused\n  \
        br i1 %valid, label %store, label %bad\n\
        store:\n  store i64 %epoch, ptr @tz.async.thread, align 8\n  ret void\n\
        bad:\n  call void @llvm.trap()\n  unreachable\n\
}\n"
            .to_owned()
    } else {
        "@tz.async.thread = internal thread_local global i64 0, align 8\n\
        @tz.async.threads = internal global i64 0, align 8\n\n\
        define internal i64 @tz.async.thread_index() nounwind {\n\
        entry:\n  %current = load i64, ptr @tz.async.thread, align 8\n  %known = icmp ne i64 %current, 0\n  \
        br i1 %known, label %done, label %assign\n\
        assign:\n  %previous = atomicrmw add ptr @tz.async.threads, i64 1 monotonic, align 8\n  \
        %fresh = add i64 %previous, 1\n  %valid = icmp ult i64 %fresh, 16777216\n  \
        br i1 %valid, label %store, label %exhausted\n\
        store:\n  store i64 %fresh, ptr @tz.async.thread, align 8\n  br label %done\n\
        exhausted:\n  call void @llvm.trap()\n  unreachable\n\
        done:\n  %index = phi i64 [ %current, %entry ], [ %fresh, %store ]\n  ret i64 %index\n}
"
            .to_owned()
    };
    if !globals
        .definitions
        .iter()
        .any(|global| global == &definition)
    {
        globals.definitions.push(definition);
    }
}

impl FunctionEmitter<'_, '_> {
    /// `Async.__take` and `Async.__put` (B08 Phase 2). A take locks the slot, and only a put
    /// unlocks it, so a take of a taken slot (a re-entrant use of the executor) and a put into an
    /// unlocked slot trap. The slot and its lock are thread-local, so plain accesses suffice.
    fn async_slot_builtin(&mut self, instance: &BuiltinInstance, ty: &Type) -> String {
        let element = &instance.types[0];
        let element_type = self.ty(element);
        let (slot, lock) = async_slot(&canonical_type(element, self.module));
        let Type::Function(parameters, _) = ty else {
            unreachable!("builtin has function type")
        };
        let state = if instance.builtin == Builtin::AsyncTake {
            ty.after_arguments(0)
        } else {
            parameters[0].clone()
        };
        let Type::Union(id, _) = &state else {
            unreachable!("checked state is Maybe")
        };
        let case = |name: &str| {
            self.module.unions[*id]
                .cases
                .iter()
                .position(|(case, _)| case == name)
                .expect("Maybe has None and Some")
        };
        let (none, some) = (case("None"), case("Some"));
        let fail = |emitter: &mut Self, valid: &str| {
            let failed = emitter.label();
            let next = emitter.label();
            emitter.branch(valid, &next, &failed);
            emitter.begin(&failed);
            emitter.instruction("call void @llvm.trap()");
            emitter.instruction("unreachable");
            emitter.begin(&next);
        };
        if instance.builtin == Builtin::AsyncTake {
            let previous = self.value(format!("load i8, ptr {lock}, align 1"));
            self.instruction(format!("store i8 2, ptr {lock}, align 1"));
            let free = self.value(format!("icmp ne i8 {previous}, 2"));
            fail(self, &free);
            let output = self.slot(&state);
            let empty = self.construct_value(&state, none, None);
            self.instruction(format!("store {} {empty}, ptr {output}", self.ty(&state)));
            let full = self.value(format!("icmp eq i8 {previous}, 1"));
            let take = self.label();
            let done = self.label();
            self.branch(&full, &take, &done);
            self.begin(&take);
            let value = self.value(format!("load {element_type}, ptr {slot}, align 16"));
            self.instruction(format!(
                "store {element_type} zeroinitializer, ptr {slot}, align 16"
            ));
            let taken = self.construct_value(&state, some, Some((value, element_type.clone())));
            self.instruction(format!("store {} {taken}, ptr {output}", self.ty(&state)));
            self.jump(&done);
            self.begin(&done);
            self.value(format!("load {}, ptr {output}", self.ty(&state)))
        } else {
            let current = self.value(format!("load i8, ptr {lock}, align 1"));
            let taken = self.value(format!("icmp eq i8 {current}, 2"));
            fail(self, &taken);
            let state_type = self.ty(&state);
            let tag = self.value(format!("extractvalue {state_type} %arg0, 0"));
            let present = self.value(format!("icmp eq i32 {tag}, {some}"));
            let store = self.label();
            let empty = self.label();
            let done = self.label();
            self.branch(&present, &store, &empty);
            self.begin(&store);
            let value = self.payload_value(&state, "%arg0", element);
            self.instruction(format!(
                "store {element_type} {value}, ptr {slot}, align 16"
            ));
            self.instruction(format!("store i8 1, ptr {lock}, align 1"));
            self.jump(&done);
            self.begin(&empty);
            self.instruction(format!("store i8 0, ptr {lock}, align 1"));
            self.jump(&done);
            self.begin(&done);
            "0".to_owned()
        }
    }

    fn sequence_next(&mut self, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("builtin function type");
        };
        let sequence = &parameters[0];
        let Type::Record(id, arguments) = sequence else {
            unreachable!("validated sequence representation");
        };
        let fields = self.module.types().record_fields(*id, arguments);
        let head_type = &fields[0];
        let step_type = &fields[1];
        let result_type = ty.after_arguments(1);
        let callback_type = Type::function(vec![Type::Unit], result_type.clone());
        let result_slot = self.slot(&result_type);
        let head = self.value(format!("extractvalue {} %arg0, 0", self.ty(sequence)));
        let step = self.value(format!("extractvalue {} %arg0, 1", self.ty(sequence)));
        let tag = if self.module.types().recursive(head_type) {
            self.recursive_tag(head_type, &head)
        } else {
            self.value(format!("extractvalue {} {head}, 0", self.ty(head_type)))
        };
        let has_head = self.value(format!("icmp eq i32 {tag}, 1"));
        let ready = self.label();
        let deferred = self.label();
        let invoke = self.label();
        let empty = self.label();
        let done = self.label();
        self.branch(&has_head, &ready, &deferred);
        self.begin(&ready);
        self.drop_value(step_type, &step);
        let result = self.value(format!(
            "insertvalue {} zeroinitializer, {} {head}, 1",
            self.ty(&result_type),
            self.ty(head_type)
        ));
        self.instruction(format!(
            "store {} {result}, ptr {result_slot}",
            self.ty(&result_type)
        ));
        self.jump(&done);
        self.begin(&deferred);
        self.drop_value(head_type, &head);
        let tag = self.value(format!("extractvalue {} {step}, 0", self.ty(step_type)));
        let has_step = self.value(format!("icmp eq i32 {tag}, 1"));
        self.branch(&has_step, &invoke, &empty);
        self.begin(&invoke);
        let callback = self.payload_value(step_type, &step, &callback_type);
        let result = self
            .apply_value(&callback, &callback_type, Some((&Type::Unit, "0")), false)
            .0;
        self.instruction(format!(
            "store {} {result}, ptr {result_slot}",
            self.ty(&result_type)
        ));
        self.jump(&done);
        self.begin(&empty);
        self.instruction(format!(
            "store {} zeroinitializer, ptr {result_slot}",
            self.ty(&result_type)
        ));
        self.jump(&done);
        self.begin(&done);
        self.value(format!("load {}, ptr {result_slot}", self.ty(&result_type)))
    }

    fn integer_builtin(&mut self, instance: &BuiltinInstance, signature: &Type) -> String {
        use Builtin::*;
        let Type::Integer(bits, signed) = instance.types[0] else {
            unreachable!("integer constraints checked")
        };
        let ty = format!("i{bits}");
        let order = if signed { "s" } else { "u" };
        match instance.builtin {
            IntMin | IntMax => {
                let operation = if instance.builtin == IntMin {
                    "min"
                } else {
                    "max"
                };
                let name = format!("@llvm.{order}{operation}.{ty}");
                self.intrinsics
                    .insert(format!("declare {ty} {name}({ty}, {ty})"));
                self.value(format!("call {ty} {name}({ty} %arg0, {ty} %arg1)"))
            }
            IntClamp => {
                let valid = self.value(format!("icmp {order}le {ty} %arg1, %arg2"));
                self.guard(&valid, TrapKind::NumericRuntime);
                let low = self.value(format!("icmp {order}lt {ty} %arg0, %arg1"));
                let value = self.value(format!("select i1 {low}, {ty} %arg1, {ty} %arg0"));
                let high = self.value(format!("icmp {order}gt {ty} {value}, %arg2"));
                self.value(format!("select i1 {high}, {ty} %arg2, {ty} {value}"))
            }
            IntAbs | IntUnsignedAbs => {
                let negative = self.value(format!("icmp slt {ty} %arg0, 0"));
                let negated = self.value(format!("sub {ty} 0, %arg0"));
                self.value(format!("select i1 {negative}, {ty} {negated}, {ty} %arg0"))
            }
            IntAbsDiff => {
                let order = self.value(format!("icmp {order}lt {ty} %arg0, %arg1"));
                let left = self.value(format!("sub {ty} %arg1, %arg0"));
                let right = self.value(format!("sub {ty} %arg0, %arg1"));
                self.value(format!("select i1 {order}, {ty} {left}, {ty} {right}"))
            }
            IntCountOnes | IntLeadingZeros | IntTrailingZeros => {
                let operation = match instance.builtin {
                    IntCountOnes => "ctpop",
                    IntLeadingZeros => "ctlz",
                    _ => "cttz",
                };
                let name = format!("@llvm.{operation}.{ty}");
                let zero = instance.builtin != IntCountOnes;
                self.intrinsics.insert(format!(
                    "declare {ty} {name}({ty}{})",
                    if zero { ", i1" } else { "" }
                ));
                let value = self.value(format!(
                    "call {ty} {name}({ty} %arg0{})",
                    if zero { ", i1 false" } else { "" }
                ));
                if bits == 64 {
                    value
                } else {
                    self.value(format!(
                        "{} {ty} {value} to i64",
                        if bits < 64 { "zext" } else { "trunc" }
                    ))
                }
            }
            IntRotateLeft | IntRotateRight => {
                let amount = self.value(format!("and i64 %arg1, {}", bits - 1));
                let amount = if bits == 64 {
                    amount
                } else {
                    self.value(format!(
                        "{} i64 {amount} to {ty}",
                        if bits < 64 { "trunc" } else { "zext" }
                    ))
                };
                let operation = if instance.builtin == IntRotateLeft {
                    "fshl"
                } else {
                    "fshr"
                };
                let name = format!("@llvm.{operation}.{ty}");
                self.intrinsics
                    .insert(format!("declare {ty} {name}({ty}, {ty}, {ty})"));
                self.value(format!(
                    "call {ty} {name}({ty} %arg0, {ty} %arg0, {ty} {amount})"
                ))
            }
            IntSwapBytes if bits == 8 => "%arg0".into(),
            IntSwapBytes | IntReverseBits => {
                let operation = if instance.builtin == IntSwapBytes {
                    "bswap"
                } else {
                    "bitreverse"
                };
                let name = format!("@llvm.{operation}.{ty}");
                self.intrinsics.insert(format!("declare {ty} {name}({ty})"));
                self.value(format!("call {ty} {name}({ty} %arg0)"))
            }
            IntIsPowerOfTwo => {
                let nonzero = self.value(format!("icmp ne {ty} %arg0, 0"));
                let previous = self.value(format!("sub {ty} %arg0, 1"));
                let common = self.value(format!("and {ty} %arg0, {previous}"));
                let single = self.value(format!("icmp eq {ty} {common}, 0"));
                self.value(format!("and i1 {nonzero}, {single}"))
            }
            IntWideningMul => {
                let result = self.ty(&signature.after_arguments(2));
                let extension = if signed { "sext" } else { "zext" };
                let left = self.value(format!("{extension} {ty} %arg0 to {result}"));
                let right = self.value(format!("{extension} {ty} %arg1 to {result}"));
                self.value(format!("mul {result} {left}, {right}"))
            }
            IntSaturatingAdd | IntSaturatingSub => {
                let operation = if instance.builtin == IntSaturatingAdd {
                    "add"
                } else {
                    "sub"
                };
                let name = format!("@llvm.{order}{operation}.sat.{ty}");
                self.intrinsics
                    .insert(format!("declare {ty} {name}({ty}, {ty})"));
                self.value(format!("call {ty} {name}({ty} %arg0, {ty} %arg1)"))
            }
            IntCheckedAdd | IntCheckedSub | IntCheckedMul | IntSaturatingMul | IntCheckedNeg => {
                let operation = match instance.builtin {
                    IntCheckedAdd => "add",
                    IntCheckedSub | IntCheckedNeg => "sub",
                    _ => "mul",
                };
                let (left, right) = if instance.builtin == IntCheckedNeg {
                    ("0", "%arg0")
                } else {
                    ("%arg0", "%arg1")
                };
                let (value, overflow) =
                    self.checked_integer_arithmetic(bits, signed, operation, left, right);
                if instance.builtin == IntSaturatingMul {
                    let maximum = if signed {
                        (1u128 << (bits - 1)) - 1
                    } else {
                        u128::MAX >> (128 - bits)
                    };
                    let limit = if signed {
                        let signs = self.value(format!("xor {ty} {left}, {right}"));
                        let negative = self.value(format!("icmp slt {ty} {signs}, 0"));
                        self.value(format!(
                            "select i1 {negative}, {ty} {}, {ty} {maximum}",
                            1u128 << (bits - 1)
                        ))
                    } else {
                        maximum.to_string()
                    };
                    self.value(format!("select i1 {overflow}, {ty} {limit}, {ty} {value}"))
                } else {
                    let result =
                        signature.after_arguments(instance.builtin.scheme().parameters.len());
                    self.integer_none_on(&result, &overflow);
                    self.integer_some(&result, &instance.types[0], &value)
                }
            }
            IntCheckedDiv | IntCheckedRem => {
                let result = signature.after_arguments(2);
                let mut invalid = self.value(format!("icmp eq {ty} %arg1, 0"));
                if signed {
                    let minimum =
                        self.value(format!("icmp eq {ty} %arg0, {}", 1u128 << (bits - 1)));
                    let minus_one = self.value(format!("icmp eq {ty} %arg1, -1"));
                    let overflow = self.value(format!("and i1 {minimum}, {minus_one}"));
                    invalid = self.value(format!("or i1 {invalid}, {overflow}"));
                }
                self.integer_none_on(&result, &invalid);
                let operation = if instance.builtin == IntCheckedDiv {
                    "div"
                } else {
                    "rem"
                };
                let value = self.value(format!("{order}{operation} {ty} %arg0, %arg1"));
                self.integer_some(&result, &instance.types[0], &value)
            }
            IntWrappingPow | IntCheckedPow => self.integer_power(
                &instance.types[0],
                &signature.after_arguments(2),
                instance.builtin == IntCheckedPow,
            ),
            _ => unreachable!("integer builtins only"),
        }
    }

    fn integer_none_on(&mut self, result: &Type, invalid: &str) {
        let Type::Union(id, _) = result else {
            unreachable!("checked result is Maybe")
        };
        let none = self.module.unions[*id]
            .cases
            .iter()
            .position(|(name, _)| name == "None")
            .expect("Maybe.None exists");
        let failure = self.label();
        let success = self.label();
        self.branch(invalid, &failure, &success);
        self.begin(&failure);
        let value = self.construct_value(result, none, None);
        self.instruction(format!("ret {} {value}", self.ty(result)));
        self.begin(&success);
    }

    fn integer_some(&mut self, result: &Type, element: &Type, value: &str) -> String {
        let Type::Union(id, _) = result else {
            unreachable!("checked result is Maybe")
        };
        let some = self.module.unions[*id]
            .cases
            .iter()
            .position(|(name, _)| name == "Some")
            .expect("Maybe.Some exists");
        self.construct_value(result, some, Some((value.to_owned(), self.ty(element))))
    }

    fn checked_integer_arithmetic(
        &mut self,
        bits: u16,
        signed: bool,
        operation: &str,
        left: &str,
        right: &str,
    ) -> (String, String) {
        let ty = format!("i{bits}");
        if bits == 128 && operation == "mul" {
            let magnitude = |emitter: &mut Self, value: &str| {
                if !signed {
                    return value.to_owned();
                }
                let negative = emitter.value(format!("icmp slt i128 {value}, 0"));
                let negated = emitter.value(format!("sub i128 0, {value}"));
                emitter.value(format!(
                    "select i1 {negative}, i128 {negated}, i128 {value}"
                ))
            };
            let left_abs = magnitude(self, left);
            let right_abs = magnitude(self, right);
            let left_low = self.value(format!("and i128 {left_abs}, 18446744073709551615"));
            let left_high = self.value(format!("lshr i128 {left_abs}, 64"));
            let right_low = self.value(format!("and i128 {right_abs}, 18446744073709551615"));
            let right_high = self.value(format!("lshr i128 {right_abs}, 64"));
            let low = self.value(format!("mul i128 {left_low}, {right_low}"));
            let cross_left = self.value(format!("mul i128 {left_high}, {right_low}"));
            let cross_right = self.value(format!("mul i128 {left_low}, {right_high}"));
            let left_nonzero = self.value(format!("icmp ne i128 {left_high}, 0"));
            let right_nonzero = self.value(format!("icmp ne i128 {right_high}, 0"));
            let high_product = self.value(format!("and i1 {left_nonzero}, {right_nonzero}"));
            let high_left = self.value(format!("icmp ugt i128 {cross_left}, 18446744073709551615"));
            let high_right =
                self.value(format!("icmp ugt i128 {cross_right}, 18446744073709551615"));
            let low_carry = self.value(format!("lshr i128 {low}, 64"));
            let cross_left_low = self.value(format!("and i128 {cross_left}, 18446744073709551615"));
            let cross_right_low =
                self.value(format!("and i128 {cross_right}, 18446744073709551615"));
            let high = self.value(format!("add i128 {low_carry}, {cross_left_low}"));
            let high = self.value(format!("add i128 {high}, {cross_right_low}"));
            let carry = self.value(format!("icmp ugt i128 {high}, 18446744073709551615"));
            let overflow = self.value(format!("or i1 {high_product}, {high_left}"));
            let overflow = self.value(format!("or i1 {overflow}, {high_right}"));
            let mut overflow = self.value(format!("or i1 {overflow}, {carry}"));
            if signed {
                let signs = self.value(format!("xor i128 {left}, {right}"));
                let negative = self.value(format!("icmp slt i128 {signs}, 0"));
                let limit = self.value(format!(
                    "select i1 {negative}, i128 {}, i128 {}",
                    1u128 << 127,
                    (1u128 << 127) - 1
                ));
                let magnitude = self.value(format!("mul i128 {left_abs}, {right_abs}"));
                let too_large = self.value(format!("icmp ugt i128 {magnitude}, {limit}"));
                overflow = self.value(format!("or i1 {overflow}, {too_large}"));
            }
            let value = self.value(format!("mul i128 {left}, {right}"));
            return (value, overflow);
        }
        let name = format!(
            "@llvm.{}{operation}.with.overflow.{ty}",
            if signed { "s" } else { "u" }
        );
        self.intrinsics
            .insert(format!("declare {{ {ty}, i1 }} {name}({ty}, {ty})"));
        let pair = self.value(format!(
            "call {{ {ty}, i1 }} {name}({ty} {left}, {ty} {right})"
        ));
        (
            self.value(format!("extractvalue {{ {ty}, i1 }} {pair}, 0")),
            self.value(format!("extractvalue {{ {ty}, i1 }} {pair}, 1")),
        )
    }

    fn integer_power(&mut self, element: &Type, result: &Type, checked: bool) -> String {
        let Type::Integer(bits, signed) = *element else {
            unreachable!("integer power type checked")
        };
        let ty = self.ty(element);
        let negative = self.value("icmp slt i64 %arg1, 0");
        if checked {
            self.integer_none_on(result, &negative);
        } else {
            let valid = self.value(format!("xor i1 {negative}, true"));
            self.guard(&valid, TrapKind::NumericRuntime);
        }
        let accumulator = self.spill(element, "1");
        let base = self.spill(element, "%arg0");
        let exponent = self.spill(&Type::I64, "%arg1");
        let test = self.label();
        let work = self.label();
        let multiply = self.label();
        let advance = self.label();
        let square = self.label();
        let done = self.label();
        self.jump(&test);
        self.begin(&test);
        let remaining = self.value(format!("load i64, ptr {exponent}"));
        let more = self.value(format!("icmp ne i64 {remaining}, 0"));
        self.branch(&more, &work, &done);
        self.begin(&work);
        let low = self.value(format!("and i64 {remaining}, 1"));
        let odd = self.value(format!("icmp ne i64 {low}, 0"));
        self.branch(&odd, &multiply, &advance);
        self.begin(&multiply);
        let value = self.value(format!("load {ty}, ptr {accumulator}"));
        let factor = self.value(format!("load {ty}, ptr {base}"));
        let product = if checked {
            let (product, overflow) =
                self.checked_integer_arithmetic(bits, signed, "mul", &value, &factor);
            self.integer_none_on(result, &overflow);
            product
        } else {
            self.value(format!("mul {ty} {value}, {factor}"))
        };
        self.instruction(format!("store {ty} {product}, ptr {accumulator}"));
        self.jump(&advance);
        self.begin(&advance);
        let remaining = self.value(format!("lshr i64 {remaining}, 1"));
        self.instruction(format!("store i64 {remaining}, ptr {exponent}"));
        let more = self.value(format!("icmp ne i64 {remaining}, 0"));
        self.branch(&more, &square, &done);
        self.begin(&square);
        let factor = self.value(format!("load {ty}, ptr {base}"));
        let product = if checked {
            let (product, overflow) =
                self.checked_integer_arithmetic(bits, signed, "mul", &factor, &factor);
            self.integer_none_on(result, &overflow);
            product
        } else {
            self.value(format!("mul {ty} {factor}, {factor}"))
        };
        self.instruction(format!("store {ty} {product}, ptr {base}"));
        self.jump(&test);
        self.begin(&done);
        let value = self.value(format!("load {ty}, ptr {accumulator}"));
        if checked {
            self.integer_some(result, element, &value)
        } else {
            value
        }
    }
}

fn emit_character_conversion(
    instance: &BuiltinInstance,
    ty: &Type,
    module: &CheckedModule,
) -> String {
    let symbol = builtin_symbol(instance, module);
    let result = ty.after_arguments(1);
    let llvm = llvm_type(&result, module);
    let input = if matches!(instance.builtin, Builtin::CharToU16 | Builtin::CharOfU16) {
        "i16"
    } else {
        "i32"
    };
    let body = if matches!(
        instance.builtin,
        Builtin::Utf8CharOfU32 | Builtin::Utf8CharOfU32Unchecked
    ) {
        let mut body = String::from(
            "  %below = icmp ult i32 %value, 55296\n  %above = icmp ugt i32 %value, 57343\n  %outside = or i1 %below, %above\n  %within = icmp ule i32 %value, 1114111\n  %valid = and i1 %outside, %within\n  br i1 %valid, label %valid_value, label %invalid\ninvalid:\n",
        );
        if instance.builtin == Builtin::Utf8CharOfU32 {
            let Type::Union(id, _) = &result else {
                unreachable!()
            };
            let cases = &module.unions[*id].cases;
            let none = cases.iter().position(|(name, _)| name == "None").unwrap();
            let some = cases.iter().position(|(name, _)| name == "Some").unwrap();
            let _ = writeln!(
                body,
                "  %none = insertvalue {llvm} zeroinitializer, i32 {none}, 0\n  ret {llvm} %none\nvalid_value:\n  %some = insertvalue {llvm} zeroinitializer, i32 {some}, 0\n  %result = insertvalue {llvm} %some, i32 %value, 1\n  ret {llvm} %result"
            );
        } else {
            body.push_str(
                "  call void @llvm.trap()\n  unreachable\nvalid_value:\n  ret i32 %value\n",
            );
        }
        body
    } else {
        format!("  ret {llvm} %value\n")
    };
    format!("define internal {llvm} {symbol}({input} %value) nounwind {{\nentry:\n{body}}}\n\n")
}

fn emit_display(instance: &BuiltinInstance, module: &CheckedModule) -> String {
    let symbol = builtin_symbol(instance, module);
    let ty = &instance.types[0];
    let value_type = llvm_type(ty, module);
    let borrowed = instance.builtin == Builtin::Display;
    let parameter = if borrowed { "ptr" } else { &value_type };
    let mut globals = String::new();
    let mut body = String::new();
    match ty {
        Type::Char | Type::Utf8Char => {
            let value = if borrowed {
                let _ = writeln!(body, "  %value = load {value_type}, ptr %x");
                "%value"
            } else {
                "%x"
            };
            let value = if *ty == Type::Char {
                let _ = writeln!(body, "  %wide = zext i16 {value} to i32");
                "%wide"
            } else {
                value
            };
            let _ = writeln!(
                body,
                "  %result = call %tz.string @tz.character.display(i32 {value})\n  ret %tz.string %result"
            );
        }
        Type::String if !borrowed => body.push_str("  ret %tz.string %x\n"),
        Type::String => body.push_str(
            "  %s = load %tz.string, ptr %x\n\
               %data = extractvalue %tz.string %s, 0\n\
               %length = extractvalue %tz.string %s, 1\n\
               %r = call %tz.string @tz.string.new(ptr %data, i64 %length)\n\
               ret %tz.string %r\n",
        ),
        Type::Utf8String => {
            let value = if borrowed {
                body.push_str("  %s = load %tz.utf8string, ptr %x\n");
                "%s"
            } else {
                "%x"
            };
            let _ = writeln!(
                body,
                "  %data = extractvalue %tz.utf8string {value}, 0\n\
                   %length = extractvalue %tz.utf8string {value}, 1\n\
                   %r = call %tz.string @tz.string.from_utf8(ptr %data, i64 %length)"
            );
            if !borrowed {
                body.push_str("  call void @tz.free(ptr %data)\n");
            }
            body.push_str("  ret %tz.string %r\n");
        }
        Type::Bool => {
            let _ = writeln!(
                globals,
                "{symbol}.true = private unnamed_addr constant [4 x i16] [i16 116, i16 114, i16 117, i16 101]\n\
                 {symbol}.false = private unnamed_addr constant [5 x i16] [i16 102, i16 97, i16 108, i16 115, i16 101]"
            );
            let value = if borrowed {
                body.push_str("  %value = load i1, ptr %x\n");
                "%value"
            } else {
                "%x"
            };
            let _ = writeln!(
                body,
                "  %data = select i1 {value}, ptr {symbol}.true, ptr {symbol}.false\n\
                   %length = select i1 {value}, i64 4, i64 5\n\
                   %r = call %tz.string @tz.string.new(ptr %data, i64 %length)\n\
                   ret %tz.string %r"
            );
        }
        Type::Unit => {
            let _ = writeln!(
                globals,
                "{symbol}.unit = private unnamed_addr constant [2 x i16] [i16 40, i16 41]"
            );
            let _ = writeln!(
                body,
                "  %r = call %tz.string @tz.string.new(ptr {symbol}.unit, i64 2)\n\
                   ret %tz.string %r"
            );
        }
        _ => {
            let pointer = if borrowed {
                "%x"
            } else {
                let _ = writeln!(
                    body,
                    "  %slot = alloca {value_type}, align 16\n  store {value_type} %x, ptr %slot"
                );
                "%slot"
            };
            let _ = writeln!(
                body,
                "  %buffer = alloca [128 x i8], align 16\n\
                   %count = call i32 @tz_soft_format(ptr %buffer, ptr {pointer}, i32 {})\n\
                   %length = zext i32 %count to i64\n\
                   %r = call %tz.string @tz.string.from_ascii(ptr %buffer, i64 %length)\n\
                   ret %tz.string %r",
                numeric_kind(ty),
            );
        }
    }
    format!(
        "{globals}define internal %tz.string {symbol}({parameter} %x) nounwind {{\nentry:\n{body}}}\n\n"
    )
}

fn emit_parse(instance: &BuiltinInstance, ty: &Type, module: &CheckedModule) -> String {
    let symbol = builtin_symbol(instance, module);
    let payload = &instance.types[0];
    let value_type = llvm_type(payload, module);
    let result = ty.after_arguments(1);
    let result_type = llvm_type(&result, module);
    let Type::Union(id, _) = result else {
        unreachable!("Parse returns the checked standard Maybe union")
    };
    let cases = &module.unions[id].cases;
    let some = cases.iter().position(|(name, _)| name == "Some").unwrap();
    let none = cases.iter().position(|(name, _)| name == "None").unwrap();
    let mut globals = String::new();
    let mut body = String::from("  %text = load %tz.string, ptr %x\n");
    if matches!(payload, Type::Char | Type::Utf8Char) {
        let _ = writeln!(
            body,
            "  %parsed = call i32 @tz.character.parse(ptr %x, i1 {})\n  %ok = icmp sge i32 %parsed, 0\n  br i1 %ok, label %some, label %none\nsome:",
            *payload == Type::Utf8Char
        );
        body.push_str(if *payload == Type::Char {
            "  %value = trunc i32 %parsed to i16\n"
        } else {
            "  %value = add i32 %parsed, 0\n"
        });
    } else if *payload == Type::Bool {
        let _ = writeln!(
            globals,
            "{symbol}.true = private unnamed_addr constant [4 x i16] [i16 116, i16 114, i16 117, i16 101]\n\
             {symbol}.false = private unnamed_addr constant [5 x i16] [i16 102, i16 97, i16 108, i16 115, i16 101]"
        );
        let _ = writeln!(
            body,
            "  %value = call i1 @tz.string.equal(%tz.string %text, %tz.string {{ ptr {symbol}.true, i64 4 }})\n\
               %false = call i1 @tz.string.equal(%tz.string %text, %tz.string {{ ptr {symbol}.false, i64 5 }})\n\
               %ok = or i1 %value, %false\n\
               br i1 %ok, label %some, label %none\nsome:"
        );
    } else {
        let _ = writeln!(
            body,
            "  %data = extractvalue %tz.string %text, 0\n\
               %length = extractvalue %tz.string %text, 1\n\
               %slot = alloca {value_type}, align 16\n\
               %buffer = alloca [4096 x i8], align 16\n\
               %ascii = call i1 @tz.string.to_ascii(ptr %buffer, ptr %data, i64 %length)\n\
               br i1 %ascii, label %parse, label %none\nparse:\n\
               %parsed = call i32 @tz_soft_parse(ptr %slot, ptr %buffer, i64 %length, i32 {})\n\
               %ok = icmp ne i32 %parsed, 0\n\
               br i1 %ok, label %some, label %none\nsome:\n\
               %value = load {value_type}, ptr %slot",
            numeric_kind(payload),
        );
    }
    let _ = writeln!(
        body,
        "  %tag = insertvalue {result_type} zeroinitializer, i32 {some}, 0\n\
           %result = insertvalue {result_type} %tag, {value_type} %value, 1\n\
           ret {result_type} %result\nnone:\n\
           %empty = insertvalue {result_type} zeroinitializer, i32 {none}, 0\n\
           ret {result_type} %empty"
    );
    format!(
        "{globals}define internal {result_type} {symbol}(ptr %x) nounwind {{\nentry:\n{body}}}\n\n"
    )
}

/// Defines the test-only builtins that exercise polymorphic instances.
#[cfg(test)]
fn test_builtin(
    instance: &BuiltinInstance,
    ty: &Type,
    symbol: &str,
    result: &str,
    module: &CheckedModule,
) -> Option<String> {
    let Type::Function(parameters, _) = ty else {
        unreachable!("a builtin is a function")
    };
    let parameter = llvm_type(&parameters[0], module);
    let body = match instance.builtin {
        Builtin::TestAdd => {
            return Some(format!(
                "define internal {result} {symbol}({parameter} %x, {parameter} %y) nounwind {{\n\
                 entry:\n  %r = add {parameter} %x, %y\n  ret {result} %r\n}}\n\n"
            ));
        }
        Builtin::TestUnsigned => format!("ret {result} %x"),
        Builtin::TestWiden => {
            let Type::Integer(_, signed) = instance.types[0] else {
                unreachable!("Int.test_widen takes an integer")
            };
            let extend = if signed { "sext" } else { "zext" };
            format!("%r = {extend} {parameter} %x to {result}\n  ret {result} %r")
        }
        _ => return None,
    };
    Some(format!(
        "define internal {result} {symbol}({parameter} %x) nounwind {{\nentry:\n  {body}\n}}\n\n"
    ))
}

/// The C `main` of a `def main` entry, which returns `main`'s value as the exit code.
/// `Array<string> -> i32` receives the arguments that `src/runtime/arguments.c` reads.
fn console_def_main(module: &CheckedModule, uses_args: bool) -> String {
    let main = &module.functions[module.entry.unwrap()];
    let mut body = String::new();
    if uses_args {
        body.push_str("  call void @tsuzuri_os_set_args(i32 %argc, ptr %argv)\n");
    }
    let mut output = String::new();
    let argument = if main_arguments(module) {
        output.push_str("declare void @tsuzuri_arguments(i32, ptr, ptr)\n");
        body.push_str(
            "  %slot = alloca %tz.array, align 8\n  call void @tsuzuri_arguments(i32 %argc, ptr %argv, ptr %slot)\n  %arguments = load %tz.array, ptr %slot\n",
        );
        "%tz.array %arguments"
    } else {
        "i8 0"
    };
    let parameters = if body.is_empty() {
        ""
    } else {
        "i32 %argc, ptr %argv"
    };
    let _ = write!(
        output,
        "define i32 @main({parameters}) {{\nentry:\n{body}  %code = call i32 @tz.fn.{}({argument})\n  ret i32 %code\n}}\n",
        main.qualified_name()
    );
    output
}

/// The `tsuzuri_main` that a host calls to run a `def main` from a library or a WASM
/// module. A WASI module reads its arguments in `src/runtime/arguments.c`, which replaces the
/// weak empty `tsuzuri_arguments` here; other hosts pass none.
fn library_def_main(module: &CheckedModule, wasm: bool) -> String {
    let main = &module.functions[module.entry.unwrap()];
    let mut output = String::new();
    let mut body = String::new();
    let argument = if main_arguments(module) {
        let empty = "  %data = call ptr @tz.alloc(i64 1)\n  %empty = insertvalue %tz.array zeroinitializer, ptr %data, 0\n";
        if wasm {
            let _ = write!(
                output,
                "define weak void @tsuzuri_arguments(i32 %count, ptr %values, ptr %output) {{\nentry:\n{empty}  store %tz.array %empty, ptr %output\n  ret void\n}}\n"
            );
            body.push_str(
                "  %slot = alloca %tz.array, align 8\n  call void @tsuzuri_arguments(i32 0, ptr null, ptr %slot)\n  %arguments = load %tz.array, ptr %slot\n",
            );
            "%tz.array %arguments"
        } else {
            body.push_str(empty);
            "%tz.array %empty"
        }
    } else {
        "i8 0"
    };
    let _ = write!(
        output,
        "define i32 @tsuzuri_main() {{\nentry:\n{body}  %code = call i32 @tz.fn.{}({argument})\n  ret i32 %code\n}}\n",
        main.qualified_name()
    );
    output
}

fn console_main(module: &CheckedModule, uses_args: bool) -> String {
    if main_entry(module) {
        return console_def_main(module, uses_args);
    }
    if io_entry(module) {
        if uses_args {
            return "define i32 @main(i32 %argc, ptr %argv) {\nentry:\n  call void @tsuzuri_os_set_args(i32 %argc, ptr %argv)\n  %result = call i32 @tsuzuri_main()\n  ret i32 %result\n}\n".into();
        }
        return "define i32 @main() {\nentry:\n  %result = call i32 @tsuzuri_main()\n  ret i32 %result\n}\n".into();
    }
    let main = &module.functions[module.entry.unwrap()];
    let ty = &main.signature.result;
    let mut output = match ty {
        Type::Bool => String::from(
            "@tz.true = private unnamed_addr constant [4 x i8] c\"true\"\n\
             @tz.false = private unnamed_addr constant [5 x i8] c\"false\"\n",
        ),
        _ => String::new(),
    };
    if *ty != Type::Unit {
        output.push_str(include_str!("runtime/console.ll"));
    }
    let _ = writeln!(
        output,
        "define i32 @main() {{\nentry:\n  %result = call {} @tz.fn.{}()",
        llvm_type(ty, module),
        main.qualified_name()
    );
    match ty {
        Type::Char | Type::Utf8Char => {
            output.push_str("  %buffer = alloca [4 x i8], align 4\n");
            let value = if *ty == Type::Char {
                output.push_str("  %wide = zext i16 %result to i32\n");
                "%wide"
            } else {
                "%result"
            };
            let _ = writeln!(
                output,
                "  %count = call i32 @tz.character.utf8(ptr %buffer, i32 {value})\n  %length = zext i32 %count to i64\n  %printed = call i32 @tz.console.write(ptr %buffer, i64 %length)"
            );
        }
        Type::Bool => {
            output.push_str(
                "  %text = select i1 %result, ptr @tz.true, ptr @tz.false\n\
                   %length = select i1 %result, i64 4, i64 5\n\
                   %printed = call i32 @tz.console.write(ptr %text, i64 %length)\n",
            );
        }
        Type::String => {
            output.push_str(
                "  %data = extractvalue %tz.string %result, 0\n\
                   %length = extractvalue %tz.string %result, 1\n\
                   %utf8 = call %tz.utf8string @tz.utf8string.from_string(ptr %data, i64 %length)\n\
                   %bytes = extractvalue %tz.utf8string %utf8, 0\n\
                   %byte_length = extractvalue %tz.utf8string %utf8, 1\n\
                   %printed = call i32 @tz.console.write(ptr %bytes, i64 %byte_length)\n\
                   call void @tz.free(ptr %bytes)\n\
                   call void @tz.free(ptr %data)\n",
            );
        }
        Type::Utf8String => {
            output.push_str(
                "  %data = extractvalue %tz.utf8string %result, 0\n\
                   %length = extractvalue %tz.utf8string %result, 1\n\
                   %printed = call i32 @tz.console.write(ptr %data, i64 %length)\n\
                   call void @tz.free(ptr %data)\n",
            );
        }
        Type::Unit => {}
        _ if ty.is_numeric() => {
            let _ = writeln!(
                output,
                "  %slot = alloca {}, align 16\n  %buffer = alloca [128 x i8], align 16\n  store {} %result, ptr %slot\n  %count = call i32 @tz_soft_format(ptr %buffer, ptr %slot, i32 {})\n  %length = zext i32 %count to i64\n  %printed = call i32 @tz.console.write(ptr %buffer, i64 %length)",
                llvm_type(ty, module),
                llvm_type(ty, module),
                numeric_kind(ty)
            );
        }
        _ => unreachable!("main validated"),
    }
    if *ty == Type::Unit {
        output.push_str("  ret i32 0\n");
    } else {
        output.push_str(
            "  %failed = icmp slt i32 %printed, 0\n  %exit = zext i1 %failed to i32\n  ret i32 %exit\n",
        );
    }
    output.push_str("}\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;

    #[test]
    fn net_imports_name_only_the_declared_primitives() {
        let ir = "declare i64 @tsuzuri_net_open(ptr, i32, i64, i64, i64, i64)\n\
                  declare void @tsuzuri_net_unwatch(i64)\n\
                  declare i64 @other(i64)\n\
                  define i64 @tsuzuri_net_open_like() {\n  call i64 @tsuzuri_net_open(ptr null, i32 0, i64 0, i64 0, i64 0, i64 0)\n}\n";
        let lowered = with_net_imports(ir);
        assert!(lowered.contains(
            "declare i64 @tsuzuri_net_open(ptr, i32, i64, i64, i64, i64) \"wasm-import-module\"=\"tsuzuri_net\" \"wasm-import-name\"=\"open\"\n"
        ));
        assert!(lowered.contains(
            "declare void @tsuzuri_net_unwatch(i64) \"wasm-import-module\"=\"tsuzuri_net\" \"wasm-import-name\"=\"unwatch\"\n"
        ));
        // Calls, definitions, and other declarations stay as they are.
        assert!(lowered.contains("declare i64 @other(i64)\n"));
        assert!(lowered.contains(
            "define i64 @tsuzuri_net_open_like() {\n  call i64 @tsuzuri_net_open(ptr null"
        ));
        assert_eq!(lowered.matches("wasm-import-module").count(), 2);
        assert_eq!(with_net_imports("no declarations\n"), "no declarations\n");
    }

    #[test]
    fn emits_explicit_tail_loop_and_checked_arithmetic() {
        let module = analyze(
            "fn rec sum(n: i64, acc: i64) -> i64 {
                if n == 0 { acc / 2 } else { sum(n - 1, acc + n) }
             }
             export fn run() -> i64 { sum(100, 0) }
             run()",
        )
        .unwrap();
        let ir = emit(&module, Entry::Console).unwrap();
        let body = ir
            .split("define internal i64 @tz.fn.Main.sum(")
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        assert!(!body.contains("call i64 @tz.fn.Main.sum"));
        assert!(ir.contains("phi i64"));
        assert!(ir.contains("call void @llvm.trap()"));
        assert!(ir.contains("define i64 @tz_run()"));
        assert!(ir.contains("define i32 @main()"));
        assert!(!body.contains(" nsw "));
        assert!(!body.contains(" nuw "));
        assert_eq!(ir, emit(&module, Entry::Console).unwrap());
    }

    #[test]
    fn puts_array_storage_before_the_tail_loop() {
        let module = analyze(
            "fn rec f(n: i64, a: [i64]) -> i64 {
                let first = a[n & 1];
                if n == 0 { first } else { f(n - 1, [first, 2]) }
             }",
        )
        .unwrap();
        let ir = emit(&module, Entry::Library).unwrap();
        assert!(ir.find("alloca").unwrap() < ir.find("br label %loop").unwrap());
        let body = ir
            .split("define internal i64 @tz.fn.Main.f(")
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        assert!(!body.contains("call i64 @tz.fn.Main.f"));
    }

    #[test]
    fn finds_the_programs_whose_stack_can_overflow() {
        let recursive =
            |source: &str| has_recursion(&emit(&checked(source), Entry::Console).unwrap());
        assert!(recursive(
            "fn rec depth(n: i64) -> i64 { if n == 0 { 0 } else { depth(n - 1) * 3 + n } }
             export fn run() -> i64 { depth(10) }
             run()"
        ));
        assert!(recursive(
            "fn rec even(n: i64) -> bool { if n == 0 { true } else { odd(n - 1) } }
             fn rec odd(n: i64) -> bool { if n == 0 { false } else { even(n - 1) } }
             export fn run() -> i64 { if even(10) { 1 } else { 0 } }
             run()"
        ));
        // A tail call becomes a loop, and a program without calls to itself has nothing to exhaust.
        assert!(!recursive(
            "fn rec sum(n: i64, acc: i64) -> i64 { if n == 0 { acc } else { sum(n - 1, acc + n) } }
             export fn run() -> i64 { sum(100, 0) }
             run()"
        ));
        assert!(!recursive("export fn run() -> i64 { 6 * 7 }\nrun()"));
        // Only the program's own functions count, whatever the text around them looks like.
        assert!(!has_recursion(
            "define internal i64 @tz_soft_format(i64 %x) {\nentry:\n  %v = call i64 @tz_soft_format(i64 %x)\n  ret i64 %v\n}\n"
        ));
        assert!(has_recursion(
            "define internal i64 @tz.fn.A(i64 %x) {\nentry:\n  %v = call i64 @tz.apply.B(i64 %x)\n  ret i64 %v\n}\n\
             define internal i64 @tz.apply.B(i64 %x) {\nentry:\n  %v = tail call i64 @tz.fn.A(i64 %x)\n  ret i64 %v\n}\n"
        ));
    }

    #[test]
    fn emits_portable_bool_abi_and_c_header() {
        let module = analyze("export fn invert(x: bool) -> bool { !x }").unwrap();
        let ir = emit(&module, Entry::Library).unwrap();
        assert!(ir.contains("define i32 @tz_invert(i32 %arg0)"));
        assert!(ir.contains("icmp ne i32 %arg0, 0"));
        assert!(header(&module).contains("int32_t tz_invert(int32_t arg0);"));
        assert!(emit(&module, Entry::Console).is_err());
    }

    fn checked(source: &str) -> CheckedModule {
        analyze(source)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
    }

    fn library(source: &str) -> String {
        let module = checked(source);
        let ir = emit(&module, Entry::Library).unwrap();
        assert_eq!(ir, emit(&module, Entry::Library).unwrap(), "{source}");
        ir
    }

    #[test]
    fn emits_distinct_string_literals_and_indexing() {
        let ir = library(
            "def wide :: i64 -> i16u\nfn wide index = { let text = \"A😀\"; text[index] }\n\
             def narrow :: i64 -> i8u\nfn narrow index = { let text = u8\"A😀\"; text[index] }\n\
             def moved :: utf8string\nfn moved = { let text = u8\"A😀\"; text }\n\
             def ordered :: string -> string -> bool\nfn ordered left right = left < right\n\
             def same :: utf8string -> utf8string -> bool\nfn same left right = left == right",
        );
        assert!(ir.contains("constant [3 x i16] [i16 65, i16 55357, i16 56832]"));
        assert!(ir.contains("constant [5 x i8] c\"\\41\\F0\\9F\\98\\80\""));
        for (function, element) in [("wide", "i16"), ("narrow", "i8")] {
            let start = format!("define internal {element} @tz.fn.Main.{function}(");
            let body = ir
                .split(&start)
                .nth(1)
                .unwrap()
                .split("\n}")
                .next()
                .unwrap();
            assert!(body.contains("icmp ult i64"));
            assert!(body.contains(&format!("getelementptr inbounds {element},")));
            assert!(body.contains(&format!("load {element},")));
        }
        assert!(ir.contains("call %tz.utf8string @tz.utf8string.new("));
        assert!(ir.contains("call i32 @tz.string.compare("));
        assert!(ir.contains("call i1 @tz.utf8string.equal("));
        assert_eq!(ir.matches("declare void @llvm.trap()").count(), 1);
    }

    #[test]
    fn emits_utf16_display_and_ascii_parse() {
        let ir = library(
            "let bytes = u8\"42\"\n\
             let text = Display.display ref bytes\n\
             let consumed = to_string bytes\n\
             let number = to_string 42\n\
             let parsed: Maybe<i64> = Parse.parse ref number\n\
             let truth = to_string true\n\
             let parsed_bool: Maybe<bool> = Parse.parse ref truth\n0",
        );
        for (name, consumed) in [("display", false), ("to_string", true)] {
            let start = format!("define internal %tz.string @tz.builtin.{name}.utf8string(");
            let body = ir
                .split(&start)
                .nth(1)
                .unwrap()
                .split("\n}")
                .next()
                .unwrap();
            assert!(body.contains("call %tz.string @tz.string.from_utf8("));
            assert_eq!(body.contains("call void @tz.free(ptr %data)"), consumed);
        }
        assert!(ir.contains("call %tz.string @tz.string.from_ascii(ptr %buffer, i64 %length)"));
        assert!(ir.contains("constant [4 x i16] [i16 116, i16 114, i16 117, i16 101]"));
        assert!(ir.contains("call i1 @tz.string.to_ascii(ptr %buffer, ptr %data, i64 %length)"));
        assert!(ir.contains("br i1 %ascii, label %parse, label %none"));
        assert!(ir.contains("call i32 @tz_soft_parse(ptr %slot, ptr %buffer, i64 %length"));
        assert_eq!(ir.matches("declare void @llvm.trap()").count(), 1);
    }

    #[test]
    fn applies_multi_argument_constrained_builtins_like_functions() {
        let add = "define internal i32 @tz.builtin.Int.test_add.i32(i32 %x, i32 %y) nounwind";
        for source in [
            "export def f :: i32 -> i32\nfn f x = Int.test_add x 1i32",
            "export def f :: i32 -> i32\nfn f x = {\n    let add = Int.test_add x;\n    add 2i32\n}",
            "export def f :: i32 -> i32\nfn f x = x |> Int.test_add 3i32",
            "def twice :: (i32 -> i32 -> i32) -> i32 -> i32\nfn twice g x = g x x\n\
             export def f :: i32 -> i32\nfn f x = twice Int.test_add x",
        ] {
            let ir = library(source);
            assert_eq!(ir.matches(add).count(), 1, "{source}\n{ir}");
            assert!(ir.contains("add i32 %x, %y"), "{source}");
        }
        // A generic higher-order function receives one instance per type.
        let ir = library(
            "def apply :: ('a -> 'a -> 'a) -> 'a -> 'a\nfn apply g x = g x x\n\
             export def small :: i32 -> i32\nfn small x = apply Int.test_add x\n\
             export def large :: i64 -> i64\nfn large x = apply Int.test_add x\n\
             export def again :: i32 -> i32\nfn again x = Int.test_add x x",
        );
        assert_eq!(ir.matches(add).count(), 1, "{ir}");
        assert_eq!(
            ir.matches("define internal i64 @tz.builtin.Int.test_add.i64(i64 %x, i64 %y)")
                .count(),
            1,
            "{ir}"
        );
        let error = analyze("def f :: f64 -> f64\nfn f x = Int.test_add x x").unwrap_err();
        assert_eq!(error.code, "E1005", "{}", error.message);
    }

    #[test]
    fn solves_unsigned_and_widened_builtin_results_at_call_sites() {
        let ir = library(
            "export def unsigned :: i32 -> i32u\nfn unsigned x = Int.test_unsigned x\n\
             export def widen :: i32 -> i64\nfn widen x = Int.test_widen x\n\
             def widen_unsigned :: i64u -> i128u\nfn widen_unsigned x = x |> Int.test_widen\n\
             let defaulted: i64 = Int.test_widen 1\nlet long: i128 = Int.test_widen 1l\n0",
        );
        for definition in [
            "define internal i32 @tz.builtin.Int.test_unsigned.i32(i32 %x) nounwind",
            "define internal i64 @tz.builtin.Int.test_widen.i32(i32 %x) nounwind",
            "define internal i128 @tz.builtin.Int.test_widen.i64u(i64 %x) nounwind",
            "define internal i128 @tz.builtin.Int.test_widen.i64(i64 %x) nounwind",
        ] {
            assert_eq!(ir.matches(definition).count(), 1, "{definition}\n{ir}");
        }
        assert!(ir.contains("sext i32 %x to i64"));
        assert!(ir.contains("zext i64 %x to i128"));
        for (source, code, message) in [
            (
                "def f :: i32 -> i32\nfn f x = Int.test_unsigned x",
                "E1003",
                "",
            ),
            ("let x: i128 = Int.test_widen 1\n0", "E1003", ""),
            (
                "def f :: i128 -> i128\nfn f x = Int.test_widen x",
                "E1005",
                "128-bit integers have no wider integer type",
            ),
            (
                "def f :: Integer<'a> => 'a -> 'a\nfn f x = {\n    let _ = Int.test_widen x;\n    x\n}",
                "E1015",
                "generic code cannot use it",
            ),
            (
                "def f :: f64 -> f64\nfn f x = Int.test_unsigned x",
                "E1005",
                "Integer<f64>",
            ),
        ] {
            let error = analyze(source).expect_err(source);
            assert_eq!(error.code, code, "{source}\n{}", error.message);
            assert!(
                error.message.contains(message),
                "{source}\n{}",
                error.message
            );
        }
    }

    #[test]
    fn emits_one_unreachable_instance_per_result_type() {
        let ir = library(
            "export def f :: bool -> i64\nfn f b = if b then 1 else unreachable ()\n\
             export def g :: bool -> i64\nfn g b = if b then 2 else unreachable ()\n\
             def h :: bool -> string\nfn h b = if b then \"x\" else unreachable ()\n\
             def u :: bool -> utf8string\nfn u b = if b then u8\"x\" else unreachable ()",
        );
        for definition in [
            "define internal i64 @tz.builtin.unreachable.i64(i8 %unit) noreturn nounwind",
            "define internal %tz.string @tz.builtin.unreachable.string(i8 %unit) noreturn nounwind",
            "define internal %tz.utf8string @tz.builtin.unreachable.utf8string(i8 %unit) noreturn nounwind",
        ] {
            assert_eq!(ir.matches(definition).count(), 1, "{definition}\n{ir}");
        }
        let library = library("export def answer :: i64\nfn answer = 42");
        assert!(!library.contains("@tz.builtin."), "{library}");
        assert!(!library.contains("@tz.fn.Math."), "{library}");
        assert!(!library.contains("@tz.string."), "{library}");
        assert!(!library.contains("@tz.utf8string."), "{library}");
    }

    const HEAP_LIMIT_LINES: [&str; 3] = [
        "  %fits = icmp ule i64 %size, 16777184",
        "  %within = icmp ule i32 %end, 16777216",
        "  %fits = icmp ule i64 %new_size, 16777184",
    ];

    #[test]
    fn wasm_heap_limit_lines_are_unique() {
        let heap = include_str!("runtime/heap-wasm.ll");
        let heap64 = include_str!("runtime/heap-wasm64.ll");
        let threads = include_str!("runtime/heap-wasm-threads.ll");
        for target in HEAP_LIMIT_LINES {
            assert_eq!(heap.lines().filter(|line| *line == target).count(), 1);
            assert_eq!(threads.lines().filter(|line| *line == target).count(), 0);
            let target64 = target.replace("i32 %end", "i64 %end");
            assert_eq!(heap64.lines().filter(|line| *line == target64).count(), 1);
        }
        assert!(!heap64.contains("i32 %end"));
    }

    #[test]
    fn wasm_heap_limit_above_2_gib_compares_the_remaining_room() {
        let heap = include_str!("runtime/heap-wasm.ll");
        let rewritten = with_wasm_heap_limit(heap.to_owned(), 4294901760);
        assert!(rewritten.contains(
            "  %end = add i32 %begin, %needed\n  %room = sub i32 4294901760, %begin\n  %within = icmp ule i32 %needed, %room\n"
        ));
        assert_eq!(rewritten.matches("4294901728\n").count(), 2);
        assert_eq!(rewritten.lines().count(), heap.lines().count() + 1);
        let at_2_gib = with_wasm_heap_limit(heap.to_owned(), 1 << 31);
        assert!(at_2_gib.contains("  %within = icmp ule i32 %end, 2147483648\n"));
        let heap64 = include_str!("runtime/heap-wasm64.ll");
        let rewritten = with_wasm_heap_limit(heap64.to_owned(), 1 << 34);
        assert!(rewritten.contains("  %within = icmp ule i64 %end, 17179869184\n"));
        assert_eq!(rewritten.matches("17179869152\n").count(), 2);
        assert_eq!(rewritten.lines().count(), heap64.lines().count());
    }

    #[test]
    fn stack_checks_follow_static_allocas_in_every_definition() {
        let ir = "@text = constant [8 x i8] c\"define {\"\ndeclare void @external()\ndefine internal i64 @a(i64 %x) nounwind {\nentry:\n  ret i64 %x\n}\ndefine void @b() {\n  ret void\n}\ndefine i32 @c() {\n0: ; entry\n  ret i32 0\n}\n";
        let call = "  call void @tz.stack.check()\n";
        for workers in [false, true] {
            let checked = with_stack_checks(ir.to_owned(), workers);
            assert!(checked.contains(&format!("nounwind {{\nentry:\n{call}  ret i64 %x")));
            assert!(checked.contains(&format!("@b() {{\n{call}  ret void")));
            assert!(checked.contains(&format!("@c() {{\n0: ; entry\n{call}  ret i32 0")));
            assert_eq!(checked.matches(call).count(), 3);
            assert_eq!(
                checked
                    .matches("define internal void @tz.stack.check()")
                    .count(),
                1
            );
            assert!(checked.contains("add i32 %base, 4096"));
            assert_eq!(
                checked.contains("@tsuzuri_stack_top = addrspace(1) global i32 0"),
                workers
            );
            assert!(checked.replace(call, "").starts_with(ir));
            assert_eq!(
                checked
                    .matches("declare ptr @llvm.frameaddress.p0(i32 immarg)")
                    .count(),
                1
            );
        }
        let allocas = "define void @d(i64 %n) {\nentry:\n  %slot = alloca i64, align 16\n  store i64 %n, ptr %slot\n  %buffer = alloca [128 x i8], align 16\n  %dynamic = alloca i8, i64 %n, align 16\n  br label %next\nnext:\n  %late = alloca i8, align 1\n  ret void\n}\n";
        assert!(with_stack_checks(allocas.to_owned(), false).starts_with(&format!(
            "define void @d(i64 %n) {{\nentry:\n  %slot = alloca i64, align 16\n  %buffer = alloca [128 x i8], align 16\n{call}  store i64 %n, ptr %slot\n  %dynamic = alloca i8, i64 %n, align 16\n  br label %next\nnext:\n  %late = alloca i8, align 1\n  ret void\n}}\n"
        )));
    }

    #[test]
    fn wasm_heap_limit_default_is_identity() {
        let heap = include_str!("runtime/heap-wasm.ll");
        for text in [heap.to_owned(), heap.replace('\n', "\r\n")] {
            assert_eq!(with_wasm_heap_limit(text.clone(), 16777216), text);
        }
    }

    #[test]
    fn wasm_heap_limit_rewrites_whole_lines_only() {
        let constant =
            "@s = private constant [39 x i8] c\"  %within = icmp ule i32 %end, 16777216\"";
        let heap = format!("{constant}\n{}", include_str!("runtime/heap-wasm.ll"));
        for (text, ending) in [(heap.clone(), "\n"), (heap.replace('\n', "\r\n"), "\r\n")] {
            let rewritten = with_wasm_heap_limit(text.clone(), 67108864);
            let (first, body) = rewritten.split_once(ending).unwrap();
            assert_eq!(first, constant);
            for line in [
                "  %fits = icmp ule i64 %size, 67108832",
                "  %within = icmp ule i32 %end, 67108864",
                "  %fits = icmp ule i64 %new_size, 67108832",
            ] {
                assert_eq!(
                    body.matches(&format!("{line}{ending}")).count(),
                    1,
                    "{line}"
                );
            }
            assert!(!body.contains("16777184") && !body.contains("16777216"));
            assert_eq!(
                rewritten.matches(ending).count(),
                text.matches(ending).count()
            );
            assert_eq!(rewritten.matches('\n').count(), text.matches('\n').count());
            let restored = rewritten
                .replace("67108832", "16777184")
                .replace("67108864", "16777216");
            assert_eq!(restored, text);
        }
    }
}
