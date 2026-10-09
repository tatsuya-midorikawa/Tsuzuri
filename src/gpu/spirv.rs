//! SPIR-V for Vulkan compute kernels (F09 Phase 3).
//!
//! The emitter is deterministic and has no dependency: it writes the words of a SPIR-V 1.3 module (the Vulkan 1.1
//! baseline) for the kernel of a [`GpuKernel`]. LLVM's SPIR-V target was evaluated first and cannot serve (see the
//! F09 record): the Vulkan compute path needs HLSL resource intrinsics that differ between LLVM versions, the system
//! `clang` of macOS has no SPIR-V target, and the target never emits `NoContraction`, which Vulkan requires for every
//! operation that must not be fused, reassociated, or replaced by a reciprocal.
//!
//! Module: `Shader` (+ `Int64` for 64-bit lanes and locals), Logical/GLSL450, entry points `map_main` and (for a 32-bit
//! integer input) `init_main` with `LocalSize 256 1 1`. Set 0 binding 0 is the input storage buffer (`NonWritable`),
//! binding 1 the output storage buffer, and the push constants are `{ u32 length; u32 base }`: invocation `i = base + x`
//! works when `i < length`, so a count above `maxComputeWorkGroupCount[0]` is dispatched in chunks.
//!
//! Strict mode reproduces the language semantics or refuses the kernel:
//! - Integers (`i32`, `i32u`, `i64`, `i64u`) use signless SPIR-V integers and the instruction that matches the static
//!   type: wrapping `+ - *`, bit operations, shifts masked to `width - 1`, the signed or unsigned comparisons, and
//!   `OpSConvert`/`OpUConvert` casts. Division, remainder, and `**` trap or call a library on the CPU and are rejected.
//! - `f32` operations are `OpFAdd/OpFSub/OpFMul/OpFNegate`, each decorated `NoContraction` (Vulkan assumes
//!   AllowContract, AllowReassoc, AllowRecip, and AllowTransform for every other operation), ordered comparisons, and
//!   `!=` as `OpFUnordNotEqual`. The entry points set `SignedZeroInfNanPreserve`, `DenormPreserve`, and
//!   `RoundingModeRTE` for width 32, which a device must report (see `FEATURE_STRICT_FLOAT`). Vulkan makes
//!   `OpFAdd/OpFSub/OpFMul` and the conversions correctly rounded under those modes, so these results equal the CPU's.
//! - `f32` to integer casts are emulated with ordered comparisons and selects (saturation, NaN to 0) around
//!   `OpConvertFToS/U`, which is only ever applied to an in-range value; this matches `llvm.fptosi.sat`/`fptoui.sat`.
//! - `f32` division is rejected: `OpFDiv` is only within 2.5 ULP. `f64`, `bool` lanes, and float remainder are rejected.
//!
//! Relaxed mode (`Gpu.map_relaxed`) has the lane and cast rules of relaxed WGSL, emits no float decorations or modes
//! (so no device feature is needed), and allows `/` as `OpFDiv`.

use std::collections::{BTreeMap, BTreeSet};

use super::{GpuKernel, unsupported};
use crate::{
    check::{CheckedFunction, FunctionRef, Type, TypedExpr, TypedExprKind},
    diagnostic::{Diagnostic, Span},
    syntax::{BinaryOp, UnaryOp},
};

/// Device features a kernel needs: bit 1 is `shaderInt64`, bit 2 is the strict float32 controls (signed zero, inf, and
/// NaN preserve, denormal preserve, round to nearest even, independent 32-bit modes). Bit 0 is WebGPU's `shader-f16`.
pub const FEATURE_INT64: u32 = 1 << 1;
pub const FEATURE_STRICT_FLOAT: u32 = 1 << 2;

/// The storage lane of one side of a kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// `i32` or `i32u` (lane kind 1 of the runtime ABI).
    Int32,
    /// `f32` (kind 2).
    Float32,
    /// `i64` or `i64u` (kind 4).
    Int64,
}

impl Lane {
    /// The lane kind of the runtime ABI.
    pub fn kind(self) -> u32 {
        match self {
            Lane::Int32 => 1,
            Lane::Float32 => 2,
            Lane::Int64 => 4,
        }
    }
}

/// A Vulkan compute module for one kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpirvKernel {
    pub words: Vec<u32>,
    /// `FEATURE_*` bits the module needs from the device.
    pub features: u32,
    pub input: Lane,
    pub output: Lane,
    pub relaxed: bool,
    /// Whether the module has the `init_main` entry point (a 32-bit integer input).
    pub init: bool,
    /// The operations one lane executes, with calls counted at the size of their callees: the cost estimate that
    /// `Gpu.Auto` uses.
    pub weight: u32,
}

impl SpirvKernel {
    /// The module as little-endian bytes, the form that is embedded and handed to Vulkan.
    pub fn bytes(&self) -> Vec<u8> {
        self.words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }
}

impl GpuKernel<'_> {
    /// Strict SPIR-V: the CPU reference's integer and `f32` semantics, or an E1018 diagnostic.
    pub fn spirv(&self) -> Result<SpirvKernel, Diagnostic> {
        Emitter::run(self, false)
    }

    /// Relaxed SPIR-V for `Gpu.map_relaxed` and `Gpu.init_relaxed`.
    pub fn spirv_relaxed(&self) -> Result<SpirvKernel, Diagnostic> {
        Emitter::run(self, true)
    }
}

