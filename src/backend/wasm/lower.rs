//! One function of the module as WebAssembly.
//!
//! Every LLVM value is a few WebAssembly locals: one per scalar, so that a struct, an array or a vector
//! is the locals of its scalars, and a 128-bit integer two. An aggregate of more than `FLATTEN_LEAVES`
//! scalars is the address of an immutable copy in the frame. The constants and the addresses of
//! slots and globals are not locals but `Atom`s that are pushed where they are used, and
//! `extractvalue`, `insertvalue` and the like are renamings of atoms, with no code.
//!
//! The blocks are not structured, so they become the cases of one loop: `loop { block { block { ..
//! br_table .. } case 0 } case 1 .. }`, a branch sets the number of the next block and goes back to the
//! loop. A phi is a local that every edge into its block assigns, the new values all pushed on the
//! stack before the first is stored, so that the phis of a block change together. A `ret` leaves the
//! function through one exit block, where the frame is released.

use super::super::error::{Failure, Res, bail};
use super::super::func::{CallArg, Callee, Const, Func, Inst, NONE, Op};
use super::super::module::{Ext, Module};
use super::super::types::{Compound, Ty, Types};
use super::abi::{self, Abi, ParamPlan, RetPlan};
use super::bin::{self, Vt, op};
use super::object::{Body, Kind, Reloc};
use super::{FnInfo, Shared, WInfo};

/// A value that is pushed on the stack where it is used.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Atom {
    Local(u32),
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    /// The address `fp + offset` of the frame.
    Frame(u32),
    /// The address of a global, plus an offset.
    Data {
        sym: u32,
        off: i32,
    },
    /// The table slot of a function.
    Func {
        sym: u32,
    },
}

#[derive(Clone, Debug)]
pub(super) enum Val {
    Void,
    Atoms(Vec<Atom>),
    /// A large aggregate: the address of an immutable copy in memory.
    Ref(Atom),
}

pub(super) struct Lower<'w, 'a> {
    pub shared: &'w Shared<'a>,
    pub module: &'a Module<'a>,
    pub types: &'w mut Types<'a>,
    pub func: &'w Func<'a>,
    pub abi: &'w Abi,
    pub code: Vec<u8>,
    pub relocs: Vec<Reloc>,
    pub call_types: Vec<(Vec<Vt>, Vec<Vt>)>,
    /// The type of every local, the parameters first.
    pub local_vts: Vec<Vt>,
    pub vals: Vec<Option<Val>>,
    pub frame: u32,
    pub fp: Option<u32>,
    pub pc: u32,
    /// The number of blocks of the dispatch loop, and the position of each block in it.
    pub n_blocks: u32,
    pub position: Vec<u32>,
    pub current: u32,
    /// The structured constructs (`if`) that are open inside the code of the current block.
    pub depth: u32,
    pub sret: Option<u32>,
    pub result_locals: Option<Vec<u32>>,
}

pub(super) fn lower_function<'a>(
    shared: &Shared<'a>,
    types: &mut Types<'a>,
    func: &Func<'a>,
    info: &FnInfo,
) -> Res<Body> {
    let lower = Lower {
        shared,
        module: shared.module,
        types,
        func,
        abi: &info.abi,
        code: Vec::new(),
        relocs: Vec::new(),
        call_types: Vec::new(),
        local_vts: info.abi.wasm_params.clone(),
        vals: vec![None; func.n_locals as usize],
        frame: 0,
        fp: None,
        pc: 0,
        n_blocks: 0,
        position: vec![u32::MAX; func.blocks.len()],
        current: 0,
        depth: 0,
        sret: None,
        result_locals: None,
    };
    lower.run()
}

fn align_up(value: u32, align: u32) -> u32 {
    value.div_ceil(align) * align
}

impl<'w, 'a> Lower<'w, 'a> {
    // ---- emission ----

    pub(super) fn op(&mut self, byte: u8) {
        self.code.push(byte);
    }

    pub(super) fn uleb(&mut self, value: u64) {
        bin::uleb(&mut self.code, value);
    }

    pub(super) fn i32_const(&mut self, value: i32) {
        self.op(op::I32_CONST);
        bin::sleb(&mut self.code, i64::from(value));
    }

    pub(super) fn i64_const(&mut self, value: i64) {
        self.op(op::I64_CONST);
        bin::sleb(&mut self.code, value);
    }

    pub(super) fn local_get(&mut self, index: u32) {
        self.op(op::LOCAL_GET);
        self.uleb(u64::from(index));
    }

    pub(super) fn local_set(&mut self, index: u32) {
        self.op(op::LOCAL_SET);
        self.uleb(u64::from(index));
    }

    pub(super) fn new_local(&mut self, vt: Vt) -> u32 {
        self.local_vts.push(vt);
        (self.local_vts.len() - 1) as u32
    }

    /// Stores the value on the top of the stack in a new local.
    pub(super) fn save(&mut self, vt: Vt) -> Atom {
        let local = self.new_local(vt);
        self.local_set(local);
        Atom::Local(local)
    }

    fn reloc(&mut self, kind: Kind, index: u32, addend: i32) {
        self.relocs.push(Reloc {
            offset: self.code.len() as u32,
            kind,
            index,
            addend,
        });
    }

    pub(super) fn fp(&mut self) -> u32 {
        match self.fp {
            Some(fp) => fp,
            None => {
                let fp = self.new_local(Vt::I32);
                self.fp = Some(fp);
                fp
            }
        }
    }

