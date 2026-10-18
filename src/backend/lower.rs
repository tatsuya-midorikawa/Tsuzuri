//! One function of the module as Cranelift IR.
//!
//! Every LLVM value maps to Cranelift values: a scalar to one, an aggregate or a vector to one per
//! scalar leaf, and an aggregate of more than `FLATTEN_LEAVES` leaves to the address of an immutable
//! copy in memory. A pointer into a stack slot or a global stays symbolic until something needs the
//! address, so that a load or store of an `alloca` is one frame-relative access. Blocks are lowered
//! in reverse postorder, which puts every definition before its uses; a phi is a block parameter.

use cranelift_codegen::ir::immediates::Imm64;
use cranelift_codegen::ir::{
    self, BlockArg, ExtFuncData, ExternalName, FuncRef, GlobalValue, GlobalValueData, InstBuilder,
    MemFlags, SourceLoc, StackSlot, StackSlotData, StackSlotKind, TrapCode, UserExternalName,
    Value, types,
};
use cranelift_codegen::isa::TargetFrontendConfig;
use cranelift_frontend::{FunctionBuilder, Switch};

use super::abi::{self, Abi, ParamPlan, RetPlan};
use super::error::{Failure, FxMap, Res, bail};
use super::func::{Const, Func, Inst, NONE, Op};
use super::module::Module;
use super::target::{Arch, Target};
use super::types::{Compound, Ty, Types};

/// What the main thread knows about a symbol once it has declared it.
pub(crate) struct SymInfo {
    pub class: SymClass,
    pub id: u32,
    /// Defined in this object and not replaceable at link time: addressable without the GOT.
    pub colocated: bool,
}

pub(crate) enum SymClass {
    Function(Box<Abi>),
    Data { tls: bool },
    Unsupported(&'static str),
}

/// The Cranelift form of one LLVM value.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Val {
    Void,
    /// One scalar: an integer in its container type, a float, or a pointer.
    S(Value),
    /// The scalar leaves of an aggregate or a vector, at `arena[start..start + len]`.
    Multi {
        start: u32,
        len: u32,
    },
    /// The address of an immutable copy of a large aggregate.
    Ref(Value),
    /// A pointer into a stack slot.
    Slot {
        slot: StackSlot,
        off: i64,
    },
    /// A pointer to a function or global symbol.
    Sym {
        id: u32,
        off: i64,
    },
    /// A pointer plus a constant.
    Off {
        base: Value,
        off: i64,
    },
}

pub(crate) struct Lower<'w, 'a> {
    pub(super) b: FunctionBuilder<'w>,
    pub(super) types: &'w mut Types<'a>,
    pub(super) module: &'a Module<'a>,
    pub(super) syms: &'w [SymInfo],
    pub(super) target: &'w Target,
    pub(super) frontend: TargetFrontendConfig,
    pub(super) func: &'w Func<'a>,
    pub(super) abi: &'w Abi,
    pub(super) vals: Vec<Option<Val>>,
    pub(super) arena: Vec<Value>,
    pub(super) blocks: Vec<Option<ir::Block>>,
    pub(super) func_refs: FxMap<(u32, bool), FuncRef>,
    pub(super) gvs: FxMap<u32, GlobalValue>,
    /// The slot that holds a large aggregate phi, by the phi's value number.
    pub(super) phi_slots: FxMap<u32, StackSlot>,
    pub(super) sret: Option<Value>,
}

pub(super) fn imm32(value: i64) -> Option<i32> {
    i32::try_from(value).ok()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_function<'a>(
    function: &mut ir::Function,
    context: &mut cranelift_frontend::FunctionBuilderContext,
    module: &'a Module<'a>,
    types: &mut Types<'a>,
    syms: &[SymInfo],
    target: &Target,
    frontend: TargetFrontendConfig,
    func: &Func<'a>,
    abi: &Abi,
) -> Res<()> {
    if module.debug {
        // Relative to a fixed base, a location is its metadata id whatever the first one is.
        function.params.ensure_base_srcloc(SourceLoc::new(0));
    }
    let b = FunctionBuilder::new(function, context);
    let mut lower = Lower {
        b,
        types,
        module,
        syms,
        target,
        frontend,
        func,
        abi,
        vals: vec![None; func.n_locals as usize],
        arena: Vec::new(),
        blocks: vec![None; func.blocks.len()],
        func_refs: FxMap::default(),
        gvs: FxMap::default(),
        phi_slots: FxMap::default(),
        sret: None,
    };
    lower.run()?;
    lower.b.seal_all_blocks();
    lower.b.finalize();
    Ok(())
}

