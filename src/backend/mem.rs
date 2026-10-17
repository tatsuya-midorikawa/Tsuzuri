//! Memory: loads, stores, addresses, aggregates, vectors and atomics.

use cranelift_codegen::ir::{self, AtomicRmwOp, InstBuilder, Value, types};

use super::error::{Failure, Res, bail};
use super::func::{Const, Op, Ordering, RmwOp};
use super::lower::{Lower, Place, Val, imm32, plain};
use super::types::{Compound, Leaf, Ty};

impl<'w, 'a> Lower<'w, 'a> {
    /// The Cranelift type of a memory access of a scalar.
    fn mem_type(&self, leaf: Ty) -> Res<ir::Type> {
        Ok(match leaf {
            Ty::Int(bits) => match self.types.store_size_of(Ty::Int(bits))? {
                1 => types::I8,
                2 => types::I16,
                4 => types::I32,
                8 => types::I64,
                16 => types::I128,
                size => bail!("a memory access of an i{bits} ({size} bytes)"),
            },
            Ty::Float => types::F32,
            Ty::Double => types::F64,
            Ty::Ptr => self.target.pointer,
            Ty::Half => bail!("a memory access of a half"),
            other => bail!("a memory access of {}", self.types.describe(other)),
        })
    }

    pub(super) fn place_of(&mut self, pointer: Val) -> Res<Place> {
        Ok(match pointer {
            Val::Slot { slot, off } => Place::Slot(slot, off),
            Val::S(base) | Val::Ref(base) => Place::Base(base, 0),
            Val::Off { base, off } => Place::Base(base, off),
            Val::Sym { id, off } => Place::Base(self.symbol_address(id)?, off),
            Val::Multi { .. } | Val::Void => bail!("an aggregate used as an address"),
        })
    }

    pub(super) fn place_address(&mut self, place: Place) -> Res<Value> {
        let pointer = self.target.pointer;
        Ok(match place {
            Place::Slot(slot, off) => match imm32(off) {
                Some(off) => self.b.ins().stack_addr(pointer, slot, off),
                None => {
                    let base = self.b.ins().stack_addr(pointer, slot, 0);
                    self.b.ins().iadd_imm(base, off)
                }
            },
            Place::Base(base, 0) => base,
            Place::Base(base, off) => self.b.ins().iadd_imm(base, off),
        })
    }

    fn load_raw(&mut self, place: Place, extra: i64, kind: ir::Type) -> Res<Value> {
        Ok(match place {
            Place::Slot(slot, off) => match imm32(off + extra) {
                Some(off) => self.b.ins().stack_load(kind, slot, off),
                None => {
                    let address = self.place_address(Place::Slot(slot, off + extra))?;
                    self.b.ins().load(kind, plain(), address, 0)
                }
            },
            Place::Base(base, off) => match imm32(off + extra) {
                Some(off) => self.b.ins().load(kind, plain(), base, off),
                None => {
                    let address = self.place_address(Place::Base(base, off + extra))?;
                    self.b.ins().load(kind, plain(), address, 0)
                }
            },
        })
    }

    fn store_raw(&mut self, place: Place, extra: i64, value: Value) -> Res<()> {
        match place {
            Place::Slot(slot, off) => match imm32(off + extra) {
                Some(off) => {
                    self.b.ins().stack_store(value, slot, off);
                }
                None => {
                    let address = self.place_address(Place::Slot(slot, off + extra))?;
                    self.b.ins().store(plain(), value, address, 0);
                }
            },
            Place::Base(base, off) => match imm32(off + extra) {
                Some(off) => {
                    self.b.ins().store(plain(), value, base, off);
                }
                None => {
                    let address = self.place_address(Place::Base(base, off + extra))?;
                    self.b.ins().store(plain(), value, address, 0);
                }
            },
        }
        Ok(())
    }

    pub(super) fn load_leaf(&mut self, place: Place, extra: i64, leaf: Ty) -> Res<Value> {
        let kind = self.mem_type(leaf)?;
        let raw = self.load_raw(place, extra, kind)?;
        Ok(match leaf {
            Ty::Int(bits) => self.canon(raw, bits),
            _ => raw,
        })
    }