    /// A slot of `size` bytes in the frame.
    pub(super) fn slot(&mut self, size: u64, align: u32) -> Res<u32> {
        let align = align.clamp(1, 16);
        let size = u32::try_from(size).map_err(|_| Failure::new("a frame this large"))?;
        self.frame = align_up(self.frame, align);
        let offset = self.frame;
        self.frame = self
            .frame
            .checked_add(size.max(1))
            .filter(|&frame| frame < 1 << 28)
            .ok_or_else(|| Failure::new("a frame this large"))?;
        Ok(offset)
    }

    pub(super) fn symbol_of(&self, sym: u32) -> Res<u32> {
        match &self.shared.infos[sym as usize] {
            WInfo::Func(info) => Ok(info.symbol),
            WInfo::Data(info) => Ok(info.symbol),
            WInfo::Skip => bail!(
                "a reference to @{}, which the build does not need",
                self.module.syms[sym as usize].name
            ),
        }
    }

    /// Pushes an atom.
    pub(super) fn push(&mut self, atom: &Atom) -> Res<()> {
        match atom {
            Atom::Local(index) => self.local_get(*index),
            Atom::I32(value) => self.i32_const(*value),
            Atom::I64(value) => self.i64_const(*value),
            Atom::F32(bits) => {
                self.op(op::F32_CONST);
                self.code.extend_from_slice(&bits.to_le_bytes());
            }
            Atom::F64(bits) => {
                self.op(op::F64_CONST);
                self.code.extend_from_slice(&bits.to_le_bytes());
            }
            Atom::Frame(offset) => {
                let fp = self.fp();
                self.local_get(fp);
                if *offset != 0 {
                    self.i32_const(*offset as i32);
                    self.op(op::I32_ADD);
                }
            }
            Atom::Data { sym, off } => {
                let index = self.symbol_of(*sym)?;
                self.op(op::I32_CONST);
                self.reloc(Kind::MemoryAddrSleb, index, *off);
                bin::sleb_padded(&mut self.code, 0);
            }
            Atom::Func { sym } => {
                let index = self.symbol_of(*sym)?;
                self.op(op::I32_CONST);
                self.reloc(Kind::TableIndexSleb, index, 0);
                bin::sleb_padded(&mut self.code, 0);
            }
        }
        Ok(())
    }

    pub(super) fn push_all(&mut self, atoms: &[Atom]) -> Res<()> {
        for atom in atoms {
            self.push(atom)?;
        }
        Ok(())
    }

    pub(super) fn atom_vt(&self, atom: &Atom) -> Vt {
        match atom {
            Atom::Local(index) => self.local_vts[*index as usize],
            Atom::I32(_) | Atom::Frame(_) | Atom::Data { .. } | Atom::Func { .. } => Vt::I32,
            Atom::I64(_) => Vt::I64,
            Atom::F32(_) => Vt::F32,
            Atom::F64(_) => Vt::F64,
        }
    }

    /// The address `base + offset` on the stack.
    pub(super) fn push_address(&mut self, base: &Atom, offset: u64) -> Res<()> {
        let offset = i32::try_from(offset).map_err(|_| Failure::new("an offset this large"))?;
        match base {
            Atom::Frame(at) => self.push(&Atom::Frame(at.wrapping_add(offset as u32))),
            Atom::Data { sym, off } => self.push(&Atom::Data {
                sym: *sym,
                off: off.wrapping_add(offset),
            }),
            other => {
                self.push(other)?;
                if offset != 0 {
                    self.i32_const(offset);
                    self.op(op::I32_ADD);
                }
                Ok(())
            }
        }
    }

    /// A base for a memory access: the address on the stack and the offset the instruction carries.
    pub(super) fn push_base(&mut self, base: &Atom, offset: u64) -> Res<u32> {
        match base {
            Atom::Frame(at) => {
                let fp = self.fp();
                self.local_get(fp);
                u32::try_from(u64::from(*at) + offset)
                    .map_err(|_| Failure::new("an offset this large"))
            }
            Atom::Data { .. } => {
                self.push_address(base, offset)?;
                Ok(0)
            }
            other => {
                self.push(other)?;
                u32::try_from(offset).map_err(|_| Failure::new("an offset this large"))
            }
        }
    }

    pub(super) fn memarg(&mut self, align: u32, offset: u32) {
        self.uleb(u64::from(align));
        self.uleb(u64::from(offset));
    }

    /// `memory.copy` of `size` bytes, with the addresses on the stack already pushed by `dst`
    /// and `src`.
    pub(super) fn copy_memory(
        &mut self,
        dst: &Atom,
        dst_off: u64,
        src: &Atom,
        src_off: u64,
        size: u64,
    ) -> Res<()> {
        if size == 0 {
            return Ok(());
        }
        self.push_address(dst, dst_off)?;
        self.push_address(src, src_off)?;
        let size = i32::try_from(size).map_err(|_| Failure::new("a copy this large"))?;
        self.i32_const(size);
        self.op(op::PREFIX);
        self.uleb(u64::from(op::MEMORY_COPY));
        self.code.extend_from_slice(&[0x00, 0x00]);
        Ok(())
    }

    pub(super) fn fill_zero(&mut self, dst: &Atom, size: u64) -> Res<()> {
        if size == 0 {
            return Ok(());
        }
        self.push_address(dst, 0)?;
        self.i32_const(0);
        let size = i32::try_from(size).map_err(|_| Failure::new("a fill this large"))?;
        self.i32_const(size);
        self.op(op::PREFIX);
        self.uleb(u64::from(op::MEMORY_FILL));
        self.code.push(0x00);
        Ok(())
    }

    // ---- types and values ----