impl<'w, 'a> Lower<'w, 'a> {
    fn run(&mut self) -> Res<()> {
        let func = self.func;
        let order = self.reverse_postorder();
        for &id in &order {
            self.blocks[id as usize] = Some(self.b.create_block());
        }
        for &id in &order {
            self.declare_phi_params(id)?;
        }
        let entry = self.block(0);
        self.b.append_block_params_for_function_params(entry);
        self.b.switch_to_block(entry);
        self.bind_params(entry)?;
        for (position, &id) in order.iter().enumerate() {
            if position != 0 {
                let block = self.block(id);
                self.b.switch_to_block(block);
            }
            self.bind_phis(id)?;
            let block = &func.blocks[id as usize];
            for index in block.first..block.last {
                let inst = func.insts[index as usize];
                if matches!(inst, Inst::Phi { .. }) {
                    continue;
                }
                if let Some(&location) = func.dbg.get(index as usize) {
                    self.b.set_srcloc(if location == NONE {
                        SourceLoc::default()
                    } else {
                        SourceLoc::new(location)
                    });
                }
                self.inst(inst, id)?;
            }
        }
        Ok(())
    }

    pub(super) fn block(&self, id: u32) -> ir::Block {
        self.blocks[id as usize].expect("a reachable block")
    }

    fn successors(&self, id: u32) -> Vec<u32> {
        let block = &self.func.blocks[id as usize];
        match self.func.insts[block.last as usize - 1] {
            Inst::Br { target } => vec![target],
            Inst::CondBr {
                then_block,
                else_block,
                ..
            } => vec![then_block, else_block],
            Inst::Switch {
                default,
                start,
                len,
                ..
            } => {
                let mut out = vec![default];
                out.extend(
                    self.func.cases[start as usize..(start + len) as usize]
                        .iter()
                        .map(|&(_, block)| block),
                );
                out
            }
            _ => Vec::new(),
        }
    }

    /// The blocks reachable from the entry, each after every block that dominates it.
    fn reverse_postorder(&self) -> Vec<u32> {
        let count = self.func.blocks.len();
        let mut seen = vec![false; count];
        let mut post = Vec::with_capacity(count);
        let mut stack: Vec<(u32, Vec<u32>, usize)> = Vec::new();
        seen[0] = true;
        stack.push((0, self.successors(0), 0));
        while let Some((id, successors, next)) = stack.last_mut() {
            if let Some(&target) = successors.get(*next) {
                *next += 1;
                if !seen[target as usize] {
                    seen[target as usize] = true;
                    let list = self.successors(target);
                    stack.push((target, list, 0));
                }
            } else {
                post.push(*id);
                stack.pop();
            }
        }
        post.reverse();
        post
    }

    fn declare_phi_params(&mut self, id: u32) -> Res<()> {
        let func = self.func;
        let block = &func.blocks[id as usize];
        let clif = self.block(id);
        for index in block.first..block.last {
            let Inst::Phi { dst, ty, .. } = func.insts[index as usize] else {
                break;
            };
            if self.is_large(ty)? {
                let slot = self.new_slot_for(ty)?;
                self.phi_slots.insert(dst, slot);
                continue;
            }
            for kind in self.leaf_clif_types(ty)? {
                self.b.append_block_param(clif, kind);
            }
        }
        Ok(())
    }

    fn bind_phis(&mut self, id: u32) -> Res<()> {
        let func = self.func;
        let block = &func.blocks[id as usize];
        let clif = self.block(id);
        let mut next = 0usize;
        for index in block.first..block.last {
            let Inst::Phi { dst, ty, .. } = func.insts[index as usize] else {
                break;
            };
            if self.is_large(ty)? {
                let slot = self.phi_slots[&dst];
                let address = self.b.ins().stack_addr(self.target.pointer, slot, 0);
                self.vals[dst as usize] = Some(Val::Ref(address));
                continue;
            }
            let count = self.leaf_clif_types(ty)?.len();
            let params: Vec<Value> = self.b.block_params(clif)[next..next + count].to_vec();
            next += count;
            self.vals[dst as usize] = Some(self.leaves_to_val(ty, &params));
        }
        Ok(())
    }