    pub(super) fn store_leaf(
        &mut self,
        place: Place,
        extra: i64,
        leaf: Ty,
        value: Value,
    ) -> Res<()> {
        self.mem_type(leaf)?;
        self.store_raw(place, extra, value)
    }

    /// The integer type that holds `lanes` bits in memory.
    fn mask_type(lanes: usize) -> Res<ir::Type> {
        Ok(match lanes.div_ceil(8) {
            1 => types::I8,
            2 => types::I16,
            4 => types::I32,
            8 => types::I64,
            bytes => bail!("a mask vector of {lanes} lanes ({bytes} bytes) in memory"),
        })
    }

    /// Loads the leaves of a layout; a mask vector's lanes come from the bits of one integer.
    pub(super) fn load_leaves(&mut self, place: Place, layout: &[Leaf]) -> Res<Vec<Value>> {
        let mut values = Vec::with_capacity(layout.len());
        let mut index = 0;
        while index < layout.len() {
            let leaf = layout[index];
            match leaf.packed {
                None => {
                    values.push(self.load_leaf(place, leaf.offset as i64, leaf.ty)?);
                    index += 1;
                }
                Some((0, lanes)) => {
                    let kind = Self::mask_type(usize::from(lanes))?;
                    let word = self.load_raw(place, leaf.offset as i64, kind)?;
                    for bit in 0..i64::from(lanes) {
                        let shifted = self.b.ins().ushr_imm(word, bit);
                        let lane = self.b.ins().band_imm(shifted, 1);
                        values.push(if kind == types::I8 {
                            lane
                        } else {
                            self.b.ins().ireduce(types::I8, lane)
                        });
                    }
                    index += usize::from(lanes);
                }
                Some(_) => bail!("a mask lane that does not start its vector"),
            }
        }
        Ok(values)
    }

    /// Stores the leaves of a layout; the lanes of a mask vector are packed into one integer.
    pub(super) fn store_leaves(
        &mut self,
        place: Place,
        layout: &[Leaf],
        values: &[Value],
    ) -> Res<()> {
        let mut index = 0;
        while index < layout.len() {
            let leaf = layout[index];
            match leaf.packed {
                None => {
                    self.store_leaf(place, leaf.offset as i64, leaf.ty, values[index])?;
                    index += 1;
                }
                Some((0, lanes)) => {
                    let kind = Self::mask_type(usize::from(lanes))?;
                    let mut word = self.b.ins().iconst(kind, 0);
                    for bit in 0..usize::from(lanes) {
                        let lane = values[index + bit];
                        let wide = if kind == types::I8 {
                            lane
                        } else {
                            self.b.ins().uextend(kind, lane)
                        };
                        let shifted = self.b.ins().ishl_imm(wide, bit as i64);
                        word = self.b.ins().bor(word, shifted);
                    }
                    self.store_raw(place, leaf.offset as i64, word)?;
                    index += usize::from(lanes);
                }
                Some(_) => bail!("a mask lane that does not start its vector"),
            }
        }
        Ok(())
    }

    /// Copies `size` bytes between memory that does not overlap.
    pub(super) fn copy_memory(&mut self, dest: Value, source: Value, size: u64) -> Res<()> {
        let config = self.frontend;
        self.b
            .emit_small_memory_copy(config, dest, source, size, 1, 1, true, plain());
        Ok(())
    }

    /// Copies `size` bytes between memory that may overlap.
    pub(super) fn copy_memory_overlapping(
        &mut self,
        dest: Value,
        source: Value,
        size: u64,
    ) -> Res<()> {
        let config = self.frontend;
        self.b
            .emit_small_memory_copy(config, dest, source, size, 1, 1, false, plain());
        Ok(())
    }

    pub(super) fn fill_zero(&mut self, address: Value, size: u64) -> Res<()> {
        let config = self.frontend;
        self.b
            .emit_small_memset(config, address, 0, size, 1, plain());
        Ok(())
    }