    pub(super) fn flat(&self, ty: Ty) -> Res<Vec<Vt>> {
        abi::flat(self.types, ty)
    }

    pub(super) fn is_large(&self, ty: Ty) -> Res<bool> {
        self.types.is_large(ty)
    }

    pub(super) fn vector_shape(&self, ty: Ty) -> Option<(u32, Ty)> {
        match self.types.shape(ty) {
            Some(Compound::Vector { len, elem }) => Some((*len, *elem)),
            _ => None,
        }
    }

    pub(super) fn zero_atom(vt: Vt) -> Atom {
        match vt {
            Vt::I32 => Atom::I32(0),
            Vt::I64 => Atom::I64(0),
            Vt::F32 => Atom::F32(0),
            Vt::F64 => Atom::F64(0),
        }
    }

    /// The value of every bit zero.
    pub(super) fn zero_val(&mut self, ty: Ty) -> Res<Val> {
        if ty == Ty::Void {
            return Ok(Val::Void);
        }
        if self.is_large(ty)? {
            let size = self.types.size_of(ty)?;
            let align = self.types.align_of(ty)?;
            let offset = self.slot(size, align)?;
            self.fill_zero(&Atom::Frame(offset), size)?;
            return Ok(Val::Ref(Atom::Frame(offset)));
        }
        let atoms = self.flat(ty)?.into_iter().map(Self::zero_atom).collect();
        Ok(Val::Atoms(atoms))
    }

    pub(super) fn const_val(&mut self, constant: &Const, ty: Ty) -> Res<Val> {
        match (constant, ty) {
            (Const::Zero, _) => self.zero_val(ty),
            (Const::Int(value), Ty::Int(bits)) => {
                let mask = if bits >= 128 {
                    u128::MAX
                } else {
                    (1u128 << bits) - 1
                };
                let value = (*value as u128) & mask;
                Ok(Val::Atoms(match bits {
                    1..=32 => vec![Atom::I32(value as u32 as i32)],
                    33..=64 => vec![Atom::I64(value as u64 as i64)],
                    128 => vec![
                        Atom::I64(value as u64 as i64),
                        Atom::I64((value >> 64) as u64 as i64),
                    ],
                    _ => bail!("an integer of {bits} bits"),
                }))
            }
            (Const::Int(value), Ty::Ptr) => Ok(Val::Atoms(vec![Atom::I32(*value as i32)])),
            (Const::Float(bits), Ty::Double) => Ok(Val::Atoms(vec![Atom::F64(*bits)])),
            (Const::Float(bits), Ty::Float) => Ok(Val::Atoms(vec![Atom::F32(
                (f64::from_bits(*bits) as f32).to_bits(),
            )])),
            (Const::Addr { sym: None, offset }, Ty::Ptr) => {
                Ok(Val::Atoms(vec![Atom::I32(*offset as i32)]))
            }
            (Const::Addr { sym: None, offset }, Ty::Int(bits @ 1..=64)) => {
                // `ptrtoint (ptr getelementptr (T, ptr null, 1) to i64)`: the size of a type.
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let value = (*offset as u64) & mask;
                Ok(Val::Atoms(vec![if bits <= 32 {
                    Atom::I32(value as u32 as i32)
                } else {
                    Atom::I64(value as i64)
                }]))
            }
            (Const::Addr { sym: Some(_), .. }, Ty::Int(64)) => {
                // `ptrtoint (ptr @g to i64)` of a 32-bit address: it extends with zeros.
                let address = self.const_val(constant, Ty::Ptr)?;
                self.push_all(&self.atoms(&address)?)?;
                self.op(op::I64_EXTEND_I32_U);
                Ok(Val::Atoms(vec![self.save(Vt::I64)]))
            }
            (
                Const::Addr {
                    sym: Some(sym),
                    offset,
                },
                Ty::Ptr | Ty::Int(32),
            ) => match &self.shared.infos[*sym as usize] {
                WInfo::Func(_) => {
                    if *offset != 0 {
                        bail!(
                            "an offset into the function @{}",
                            self.module.syms[*sym as usize].name
                        );
                    }
                    Ok(Val::Atoms(vec![Atom::Func { sym: *sym }]))
                }
                WInfo::Data(_) => Ok(Val::Atoms(vec![Atom::Data {
                    sym: *sym,
                    off: *offset as i32,
                }])),
                WInfo::Skip => bail!(
                    "a reference to @{}, which the build does not need",
                    self.module.syms[*sym as usize].name
                ),
            },
            (Const::Agg(items), Ty::Compound(_)) => {
                if self.is_large(ty)? {
                    bail!(
                        "a constant aggregate of {} as a value",
                        self.types.describe(ty)
                    );
                }
                let shape = self
                    .types
                    .shape(ty)
                    .cloned()
                    .ok_or_else(|| Failure::new("an aggregate constant of a scalar type"))?;
                let mut atoms = Vec::new();
                match shape {
                    Compound::Struct { fields, .. } => {
                        if fields.len() != items.len() {
                            bail!(
                                "a struct constant with {} fields for {}",
                                items.len(),
                                fields.len()
                            );
                        }
                        for (item, &field) in items.iter().zip(fields.iter()) {
                            atoms.extend(self.const_atoms(item, field)?);
                        }
                    }
                    Compound::Array { elem, .. } | Compound::Vector { elem, .. } => {
                        for item in items {
                            atoms.extend(self.const_atoms(item, elem)?);
                        }
                    }
                }
                Ok(Val::Atoms(atoms))
            }
            (other, _) => bail!("the constant {other:?} of type {}", self.types.describe(ty)),
        }
    }