    /// The function's parameters become the values of its parameter locals.
    fn bind_params(&mut self, entry: ir::Block) -> Res<()> {
        let incoming: Vec<Value> = self.b.block_params(entry).to_vec();
        let mut next = 0usize;
        if matches!(self.abi.ret, RetPlan::Sret) {
            self.sret = Some(incoming[0]);
            next = 1;
        }
        for (index, plan) in self.abi.params.iter().enumerate() {
            let value = match plan {
                ParamPlan::Scalar => {
                    next += 1;
                    Val::S(incoming[next - 1])
                }
                ParamPlan::Leaves(kinds) => {
                    let leaves = &incoming[next..next + kinds.len()];
                    next += kinds.len();
                    self.multi(leaves)
                }
                ParamPlan::Indirect => {
                    next += 1;
                    Val::Ref(incoming[next - 1])
                }
            };
            self.vals[index] = Some(value);
        }
        Ok(())
    }

    // ---- values ----

    pub(super) fn multi(&mut self, leaves: &[Value]) -> Val {
        let start = self.arena.len() as u32;
        self.arena.extend_from_slice(leaves);
        Val::Multi {
            start,
            len: leaves.len() as u32,
        }
    }

    /// The leaves of a flattened value, copied out of the arena.
    pub(super) fn leaves(&mut self, value: Val) -> Res<Vec<Value>> {
        match value {
            Val::Multi { start, len } => {
                Ok(self.arena[start as usize..(start + len) as usize].to_vec())
            }
            Val::Void => Ok(Vec::new()),
            other => Ok(vec![self.scalar(other)?]),
        }
    }

    /// Builds the value of type `ty` from its leaves.
    pub(super) fn leaves_to_val(&mut self, ty: Ty, leaves: &[Value]) -> Val {
        match ty {
            Ty::Compound(_) => self.multi(leaves),
            _ => Val::S(leaves[0]),
        }
    }

    pub(super) fn is_large(&mut self, ty: Ty) -> Res<bool> {
        self.types.is_large(ty)
    }

    pub(super) fn leaf_clif_types(&mut self, ty: Ty) -> Res<Vec<ir::Type>> {
        match ty {
            Ty::Compound(_) => abi::leaf_types(self.types, self.target, ty),
            _ => Ok(vec![self.clif_type(ty)?]),
        }
    }

    pub(super) fn clif_type(&self, ty: Ty) -> Res<ir::Type> {
        abi::scalar_type(self.types, self.target, ty)
    }

    /// A value of an operand: the local's value or the constant's.
    pub(super) fn val(&mut self, op: Op, ty: Ty) -> Res<Val> {
        match op {
            Op::Local(index) => match self.vals.get(index as usize).copied().flatten() {
                Some(value) => Ok(value),
                None => bail!("a value used before it is defined (%{index})"),
            },
            Op::Const(index) => {
                let func = self.func;
                self.const_val(&func.consts[index as usize], ty)
            }
        }
    }

    /// The scalar of an operand of scalar type.
    pub(super) fn scalar_op(&mut self, op: Op, ty: Ty) -> Res<Value> {
        let value = self.val(op, ty)?;
        self.scalar(value)
    }

    /// The Cranelift value of a scalar, with a symbolic pointer turned into an address.
    pub(super) fn scalar(&mut self, value: Val) -> Res<Value> {
        let pointer = self.target.pointer;
        Ok(match value {
            Val::S(value) | Val::Ref(value) => value,
            Val::Slot { slot, off } => match imm32(off) {
                Some(off) => self.b.ins().stack_addr(pointer, slot, off),
                None => {
                    let base = self.b.ins().stack_addr(pointer, slot, 0);
                    self.b.ins().iadd_imm(base, off)
                }
            },
            Val::Sym { id, off } => {
                let base = self.symbol_address(id)?;
                if off == 0 {
                    base
                } else {
                    self.b.ins().iadd_imm(base, off)
                }
            }
            Val::Off { base, off } => self.b.ins().iadd_imm(base, off),
            Val::Multi { len: 1, start } => self.arena[start as usize],
            Val::Multi { .. } | Val::Void => bail!("an aggregate where a scalar is expected"),
        })
    }

