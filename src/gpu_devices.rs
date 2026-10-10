//! Kernels for non-CPU backends (F09 Phase 2).
//!
//! A program that builds a non-CPU `Gpu.Backend` value may run `Gpu.init` and `Gpu.map` on a
//! device of that backend, so the compiler embeds the kernel of every call site. Two passes do it:
//!
//! 1. `retarget_calls`, before specialization: the user's references to `Gpu.request`, `Gpu.init`,
//!    `Gpu.map`, `Gpu.init_relaxed`, and `Gpu.map_relaxed` name the std functions `request_on`,
//!    `init_on`, and `map_on` instead. The `_on` functions take one more argument, the kernel
//!    number, which a retargeted call fills with a marker that says whether the call was relaxed.
//!    They are private to `std/Gpu.tz`, so user code cannot call them with a number of its own
//!    (E1022); `Gpu.__run` also compares the lane kinds of the numbered kernel with its element types.
//! 2. `lower_kernels`, after specialization, when every callback is a known function: it builds the
//!    kernels, stores them in `CheckedModule::gpu`, and replaces each marker with the number of
//!    the call's kernel (-1 when a strict call has no kernel for a device, which traps there).
//!
//! A program that never names a non-CPU backend takes neither pass, so its IR does not change.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    check::{CheckedModule, FunctionRef, ModuleOrigin, Type, TypedExpr, TypedExprKind},
    diagnostic::Diagnostic,
    gpu,
};

/// The device features that a kernel can need (`tsuzuri_gpu_open`'s `features`).
pub const FEATURE_F16: u32 = 1;

/// The bit of a kernel's `flags` that marks floating-point operations as relaxed.
pub const FLAG_RELAXED: u32 = 1;

/// The lane kinds of a kernel descriptor: 32-bit integers, f32, and f16.
pub const LANE_32: u32 = 1;
pub const LANE_F32: u32 = 2;
pub const LANE_F16: u32 = 3;

/// A retargeted call carries `MARKER + relaxed` as its kernel number until `lower_kernels`.
const MARKER: u128 = 1 << 40;

/// The kernel number of a call whose kernel does not exist for a device.
const NO_KERNEL: u128 = u64::MAX as u128;

/// One kernel of the program: the embedded sources of a callback for the non-CPU backends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuKernelBlob {
    /// The function id of the callback.
    pub callback: usize,
    pub relaxed: bool,
    /// `FLAG_RELAXED` for a relaxed kernel.
    pub flags: u32,
    /// The input lane kind in bits 0 to 7 and the output lane kind in bits 8 to 15.
    pub lanes: u32,
    /// The device features the kernel needs.
    pub features: u32,
    /// WGSL text for WebGPU.
    pub wgsl: Option<String>,
    /// A SPIR-V module for Vulkan, when a backend wants one (Phase 3).
    pub spirv: Option<Vec<u8>>,
}

/// The kernels that a program embeds and the features they need together.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuProgram {
    pub kernels: Vec<GpuKernelBlob>,
    pub features: u32,
}

fn is_user(module: &CheckedModule, id: usize) -> bool {
    module.functions[id].origin.module != ModuleOrigin::Std
}

/// Whether some user function builds a `Gpu.Backend` other than `CpuReference`.
pub fn uses_devices(module: &CheckedModule) -> bool {
    let Some(backend) = module
        .unions
        .iter()
        .position(|union| union.origin == ModuleOrigin::Std && union.name == "Gpu.Backend")
    else {
        return false;
    };
    (0..module.functions.len())
        .filter(|id| is_user(module, *id))
        .any(|id| {
            let mut pending = vec![&module.functions[id].body];
            while let Some(expression) = pending.pop() {
                if let TypedExprKind::Construct {
                    union_id, case_id, ..
                } = &expression.kind
                    && *union_id == backend
                    && *case_id != 0
                {
                    return true;
                }
                pending.extend(expression.children());
            }
            false
        })
}

fn std_gpu_function(module: &CheckedModule, name: &str) -> Option<usize> {
    module.functions.iter().position(|function| {
        function.module == "Gpu"
            && function.origin.module == ModuleOrigin::Std
            && function.name == name
    })
}

/// Names the device-aware std functions at the user's calls (pass 1).
pub(crate) fn retarget_calls(module: &mut CheckedModule) {
    if !uses_devices(module) {
        return;
    }
    let find = |name| std_gpu_function(module, name);
    let (Some(request), Some(request_on), Some(init_on), Some(map_on)) = (
        find("request"),
        find("request_on"),
        find("init_on"),
        find("map_on"),
    ) else {
        return;
    };
    // (callee, retargeted callee, number of arguments before the kernel number, relaxed)
    let mut calls = BTreeMap::new();
    for (name, target, relaxed) in [
        ("init", init_on, false),
        ("init_relaxed", init_on, true),
        ("map", map_on, false),
        ("map_relaxed", map_on, true),
    ] {
        if let Some(id) = find(name) {
            calls.insert(
                id,
                (
                    target,
                    module.functions[id].signature.parameters.len(),
                    relaxed,
                ),
            );
        }
    }
    let users: Vec<usize> = (0..module.functions.len())
        .filter(|id| is_user(module, *id))
        .collect();
    for id in users {
        retarget(&mut module.functions[id].body, request, request_on, &calls);
    }
}