    fn const_atoms(&mut self, constant: &Const, ty: Ty) -> Res<Vec<Atom>> {
        match self.const_val(constant, ty)? {
            Val::Atoms(atoms) => Ok(atoms),
            Val::Void => Ok(Vec::new()),
            Val::Ref(_) => bail!("a large aggregate inside a constant"),
        }
    }

    pub(super) fn val(&mut self, operand: Op, ty: Ty) -> Res<Val> {
        match operand {
            Op::Local(id) => match self.vals.get(id as usize).and_then(Clone::clone) {
                Some(value) => Ok(value),
                None => bail!("a value that is used before it is defined"),
            },
            Op::Const(id) => {
                let func = self.func;
                self.const_val(&func.consts[id as usize], ty)
            }
        }
    }

    /// The atoms of a value that is not a large aggregate.
    pub(super) fn atoms(&self, value: &Val) -> Res<Vec<Atom>> {
        match value {
            Val::Atoms(atoms) => Ok(atoms.clone()),
            Val::Void => Ok(Vec::new()),
            Val::Ref(_) => bail!("a large aggregate where its scalars are needed"),
        }
    }

    pub(super) fn single(&self, value: &Val) -> Res<Atom> {
        match value {
            Val::Atoms(atoms) if atoms.len() == 1 => Ok(atoms[0].clone()),
            _ => bail!("a value of more than one scalar where one is expected"),
        }
    }

    pub(super) fn set(&mut self, dst: u32, value: Val) {
        self.vals[dst as usize] = Some(value);
    }

    // ---- the function ----

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