    /// The address of a symbol: a function's, a global's, or this thread's copy of a thread-local.
    pub(super) fn symbol_address(&mut self, id: u32) -> Res<Value> {
        let pointer = self.target.pointer;
        let info = &self.syms[id as usize];
        match &info.class {
            SymClass::Function(_) => {
                let function = self.func_ref(id, false)?;
                Ok(self.b.ins().func_addr(pointer, function))
            }
            SymClass::Data { tls } => {
                let tls = *tls;
                let global = self.global_value(id)?;
                Ok(if tls {
                    self.b.ins().tls_value(pointer, global)
                } else {
                    self.b.ins().symbol_value(pointer, global)
                })
            }
            SymClass::Unsupported(what) => {
                bail!("{what} (@{})", self.module.syms[id as usize].name)
            }
        }
    }

    fn user_name(&mut self, namespace: u32, id: u32) -> ExternalName {
        let reference = self
            .b
            .func
            .declare_imported_user_function(UserExternalName::new(namespace, id));
        ExternalName::user(reference)
    }

    pub(super) fn global_value(&mut self, id: u32) -> Res<GlobalValue> {
        if let Some(&global) = self.gvs.get(&id) {
            return Ok(global);
        }
        let info = &self.syms[id as usize];
        let (colocated, tls) = match info.class {
            SymClass::Data { tls } => (info.colocated, tls),
            _ => bail!("the address of a function used as data"),
        };
        let name = self.user_name(1, info.id);
        let global = self.b.func.create_global_value(GlobalValueData::Symbol {
            name,
            offset: Imm64::new(0),
            colocated,
            tls,
        });
        self.gvs.insert(id, global);
        Ok(global)
    }

    /// A reference to a function symbol for a call, or for taking its address: the two differ on
    /// Mach-O, where a branch to a function of another object is direct but its address is not.
    pub(super) fn func_ref(&mut self, id: u32, for_call: bool) -> Res<FuncRef> {
        if let Some(&reference) = self.func_refs.get(&(id, for_call)) {
            return Ok(reference);
        }
        let info = &self.syms[id as usize];
        let SymClass::Function(abi) = &info.class else {
            bail!("a call to data");
        };
        let colocated = info.colocated || (for_call && self.target.direct_imports);
        let signature = self.b.import_signature(abi.sig.clone());
        let function_id = info.id;
        let name = self.user_name(0, function_id);
        let reference = self.b.import_function(ExtFuncData {
            name,
            signature,
            colocated,
        });
        self.func_refs.insert((id, for_call), reference);
        Ok(reference)
    }

    // ---- constants ----

    pub(super) fn iconst(&mut self, ty: ir::Type, value: i128) -> Value {
        if ty == types::I128 {
            let low = self.b.ins().iconst(types::I64, value as i64);
            let high = self.b.ins().iconst(types::I64, (value >> 64) as i64);
            return self.b.ins().iconcat(low, high);
        }
        let bits = ty.bits();
        // The immediate is the value's low bits, sign-extended from the type's width.
        let wrapped = if bits >= 64 {
            value as i64
        } else {
            ((value as i64) << (64 - bits)) >> (64 - bits)
        };
        self.b.ins().iconst(ty, wrapped)
    }

    /// A constant of type `ty` as a value.
    pub(super) fn const_val(&mut self, constant: &Const, ty: Ty) -> Res<Val> {
        match ty {
            Ty::Int(bits) => {
                let cty = self.clif_type(ty)?;
                let value = match constant {
                    Const::Int(value) => *value,
                    Const::Zero => 0,
                    Const::Addr { sym: None, offset } => i128::from(*offset),
                    Const::Addr {
                        sym: Some(id),
                        offset,
                    } if bits == 64 => {
                        return Ok(Val::Sym {
                            id: *id,
                            off: *offset,
                        });
                    }
                    _ => bail!("a constant that is not an integer where an integer is expected"),
                };
                let value = if bits >= 128 {
                    value
                } else {
                    ((value as u128) & ((1u128 << bits) - 1)) as i128
                };
                Ok(Val::S(self.iconst(cty, value)))
            }
            Ty::Float => {
                let bits = match constant {
                    Const::Float(bits) => (f64::from_bits(*bits) as f32).to_bits(),
                    Const::Zero => 0,
                    _ => bail!("a constant that is not a float where a float is expected"),
                };
                Ok(Val::S(
                    self.b
                        .ins()
                        .f32const(ir::immediates::Ieee32::with_bits(bits)),
                ))
            }
            Ty::Double => {
                let bits = match constant {
                    Const::Float(bits) => *bits,
                    Const::Zero => 0,
                    _ => bail!("a constant that is not a float where a float is expected"),
                };
                Ok(Val::S(
                    self.b
                        .ins()
                        .f64const(ir::immediates::Ieee64::with_bits(bits)),
                ))
            }
            Ty::Ptr => match constant {
                Const::Addr {
                    sym: Some(id),
                    offset,
                } => Ok(Val::Sym {
                    id: *id,
                    off: *offset,
                }),
                Const::Addr { sym: None, offset } => {
                    Ok(Val::S(self.b.ins().iconst(self.target.pointer, *offset)))
                }
                Const::Zero => Ok(Val::S(self.b.ins().iconst(self.target.pointer, 0))),
                Const::Int(value) => Ok(Val::S(
                    self.b.ins().iconst(self.target.pointer, *value as i64),
                )),
                _ => bail!("a constant that is not a pointer where a pointer is expected"),
            },
            Ty::Compound(_) => {
                if self.is_large(ty)? {
                    return self.large_const(constant, ty);
                }
                let mut leaves = Vec::new();
                self.const_leaves(constant, ty, &mut leaves)?;
                Ok(self.multi(&leaves))
            }
            Ty::Half => bail!("a half-precision constant"),
            other => bail!("a constant of type {}", self.types.describe(other)),
        }
    }