    pub(super) fn load_value(&mut self, place: Place, ty: Ty) -> Res<Val> {
        match ty {
            Ty::Compound(_) => {
                if self.is_large(ty)? {
                    let slot = self.new_slot_for(ty)?;
                    let destination = self.b.ins().stack_addr(self.target.pointer, slot, 0);
                    let source = self.place_address(place)?;
                    let size = self.types.size_of(ty)?;
                    self.copy_memory(destination, source, size)?;
                    return Ok(Val::Ref(destination));
                }
                let mut layout = Vec::new();
                self.types.leaf_layout(ty, 0, &mut layout)?;
                let leaves = self.load_leaves(place, &layout)?;
                Ok(self.multi(&leaves))
            }
            _ => Ok(Val::S(self.load_leaf(place, 0, ty)?)),
        }
    }

    pub(super) fn store_value(&mut self, place: Place, ty: Ty, value: Val) -> Res<()> {
        match ty {
            Ty::Compound(_) => {
                if self.is_large(ty)? {
                    let Val::Ref(source) = value else {
                        bail!("a large aggregate that is not in memory");
                    };
                    let destination = self.place_address(place)?;
                    let size = self.types.size_of(ty)?;
                    return self.copy_memory(destination, source, size);
                }
                let mut layout = Vec::new();
                self.types.leaf_layout(ty, 0, &mut layout)?;
                let leaves = self.leaves(value)?;
                self.store_leaves(place, &layout, &leaves)
            }
            _ => {
                let scalar = self.scalar(value)?;
                self.store_leaf(place, 0, ty, scalar)
            }
        }
    }

    pub(super) fn load(&mut self, pointer: Val, ty: Ty, atomic: Ordering) -> Res<Val> {
        let place = self.place_of(pointer)?;
        if atomic == Ordering::Atomic {
            let kind = self.mem_type(ty)?;
            if !matches!(ty, Ty::Int(_) | Ty::Ptr) || kind == types::I128 {
                bail!("an atomic load of {}", self.types.describe(ty));
            }
            let address = self.place_address(place)?;
            let value = self.b.ins().atomic_load(kind, plain(), address);
            return Ok(Val::S(value));
        }
        self.load_value(place, ty)
    }

    pub(super) fn store(&mut self, pointer: Val, ty: Ty, value: Val, atomic: Ordering) -> Res<()> {
        let place = self.place_of(pointer)?;
        if atomic == Ordering::Atomic {
            let kind = self.mem_type(ty)?;
            if !matches!(ty, Ty::Int(_) | Ty::Ptr) || kind == types::I128 {
                bail!("an atomic store of {}", self.types.describe(ty));
            }
            let address = self.place_address(place)?;
            let scalar = self.scalar(value)?;
            self.b.ins().atomic_store(plain(), scalar, address);
            return Ok(());
        }
        self.store_value(place, ty, value)
    }

    // ---- addresses ----

    /// The value of an index operand as a signed 64-bit constant, if it is one.
    fn constant_index(&self, op: Op, ty: Ty) -> Option<i64> {
        let Op::Const(id) = op else { return None };
        let value = match &self.func.consts[id as usize] {
            Const::Int(value) => *value,
            Const::Zero => 0,
            _ => return None,
        };
        Some(match ty {
            Ty::Int(bits) if bits < 64 => ((value as i64) << (64 - bits)) >> (64 - bits),
            _ => value as i64,
        })
    }