mod op {
    pub const NAME: u16 = 5;
    pub const EXTENSION: u16 = 10;
    pub const MEMORY_MODEL: u16 = 14;
    pub const ENTRY_POINT: u16 = 15;
    pub const EXECUTION_MODE: u16 = 16;
    pub const CAPABILITY: u16 = 17;
    pub const TYPE_VOID: u16 = 19;
    pub const TYPE_BOOL: u16 = 20;
    pub const TYPE_INT: u16 = 21;
    pub const TYPE_FLOAT: u16 = 22;
    pub const TYPE_VECTOR: u16 = 23;
    pub const TYPE_RUNTIME_ARRAY: u16 = 29;
    pub const TYPE_STRUCT: u16 = 30;
    pub const TYPE_POINTER: u16 = 32;
    pub const TYPE_FUNCTION: u16 = 33;
    pub const CONSTANT_TRUE: u16 = 41;
    pub const CONSTANT_FALSE: u16 = 42;
    pub const CONSTANT: u16 = 43;
    pub const FUNCTION: u16 = 54;
    pub const FUNCTION_PARAMETER: u16 = 55;
    pub const FUNCTION_END: u16 = 56;
    pub const FUNCTION_CALL: u16 = 57;
    pub const VARIABLE: u16 = 59;
    pub const LOAD: u16 = 61;
    pub const STORE: u16 = 62;
    pub const ACCESS_CHAIN: u16 = 65;
    pub const DECORATE: u16 = 71;
    pub const MEMBER_DECORATE: u16 = 72;
    pub const COMPOSITE_EXTRACT: u16 = 81;
    pub const CONVERT_F_TO_U: u16 = 109;
    pub const CONVERT_F_TO_S: u16 = 110;
    pub const CONVERT_S_TO_F: u16 = 111;
    pub const CONVERT_U_TO_F: u16 = 112;
    pub const U_CONVERT: u16 = 113;
    pub const S_CONVERT: u16 = 114;
    pub const S_NEGATE: u16 = 126;
    pub const F_NEGATE: u16 = 127;
    pub const I_ADD: u16 = 128;
    pub const F_ADD: u16 = 129;
    pub const I_SUB: u16 = 130;
    pub const F_SUB: u16 = 131;
    pub const I_MUL: u16 = 132;
    pub const F_MUL: u16 = 133;
    pub const F_DIV: u16 = 136;
    pub const IS_NAN: u16 = 156;
    pub const LOGICAL_EQUAL: u16 = 164;
    pub const LOGICAL_NOT_EQUAL: u16 = 165;
    pub const LOGICAL_OR: u16 = 166;
    pub const LOGICAL_NOT: u16 = 168;
    pub const SELECT: u16 = 169;
    pub const I_EQUAL: u16 = 170;
    pub const I_NOT_EQUAL: u16 = 171;
    pub const U_GREATER_THAN: u16 = 172;
    pub const S_GREATER_THAN: u16 = 173;
    pub const U_GREATER_THAN_EQUAL: u16 = 174;
    pub const S_GREATER_THAN_EQUAL: u16 = 175;
    pub const U_LESS_THAN: u16 = 176;
    pub const S_LESS_THAN: u16 = 177;
    pub const U_LESS_THAN_EQUAL: u16 = 178;
    pub const S_LESS_THAN_EQUAL: u16 = 179;
    pub const F_ORD_EQUAL: u16 = 180;
    pub const F_UNORD_NOT_EQUAL: u16 = 183;
    pub const F_ORD_LESS_THAN: u16 = 184;
    pub const F_ORD_GREATER_THAN: u16 = 186;
    pub const F_ORD_LESS_THAN_EQUAL: u16 = 188;
    pub const F_ORD_GREATER_THAN_EQUAL: u16 = 190;
    pub const SHIFT_RIGHT_LOGICAL: u16 = 194;
    pub const SHIFT_RIGHT_ARITHMETIC: u16 = 195;
    pub const SHIFT_LEFT_LOGICAL: u16 = 196;
    pub const BITWISE_OR: u16 = 197;
    pub const BITWISE_XOR: u16 = 198;
    pub const BITWISE_AND: u16 = 199;
    pub const NOT: u16 = 200;
    pub const PHI: u16 = 245;
    pub const SELECTION_MERGE: u16 = 247;
    pub const LABEL: u16 = 248;
    pub const BRANCH: u16 = 249;
    pub const BRANCH_CONDITIONAL: u16 = 250;
    pub const RETURN: u16 = 253;
    pub const RETURN_VALUE: u16 = 254;
}

const CAPABILITY_SHADER: u32 = 1;
const CAPABILITY_INT64: u32 = 11;
const CAPABILITY_DENORM_PRESERVE: u32 = 4464;
const CAPABILITY_SIGNED_ZERO_INF_NAN_PRESERVE: u32 = 4466;
const CAPABILITY_ROUNDING_MODE_RTE: u32 = 4467;
const MODE_LOCAL_SIZE: u32 = 17;
const MODE_DENORM_PRESERVE: u32 = 4459;
const MODE_SIGNED_ZERO_INF_NAN_PRESERVE: u32 = 4461;
const MODE_ROUNDING_MODE_RTE: u32 = 4462;
const DECORATION_BLOCK: u32 = 2;
const DECORATION_ARRAY_STRIDE: u32 = 6;
const DECORATION_BUILT_IN: u32 = 11;
const DECORATION_NON_WRITABLE: u32 = 24;
const DECORATION_BINDING: u32 = 33;
const DECORATION_DESCRIPTOR_SET: u32 = 34;
const DECORATION_OFFSET: u32 = 35;
const DECORATION_NO_CONTRACTION: u32 = 42;
const BUILT_IN_GLOBAL_INVOCATION_ID: u32 = 28;
const STORAGE_INPUT: u32 = 1;
const STORAGE_PUSH_CONSTANT: u32 = 9;
const STORAGE_FUNCTION: u32 = 7;
const STORAGE_BUFFER: u32 = 12;
const EXECUTION_MODEL_GLCOMPUTE: u32 = 5;
const WORKGROUP_SIZE: u32 = 256;
const SPIRV_VERSION_1_3: u32 = 0x0001_0300;
const SPIRV_MAGIC: u32 = 0x0723_0203;

fn instruction(opcode: u16, operands: &[u32]) -> Vec<u32> {
    let mut words = Vec::with_capacity(operands.len() + 1);
    words.push(((operands.len() as u32 + 1) << 16) | u32::from(opcode));
    words.extend_from_slice(operands);
    words
}

/// A literal string: UTF-8, NUL terminated, padded to whole words.
fn string_words(text: &str) -> Vec<u32> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    bytes
        .chunks(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Bool,
    I32,
    U32,
    I64,
    U64,
    F32,
}

impl Kind {
    fn signed(self) -> bool {
        matches!(self, Kind::I32 | Kind::I64)
    }

    fn is_integer(self) -> bool {
        matches!(self, Kind::I32 | Kind::U32 | Kind::I64 | Kind::U64)
    }

    fn width(self) -> u32 {
        match self {
            Kind::I64 | Kind::U64 => 64,
            _ => 32,
        }
    }