    /// The leaves of a constant aggregate or vector.
    fn const_leaves(&mut self, constant: &Const, ty: Ty, out: &mut Vec<Value>) -> Res<()> {
        let Ty::Compound(_) = ty else {
            let value = self.const_val(constant, ty)?;
            out.push(self.scalar(value)?);
            return Ok(());
        };
        let shape = self.types.shape(ty).cloned().expect("a compound type");
        match (constant, shape) {
            (Const::Zero, shape) => {
                let leaf_types = abi::leaf_types(self.types, self.target, ty)?;
                let _ = shape;
                for kind in leaf_types {
                    out.push(self.zero(kind));
                }
            }
            (Const::Agg(items), Compound::Struct { fields, .. }) => {
                if items.len() != fields.len() {
                    bail!(
                        "a struct constant with {} fields for {} fields",
                        items.len(),
                        fields.len()
                    );
                }
                for (item, &field) in items.iter().zip(fields.iter()) {
                    self.const_leaves(item, field, out)?;
                }
            }
            (Const::Agg(items), Compound::Array { elem, .. } | Compound::Vector { elem, .. }) => {
                for item in items {
                    self.const_leaves(item, elem, out)?;
                }
            }
            (Const::Bytes(bytes), Compound::Array { .. }) => {
                let byte = types::I8;
                for &value in bytes {
                    out.push(self.b.ins().iconst(byte, i64::from(value as i8)));
                }
            }
            _ => bail!("a constant that does not match its aggregate type"),
        }
        Ok(())
    }

    pub(super) fn zero(&mut self, kind: ir::Type) -> Value {
        if kind == types::F32 {
            self.b.ins().f32const(ir::immediates::Ieee32::with_bits(0))
        } else if kind == types::F64 {
            self.b.ins().f64const(ir::immediates::Ieee64::with_bits(0))
        } else {
            self.iconst(kind, 0)
        }
    }

    /// A large constant aggregate: built in a stack slot, the way the program would have built it.
    fn large_const(&mut self, constant: &Const, ty: Ty) -> Res<Val> {
        let slot = self.new_slot_for(ty)?;
        let address = self.b.ins().stack_addr(self.target.pointer, slot, 0);
        match constant {
            Const::Zero => self.fill_zero(address, self.types.store_size_of(ty)?)?,
            _ => self.store_const(Place::Base(address, 0), constant, ty)?,
        }
        Ok(Val::Ref(address))
    }

    /// Stores a constant of type `ty` at `place`, leaf by leaf.
    pub(super) fn store_const(&mut self, place: Place, constant: &Const, ty: Ty) -> Res<()> {
        let mut layout = Vec::new();
        self.types.leaf_layout(ty, 0, &mut layout)?;
        let mut values = Vec::new();
        self.const_leaves(constant, ty, &mut values)?;
        self.store_leaves(place, &layout, &values)
    }

    // ---- stack slots ----