    pub(super) fn gep(&mut self, source: Ty, base: Val, start: u32, len: u32) -> Res<Val> {
        let func = self.func;
        let indices = &func.gep_idx[start as usize..(start + len) as usize];
        let mut offset = 0i64;
        let mut dynamic: Vec<Value> = Vec::new();
        let mut current = source;
        for (position, &(index_ty, index)) in indices.iter().enumerate() {
            let constant = self.constant_index(index, index_ty);
            let stride;
            if position == 0 {
                stride = self.types.size_of(source)?;
            } else {
                match self.types.shape(current).cloned() {
                    Some(Compound::Struct { fields, .. }) => {
                        let Some(field) = constant else {
                            bail!("a variable index into a struct");
                        };
                        let Some(&field_ty) = fields.get(field as usize) else {
                            bail!("a struct index out of range");
                        };
                        offset += self
                            .types
                            .field_offset(current, field as usize)
                            .ok_or_else(|| Failure::new("a struct index out of range"))?
                            as i64;
                        current = field_ty;
                        continue;
                    }
                    Some(Compound::Array { elem, .. } | Compound::Vector { elem, .. }) => {
                        stride = self.types.size_of(elem)?;
                        current = elem;
                    }
                    None => bail!("an index into {}", self.types.describe(current)),
                }
            }
            match constant {
                Some(value) => {
                    offset = offset.wrapping_add(value.wrapping_mul(stride as i64));
                }
                None => {
                    let Ty::Int(bits) = index_ty else {
                        bail!("an index of type {}", self.types.describe(index_ty));
                    };
                    let value = self.scalar_op(index, index_ty)?;
                    let signed = self.sext_in(value, bits);
                    let wide = if self.b.func.dfg.value_type(signed) == types::I64 {
                        signed
                    } else if self.b.func.dfg.value_type(signed) == types::I128 {
                        self.b.ins().ireduce(types::I64, signed)
                    } else {
                        self.b.ins().sextend(types::I64, signed)
                    };
                    dynamic.push(if stride == 1 {
                        wide
                    } else {
                        self.b.ins().imul_imm(wide, stride as i64)
                    });
                }
            }
        }
        if dynamic.is_empty() {
            return Ok(match base {
                Val::Slot { slot, off } => Val::Slot {
                    slot,
                    off: off.wrapping_add(offset),
                },
                Val::Sym { id, off } => Val::Sym {
                    id,
                    off: off.wrapping_add(offset),
                },
                Val::Off { base, off } => Val::Off {
                    base,
                    off: off.wrapping_add(offset),
                },
                other => {
                    let value = self.scalar(other)?;
                    if offset == 0 {
                        Val::S(value)
                    } else {
                        Val::Off {
                            base: value,
                            off: offset,
                        }
                    }
                }
            });
        }
        let (mut sum, carried) = match base {
            Val::Off { base, off } => (base, off),
            other => (self.scalar(other)?, 0),
        };
        for value in dynamic {
            sum = self.b.ins().iadd(sum, value);
        }
        let total = offset.wrapping_add(carried);
        Ok(if total == 0 {
            Val::S(sum)
        } else {
            Val::Off {
                base: sum,
                off: total,
            }
        })
    }

    // ---- aggregates ----

    /// Where the element at `indices[start..start + len]` of an aggregate of type `ty` is: its
    /// first leaf, its byte offset, and its type.
    fn aggregate_path(&mut self, ty: Ty, start: u32, len: u32) -> Res<(u64, u64, Ty)> {
        let func = self.func;
        let mut current = ty;
        let mut first_leaf = 0u64;
        let mut byte = 0u64;
        for &index in &func.indices[start as usize..(start + len) as usize] {
            match self.types.shape(current).cloned() {
                Some(Compound::Struct { fields, .. }) => {
                    for &field in &fields[..index as usize] {
                        first_leaf += self.types.leaves(field)?;
                    }
                    byte += self
                        .types
                        .field_offset(current, index as usize)
                        .ok_or_else(|| Failure::new("a struct index out of range"))?;
                    current = fields[index as usize];
                }
                Some(Compound::Array { elem, .. }) => {
                    first_leaf += u64::from(index) * self.types.leaves(elem)?;
                    byte += u64::from(index) * self.types.size_of(elem)?;
                    current = elem;
                }
                _ => bail!("an index into {}", self.types.describe(current)),
            }
        }
        Ok((first_leaf, byte, current))
    }

    pub(super) fn extract_value(
        &mut self,
        ty: Ty,
        aggregate: Val,
        start: u32,
        len: u32,
    ) -> Res<Val> {
        let (first, byte, element) = self.aggregate_path(ty, start, len)?;
        match aggregate {
            Val::Multi { start: base, .. } => {
                let at = base as usize + first as usize;
                if matches!(element, Ty::Compound(_)) {
                    Ok(Val::Multi {
                        start: at as u32,
                        len: self.types.leaves(element)? as u32,
                    })
                } else {
                    Ok(Val::S(self.arena[at]))
                }
            }
            Val::Ref(address) => {
                if matches!(element, Ty::Compound(_)) && self.is_large(element)? {
                    return Ok(Val::Ref(self.b.ins().iadd_imm(address, byte as i64)));
                }
                self.load_value(Place::Base(address, byte as i64), element)
            }
            _ => bail!("an extractvalue of a value that is not an aggregate"),
        }
    }