    fn lane(self) -> Option<Lane> {
        match self {
            Kind::I32 | Kind::U32 => Some(Lane::Int32),
            Kind::I64 | Kind::U64 => Some(Lane::Int64),
            Kind::F32 => Some(Lane::Float32),
            Kind::Bool => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TypeKey {
    Void,
    Bool,
    Int(u32),
    Float,
    Vector3(u32),
    Pointer(u32, u32),
    RuntimeArray(u32),
    Function(u32, Vec<u32>),
}

#[derive(Default)]
struct Module {
    next: u32,
    capabilities: BTreeSet<u32>,
    extensions: BTreeSet<&'static str>,
    entry_points: Vec<Vec<u32>>,
    execution_modes: Vec<Vec<u32>>,
    debug: Vec<Vec<u32>>,
    annotations: Vec<Vec<u32>>,
    globals: Vec<Vec<u32>>,
    functions: Vec<Vec<u32>>,
    types: BTreeMap<TypeKey, u32>,
    constants: BTreeMap<(u32, u64), u32>,
    booleans: BTreeMap<bool, u32>,
    decorated_arrays: BTreeSet<u32>,
    no_contraction: Vec<u32>,
    uses_float: bool,
    uses_int64: bool,
}

impl Module {
    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    fn ty(&mut self, key: TypeKey) -> u32 {
        if let Some(id) = self.types.get(&key) {
            return *id;
        }
        let id = self.id();
        let words = match &key {
            TypeKey::Void => instruction(op::TYPE_VOID, &[id]),
            TypeKey::Bool => instruction(op::TYPE_BOOL, &[id]),
            TypeKey::Int(width) => instruction(op::TYPE_INT, &[id, *width, 0]),
            TypeKey::Float => instruction(op::TYPE_FLOAT, &[id, 32]),
            TypeKey::Vector3(element) => instruction(op::TYPE_VECTOR, &[id, *element, 3]),
            TypeKey::Pointer(class, pointee) => {
                instruction(op::TYPE_POINTER, &[id, *class, *pointee])
            }
            TypeKey::RuntimeArray(element) => instruction(op::TYPE_RUNTIME_ARRAY, &[id, *element]),
            TypeKey::Function(result, parameters) => {
                let mut operands = vec![id, *result];
                operands.extend(parameters);
                instruction(op::TYPE_FUNCTION, &operands)
            }
        };
        self.globals.push(words);
        self.types.insert(key, id);
        id
    }

    fn kind_type(&mut self, kind: Kind) -> u32 {
        match kind {
            Kind::Bool => self.ty(TypeKey::Bool),
            Kind::I32 | Kind::U32 => self.ty(TypeKey::Int(32)),
            Kind::I64 | Kind::U64 => {
                self.uses_int64 = true;
                self.capabilities.insert(CAPABILITY_INT64);
                self.ty(TypeKey::Int(64))
            }
            Kind::F32 => {
                self.uses_float = true;
                self.ty(TypeKey::Float)
            }
        }
    }

    /// An `OpConstant` of a 32- or 64-bit type; `value` holds the bits.
    fn constant(&mut self, ty: u32, value: u64, wide: bool) -> u32 {
        if let Some(id) = self.constants.get(&(ty, value)) {
            return *id;
        }
        let id = self.id();
        let mut operands = vec![ty, id, value as u32];
        if wide {
            operands.push((value >> 32) as u32);
        }
        self.globals.push(instruction(op::CONSTANT, &operands));
        self.constants.insert((ty, value), id);
        id
    }

    fn boolean(&mut self, value: bool) -> u32 {
        if let Some(id) = self.booleans.get(&value) {
            return *id;
        }
        let ty = self.ty(TypeKey::Bool);
        let id = self.id();
        let opcode = if value {
            op::CONSTANT_TRUE
        } else {
            op::CONSTANT_FALSE
        };
        self.globals.push(instruction(opcode, &[ty, id]));
        self.booleans.insert(value, id);
        id
    }

    fn variable(&mut self, pointer: u32, class: u32) -> u32 {
        let id = self.id();
        self.globals
            .push(instruction(op::VARIABLE, &[pointer, id, class]));
        id
    }

    fn name(&mut self, target: u32, text: &str) {
        let mut operands = vec![target];
        operands.extend(string_words(text));
        self.debug.push(instruction(op::NAME, &operands));
    }
}

#[derive(Clone, Copy)]
struct Val {
    id: u32,
    kind: Kind,
}

#[derive(Clone, Copy)]
enum Slot {
    Value(Val),
    Variable(u32, Kind),
}

struct Interface {
    gid: u32,
    input: u32,
    output: u32,
    push: u32,
    input_kind: Kind,
    output_kind: Kind,
}

struct Emitter<'a, 'k> {
    kernel: &'a GpuKernel<'k>,
    relaxed: bool,
    module: Module,
    body: Vec<Vec<u32>>,
    variables: Vec<Vec<u32>>,
    block: u32,
    locals: BTreeMap<usize, Slot>,
    functions: BTreeMap<usize, u32>,
    weights: BTreeMap<usize, u32>,
    weight: u32,
}

impl<'a, 'k> Emitter<'a, 'k> {
    fn run(kernel: &'a GpuKernel<'k>, relaxed: bool) -> Result<SpirvKernel, Diagnostic> {
        let mut emitter = Emitter {
            kernel,
            relaxed,
            module: Module::default(),
            body: Vec::new(),
            variables: Vec::new(),
            block: 0,
            locals: BTreeMap::new(),
            functions: BTreeMap::new(),
            weights: BTreeMap::new(),
            weight: 0,
        };
        emitter.module()
    }

    fn function_of(&self, id: usize) -> &'k CheckedFunction {
        let module = self.kernel.module;
        &module.functions[id]
    }

    fn kind_of(&self, ty: &Type, span: Span) -> Result<Kind, Diagnostic> {
        let kind = match ty {
            Type::Bool => Kind::Bool,
            Type::Integer(32, true) => Kind::I32,
            Type::Integer(32, false) => Kind::U32,
            Type::Integer(64, true) => Kind::I64,
            Type::Integer(64, false) => Kind::U64,
            Type::Binary(32) => Kind::F32,
            Type::Binary(64) => {
                return Err(unsupported(
                    "Vulkan kernels do not support f64; use the CPU reference path or f32",
                    span,
                ));
            }
            _ => {
                return Err(unsupported(
                    "Vulkan kernels support bool, 32/64-bit integer, and f32 scalars only",
                    span,
                ));
            }
        };
        if self.relaxed && matches!(kind, Kind::I64 | Kind::U64) {
            return Err(unsupported(
                "relaxed GPU kernels support f32, i32, and i32u values; 64-bit integers need the strict API",
                span,
            ));
        }
        Ok(kind)
    }

    fn lane_of(&self, ty: &Type, span: Span) -> Result<Kind, Diagnostic> {
        let kind = self.kind_of(ty, span)?;
        if kind == Kind::Bool {
            return Err(unsupported(
                "Vulkan kernels require i32, i32u, i64, i64u, or f32 buffer lanes; bool lanes are unavailable",
                span,
            ));
        }
        Ok(kind)
    }

    fn fresh(&mut self) -> u32 {
        self.module.id()
    }

    fn emit(&mut self, words: Vec<u32>) {
        self.body.push(words);
    }

    fn label(&mut self, id: u32) {
        self.emit(instruction(op::LABEL, &[id]));
        self.block = id;
    }

    /// An instruction with a result: returns the new id.
    fn result(&mut self, opcode: u16, ty: u32, operands: &[u32]) -> u32 {
        let id = self.fresh();
        let mut all = vec![ty, id];
        all.extend_from_slice(operands);
        self.emit(instruction(opcode, &all));
        id
    }

    /// A floating-point operation; strict kernels forbid fusing, reassociating, and replacing it.
    fn float_result(&mut self, opcode: u16, ty: u32, operands: &[u32]) -> u32 {
        let id = self.result(opcode, ty, operands);
        if !self.relaxed {
            self.module.no_contraction.push(id);
        }
        id
    }

    fn type_of(&mut self, kind: Kind) -> u32 {
        self.module.kind_type(kind)
    }

    fn integer_constant(&mut self, kind: Kind, value: u64) -> u32 {
        let ty = self.type_of(kind);
        if kind.width() == 64 {
            self.module.constant(ty, value, true)
        } else {
            self.module.constant(ty, value & 0xFFFF_FFFF, false)
        }
    }

    fn float_constant(&mut self, bits: u32) -> u32 {
        let ty = self.type_of(Kind::F32);
        self.module.constant(ty, u64::from(bits), false)
    }

    fn module(&mut self) -> Result<SpirvKernel, Diagnostic> {
        let root = self.function_of(self.kernel.function);
        let input_kind = self.lane_of(&root.signature.parameters[0], root.span)?;
        let output_kind = self.lane_of(&root.signature.result, root.span)?;
        let void = self.module.ty(TypeKey::Void);
        let void_function = self.module.ty(TypeKey::Function(void, Vec::new()));
        let interface = self.interface(input_kind, output_kind);
        for id in self.call_order() {
            self.function(id)?;
        }
        let root_id = self.functions[&self.kernel.function];
        let mut entries =
            vec![self.entry_point("map_main", false, root_id, &interface, void, void_function)];
        let init = matches!(input_kind, Kind::I32 | Kind::U32);
        if init {
            entries.push(self.entry_point(
                "init_main",
                true,
                root_id,
                &interface,
                void,
                void_function,
            ));
        }
        self.finish(entries, &interface, init)
    }

    /// The global variables of the interface: the invocation id, both buffers, and the push constants.
    fn interface(&mut self, input_kind: Kind, output_kind: Kind) -> Interface {
        let u32_type = self.module.kind_type(Kind::U32);
        let vector = self.module.ty(TypeKey::Vector3(u32_type));
        let input_pointer = self.module.ty(TypeKey::Pointer(STORAGE_INPUT, vector));
        let gid = self.module.variable(input_pointer, STORAGE_INPUT);
        self.module.annotations.push(instruction(
            op::DECORATE,
            &[gid, DECORATION_BUILT_IN, BUILT_IN_GLOBAL_INVOCATION_ID],
        ));
        self.module.name(gid, "invocation");
        let mut buffers = Vec::new();
        for (binding, kind, writable) in [(0, input_kind, false), (1, output_kind, true)] {
            let element = self.module.kind_type(kind);
            let array = self.module.ty(TypeKey::RuntimeArray(element));
            let block = self.module.id();
            self.module
                .globals
                .push(instruction(op::TYPE_STRUCT, &[block, array]));
            let pointer = self.module.ty(TypeKey::Pointer(STORAGE_BUFFER, block));
            let variable = self.module.variable(pointer, STORAGE_BUFFER);
            if self.module.decorated_arrays.insert(array) {
                let stride = if kind.width() == 64 { 8 } else { 4 };
                self.module.annotations.push(instruction(
                    op::DECORATE,
                    &[array, DECORATION_ARRAY_STRIDE, stride],
                ));
            }
            let annotations = &mut self.module.annotations;
            annotations.push(instruction(
                op::MEMBER_DECORATE,
                &[block, 0, DECORATION_OFFSET, 0],
            ));
            if !writable {
                annotations.push(instruction(
                    op::MEMBER_DECORATE,
                    &[block, 0, DECORATION_NON_WRITABLE],
                ));
            }
            annotations.push(instruction(op::DECORATE, &[block, DECORATION_BLOCK]));
            annotations.push(instruction(
                op::DECORATE,
                &[variable, DECORATION_DESCRIPTOR_SET, 0],
            ));
            annotations.push(instruction(
                op::DECORATE,
                &[variable, DECORATION_BINDING, binding],
            ));
            let name = if writable {
                "output_values"
            } else {
                "input_values"
            };
            self.module.name(variable, name);
            buffers.push(variable);
        }
        let push_block = self.module.id();
        self.module.globals.push(instruction(
            op::TYPE_STRUCT,
            &[push_block, u32_type, u32_type],
        ));
        let annotations = &mut self.module.annotations;
        annotations.push(instruction(
            op::MEMBER_DECORATE,
            &[push_block, 0, DECORATION_OFFSET, 0],
        ));
        annotations.push(instruction(
            op::MEMBER_DECORATE,
            &[push_block, 1, DECORATION_OFFSET, 4],
        ));
        annotations.push(instruction(op::DECORATE, &[push_block, DECORATION_BLOCK]));
        let push_pointer = self
            .module
            .ty(TypeKey::Pointer(STORAGE_PUSH_CONSTANT, push_block));
        let push = self.module.variable(push_pointer, STORAGE_PUSH_CONSTANT);
        self.module.name(push, "params");
        Interface {
            gid,
            input: buffers[0],
            output: buffers[1],
            push,
            input_kind,
            output_kind,
        }
    }

    /// The reachable functions with every callee before its callers (depth-first post order).
    fn call_order(&self) -> Vec<usize> {
        fn visit(
            kernel: &GpuKernel<'_>,
            id: usize,
            seen: &mut BTreeSet<usize>,
            order: &mut Vec<usize>,
        ) {
            if !seen.insert(id) {
                return;
            }
            let mut callees = BTreeSet::new();
            let mut pending = vec![&kernel.module.functions[id].body];
            while let Some(expression) = pending.pop() {
                if let TypedExprKind::Call(callee, _) = &expression.kind
                    && let TypedExprKind::Function(FunctionRef::User(called))
                    | TypedExprKind::Closure(called, _) = &callee.kind
                {
                    callees.insert(*called);
                }
                pending.extend(expression.children());
            }
            for callee in callees {
                visit(kernel, callee, seen, order);
            }
            order.push(id);
        }
        let mut order = Vec::new();
        visit(
            self.kernel,
            self.kernel.function,
            &mut BTreeSet::new(),
            &mut order,
        );
        order
    }

    fn function(&mut self, id: usize) -> Result<(), Diagnostic> {
        let function = self.function_of(id);
        let mut parameter_kinds = Vec::new();
        for local in &function.parameters {
            parameter_kinds.push(self.kind_of(&local.ty, local.span)?);
        }
        let result_kind = self.kind_of(&function.signature.result, function.span)?;
        let result_type = self.type_of(result_kind);
        let parameter_types: Vec<u32> = parameter_kinds
            .iter()
            .map(|kind| self.module.kind_type(*kind))
            .collect();
        let function_type = self
            .module
            .ty(TypeKey::Function(result_type, parameter_types.clone()));
        let function_id = self.fresh();
        self.functions.insert(id, function_id);
        self.module.name(function_id, &format!("kernel_{id}"));
        self.body.clear();
        self.variables.clear();
        self.locals.clear();
        self.weight = 0;
        let mut header = vec![instruction(
            op::FUNCTION,
            &[result_type, function_id, 0, function_type],
        )];
        let mut parameters = Vec::new();
        for (index, local) in function.parameters.iter().enumerate() {
            let parameter = self.fresh();
            header.push(instruction(
                op::FUNCTION_PARAMETER,
                &[parameter_types[index], parameter],
            ));
            parameters.push((local, parameter, parameter_kinds[index]));
        }
        let entry = self.fresh();
        self.block = entry;
        for (local, parameter, kind) in parameters {
            let slot = if local.mutable {
                let variable = self.new_variable(kind);
                self.emit(instruction(op::STORE, &[variable, parameter]));
                Slot::Variable(variable, kind)
            } else {
                Slot::Value(Val {
                    id: parameter,
                    kind,
                })
            };
            self.locals.insert(local.id, slot);
        }
        let Some(value) = self.expression(&function.body)? else {
            return Err(unsupported(
                "GPU kernel functions must return a scalar value",
                function.span,
            ));
        };
        if value.kind != result_kind {
            return Err(unsupported(
                "GPU kernel function result type mismatch",
                function.span,
            ));
        }
        self.emit(instruction(op::RETURN_VALUE, &[value.id]));
        self.weights.insert(id, self.weight);
        let mut words = header.concat();
        words.extend(instruction(op::LABEL, &[entry]));
        words.extend(self.variables.concat());
        words.extend(self.body.concat());
        words.extend(instruction(op::FUNCTION_END, &[]));
        self.module.functions.push(words);
        Ok(())
    }

    fn new_variable(&mut self, kind: Kind) -> u32 {
        let ty = self.type_of(kind);
        let pointer = self.module.ty(TypeKey::Pointer(STORAGE_FUNCTION, ty));
        let id = self.fresh();
        self.variables
            .push(instruction(op::VARIABLE, &[pointer, id, STORAGE_FUNCTION]));
        id
    }

    /// One of the two entry points: `map_main` reads lane `base + x` of the input; `init_main` passes the index.
    fn entry_point(
        &mut self,
        name: &str,
        init: bool,
        kernel_function: u32,
        interface: &Interface,
        void: u32,
        void_function: u32,
    ) -> u32 {
        self.body.clear();
        self.variables.clear();
        let u32_type = self.type_of(Kind::U32);
        let bool_type = self.type_of(Kind::Bool);
        let id = self.fresh();
        let entry = self.fresh();
        self.block = entry;
        let zero = self.integer_constant(Kind::U32, 0);
        let one = self.integer_constant(Kind::U32, 1);
        let vector = self.module.ty(TypeKey::Vector3(u32_type));
        let invocation = self.result(op::LOAD, vector, &[interface.gid]);
        let x = self.result(op::COMPOSITE_EXTRACT, u32_type, &[invocation, 0]);
        let push_pointer = self
            .module
            .ty(TypeKey::Pointer(STORAGE_PUSH_CONSTANT, u32_type));
        let base_pointer = self.result(op::ACCESS_CHAIN, push_pointer, &[interface.push, one]);
        let base = self.result(op::LOAD, u32_type, &[base_pointer]);
        let length_pointer = self.result(op::ACCESS_CHAIN, push_pointer, &[interface.push, zero]);
        let length = self.result(op::LOAD, u32_type, &[length_pointer]);
        let index = self.result(op::I_ADD, u32_type, &[x, base]);
        let inside = self.result(op::U_LESS_THAN, bool_type, &[index, length]);
        let body = self.fresh();
        let end = self.fresh();
        self.emit(instruction(op::SELECTION_MERGE, &[end, 0]));
        self.emit(instruction(op::BRANCH_CONDITIONAL, &[inside, body, end]));
        self.label(body);
        let argument = if init {
            index
        } else {
            let element = self.type_of(interface.input_kind);
            let pointer_type = self.module.ty(TypeKey::Pointer(STORAGE_BUFFER, element));
            let pointer = self.result(
                op::ACCESS_CHAIN,
                pointer_type,
                &[interface.input, zero, index],
            );
            self.result(op::LOAD, element, &[pointer])
        };
        let output_type = self.type_of(interface.output_kind);
        let value = self.result(op::FUNCTION_CALL, output_type, &[kernel_function, argument]);
        let pointer_type = self
            .module
            .ty(TypeKey::Pointer(STORAGE_BUFFER, output_type));
        let pointer = self.result(
            op::ACCESS_CHAIN,
            pointer_type,
            &[interface.output, zero, index],
        );
        self.emit(instruction(op::STORE, &[pointer, value]));
        self.emit(instruction(op::BRANCH, &[end]));
        self.label(end);
        self.emit(instruction(op::RETURN, &[]));
        let mut words = instruction(op::FUNCTION, &[void, id, 0, void_function]);
        words.extend(instruction(op::LABEL, &[entry]));
        words.extend(self.body.drain(..).flatten());
        words.extend(instruction(op::FUNCTION_END, &[]));
        self.module.functions.push(words);
        self.module.name(id, name);
        id
    }

    fn finish(
        &mut self,
        entries: Vec<u32>,
        interface: &Interface,
        init: bool,
    ) -> Result<SpirvKernel, Diagnostic> {
        let strict_float = !self.relaxed && self.module.uses_float;
        let weight = self.weights[&self.kernel.function];
        let module = &mut self.module;
        module.capabilities.insert(CAPABILITY_SHADER);
        if strict_float {
            module
                .capabilities
                .insert(CAPABILITY_SIGNED_ZERO_INF_NAN_PRESERVE);
            module.capabilities.insert(CAPABILITY_DENORM_PRESERVE);
            module.capabilities.insert(CAPABILITY_ROUNDING_MODE_RTE);
            module.extensions.insert("SPV_KHR_float_controls");
        }
        let names = ["map_main", "init_main"];
        for (index, entry) in entries.iter().enumerate() {
            let mut operands = vec![EXECUTION_MODEL_GLCOMPUTE, *entry];
            operands.extend(string_words(names[index]));
            operands.push(interface.gid);
            module
                .entry_points
                .push(instruction(op::ENTRY_POINT, &operands));
            module.execution_modes.push(instruction(
                op::EXECUTION_MODE,
                &[*entry, MODE_LOCAL_SIZE, WORKGROUP_SIZE, 1, 1],
            ));
            if strict_float {
                for mode in [
                    MODE_DENORM_PRESERVE,
                    MODE_SIGNED_ZERO_INF_NAN_PRESERVE,
                    MODE_ROUNDING_MODE_RTE,
                ] {
                    module
                        .execution_modes
                        .push(instruction(op::EXECUTION_MODE, &[*entry, mode, 32]));
                }
            }
        }
        for id in std::mem::take(&mut module.no_contraction) {
            module
                .annotations
                .push(instruction(op::DECORATE, &[id, DECORATION_NO_CONTRACTION]));
        }
        let mut words = vec![SPIRV_MAGIC, SPIRV_VERSION_1_3, 0, module.next + 1, 0];
        for capability in &module.capabilities {
            words.extend(instruction(op::CAPABILITY, &[*capability]));
        }
        for extension in &module.extensions {
            words.extend(instruction(op::EXTENSION, &string_words(extension)));
        }
        words.extend(instruction(op::MEMORY_MODEL, &[0, 1]));
        for section in [
            &module.entry_points,
            &module.execution_modes,
            &module.debug,
            &module.annotations,
            &module.globals,
            &module.functions,
        ] {
            words.extend(section.iter().flatten());
        }
        let mut features = 0;
        if module.uses_int64 {
            features |= FEATURE_INT64;
        }
        if strict_float {
            features |= FEATURE_STRICT_FLOAT;
        }
        Ok(SpirvKernel {
            words,
            features,
            input: interface
                .input_kind
                .lane()
                .expect("lane kinds are not bool"),
            output: interface
                .output_kind
                .lane()
                .expect("lane kinds are not bool"),
            relaxed: self.relaxed,
            init,
            weight,
        })
    }

    fn value(&mut self, expression: &TypedExpr) -> Result<Val, Diagnostic> {
        self.expression(expression)?
            .ok_or_else(|| unsupported("a GPU kernel expression has no value", expression.span))
    }

    fn expression(&mut self, expression: &TypedExpr) -> Result<Option<Val>, Diagnostic> {
        self.weight = self.weight.saturating_add(1);
        let span = expression.span;
        Ok(match &expression.kind {
            TypedExprKind::Int(value) => {
                let kind = self.kind_of(&expression.ty, span)?;
                if !kind.is_integer() {
                    return Err(unsupported(
                        "an integer literal needs an integer type",
                        span,
                    ));
                }
                Some(Val {
                    id: self.integer_constant(kind, *value as u64),
                    kind,
                })
            }
            TypedExprKind::Float(text) => {
                let kind = self.kind_of(&expression.ty, span)?;
                if kind != Kind::F32 {
                    return Err(unsupported(
                        "a floating-point literal needs the f32 type",
                        span,
                    ));
                }
                Some(Val {
                    id: self.float_constant(f32_bits(text)),
                    kind,
                })
            }
            TypedExprKind::Bool(value) => Some(Val {
                id: self.module.boolean(*value),
                kind: Kind::Bool,
            }),
            TypedExprKind::Unit => None,
            TypedExprKind::Local(id) => Some(self.local(*id, span)?),
            TypedExprKind::Unary(operator, operand) => Some(self.unary(*operator, operand, span)?),
            TypedExprKind::Binary(operator, left, right) => {
                Some(self.binary(*operator, left, right, span)?)
            }
            TypedExprKind::Cast(operand) => Some(self.cast(operand, &expression.ty, span)?),
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.branch(condition, then_branch, else_branch, &expression.ty, span)?,
            TypedExprKind::Block { bindings, result } => {
                for (local, initializer) in bindings {
                    let value = self.expression(initializer)?;
                    if local.ty == Type::Unit {
                        continue;
                    }
                    let kind = self.kind_of(&local.ty, local.span)?;
                    let Some(value) = value else {
                        return Err(unsupported("a GPU local needs a value", local.span));
                    };
                    let slot = if local.mutable {
                        let variable = self.new_variable(kind);
                        self.emit(instruction(op::STORE, &[variable, value.id]));
                        Slot::Variable(variable, kind)
                    } else {
                        Slot::Value(value)
                    };
                    self.locals.insert(local.id, slot);
                }
                self.expression(result)?
            }
            TypedExprKind::Assign(place, value) => {
                let TypedExprKind::Local(id) = &place.kind else {
                    return Err(unsupported("GPU assignment requires a scalar local", span));
                };
                let value = self.value(value)?;
                let Some(Slot::Variable(variable, _)) = self.locals.get(id).copied() else {
                    return Err(unsupported(
                        "GPU assignment requires a mutable scalar local",
                        span,
                    ));
                };
                self.emit(instruction(op::STORE, &[variable, value.id]));
                None
            }
            TypedExprKind::Call(callee, arguments) => {
                let id = match &callee.kind {
                    TypedExprKind::Function(FunctionRef::User(id))
                    | TypedExprKind::Closure(id, _) => *id,
                    _ => return Err(unsupported("GPU calls require known functions", span)),
                };
                let mut operands = Vec::new();
                for argument in arguments {
                    operands.push(self.value(argument)?.id);
                }
                let kind = self.kind_of(&expression.ty, span)?;
                let ty = self.type_of(kind);
                let function = *self.functions.get(&id).ok_or_else(|| {
                    unsupported("a GPU kernel calls a function that is not emitted", span)
                })?;
                self.weight = self
                    .weight
                    .saturating_add(self.weights.get(&id).copied().unwrap_or(0));
                let mut all = vec![function];
                all.extend(operands);
                Some(Val {
                    id: self.result(op::FUNCTION_CALL, ty, &all),
                    kind,
                })
            }
            _ => {
                return Err(unsupported(
                    "expression is not supported by SPIR-V generation",
                    span,
                ));
            }
        })
    }

    fn local(&mut self, id: usize, span: Span) -> Result<Val, Diagnostic> {
        match self.locals.get(&id).copied() {
            Some(Slot::Value(value)) => Ok(value),
            Some(Slot::Variable(variable, kind)) => {
                let ty = self.type_of(kind);
                Ok(Val {
                    id: self.result(op::LOAD, ty, &[variable]),
                    kind,
                })
            }
            None => Err(unsupported(
                "a GPU kernel reads a local that is not in scope",
                span,
            )),
        }
    }

    fn unary(
        &mut self,
        operator: UnaryOp,
        operand: &TypedExpr,
        span: Span,
    ) -> Result<Val, Diagnostic> {
        let value = self.value(operand)?;
        let ty = self.type_of(value.kind);
        let id = match (operator, value.kind) {
            (UnaryOp::Plus, _) => return Ok(value),
            (UnaryOp::Negate, Kind::F32) => self.float_result(op::F_NEGATE, ty, &[value.id]),
            (UnaryOp::Negate, kind) if kind.is_integer() => {
                self.result(op::S_NEGATE, ty, &[value.id])
            }
            (UnaryOp::Not, Kind::Bool) => self.result(op::LOGICAL_NOT, ty, &[value.id]),
            (UnaryOp::BitNot, kind) if kind.is_integer() => self.result(op::NOT, ty, &[value.id]),
            _ => {
                return Err(unsupported(
                    "unsupported unary operator in a GPU kernel",
                    span,
                ));
            }
        };
        Ok(Val {
            id,
            kind: value.kind,
        })
    }

    fn binary(
        &mut self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
        span: Span,
    ) -> Result<Val, Diagnostic> {
        if matches!(operator, BinaryOp::And | BinaryOp::Or) {
            return self.short_circuit(operator, left, right);
        }
        let lhs = self.value(left)?;
        let rhs = self.value(right)?;
        let kind = lhs.kind;
        let ty = self.type_of(kind);
        let bool_type = self.type_of(Kind::Bool);
        let compare =
            |signed: u16, unsigned: u16| (if kind.signed() { signed } else { unsigned }, true);
        let (opcode, comparison) = if kind == Kind::F32 {
            match operator {
                BinaryOp::Add => (op::F_ADD, false),
                BinaryOp::Subtract => (op::F_SUB, false),
                BinaryOp::Multiply => (op::F_MUL, false),
                BinaryOp::Divide if self.relaxed => (op::F_DIV, false),
                BinaryOp::Divide => {
                    return Err(unsupported(
                        "strict GPU kernels cannot reproduce floating-point division: SPIR-V OpFDiv may differ by up to 2.5 ULP; use the relaxed API or compute it on the CPU",
                        span,
                    ));
                }
                BinaryOp::Equal => (op::F_ORD_EQUAL, true),
                BinaryOp::NotEqual => (op::F_UNORD_NOT_EQUAL, true),
                BinaryOp::Less => (op::F_ORD_LESS_THAN, true),
                BinaryOp::LessEqual => (op::F_ORD_LESS_THAN_EQUAL, true),
                BinaryOp::Greater => (op::F_ORD_GREATER_THAN, true),
                BinaryOp::GreaterEqual => (op::F_ORD_GREATER_THAN_EQUAL, true),
                BinaryOp::Remainder => {
                    return Err(unsupported(
                        "GPU kernels do not support floating-point remainder; compute it on the CPU",
                        span,
                    ));
                }
                _ => {
                    return Err(unsupported(
                        "unsupported floating-point operator in a GPU kernel",
                        span,
                    ));
                }
            }
        } else if kind == Kind::Bool {
            match operator {
                BinaryOp::Equal => (op::LOGICAL_EQUAL, true),
                BinaryOp::NotEqual => (op::LOGICAL_NOT_EQUAL, true),
                _ => {
                    return Err(unsupported(
                        "unsupported boolean operator in a GPU kernel",
                        span,
                    ));
                }
            }
        } else {
            match operator {
                BinaryOp::Add => (op::I_ADD, false),
                BinaryOp::Subtract => (op::I_SUB, false),
                BinaryOp::Multiply => (op::I_MUL, false),
                BinaryOp::BitAnd => (op::BITWISE_AND, false),
                BinaryOp::BitOr => (op::BITWISE_OR, false),
                BinaryOp::BitXor => (op::BITWISE_XOR, false),
                BinaryOp::Equal => (op::I_EQUAL, true),
                BinaryOp::NotEqual => (op::I_NOT_EQUAL, true),
                BinaryOp::Less => compare(op::S_LESS_THAN, op::U_LESS_THAN),
                BinaryOp::LessEqual => compare(op::S_LESS_THAN_EQUAL, op::U_LESS_THAN_EQUAL),
                BinaryOp::Greater => compare(op::S_GREATER_THAN, op::U_GREATER_THAN),
                BinaryOp::GreaterEqual => {
                    compare(op::S_GREATER_THAN_EQUAL, op::U_GREATER_THAN_EQUAL)
                }
                BinaryOp::ShiftLeft | BinaryOp::ShiftRight | BinaryOp::ShiftRightUnsigned => {
                    if rhs.kind != kind {
                        return Err(unsupported(
                            "a GPU shift count must have the type of the value",
                            span,
                        ));
                    }
                    let mask = self.integer_constant(kind, u64::from(kind.width() - 1));
                    let count = self.result(op::BITWISE_AND, ty, &[rhs.id, mask]);
                    let opcode = match operator {
                        BinaryOp::ShiftLeft => op::SHIFT_LEFT_LOGICAL,
                        BinaryOp::ShiftRight if kind.signed() => op::SHIFT_RIGHT_ARITHMETIC,
                        _ => op::SHIFT_RIGHT_LOGICAL,
                    };
                    return Ok(Val {
                        id: self.result(opcode, ty, &[lhs.id, count]),
                        kind,
                    });
                }
                BinaryOp::Power => {
                    return Err(unsupported(
                        "GPU kernels do not support '**'; multiply explicitly",
                        span,
                    ));
                }
                _ => {
                    return Err(unsupported(
                        "GPU kernel division/remainder may trap on the CPU and are not supported",
                        span,
                    ));
                }
            }
        };
        let id = if comparison {
            self.result(opcode, bool_type, &[lhs.id, rhs.id])
        } else if kind == Kind::F32 {
            self.float_result(opcode, ty, &[lhs.id, rhs.id])
        } else {
            self.result(opcode, ty, &[lhs.id, rhs.id])
        };
        Ok(Val {
            id,
            kind: if comparison { Kind::Bool } else { kind },
        })
    }

    /// `a && b` and `a || b`: the right operand runs only when the left one does not decide.
    fn short_circuit(
        &mut self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
    ) -> Result<Val, Diagnostic> {
        let lhs = self.value(left)?;
        let left_end = self.block;
        let evaluate = self.fresh();
        let merge = self.fresh();
        self.emit(instruction(op::SELECTION_MERGE, &[merge, 0]));
        let and = operator == BinaryOp::And;
        let targets = if and {
            [evaluate, merge]
        } else {
            [merge, evaluate]
        };
        self.emit(instruction(
            op::BRANCH_CONDITIONAL,
            &[lhs.id, targets[0], targets[1]],
        ));
        self.label(evaluate);
        let rhs = self.value(right)?;
        let right_end = self.block;
        self.emit(instruction(op::BRANCH, &[merge]));
        self.label(merge);
        let bool_type = self.type_of(Kind::Bool);
        let shortcut = self.module.boolean(!and);
        let id = self.result(op::PHI, bool_type, &[shortcut, left_end, rhs.id, right_end]);
        Ok(Val {
            id,
            kind: Kind::Bool,
        })
    }

    fn branch(
        &mut self,
        condition: &TypedExpr,
        then_branch: &TypedExpr,
        else_branch: &TypedExpr,
        ty: &Type,
        span: Span,
    ) -> Result<Option<Val>, Diagnostic> {
        let condition = self.value(condition)?;
        let then_label = self.fresh();
        let else_label = self.fresh();
        let merge = self.fresh();
        self.emit(instruction(op::SELECTION_MERGE, &[merge, 0]));
        self.emit(instruction(
            op::BRANCH_CONDITIONAL,
            &[condition.id, then_label, else_label],
        ));
        self.label(then_label);
        let then_value = self.expression(then_branch)?;
        let then_end = self.block;
        self.emit(instruction(op::BRANCH, &[merge]));
        self.label(else_label);
        let else_value = self.expression(else_branch)?;
        let else_end = self.block;
        self.emit(instruction(op::BRANCH, &[merge]));
        self.label(merge);
        if *ty == Type::Unit {
            return Ok(None);
        }
        let kind = self.kind_of(ty, span)?;
        let (Some(then_value), Some(else_value)) = (then_value, else_value) else {
            return Err(unsupported(
                "a GPU conditional needs a value in both branches",
                span,
            ));
        };
        let result_type = self.type_of(kind);
        let id = self.result(
            op::PHI,
            result_type,
            &[then_value.id, then_end, else_value.id, else_end],
        );
        Ok(Some(Val { id, kind }))
    }

    fn cast(&mut self, operand: &TypedExpr, target: &Type, span: Span) -> Result<Val, Diagnostic> {
        let value = self.value(operand)?;
        let to = self.kind_of(target, span)?;
        if value.kind == to {
            return Ok(value);
        }
        let ty = self.type_of(to);
        let id = match (value.kind, to) {
            (from, to) if from.is_integer() && to.is_integer() => {
                if from.width() == to.width() {
                    // The same bits: SPIR-V integers carry no signedness.
                    return Ok(Val {
                        id: value.id,
                        kind: to,
                    });
                }
                let opcode = if to.width() < from.width() || !from.signed() {
                    op::U_CONVERT
                } else {
                    op::S_CONVERT
                };
                self.result(opcode, ty, &[value.id])
            }
            (from, Kind::F32) if from.is_integer() => {
                let opcode = if from.signed() {
                    op::CONVERT_S_TO_F
                } else {
                    op::CONVERT_U_TO_F
                };
                self.float_result(opcode, ty, &[value.id])
            }
            (Kind::F32, to) if to.is_integer() => {
                if self.relaxed {
                    return Err(unsupported(
                        "relaxed GPU kernels cannot reproduce Tsuzuri float-to-integer or f64 casts; compute them on the CPU",
                        span,
                    ));
                }
                self.saturating_float_to_integer(value.id, to)
            }
            _ => return Err(unsupported("unsupported cast in a GPU kernel", span)),
        };
        Ok(Val { id, kind: to })
    }

    /// `x as iN`/`uN` for an `f32`: truncation toward zero, saturation at both ends, NaN to 0. `OpConvertFToS/U` is
    /// undefined outside the range of the result, so it only ever sees a value that is inside.
    fn saturating_float_to_integer(&mut self, x: u32, to: Kind) -> u32 {
        let float = self.type_of(Kind::F32);
        let bool_type = self.type_of(Kind::Bool);
        let integer = self.type_of(to);
        let zero = self.float_constant(0);
        let is_nan = self.result(op::IS_NAN, bool_type, &[x]);
        let wide = to.width() == 64;
        // The power of two above the range, which f32 represents exactly: 2^31, 2^32, 2^63, or 2^64.
        let high_bits: u32 = match (wide, to.signed()) {
            (false, true) => 0x4F00_0000,
            (false, false) => 0x4F80_0000,
            (true, true) => 0x5F00_0000,
            (true, false) => 0x5F80_0000,
        };
        let high = self.float_constant(high_bits);
        let at_least_high = self.result(op::F_ORD_GREATER_THAN_EQUAL, bool_type, &[x, high]);
        let (low_bound, maximum, minimum) = if to.signed() {
            let low = self.float_constant(high_bits | 0x8000_0000);
            let (maximum, minimum) = if wide {
                (i64::MAX as u64, i64::MIN as u64)
            } else {
                (i32::MAX as u64, i32::MIN as u32 as u64)
            };
            (low, maximum, minimum)
        } else {
            (zero, if wide { u64::MAX } else { u64::from(u32::MAX) }, 0)
        };
        let below_low = self.result(op::F_ORD_LESS_THAN, bool_type, &[x, low_bound]);
        let outside = self.result(op::LOGICAL_OR, bool_type, &[below_low, at_least_high]);
        let outside = self.result(op::LOGICAL_OR, bool_type, &[outside, is_nan]);
        let safe = self.result(op::SELECT, float, &[outside, zero, x]);
        let opcode = if to.signed() {
            op::CONVERT_F_TO_S
        } else {
            op::CONVERT_F_TO_U
        };
        let converted = self.result(opcode, integer, &[safe]);
        let maximum = self.integer_constant(to, maximum);
        let minimum = self.integer_constant(to, minimum);
        let zero_integer = self.integer_constant(to, 0);
        let saturated_high = self.result(op::SELECT, integer, &[at_least_high, maximum, converted]);
        let saturated = self.result(op::SELECT, integer, &[below_low, minimum, saturated_high]);
        self.result(op::SELECT, integer, &[is_nan, zero_integer, saturated])
    }
}

/// The bits of an `f32` literal: `TypedExprKind::Float` holds the value widened to `f64` as `0x<16 hex digits>`.
fn f32_bits(text: &str) -> u32 {
    let wide = match text.strip_prefix("0x") {
        Some(hex) => {
            f64::from_bits(u64::from_str_radix(hex, 16).expect("float literal is LLVM hex"))
        }
        None => text.parse::<f64>().expect("float literal is decimal"),
    };
    (wide as f32).to_bits()
}