fn retarget(
    expression: &mut TypedExpr,
    request: usize,
    request_on: usize,
    calls: &BTreeMap<usize, (usize, usize, bool)>,
) {
    for child in expression.children_mut() {
        retarget(child, request, request_on, calls);
    }
    match &mut expression.kind {
        TypedExprKind::Function(FunctionRef::User(id)) if *id == request => *id = request_on,
        TypedExprKind::Call(callee, arguments) => {
            let TypedExprKind::GenericFunction(id, types) = &callee.kind else {
                return;
            };
            let Some((target, arity, relaxed)) = calls.get(id) else {
                return;
            };
            if arguments.len() != *arity {
                return;
            }
            callee.kind = TypedExprKind::GenericFunction(*target, types.clone());
            if let Type::Function(parameters, _) = &mut callee.ty {
                parameters.push(Type::I64);
            }
            arguments.push(TypedExpr {
                kind: TypedExprKind::Int(MARKER + u128::from(*relaxed)),
                ty: Type::I64,
                span: expression.span,
            });
        }
        _ => {}
    }
}

/// The instances of `init_on` (true) and `map_on` (false) in the specialized module.
fn instances(module: &CheckedModule) -> BTreeMap<usize, bool> {
    module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| {
            function.module == "Gpu" && function.origin.module == ModuleOrigin::Std
        })
        .filter_map(|(id, function)| match function.name.split('.').next() {
            Some("init_on") => Some((id, true)),
            Some("map_on") => Some((id, false)),
            _ => None,
        })
        .collect()
}

/// The callback and the marker of a call of an `init_on` or `map_on` instance.
fn device_call<'a>(
    expression: &'a TypedExpr,
    instances: &BTreeMap<usize, bool>,
) -> Option<(usize, &'a TypedExpr)> {
    let TypedExprKind::Call(callee, arguments) = &expression.kind else {
        return None;
    };
    let TypedExprKind::Function(FunctionRef::User(id)) = &callee.kind else {
        return None;
    };
    let init = *instances.get(id)?;
    let callback = match &arguments.get(if init { 2 } else { 1 })?.kind {
        TypedExprKind::Function(FunctionRef::User(callback)) => *callback,
        TypedExprKind::Closure(callback, captures) if captures.is_empty() => *callback,
        _ => return None,
    };
    Some((callback, arguments.last()?))
}

pub(crate) fn marked(marker: &TypedExpr) -> Option<bool> {
    match marker.kind {
        TypedExprKind::Int(value) if value >= MARKER && value - MARKER <= 1 => Some(value > MARKER),
        _ => None,
    }
}

/// The lane kind of a buffer element type, or `None` for a type that no kernel descriptor has.
pub(crate) fn lane(ty: &Type) -> Option<u32> {
    match ty {
        Type::Integer(32, _) => Some(LANE_32),
        Type::Binary(32) => Some(LANE_F32),
        Type::Binary(16) => Some(LANE_F16),
        _ => None,
    }
}

/// The kernel of a callback for the non-CPU backends, or `None` when a strict call has none.
fn blob(
    module: &CheckedModule,
    callback: usize,
    relaxed: bool,
) -> Result<Option<GpuKernelBlob>, Diagnostic> {
    let kernel = gpu::extract_kernel(module, callback)?;
    let wgsl = if relaxed {
        Some(kernel.wgsl_relaxed()?)
    } else {
        kernel.wgsl().ok()
    };
    let signature = &module.functions[callback].signature;
    let (Some(wgsl), Some(input), Some(output)) = (
        wgsl,
        lane(&signature.parameters[0]),
        lane(&signature.result),
    ) else {
        return Ok(None);
    };
    let features = if wgsl.contains("enable f16;") {
        FEATURE_F16
    } else {
        0
    };
    Ok(Some(GpuKernelBlob {
        callback,
        relaxed,
        flags: if relaxed { FLAG_RELAXED } else { 0 },
        lanes: input | (output << 8),
        features,
        wgsl: Some(wgsl),
        spirv: None,
    }))
}

/// Builds the kernels of the calls of `init_on` and `map_on` and numbers them (pass 2).
pub(crate) fn lower_kernels(module: &mut CheckedModule) -> Result<(), Diagnostic> {
    let instances = instances(module);
    if instances.is_empty() {
        return Ok(());
    }
    let mut sites = BTreeSet::new();
    for id in (0..module.functions.len()).filter(|id| is_user(module, *id)) {
        let mut pending = vec![&module.functions[id].body];
        while let Some(expression) = pending.pop() {
            if let Some((callback, marker)) = device_call(expression, &instances)
                && let Some(relaxed) = marked(marker)
            {
                sites.insert((callback, relaxed));
            }
            pending.extend(expression.children());
        }
    }
    let mut kernels = Vec::new();
    let mut numbers = BTreeMap::new();
    for (callback, relaxed) in sites {
        if let Some(blob) = blob(module, callback, relaxed)? {
            numbers.insert((callback, relaxed), kernels.len() as u128);
            kernels.push(blob);
        }
    }
    let users: Vec<usize> = (0..module.functions.len())
        .filter(|id| is_user(module, *id))
        .collect();
    for id in users {
        number(&mut module.functions[id].body, &instances, &numbers);
    }
    let features = kernels.iter().fold(0, |all, kernel| all | kernel.features);
    module.gpu = GpuProgram { kernels, features };
    Ok(())
}

fn number(
    expression: &mut TypedExpr,
    instances: &BTreeMap<usize, bool>,
    numbers: &BTreeMap<(usize, bool), u128>,
) {
    for child in expression.children_mut() {
        number(child, instances, numbers);
    }
    let Some((callback, marker)) = device_call(expression, instances) else {
        return;
    };
    let Some(relaxed) = marked(marker) else {
        return;
    };
    let value = numbers
        .get(&(callback, relaxed))
        .copied()
        .unwrap_or(NO_KERNEL);
    if let TypedExprKind::Call(_, arguments) = &mut expression.kind
        && let Some(marker) = arguments.last_mut()
    {
        marker.kind = TypedExprKind::Int(value);
    }
}