    /// The phis at the start of a block.
    fn phis(&self, id: u32) -> Vec<Inst<'a>> {
        let block = &self.func.blocks[id as usize];
        self.func.insts[block.first as usize..block.last as usize]
            .iter()
            .take_while(|inst| matches!(inst, Inst::Phi { .. }))
            .copied()
            .collect()
    }

    fn run(mut self) -> Res<Body> {
        let func = self.func;
        let order = self.reverse_postorder();
        self.n_blocks = order.len() as u32;
        for (position, &id) in order.iter().enumerate() {
            self.position[id as usize] = position as u32;
        }
        if self.n_blocks > 1 {
            self.pc = self.new_local(Vt::I32);
        }
        self.bind_params()?;
        // The phis are locals before any block runs, because an edge assigns them.
        for &id in &order {
            for phi in self.phis(id) {
                let Inst::Phi { dst, ty, .. } = phi else {
                    continue;
                };
                if self.is_large(ty)? {
                    let size = self.types.size_of(ty)?;
                    let align = self.types.align_of(ty)?;
                    let offset = self.slot(size, align)?;
                    self.set(dst, Val::Ref(Atom::Frame(offset)));
                } else {
                    let locals: Vec<Atom> = self
                        .flat(ty)?
                        .into_iter()
                        .map(|vt| Atom::Local(self.new_local(vt)))
                        .collect();
                    self.set(dst, Val::Atoms(locals));
                }
            }
        }

        // The body: an exit block around the dispatch loop.
        self.op(op::BLOCK);
        self.code.push(0x40);
        if self.n_blocks > 1 {
            self.op(op::LOOP);
            self.code.push(0x40);
            for _ in 0..self.n_blocks {
                self.op(op::BLOCK);
                self.code.push(0x40);
            }
            self.local_get(self.pc);
            self.op(op::BR_TABLE);
            self.uleb(u64::from(self.n_blocks - 1));
            for target in 0..self.n_blocks - 1 {
                self.uleb(u64::from(target));
            }
            self.uleb(u64::from(self.n_blocks - 1));
        }
        for (position, &id) in order.iter().enumerate() {
            self.current = position as u32;
            self.depth = 0;
            if self.n_blocks > 1 {
                // The end of the block that the dispatch jumps out of: its code follows.
                self.op(op::END);
            }
            let block = &func.blocks[id as usize];
            for index in block.first..block.last {
                let inst = func.insts[index as usize];
                if matches!(inst, Inst::Phi { .. }) {
                    continue;
                }
                self.inst(inst, id)?;
            }
        }
        if self.n_blocks > 1 {
            self.op(op::END);
        }
        self.op(op::END);
        self.finish()
    }

    /// The prologue, the exit and the relocations of the body that was written.
    fn finish(mut self) -> Res<Body> {
        let mut prologue = Vec::new();
        let mut prologue_relocs = Vec::new();
        let frame = align_up(self.frame, 16);
        let sp = self.shared.sp_symbol;
        if self.fp.is_some() && frame > 0 {
            let fp = self.fp.unwrap_or(0);
            prologue.push(op::GLOBAL_GET);
            prologue_relocs.push(Reloc {
                offset: prologue.len() as u32,
                kind: Kind::GlobalIndex,
                index: sp,
                addend: 0,
            });
            bin::uleb_padded(&mut prologue, 0);
            prologue.push(op::I32_CONST);
            bin::sleb(&mut prologue, i64::from(frame));
            prologue.push(op::I32_SUB);
            prologue.push(op::LOCAL_TEE);
            bin::uleb(&mut prologue, u64::from(fp));
            prologue.push(op::GLOBAL_SET);
            prologue_relocs.push(Reloc {
                offset: prologue.len() as u32,
                kind: Kind::GlobalIndex,
                index: sp,
                addend: 0,
            });
            bin::uleb_padded(&mut prologue, 0);
        }
        // After the exit block: release the frame and return the values.
        let mut epilogue = Vec::new();
        let mut epilogue_relocs = Vec::new();
        if self.fp.is_some() && frame > 0 {
            let fp = self.fp.unwrap_or(0);
            epilogue.push(op::LOCAL_GET);
            bin::uleb(&mut epilogue, u64::from(fp));
            epilogue.push(op::I32_CONST);
            bin::sleb(&mut epilogue, i64::from(frame));
            epilogue.push(op::I32_ADD);
            epilogue.push(op::GLOBAL_SET);
            epilogue_relocs.push(Reloc {
                offset: epilogue.len() as u32,
                kind: Kind::GlobalIndex,
                index: sp,
                addend: 0,
            });
            bin::uleb_padded(&mut epilogue, 0);
        }
        if let Some(results) = &self.result_locals {
            for &local in results {
                epilogue.push(op::LOCAL_GET);
                bin::uleb(&mut epilogue, u64::from(local));
            }
        } else if !self.abi.wasm_results.is_empty() {
            epilogue.push(op::UNREACHABLE);
        }
        // The exit block has ended; what follows is the frame's release, the values and the end.
        let mut locals = Vec::new();
        let declared = &self.local_vts[self.abi.wasm_params.len()..];
        let mut runs: Vec<(u32, Vt)> = Vec::new();
        for &vt in declared {
            match runs.last_mut() {
                Some((count, last)) if *last == vt => *count += 1,
                _ => runs.push((1, vt)),
            }
        }
        bin::uleb(&mut locals, runs.len() as u64);
        for (count, vt) in runs {
            bin::uleb(&mut locals, u64::from(count));
            locals.push(vt.byte());
        }
        let mut code = locals;
        let mut relocs = Vec::with_capacity(self.relocs.len());
        let prologue_at = code.len() as u32;
        for mut reloc in prologue_relocs {
            reloc.offset += prologue_at;
            relocs.push(reloc);
        }
        code.extend_from_slice(&prologue);
        let body_at = code.len() as u32;
        for mut reloc in std::mem::take(&mut self.relocs) {
            reloc.offset += body_at;
            relocs.push(reloc);
        }
        code.extend_from_slice(&self.code);
        let epilogue_at = code.len() as u32;
        for mut reloc in epilogue_relocs {
            reloc.offset += epilogue_at;
            relocs.push(reloc);
        }
        code.extend_from_slice(&epilogue);
        code.push(op::END);
        Ok(Body {
            code,
            relocs,
            types: self.call_types,
        })
    }

    fn bind_params(&mut self) -> Res<()> {
        let func = self.func;
        let mut next = 0u32;
        if self.abi.ret == RetPlan::Sret {
            self.sret = Some(0);
            next = 1;
        }
        for (index, plan) in self.abi.params.iter().enumerate() {
            let param_ty = func.header.params[index].ty;
            match plan {
                ParamPlan::Indirect => {
                    self.set(index as u32, Val::Ref(Atom::Local(next)));
                    next += 1;
                }
                ParamPlan::Leaves(vts) => {
                    let mut atoms = Vec::with_capacity(vts.len());
                    for _ in vts {
                        atoms.push(Atom::Local(next));
                        next += 1;
                    }
                    // The caller of a function that other objects call extends a narrow integer
                    // as its attribute says; the translator keeps the bits above the width zero.
                    if self.abi.external
                        && let Ty::Int(bits @ 1..=31) = param_ty
                        && let [Atom::Local(local)] = atoms[..]
                    {
                        self.local_get(local);
                        self.i32_const((1i64 << bits) as i32 - 1);
                        self.op(op::I32_AND);
                        atoms[0] = self.save(Vt::I32);
                    }
                    self.set(index as u32, Val::Atoms(atoms));
                }
            }
        }
        Ok(())
    }

    // ---- control flow ----

    /// The depth of the label of the dispatch loop, or of the exit block.
    fn loop_depth(&self) -> u32 {
        (self.n_blocks - 1 - self.current) + self.depth
    }

    fn exit_depth(&self) -> u32 {
        if self.n_blocks > 1 {
            self.loop_depth() + 1
        } else {
            self.depth
        }
    }

    /// Goes to a block; with `fall` the next block's code, which follows, needs no branch.
    fn goto(&mut self, target: u32, fall: bool) -> Res<()> {
        let position = self.position[target as usize];
        if position == u32::MAX {
            bail!("a branch to a block that cannot be reached");
        }
        if fall && position == self.current + 1 {
            return Ok(());
        }
        self.i32_const(position as i32);
        self.local_set(self.pc);
        self.op(op::BR);
        let depth = self.loop_depth();
        self.uleb(u64::from(depth));
        Ok(())
    }

    /// The assignments of the phis of `to` for the edge from `from`.
    fn edge(&mut self, from: u32, to: u32) -> Res<()> {
        let func = self.func;
        let phis = self.phis(to);
        if phis.is_empty() {
            return Ok(());
        }
        let mut targets: Vec<u32> = Vec::new();
        let mut large: Vec<(Atom, Atom, u64)> = Vec::new();
        for phi in phis {
            let Inst::Phi {
                dst,
                ty,
                start,
                len,
            } = phi
            else {
                continue;
            };
            let entries = &func.phi_in[start as usize..(start + len) as usize];
            let Some(&(operand, _)) = entries.iter().find(|(_, pred)| *pred == from) else {
                bail!("a phi without a value for the block it is entered from");
            };
            let value = self.val(operand, ty)?;
            let Some(Val::Ref(slot)) = self.vals[dst as usize].clone() else {
                let Some(Val::Atoms(locals)) = self.vals[dst as usize].clone() else {
                    bail!("a phi that has no place");
                };
                let atoms = self.atoms(&value)?;
                self.push_all(&atoms)?;
                for local in locals {
                    if let Atom::Local(index) = local {
                        targets.push(index);
                    }
                }
                continue;
            };
            let Val::Ref(source) = value else {
                bail!("a large aggregate phi with a value that is not in memory");
            };
            large.push((slot, source, self.types.size_of(ty)?));
        }
        for local in targets.into_iter().rev() {
            self.local_set(local);
        }
        for (slot, source, size) in large {
            self.copy_memory(&slot, 0, &source, 0, size)?;
        }
        Ok(())
    }

    fn terminator(&mut self, inst: Inst<'a>, from: u32) -> Res<bool> {
        match inst {
            Inst::Unreachable => {
                self.op(op::UNREACHABLE);
                Ok(true)
            }
            Inst::Ret { ty, val } => {
                self.ret(ty, val)?;
                Ok(true)
            }
            Inst::Br { target } => {
                self.edge(from, target)?;
                self.goto(target, true)?;
                Ok(true)
            }
            Inst::CondBr {
                cond,
                then_block,
                else_block,
            } => {
                let value = self.val(cond, super::super::types::I1)?;
                let atom = self.single(&value)?;
                self.push(&atom)?;
                self.op(op::IF);
                self.code.push(0x40);
                self.depth += 1;
                self.edge(from, then_block)?;
                self.goto(then_block, true)?;
                self.op(op::ELSE);
                self.edge(from, else_block)?;
                self.goto(else_block, true)?;
                self.depth -= 1;
                self.op(op::END);
                Ok(true)
            }
            Inst::Switch {
                ty,
                cond,
                default,
                start,
                len,
            } => {
                let func = self.func;
                let value = self.val(cond, ty)?;
                let atoms = self.atoms(&value)?;
                let [atom] = &atoms[..] else {
                    bail!("a switch on a value of more than one scalar");
                };
                let vt = self.atom_vt(atom);
                let bits = match ty {
                    Ty::Int(bits) => bits,
                    _ => bail!("a switch on {}", self.types.describe(ty)),
                };
                for &(case, target) in &func.cases[start as usize..(start + len) as usize] {
                    self.push(atom)?;
                    let mask = if bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    let case = (case as u128 as u64) & mask;
                    if vt == Vt::I32 {
                        self.i32_const(case as u32 as i32);
                        self.op(op::I32_EQ);
                    } else {
                        self.i64_const(case as i64);
                        self.op(op::I64_EQ);
                    }
                    self.op(op::IF);
                    self.code.push(0x40);
                    self.depth += 1;
                    self.edge(from, target)?;
                    self.goto(target, false)?;
                    self.depth -= 1;
                    self.op(op::END);
                }
                self.edge(from, default)?;
                self.goto(default, true)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn ret(&mut self, ty: Ty, operand: Op) -> Res<()> {
        let value = if ty == Ty::Void {
            Val::Void
        } else {
            self.val(operand, ty)?
        };
        match self.abi.ret.clone() {
            RetPlan::Void => {}
            RetPlan::Sret => {
                let sret = Atom::Local(self.sret.unwrap_or(0));
                self.store_value(&sret, 0, ty, &value)?;
            }
            RetPlan::Leaves(vts) => {
                let atoms = self.atoms(&value)?;
                if atoms.len() != vts.len() {
                    bail!("a return value of the wrong number of scalars");
                }
                let locals = match &self.result_locals {
                    Some(locals) => locals.clone(),
                    None => {
                        let locals: Vec<u32> = vts.iter().map(|&vt| self.new_local(vt)).collect();
                        self.result_locals = Some(locals.clone());
                        locals
                    }
                };
                self.push_all(&atoms)?;
                // A narrow integer returned from a function that others call is extended as it says.
                if self.abi.external
                    && let Ty::Int(bits @ 1..=31) = ty
                    && self.abi.ret_ext == Ext::Sext
                {
                    self.sext32(bits);
                }
                for &local in locals.iter().rev() {
                    self.local_set(local);
                }
            }
        }
        self.op(op::BR);
        let depth = self.exit_depth();
        self.uleb(u64::from(depth));
        Ok(())
    }

    fn inst(&mut self, inst: Inst<'a>, block: u32) -> Res<()> {
        if self.terminator(inst, block)? {
            return Ok(());
        }
        match inst {
            Inst::Bin { dst, op, ty, a, b } => {
                let (a, b) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.binary(op, ty, a, b)?;
                self.set(dst, value);
            }
            Inst::FNeg { dst, ty, a } => {
                let a = self.val(a, ty)?;
                let value = self.fneg(ty, a)?;
                self.set(dst, value);
            }
            Inst::ICmp {
                dst,
                pred,
                ty,
                a,
                b,
            } => {
                let (a, b) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.icmp(pred, ty, a, b)?;
                self.set(dst, value);
            }
            Inst::FCmp {
                dst,
                pred,
                ty,
                a,
                b,
            } => {
                let (a, b) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.fcmp(pred, ty, a, b)?;
                self.set(dst, value);
            }
            Inst::Cast {
                dst,
                op,
                from,
                to,
                a,
            } => {
                let a = self.val(a, from)?;
                let value = self.cast(op, from, to, a)?;
                self.set(dst, value);
            }
            Inst::Select {
                dst,
                cond_ty,
                cond,
                ty,
                a,
                b,
            } => {
                let (cond, a, b) = (self.val(cond, cond_ty)?, self.val(a, ty)?, self.val(b, ty)?);
                let value = self.select(cond_ty, cond, ty, a, b)?;
                self.set(dst, value);
            }
            Inst::Phi { .. } => {}
            Inst::Alloca {
                dst,
                ty,
                count,
                align,
            } => {
                let size = self.types.size_of(ty)?.saturating_mul(count);
                let natural = self.types.align_of(ty)?;
                let wanted = align.max(natural);
                if wanted > 16 {
                    bail!("an alloca aligned to {wanted} bytes");
                }
                let offset = self.slot(size, wanted)?;
                self.set(dst, Val::Atoms(vec![Atom::Frame(offset)]));
            }
            Inst::Load { dst, ty, ptr, .. } => {
                let ptr = self.val(ptr, Ty::Ptr)?;
                let base = self.single(&ptr)?;
                let value = self.load_value(&base, 0, ty)?;
                self.set(dst, value);
            }
            Inst::Store { ty, val, ptr, .. } => {
                let (value, ptr) = (self.val(val, ty)?, self.val(ptr, Ty::Ptr)?);
                let base = self.single(&ptr)?;
                self.store_value(&base, 0, ty, &value)?;
            }
            Inst::Gep {
                dst,
                ty,
                base,
                start,
                len,
            } => {
                let base = self.val(base, Ty::Ptr)?;
                let base = self.single(&base)?;
                let value = self.gep(ty, base, start, len)?;
                self.set(dst, value);
            }
            Inst::ExtractValue {
                dst,
                ty,
                agg,
                start,
                len,
            } => {
                let agg = self.val(agg, ty)?;
                let value = self.extract_value(ty, agg, start, len)?;
                self.set(dst, value);
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
                let (agg, value) = (self.val(agg, ty)?, self.val(val, val_ty)?);
                let value = self.insert_value(ty, agg, val_ty, value, start, len)?;
                self.set(dst, value);
            }
            Inst::ExtractElement {
                dst,
                ty,
                vec,
                idx_ty,
                idx,
            } => {
                let (vec, idx) = (self.val(vec, ty)?, self.val(idx, idx_ty)?);
                let value = self.extract_element(ty, vec, idx)?;
                self.set(dst, value);
            }
            Inst::InsertElement {
                dst,
                ty,
                vec,
                elem,
                idx_ty,
                idx,
            } => {
                let element = self.vector_shape(ty).map(|(_, elem)| elem);
                let Some(element) = element else {
                    bail!("an insertelement on {}", self.types.describe(ty));
                };
                let (vec, elem, idx) = (
                    self.val(vec, ty)?,
                    self.val(elem, element)?,
                    self.val(idx, idx_ty)?,
                );
                let value = self.insert_element(ty, vec, elem, idx)?;
                self.set(dst, value);
            }
            Inst::ShuffleVector {
                dst,
                ty,
                a,
                b,
                start,
                len,
            } => {
                let (a, b) = (self.val(a, ty)?, self.val(b, ty)?);
                let value = self.shuffle(ty, a, b, start, len)?;
                self.set(dst, value);
            }
            Inst::Call {
                dst,
                ret,
                callee,
                start,
                len,
                variadic,
            } => {
                let value = self.call(ret, callee, start, len, variadic)?;
                if dst != NONE {
                    self.set(dst, value);
                }
            }
            Inst::AtomicRmw {
                dst,
                op,
                ty,
                ptr,
                val,
            } => {
                let (ptr, value) = (self.val(ptr, Ty::Ptr)?, self.val(val, ty)?);
                let result = self.atomic_rmw(op, ty, ptr, value)?;
                self.set(dst, result);
            }
            Inst::CmpXchg {
                dst,
                ty,
                ptr,
                cmp,
                new,
            } => {
                let (ptr, cmp, new) = (
                    self.val(ptr, Ty::Ptr)?,
                    self.val(cmp, ty)?,
                    self.val(new, ty)?,
                );
                let result = self.cmpxchg(ty, ptr, cmp, new)?;
                self.set(dst, result);
            }
            Inst::Fence => {}
            Inst::Freeze { dst, ty, a } => {
                let value = self.val(a, ty)?;
                self.set(dst, value);
            }
            Inst::Unreachable
            | Inst::Ret { .. }
            | Inst::Br { .. }
            | Inst::CondBr { .. }
            | Inst::Switch { .. } => {}
        }
        Ok(())
    }

    // ---- calls ----

    fn call(
        &mut self,
        ret: Ty,
        callee: Callee<'a>,
        start: u32,
        len: u32,
        variadic: bool,
    ) -> Res<Val> {
        let func = self.func;
        let args = &func.call_args[start as usize..(start + len) as usize];
        match callee {
            Callee::Intrinsic(name) => self.intrinsic(name, ret, args),
            Callee::Direct(id) => {
                let WInfo::Func(info) = &self.shared.infos[id as usize] else {
                    bail!(
                        "a call to @{}, which is not a function",
                        self.module.syms[id as usize].name
                    );
                };
                let declared_variadic = matches!(
                    &self.module.syms[id as usize].kind,
                    super::super::module::SymKind::Function { sig, .. } if sig.variadic
                );
                if variadic || declared_variadic {
                    bail!(
                        "a call of the variadic function @{}",
                        self.module.syms[id as usize].name
                    );
                }
                if info.abi.params.len() != args.len() {
                    bail!(
                        "a call of @{} with {} arguments for {} parameters",
                        self.module.syms[id as usize].name,
                        args.len(),
                        info.abi.params.len()
                    );
                }
                let (abi, symbol, index) = (info.abi.clone(), info.symbol, info.index);
                let result = self.emit_call(&abi, ret, args, |lower| {
                    lower.op(op::CALL);
                    lower.reloc(Kind::FunctionIndex, symbol, 0);
                    bin::uleb_padded(&mut lower.code, index);
                    Ok(())
                })?;
                Ok(result)
            }
            Callee::Indirect(operand) => {
                if variadic {
                    bail!("a call of a variadic function through a pointer");
                }
                let params: Vec<(Ty, Ext)> = args.iter().map(|arg| (arg.ty, arg.ext)).collect();
                let abi = abi::plan(
                    self.types,
                    ret,
                    Ext::None,
                    &params,
                    false,
                    self.shared.multivalue,
                )?;
                let pointer = self.val(operand, Ty::Ptr)?;
                let pointer = self.single(&pointer)?;
                let table = self.shared.table_symbol;
                let signature = (abi.wasm_params.clone(), abi.wasm_results.clone());
                self.emit_call(&abi.clone(), ret, args, move |lower| {
                    lower.push(&pointer)?;
                    lower.op(op::CALL_INDIRECT);
                    let local = lower.call_types.len() as u32;
                    lower.call_types.push(signature.clone());
                    lower.reloc(Kind::TypeIndex, local, 0);
                    bin::uleb_padded(&mut lower.code, 0);
                    lower.reloc(Kind::TableNumber, table, 0);
                    bin::uleb_padded(&mut lower.code, 0);
                    Ok(())
                })
            }
        }
    }

    fn emit_call(
        &mut self,
        abi: &Abi,
        ret: Ty,
        args: &[CallArg],
        finish: impl FnOnce(&mut Self) -> Res<()>,
    ) -> Res<Val> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let mut value = self.val(arg.op, arg.ty)?;
            if let Some(copied) = arg.byval {
                // The callee owns a private copy of what the pointer points to.
                let size = self.types.size_of(copied)?;
                let align = self.types.align_of(copied)?.max(8);
                let copy = self.slot(size, align)?;
                let source = self.single(&value)?;
                self.copy_memory(&Atom::Frame(copy), 0, &source, 0, size)?;
                value = Val::Atoms(vec![Atom::Frame(copy)]);
            }
            values.push((arg.ty, value));
        }
        self.call_values(abi, ret, &values, finish)
    }

    /// A call of a routine of the runtime that the module defines, such as `__multi3`.
    pub(super) fn call_library(&mut self, name: &str, args: Vec<(Ty, Val)>, ret: Ty) -> Res<Val> {
        let Some(&id) = self.module.by_name.get(name) else {
            bail!("the routine {name} of the runtime is not in the module");
        };
        let WInfo::Func(info) = &self.shared.infos[id as usize] else {
            bail!("the routine {name} of the runtime is not a function");
        };
        if info.abi.params.len() != args.len() {
            bail!("a call of {name} with {} arguments", args.len());
        }
        let (abi, symbol, index) = (info.abi.clone(), info.symbol, info.index);
        self.call_values(&abi, ret, &args, |lower| {
            lower.op(op::CALL);
            lower.reloc(Kind::FunctionIndex, symbol, 0);
            bin::uleb_padded(&mut lower.code, index);
            Ok(())
        })
    }

    /// Pushes the arguments as the plan says, makes the call that `finish` emits, and reads the result.
    fn call_values(
        &mut self,
        abi: &Abi,
        ret: Ty,
        values: &[(Ty, Val)],
        finish: impl FnOnce(&mut Self) -> Res<()>,
    ) -> Res<Val> {
        let mut result_slot = None;
        if abi.ret == RetPlan::Sret {
            let size = self.types.size_of(ret)?;
            let align = self.types.align_of(ret)?;
            let offset = self.slot(size, align)?;
            self.push(&Atom::Frame(offset))?;
            result_slot = Some(offset);
        }
        for (position, ((ty, value), plan)) in values.iter().zip(&abi.params).enumerate() {
            match plan {
                ParamPlan::Leaves(_) => {
                    let atoms = self.atoms(value)?;
                    self.push_all(&atoms)?;
                    // The callee of an external function takes a narrow integer extended as the
                    // attribute of the parameter says.
                    if abi.external
                        && let Ty::Int(bits @ 1..=31) = *ty
                        && abi.ext.get(position) == Some(&Ext::Sext)
                    {
                        self.sext32(bits);
                    }
                }
                ParamPlan::Indirect => match value {
                    Val::Ref(address) => self.push(address)?,
                    _ => bail!("a large aggregate argument that is not in memory"),
                },
            }
        }
        finish(self)?;
        Ok(match &abi.ret {
            RetPlan::Void => {
                if ret == Ty::Void {
                    Val::Void
                } else {
                    Val::Atoms(Vec::new())
                }
            }
            RetPlan::Leaves(vts) => {
                let mut locals = Vec::with_capacity(vts.len());
                for &vt in vts {
                    locals.push(self.new_local(vt));
                }
                for &local in locals.iter().rev() {
                    self.local_set(local);
                }
                let mut atoms: Vec<Atom> = locals.into_iter().map(Atom::Local).collect();
                if let (Ty::Int(bits @ 1..=31), true) = (ret, abi.external) {
                    // The callee extended the value as its attribute says; the bits above the width are zero here.
                    self.push(&atoms[0])?;
                    self.i32_const((1i64 << bits) as i32 - 1);
                    self.op(op::I32_AND);
                    atoms[0] = self.save(Vt::I32);
                }
                Val::Atoms(atoms)
            }
            RetPlan::Sret => {
                let offset = result_slot.unwrap_or(0);
                if self.is_large(ret)? {
                    Val::Ref(Atom::Frame(offset))
                } else {
                    self.load_value(&Atom::Frame(offset), 0, ret)?
                }
            }
        })
    }
}