    pub(super) fn new_slot(&mut self, size: u64, align: u32) -> Res<StackSlot> {
        let size =
            u32::try_from(size.max(1)).map_err(|_| Failure::new("a stack object this large"))?;
        let align = align.clamp(1, 16);
        let shift = align.trailing_zeros() as u8;
        Ok(self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            shift,
        )))
    }

    pub(super) fn new_slot_for(&mut self, ty: Ty) -> Res<StackSlot> {
        let size = self.types.size_of(ty)?;
        let align = self.types.align_of(ty)?.max(8);
        self.new_slot(size, align)
    }

    // ---- control flow ----

    /// The arguments a branch from `from` passes to the phis of `to`; large phis are copied here.
    pub(super) fn edge_args(&mut self, from: u32, to: u32) -> Res<Vec<BlockArg>> {
        let func = self.func;
        let block = &func.blocks[to as usize];
        let mut args = Vec::new();
        let mut copies: Vec<(StackSlot, Value, Ty)> = Vec::new();
        for index in block.first..block.last {
            let Inst::Phi {
                dst,
                ty,
                start,
                len,
            } = func.insts[index as usize]
            else {
                break;
            };
            let entries = &func.phi_in[start as usize..(start + len) as usize];
            let Some(&(op, _)) = entries.iter().find(|&&(_, source)| source == from) else {
                bail!("a phi without an entry for one of its predecessors");
            };
            let value = self.val(op, ty)?;
            if self.is_large(ty)? {
                let Val::Ref(address) = value else {
                    bail!("a large phi operand that is not in memory");
                };
                copies.push((self.phi_slots[&dst], address, ty));
                continue;
            }
            for leaf in self.leaves(value)? {
                args.push(BlockArg::Value(leaf));
            }
        }
        // Read every source before writing any phi slot: a phi may take another phi's value.
        let staged: Vec<(StackSlot, Value, Ty)> = if copies.len() > 1 {
            let mut out = Vec::new();
            for (slot, address, ty) in copies {
                let temp = self.new_slot_for(ty)?;
                let temp_address = self.b.ins().stack_addr(self.target.pointer, temp, 0);
                self.copy_memory(temp_address, address, self.types.size_of(ty)?)?;
                out.push((slot, temp_address, ty));
            }
            out
        } else {
            copies
        };
        for (slot, address, ty) in staged {
            let destination = self.b.ins().stack_addr(self.target.pointer, slot, 0);
            self.copy_memory_overlapping(destination, address, self.types.size_of(ty)?)?;
        }
        Ok(args)
    }

    fn jump_to(&mut self, from: u32, to: u32) -> Res<()> {
        let args = self.edge_args(from, to)?;
        let target = self.block(to);
        self.b.ins().jump(target, &args);
        Ok(())
    }

    pub(super) fn trap(&mut self) {
        if self.target.arch == Arch::Aarch64 {
            // `llvm.trap` is `brk #1` there, which raises SIGTRAP; a bare `trap` is SIGILL.
            self.b.ins().debugtrap();
        }
        self.b.ins().trap(TrapCode::unwrap_user(1));
    }

    fn terminator_condition(&mut self, cond: Op) -> Res<Value> {
        self.scalar_op(cond, Ty::Int(1))
    }

    fn branch_inst(&mut self, inst: Inst<'a>, from: u32) -> Res<()> {
        match inst {
            Inst::Br { target } => self.jump_to(from, target),
            Inst::CondBr {
                cond,
                then_block,
                else_block,
            } => {
                let cond = self.terminator_condition(cond)?;
                // The copy of a large phi must happen on its own edge only, so such an edge goes
                // through a block of its own; the other phis travel as block arguments.
                let mut targets: Vec<(ir::Block, Vec<BlockArg>)> = Vec::new();
                let mut edges: Vec<(ir::Block, u32)> = Vec::new();
                for block in [then_block, else_block] {
                    if self.has_large_phis(block)? {
                        let edge = self.b.create_block();
                        edges.push((edge, block));
                        targets.push((edge, Vec::new()));
                    } else {
                        let args = self.edge_args(from, block)?;
                        targets.push((self.block(block), args));
                    }
                }
                self.b.ins().brif(
                    cond,
                    targets[0].0,
                    &targets[0].1,
                    targets[1].0,
                    &targets[1].1,
                );
                for (edge, block) in edges {
                    self.b.switch_to_block(edge);
                    self.jump_to(from, block)?;
                }
                Ok(())
            }
            Inst::Switch {
                ty,
                cond,
                default,
                start,
                len,
            } => self.switch(ty, cond, default, start, len, from),
            Inst::Unreachable => {
                self.trap();
                Ok(())
            }
            Inst::Ret { ty, val } => self.ret(ty, val),
            _ => bail!("a block that does not end in a terminator"),
        }
    }

    fn switch(
        &mut self,
        ty: Ty,
        cond: Op,
        default: u32,
        start: u32,
        len: u32,
        from: u32,
    ) -> Res<()> {
        let Ty::Int(bits) = ty else {
            bail!("a switch on {}", self.types.describe(ty));
        };
        if bits > 64 {
            bail!("a switch on a {bits}-bit integer");
        }
        let value = self.scalar_op(cond, ty)?;
        let func = self.func;
        let cases = &func.cases[start as usize..(start + len) as usize];
        // A destination with phis needs its arguments, which a jump table cannot carry: the table
        // goes to a block of its own that jumps on.
        let mut switch = Switch::new();
        let mut trampolines: Vec<(u32, ir::Block)> = Vec::new();
        let mut target_of = |lower: &mut Self, block: u32| -> Res<ir::Block> {
            if !lower.has_phis(block) {
                return Ok(lower.block(block));
            }
            if let Some(&(_, clif)) = trampolines.iter().find(|&&(id, _)| id == block) {
                return Ok(clif);
            }
            let clif = lower.b.create_block();
            trampolines.push((block, clif));
            Ok(clif)
        };
        let otherwise = target_of(self, default)?;
        let mask = if bits >= 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        };
        let mut seen: Vec<u128> = Vec::new();
        for &(case, block) in cases {
            let key = (case as u128) & mask;
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let target = target_of(self, block)?;
            switch.set_entry(key, target);
        }
        if bits == 1 {
            // A one-bit switch has no table: it is a branch.
            let mut one = None;
            let mut zero = None;
            for &(case, block) in cases {
                if (case as u128) & 1 == 1 {
                    one.get_or_insert(block);
                } else {
                    zero.get_or_insert(block);
                }
            }
            let then_block = one.unwrap_or(default);
            let else_block = zero.unwrap_or(default);
            let (then_clif, else_clif) =
                (target_of(self, then_block)?, target_of(self, else_block)?);
            self.b.ins().brif(value, then_clif, &[], else_clif, &[]);
        } else {
            switch.emit(&mut self.b, value, otherwise);
        }
        for (block, clif) in trampolines {
            self.b.switch_to_block(clif);
            let args = self.edge_args(from, block)?;
            let destination = self.block(block);
            self.b.ins().jump(destination, &args);
        }
        Ok(())
    }

    fn has_phis(&self, block: u32) -> bool {
        let data = &self.func.blocks[block as usize];
        data.first < data.last && matches!(self.func.insts[data.first as usize], Inst::Phi { .. })
    }

    fn has_large_phis(&mut self, block: u32) -> Res<bool> {
        let func = self.func;
        let data = &func.blocks[block as usize];
        for index in data.first..data.last {
            let Inst::Phi { ty, .. } = func.insts[index as usize] else {
                break;
            };
            if self.is_large(ty)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ret(&mut self, ty: Ty, val: Op) -> Res<()> {
        match &self.abi.ret {
            RetPlan::Void => {
                self.b.ins().return_(&[]);
            }
            RetPlan::Scalar(_) => {
                let value = self.scalar_op(val, ty)?;
                self.b.ins().return_(&[value]);
            }
            RetPlan::Leaves(_) => {
                let value = self.val(val, ty)?;
                let leaves = self.leaves(value)?;
                self.b.ins().return_(&leaves);
            }
            RetPlan::Sret => {
                let Some(pointer) = self.sret else {
                    bail!("a result pointer that was not passed");
                };
                let value = self.val(val, ty)?;
                self.store_value(Place::Base(pointer, 0), ty, value)?;
                self.b.ins().return_(&[]);
            }
        }
        Ok(())
    }

    // ---- instructions ----

    fn inst(&mut self, inst: Inst<'a>, block: u32) -> Res<()> {
        match inst {
            Inst::Load {
                dst,
                ty,
                ptr,
                atomic,
            } => {
                let pointer = self.val(ptr, Ty::Ptr)?;
                let value = self.load(pointer, ty, atomic)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::Store {
                ty,
                val,
                ptr,
                atomic,
            } => {
                let value = self.val(val, ty)?;
                let pointer = self.val(ptr, Ty::Ptr)?;
                self.store(pointer, ty, value, atomic)?;
            }
            Inst::Call {
                dst,
                ret,
                callee,
                start,
                len,
                variadic,
            } => {
                let result = self.call(ret, callee, start, len, variadic)?;
                if dst != NONE {
                    self.vals[dst as usize] = Some(result);
                }
            }
            Inst::Br { .. }
            | Inst::CondBr { .. }
            | Inst::Switch { .. }
            | Inst::Unreachable
            | Inst::Ret { .. } => self.branch_inst(inst, block)?,
            Inst::ICmp {
                dst,
                pred,
                ty,
                a,
                b,
            } => {
                let (x, y) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.icmp(pred, ty, x, y)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::FCmp {
                dst,
                pred,
                ty,
                a,
                b,
            } => {
                let (x, y) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.fcmp(pred, ty, x, y)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::Gep {
                dst,
                ty,
                base,
                start,
                len,
            } => {
                let base = self.val(base, Ty::Ptr)?;
                let value = self.gep(ty, base, start, len)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::Alloca {
                dst,
                ty,
                count,
                align,
            } => {
                let size = self
                    .types
                    .size_of(ty)?
                    .checked_mul(count)
                    .ok_or_else(|| Failure::new("an alloca this large"))?;
                let align = align.max(self.types.align_of(ty)?);
                let slot = self.new_slot(size, align)?;
                self.vals[dst as usize] = Some(Val::Slot { slot, off: 0 });
            }
            Inst::Bin { dst, op, ty, a, b } => {
                let (x, y) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.binary(op, ty, x, y)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::FNeg { dst, ty, a } => {
                let x = self.val(a, ty)?;
                let value = self.fneg(ty, x)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::Cast {
                dst,
                op,
                from,
                to,
                a,
            } => {
                let x = self.val(a, from)?;
                let value = self.cast(op, from, to, x)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::Select {
                dst,
                cond_ty,
                cond,
                ty,
                a,
                b,
            } => {
                let c = self.val(cond, cond_ty)?;
                let (x, y) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.select(cond_ty, c, ty, x, y)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::ExtractValue {
                dst,
                ty,
                agg,
                start,
                len,
            } => {
                let aggregate = self.val(agg, ty)?;
                let value = self.extract_value(ty, aggregate, start, len)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::InsertValue {
                dst,
                ty,
                agg,
                val_ty,
                val,
                start,
                len,
            } => {
                let aggregate = self.val(agg, ty)?;
                let element = self.val(val, val_ty)?;
                let value = self.insert_value(ty, aggregate, val_ty, element, start, len)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::ExtractElement {
                dst,
                ty,
                vec,
                idx_ty,
                idx,
            } => {
                let vector = self.val(vec, ty)?;
                let index = self.val(idx, idx_ty)?;
                let value = self.extract_element(ty, vector, idx_ty, index, idx)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::InsertElement {
                dst,
                ty,
                vec,
                elem,
                idx_ty,
                idx,
            } => {
                let vector = self.val(vec, ty)?;
                let element_ty = self.vector_element(ty)?;
                let element = self.val(elem, element_ty)?;
                let index = self.val(idx, idx_ty)?;
                let value = self.insert_element(ty, vector, element, idx_ty, index, idx)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::ShuffleVector {
                dst,
                ty,
                a,
                b,
                start,
                len,
            } => {
                let (x, y) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.shuffle(ty, x, y, start, len)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::AtomicRmw {
                dst,
                op,
                ty,
                ptr,
                val,
            } => {
                let pointer = self.val(ptr, Ty::Ptr)?;
                let operand = self.val(val, ty)?;
                let value = self.atomic_rmw(op, ty, pointer, operand)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::CmpXchg {
                dst,
                ty,
                ptr,
                cmp,
                new,
            } => {
                let pointer = self.val(ptr, Ty::Ptr)?;
                let (expected, replacement) = (self.val(cmp, ty)?, self.val(new, ty)?);
                let value = self.cmpxchg(ty, pointer, expected, replacement)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::Fence => {
                self.b.ins().fence();
            }
            Inst::Freeze { dst, ty, a } => {
                let value = self.val(a, ty)?;
                self.vals[dst as usize] = Some(value);
            }
            Inst::Phi { .. } => {}
        }
        Ok(())
    }
}

/// A memory address as an access wants it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Place {
    Slot(StackSlot, i64),
    Base(Value, i64),
}

/// The flags of a plain memory access: a program may read or write any address it holds.
pub(super) fn plain() -> MemFlags {
    MemFlags::new()
}