    pub(super) fn insert_value(
        &mut self,
        ty: Ty,
        aggregate: Val,
        value_ty: Ty,
        value: Val,
        start: u32,
        len: u32,
    ) -> Res<Val> {
        let (first, byte, element) = self.aggregate_path(ty, start, len)?;
        let _ = value_ty;
        match aggregate {
            Val::Multi { .. } => {
                let mut leaves = self.leaves(aggregate)?;
                let new = self.leaves(value)?;
                let at = first as usize;
                if new.len() as u64 != self.types.leaves(element)? {
                    bail!("an insertvalue of a value of another size");
                }
                leaves.splice(at..at + new.len(), new);
                Ok(self.multi(&leaves))
            }
            Val::Ref(address) => {
                let slot = self.new_slot_for(ty)?;
                let copy = self.b.ins().stack_addr(self.target.pointer, slot, 0);
                let size = self.types.size_of(ty)?;
                self.copy_memory(copy, address, size)?;
                self.store_value(Place::Base(copy, byte as i64), element, value)?;
                Ok(Val::Ref(copy))
            }
            _ => bail!("an insertvalue into a value that is not an aggregate"),
        }
    }

    // ---- vectors ----

    pub(super) fn extract_element(
        &mut self,
        ty: Ty,
        vector: Val,
        idx_ty: Ty,
        index: Val,
        index_op: Op,
    ) -> Res<Val> {
        let (len, element) = self
            .vector_shape(ty)
            .ok_or_else(|| Failure::new("an extractelement of a non-vector"))?;
        let lanes = self.leaves(vector)?;
        if let Some(lane) = self.constant_index(index_op, idx_ty) {
            return match lanes.get(lane as usize) {
                Some(&value) if lane >= 0 => Ok(Val::S(value)),
                _ => Ok(Val::S(self.zero_of(element)?)),
            };
        }
        // A lane picked at run time: through memory.
        let slot = self.spill_lanes(&lanes, element)?;
        let address = self.lane_address(slot, element, len, index, idx_ty)?;
        Ok(Val::S(self.load_leaf(
            Place::Base(address, 0),
            0,
            element,
        )?))
    }

    pub(super) fn insert_element(
        &mut self,
        ty: Ty,
        vector: Val,
        element_value: Val,
        idx_ty: Ty,
        index: Val,
        index_op: Op,
    ) -> Res<Val> {
        let (len, element) = self
            .vector_shape(ty)
            .ok_or_else(|| Failure::new("an insertelement of a non-vector"))?;
        let mut lanes = self.leaves(vector)?;
        let new = self.scalar(element_value)?;
        if let Some(lane) = self.constant_index(index_op, idx_ty) {
            if lane >= 0 && (lane as usize) < lanes.len() {
                lanes[lane as usize] = new;
            }
            return Ok(self.multi(&lanes));
        }
        let slot = self.spill_lanes(&lanes, element)?;
        let address = self.lane_address(slot, element, len, index, idx_ty)?;
        self.store_leaf(Place::Base(address, 0), 0, element, new)?;
        let mut out = Vec::with_capacity(lanes.len());
        let stride = self.types.size_of(element)? as i64;
        for lane in 0..lanes.len() {
            out.push(self.load_leaf(Place::Slot(slot, 0), lane as i64 * stride, element)?);
        }
        Ok(self.multi(&out))
    }

    fn zero_of(&mut self, ty: Ty) -> Res<Value> {
        let kind = self.clif_type(ty)?;
        Ok(self.zero(kind))
    }

    fn spill_lanes(&mut self, lanes: &[Value], element: Ty) -> Res<ir::StackSlot> {
        let stride = self.types.size_of(element)?;
        let slot = self.new_slot(stride * lanes.len() as u64, 16)?;
        for (lane, &value) in lanes.iter().enumerate() {
            self.store_leaf(
                Place::Slot(slot, 0),
                lane as i64 * stride as i64,
                element,
                value,
            )?;
        }
        Ok(slot)
    }

    /// The address of lane `index` of a vector spilled to `slot`; the index wraps like LLVM's does
    /// when it is in range, and an index out of range is poison.
    fn lane_address(
        &mut self,
        slot: ir::StackSlot,
        element: Ty,
        len: u32,
        index: Val,
        idx_ty: Ty,
    ) -> Res<Value> {
        let stride = self.types.size_of(element)? as i64;
        let Ty::Int(bits) = idx_ty else {
            bail!("a lane index of type {}", self.types.describe(idx_ty));
        };
        let _ = (len, bits);
        let value = self.scalar(index)?;
        let wide = match self.b.func.dfg.value_type(value) {
            kind if kind == types::I64 => value,
            kind if kind == types::I128 => self.b.ins().ireduce(types::I64, value),
            _ => self.b.ins().uextend(types::I64, value),
        };
        let scaled = self.b.ins().imul_imm(wide, stride);
        let base = self.b.ins().stack_addr(self.target.pointer, slot, 0);
        Ok(self.b.ins().iadd(base, scaled))
    }

    pub(super) fn shuffle(&mut self, ty: Ty, a: Val, b: Val, start: u32, len: u32) -> Res<Val> {
        let _ = ty;
        let (la, lb) = (self.leaves(a)?, self.leaves(b)?);
        let func = self.func;
        let mut out = Vec::with_capacity(len as usize);
        for &lane in &func.masks[start as usize..(start + len) as usize] {
            let lane = lane.max(0) as usize;
            let value = if lane < la.len() {
                la[lane]
            } else if let Some(&value) = lb.get(lane - la.len()) {
                value
            } else {
                bail!("a shufflevector mask lane out of range");
            };
            out.push(value);
        }
        Ok(self.multi(&out))
    }

    // ---- atomics ----

    fn atomic_type(&self, ty: Ty) -> Res<ir::Type> {
        match ty {
            Ty::Int(bits) if matches!(bits, 8 | 16 | 32 | 64) => self.clif_type(ty),
            Ty::Ptr => Ok(self.target.pointer),
            other => bail!("an atomic operation on {}", self.types.describe(other)),
        }
    }

    pub(super) fn atomic_rmw(&mut self, op: RmwOp, ty: Ty, pointer: Val, value: Val) -> Res<Val> {
        let kind = self.atomic_type(ty)?;
        let place = self.place_of(pointer)?;
        let address = self.place_address(place)?;
        let operand = self.scalar(value)?;
        let op = match op {
            RmwOp::Xchg => AtomicRmwOp::Xchg,
            RmwOp::Add => AtomicRmwOp::Add,
            RmwOp::Sub => AtomicRmwOp::Sub,
            RmwOp::And => AtomicRmwOp::And,
            RmwOp::Nand => AtomicRmwOp::Nand,
            RmwOp::Or => AtomicRmwOp::Or,
            RmwOp::Xor => AtomicRmwOp::Xor,
            RmwOp::Max => AtomicRmwOp::Smax,
            RmwOp::Min => AtomicRmwOp::Smin,
            RmwOp::UMax => AtomicRmwOp::Umax,
            RmwOp::UMin => AtomicRmwOp::Umin,
        };
        Ok(Val::S(self.b.ins().atomic_rmw(
            kind,
            plain(),
            op,
            address,
            operand,
        )))
    }

    pub(super) fn cmpxchg(
        &mut self,
        ty: Ty,
        pointer: Val,
        expected: Val,
        replacement: Val,
    ) -> Res<Val> {
        self.atomic_type(ty)?;
        let place = self.place_of(pointer)?;
        let address = self.place_address(place)?;
        let (expected, replacement) = (self.scalar(expected)?, self.scalar(replacement)?);
        let old = self
            .b
            .ins()
            .atomic_cas(plain(), address, expected, replacement);
        let success = self
            .b
            .ins()
            .icmp(ir::condcodes::IntCC::Equal, old, expected);
        Ok(self.multi(&[old, success]))
    }
}
