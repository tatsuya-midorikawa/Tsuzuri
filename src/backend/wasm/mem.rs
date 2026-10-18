//! Memory: loads and stores of scalars and aggregates, addresses, the parts of aggregates and
//! vectors, and the atomics, which in a program without threads are plain memory operations.

use super::super::error::{Failure, Res, bail};
use super::super::func::{BinOp, IPred, RmwOp};
use super::super::types::{Compound, Ty};
use super::bin::{Vt, op};
use super::lower::{Atom, Lower, Val};

impl<'w, 'a> Lower<'w, 'a> {
    // ---- loads and stores ----

    pub(super) fn load_scalar(&mut self, base: &Atom, offset: u64, ty: Ty) -> Res<Vec<Atom>> {
        let (opcode, align, vt) = match ty {
            Ty::Int(1..=8) => (op::I32_LOAD8_U, 0, Vt::I32),
            Ty::Int(9..=16) => (op::I32_LOAD16_U, 1, Vt::I32),
            Ty::Int(32) | Ty::Ptr => (op::I32_LOAD, 2, Vt::I32),
            Ty::Int(64) => (op::I64_LOAD, 3, Vt::I64),
            Ty::Float => (op::F32_LOAD, 2, Vt::F32),
            Ty::Double => (op::F64_LOAD, 3, Vt::F64),
            Ty::Int(128) => {
                let mut out = Vec::with_capacity(2);
                for half in 0..2u64 {
                    let at = self.push_base(base, offset + half * 8)?;
                    self.op(op::I64_LOAD);
                    self.memarg(3, at);
                    out.push(self.save(Vt::I64));
                }
                return Ok(out);
            }
            other => bail!("a load of {}", self.types.describe(other)),
        };
        let at = self.push_base(base, offset)?;
        self.op(opcode);
        self.memarg(align, at);
        Ok(vec![self.save(vt)])
    }

    pub(super) fn store_scalar(
        &mut self,
        base: &Atom,
        offset: u64,
        ty: Ty,
        value: &[Atom],
    ) -> Res<()> {
        let (opcode, align) = match ty {
            Ty::Int(1..=8) => (op::I32_STORE8, 0),
            Ty::Int(9..=16) => (op::I32_STORE16, 1),
            Ty::Int(32) | Ty::Ptr => (op::I32_STORE, 2),
            Ty::Int(64) => (op::I64_STORE, 3),
            Ty::Float => (op::F32_STORE, 2),
            Ty::Double => (op::F64_STORE, 3),
            Ty::Int(128) => {
                if value.len() != 2 {
                    bail!("a 128-bit store of a value that is not two words");
                }
                for (half, atom) in value.iter().enumerate() {
                    let at = self.push_base(base, offset + half as u64 * 8)?;
                    self.push(atom)?;
                    self.op(op::I64_STORE);
                    self.memarg(3, at);
                }
                return Ok(());
            }
            other => bail!("a store of {}", self.types.describe(other)),
        };
        let [atom] = value else {
            bail!("a store of a value of more than one scalar");
        };
        let at = self.push_base(base, offset)?;
        self.push(atom)?;
        self.op(opcode);
        self.memarg(align, at);
        Ok(())
    }

    pub(super) fn load_value(&mut self, base: &Atom, offset: u64, ty: Ty) -> Res<Val> {
        if ty == Ty::Void {
            return Ok(Val::Void);
        }
        if self.is_large(ty)? {
            let size = self.types.size_of(ty)?;
            let align = self.types.align_of(ty)?;
            let slot = self.slot(size, align)?;
            self.copy_memory(&Atom::Frame(slot), 0, base, offset, size)?;
            return Ok(Val::Ref(Atom::Frame(slot)));
        }
        let mut leaves = Vec::new();
        self.types.leaf_layout(ty, 0, &mut leaves)?;
        let mut atoms = Vec::with_capacity(leaves.len());
        let mut index = 0;
        while index < leaves.len() {
            let leaf = leaves[index];
            match leaf.packed {
                None => {
                    atoms.extend(self.load_scalar(base, offset + leaf.offset, leaf.ty)?);
                    index += 1;
                }
                // The lanes of a mask vector are the bits of one integer.
                Some((0, lanes)) => {
                    let lanes = usize::from(lanes);
                    let word_ty = Self::mask_word(lanes)?;
                    let word = self.load_scalar(base, offset + leaf.offset, word_ty)?[0].clone();
                    for bit in 0..lanes as i64 {
                        self.push(&word)?;
                        if word_ty == Ty::Int(64) {
                            self.i64_const(bit);
                            self.op(op::I64_SHR_U);
                            self.i64_const(1);
                            self.op(op::I64_AND);
                            self.op(op::I32_WRAP_I64);
                        } else {
                            self.i32_const(bit as i32);
                            self.op(op::I32_SHR_U);
                            self.i32_const(1);
                            self.op(op::I32_AND);
                        }
                        atoms.push(self.save(Vt::I32));
                    }
                    index += lanes;
                }
                Some(_) => bail!("a mask lane that does not start its vector"),
            }
        }
        Ok(Val::Atoms(atoms))
    }

    /// The integer that holds the bits of a mask vector of `lanes` lanes in memory.
    fn mask_word(lanes: usize) -> Res<Ty> {
        Ok(match lanes.div_ceil(8) {
            1 => Ty::Int(8),
            2 => Ty::Int(16),
            4 => Ty::Int(32),
            8 => Ty::Int(64),
            bytes => bail!("a mask vector of {lanes} lanes ({bytes} bytes) in memory"),
        })
    }

    pub(super) fn store_value(&mut self, base: &Atom, offset: u64, ty: Ty, value: &Val) -> Res<()> {
        if ty == Ty::Void {
            return Ok(());
        }
        if self.is_large(ty)? {
            let Val::Ref(source) = value else {
                bail!("a large aggregate that is not in memory is stored");
            };
            let size = self.types.size_of(ty)?;
            return self.copy_memory(base, offset, source, 0, size);
        }
        let atoms = self.atoms(value)?;
        let mut leaves = Vec::new();
        self.types.leaf_layout(ty, 0, &mut leaves)?;
        let mut next = 0usize;
        let mut index = 0;
        while index < leaves.len() {
            let leaf = leaves[index];
            match leaf.packed {
                None => {
                    let count = self.flat(leaf.ty)?.len();
                    let Some(part) = atoms.get(next..next + count) else {
                        bail!("a value with fewer scalars than its type");
                    };
                    self.store_scalar(base, offset + leaf.offset, leaf.ty, part)?;
                    next += count;
                    index += 1;
                }
                // The lanes of a mask vector are the bits of one integer.
                Some((0, lanes)) => {
                    let lanes = usize::from(lanes);
                    let word_ty = Self::mask_word(lanes)?;
                    let wide = word_ty == Ty::Int(64);
                    let Some(part) = atoms.get(next..next + lanes) else {
                        bail!("a value with fewer scalars than its type");
                    };
                    for (bit, lane) in part.iter().enumerate() {
                        self.push(lane)?;
                        if wide {
                            self.op(op::I64_EXTEND_I32_U);
                        }
                        if bit > 0 {
                            if wide {
                                self.i64_const(bit as i64);
                                self.op(op::I64_SHL);
                                self.op(op::I64_OR);
                            } else {
                                self.i32_const(bit as i32);
                                self.op(op::I32_SHL);
                                self.op(op::I32_OR);
                            }
                        }
                    }
                    let word = self.save(if wide { Vt::I64 } else { Vt::I32 });
                    self.store_scalar(base, offset + leaf.offset, word_ty, &[word])?;
                    next += lanes;
                    index += lanes;
                }
                Some(_) => bail!("a mask lane that does not start its vector"),
            }
        }
        Ok(())
    }

    // ---- addresses ----

    fn constant_index(&self, atom: &Atom, ty: Ty) -> Option<i64> {
        let bits = match ty {
            Ty::Int(bits) => bits,
            _ => return None,
        };
        match atom {
            Atom::I32(value) => {
                let raw = u64::from(*value as u32);
                Some(if bits < 32 && raw >> (bits - 1) & 1 == 1 {
                    raw as i64 - (1i64 << bits)
                } else {
                    i64::from(*value)
                })
            }
            Atom::I64(value) => Some(*value),
            _ => None,
        }
    }

    pub(super) fn gep(&mut self, ty: Ty, base: Atom, start: u32, len: u32) -> Res<Val> {
        let func = self.func;
        let indices = &func.gep_idx[start as usize..(start + len) as usize];
        let mut constant = 0i64;
        let mut dynamic: Vec<(Atom, Ty, u64)> = Vec::new();
        let mut current = ty;
        for (position, &(index_ty, operand)) in indices.iter().enumerate() {
            let index = self.val(operand, index_ty)?;
            let atom = self.atoms(&index)?;
            let Some(atom) = atom.first().cloned() else {
                bail!("a getelementptr index of no scalar");
            };
            let known = self.constant_index(&atom, index_ty);
            let stride = if position == 0 {
                self.types.size_of(ty)?
            } else {
                match self.types.shape(current).cloned() {
                    Some(Compound::Struct { fields, .. }) => {
                        let Some(field) = known else {
                            bail!("a struct field chosen by a variable");
                        };
                        let Some(&field_ty) =
                            usize::try_from(field).ok().and_then(|f| fields.get(f))
                        else {
                            bail!("a struct field out of range");
                        };
                        let offset = self
                            .types
                            .field_offset(current, field as usize)
                            .ok_or_else(|| Failure::new("a struct field out of range"))?;
                        constant += offset as i64;
                        current = field_ty;
                        continue;
                    }
                    Some(Compound::Array { elem, .. } | Compound::Vector { elem, .. }) => {
                        current = elem;
                        self.types.size_of(elem)?
                    }
                    _ => bail!("an index into {}", self.types.describe(current)),
                }
            };
            match known {
                Some(value) => constant = constant.wrapping_add(value.wrapping_mul(stride as i64)),
                None => dynamic.push((atom, index_ty, stride)),
            }
        }
        let constant = constant as i32;
        if dynamic.is_empty() {
            let result = match base {
                Atom::Frame(offset) => Atom::Frame(offset.wrapping_add(constant as u32)),
                Atom::Data { sym, off } => Atom::Data {
                    sym,
                    off: off.wrapping_add(constant),
                },
                Atom::I32(value) => Atom::I32(value.wrapping_add(constant)),
                other if constant == 0 => other,
                other => {
                    self.push(&other)?;
                    self.i32_const(constant);
                    self.op(op::I32_ADD);
                    self.save(Vt::I32)
                }
            };
            return Ok(Val::Atoms(vec![result]));
        }
        self.push(&base)?;
        for (atom, index_ty, stride) in dynamic {
            self.push(&atom)?;
            let bits = match index_ty {
                Ty::Int(bits) => bits,
                _ => bail!("a getelementptr index of {}", self.types.describe(index_ty)),
            };
            if bits > 32 {
                self.op(op::I32_WRAP_I64);
            } else {
                self.sext32(bits);
            }
            self.i32_const(stride as i32);
            self.op(op::I32_MUL);
            self.op(op::I32_ADD);
        }
        if constant != 0 {
            self.i32_const(constant);
            self.op(op::I32_ADD);
        }
        Ok(Val::Atoms(vec![self.save(Vt::I32)]))
    }

    // ---- the parts of aggregates ----

    /// The byte offset, the scalar offset and the type that an index path of `ty` reaches.
    fn path(&self, ty: Ty, indices: &[u32]) -> Res<(u64, usize, Ty)> {
        let mut bytes = 0u64;
        let mut scalars = 0usize;
        let mut current = ty;
        for &index in indices {
            match self.types.shape(current).cloned() {
                Some(Compound::Struct { fields, .. }) => {
                    let Some(&field) = fields.get(index as usize) else {
                        bail!("a struct field out of range");
                    };
                    for &before in &fields[..index as usize] {
                        scalars += self.flat(before)?.len();
                    }
                    bytes += self
                        .types
                        .field_offset(current, index as usize)
                        .ok_or_else(|| Failure::new("a struct field out of range"))?;
                    current = field;
                }
                Some(Compound::Array { elem, len }) => {
                    if u64::from(index) >= len {
                        bail!("an array element out of range");
                    }
                    scalars += index as usize * self.flat(elem)?.len();
                    bytes += u64::from(index) * self.types.size_of(elem)?;
                    current = elem;
                }
                _ => bail!("an index into {}", self.types.describe(current)),
            }
        }
        Ok((bytes, scalars, current))
    }

    pub(super) fn extract_value(&mut self, ty: Ty, agg: Val, start: u32, len: u32) -> Res<Val> {
        let func = self.func;
        let indices = &func.indices[start as usize..(start + len) as usize];
        let (bytes, scalars, part) = self.path(ty, indices)?;
        match agg {
            Val::Ref(base) => self.load_value(&base, bytes, part),
            Val::Atoms(atoms) => {
                let count = if self.is_large(part)? {
                    bail!("a large aggregate inside a small one");
                } else {
                    self.flat(part)?.len()
                };
                match atoms.get(scalars..scalars + count) {
                    Some(parts) => Ok(Val::Atoms(parts.to_vec())),
                    None => bail!("an extractvalue past the scalars of the value"),
                }
            }
            Val::Void => bail!("an extractvalue of nothing"),
        }
    }

    pub(super) fn insert_value(
        &mut self,
        ty: Ty,
        agg: Val,
        val_ty: Ty,
        value: Val,
        start: u32,
        len: u32,
    ) -> Res<Val> {
        let func = self.func;
        let indices = &func.indices[start as usize..(start + len) as usize];
        let (bytes, scalars, _) = self.path(ty, indices)?;
        match agg {
            Val::Ref(base) => {
                let size = self.types.size_of(ty)?;
                let align = self.types.align_of(ty)?;
                let slot = self.slot(size, align)?;
                self.copy_memory(&Atom::Frame(slot), 0, &base, 0, size)?;
                self.store_value(&Atom::Frame(slot), bytes, val_ty, &value)?;
                Ok(Val::Ref(Atom::Frame(slot)))
            }
            Val::Atoms(mut atoms) => {
                let new = self.atoms(&value)?;
                let Some(target) = atoms.get_mut(scalars..scalars + new.len()) else {
                    bail!("an insertvalue past the scalars of the value");
                };
                target.clone_from_slice(&new);
                Ok(Val::Atoms(atoms))
            }
            Val::Void => bail!("an insertvalue into nothing"),
        }
    }

    /// The lane that an index names when it is a constant; `None` when it is a value.
    fn lane_index(&self, index: &Val, lanes: u32) -> Res<Option<usize>> {
        let atom = self.single(index)?;
        let value = match atom {
            Atom::I32(value) => i64::from(value as u32),
            Atom::I64(value) => value,
            _ => return Ok(None),
        };
        match usize::try_from(value) {
            Ok(lane) if lane < lanes as usize => Ok(Some(lane)),
            _ => bail!("a vector element out of range"),
        }
    }

    /// The lanes of a vector, one element each, in a slot of the frame (a mask vector's lanes are a byte
    /// each here, not the bits that memory keeps), and the address of the element that the value
    /// `index` names there, kept inside the vector: an index past the end is poison, so any lane will do.
    fn spill_vector(&mut self, vec: &Val, index: &Val, lanes: u32, elem: Ty) -> Res<(Atom, Atom)> {
        let stride = self.types.size_of(elem)?;
        let align = self.types.align_of(elem)?;
        let slot = Atom::Frame(self.slot(stride * u64::from(lanes), align)?);
        let atoms = self.atoms(vec)?;
        let width = self.flat(elem)?.len();
        for lane in 0..lanes as usize {
            self.store_scalar(
                &slot,
                lane as u64 * stride,
                elem,
                &atoms[lane * width..(lane + 1) * width],
            )?;
        }
        let index = self.single(index)?;
        let last = lanes as i32 - 1;
        self.push(&index)?;
        if self.atom_vt(&index) == Vt::I64 {
            self.op(op::I32_WRAP_I64);
        }
        let position = self.save(Vt::I32);
        // `select(last, position, position > last)`: the position, or the last lane past the end.
        self.i32_const(last);
        self.push(&position)?;
        self.push(&position)?;
        self.i32_const(last);
        self.op(op::I32_GT_U);
        self.op(op::SELECT);
        let position = self.save(Vt::I32);
        self.push(&slot)?;
        self.push(&position)?;
        self.i32_const(stride as i32);
        self.op(op::I32_MUL);
        self.op(op::I32_ADD);
        let address = self.save(Vt::I32);
        Ok((slot, address))
    }

    pub(super) fn extract_element(&mut self, ty: Ty, vec: Val, index: Val) -> Res<Val> {
        let Some((lanes, elem)) = self.vector_shape(ty) else {
            bail!("an extractelement on {}", self.types.describe(ty));
        };
        let Some(lane) = self.lane_index(&index, lanes)? else {
            let (_, address) = self.spill_vector(&vec, &index, lanes, elem)?;
            return self.load_value(&address, 0, elem);
        };
        let width = self.flat(elem)?.len();
        let atoms = self.atoms(&vec)?;
        Ok(Val::Atoms(atoms[lane * width..(lane + 1) * width].to_vec()))
    }

    pub(super) fn insert_element(&mut self, ty: Ty, vec: Val, elem: Val, index: Val) -> Res<Val> {
        let Some((lanes, element)) = self.vector_shape(ty) else {
            bail!("an insertelement on {}", self.types.describe(ty));
        };
        let Some(lane) = self.lane_index(&index, lanes)? else {
            let (slot, address) = self.spill_vector(&vec, &index, lanes, element)?;
            self.store_value(&address, 0, element, &elem)?;
            let stride = self.types.size_of(element)?;
            let mut atoms = Vec::new();
            for lane in 0..u64::from(lanes) {
                atoms.extend(self.load_scalar(&slot, lane * stride, element)?);
            }
            return Ok(Val::Atoms(atoms));
        };
        let width = self.flat(element)?.len();
        let mut atoms = self.atoms(&vec)?;
        let new = self.atoms(&elem)?;
        atoms[lane * width..(lane + 1) * width].clone_from_slice(&new);
        Ok(Val::Atoms(atoms))
    }

    pub(super) fn shuffle(&mut self, ty: Ty, a: Val, b: Val, start: u32, len: u32) -> Res<Val> {
        let Some((lanes, elem)) = self.vector_shape(ty) else {
            bail!("a shufflevector on {}", self.types.describe(ty));
        };
        let width = self.flat(elem)?.len();
        let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
        let func = self.func;
        let zero = self.flat(elem)?;
        let mut out = Vec::new();
        for &mask in &func.masks[start as usize..(start + len) as usize] {
            if mask < 0 {
                out.extend(zero.iter().map(|&vt| Self::zero_atom(vt)));
                continue;
            }
            let lane = mask as usize;
            let (source, lane) = if lane < lanes as usize {
                (&a, lane)
            } else {
                (&b, lane - lanes as usize)
            };
            let Some(parts) = source.get(lane * width..(lane + 1) * width) else {
                bail!("a shuffle mask out of range");
            };
            out.extend_from_slice(parts);
        }
        Ok(Val::Atoms(out))
    }

    // ---- atomics ----

    pub(super) fn atomic_rmw(&mut self, rmw: RmwOp, ty: Ty, ptr: Val, value: Val) -> Res<Val> {
        let base = self.single(&ptr)?;
        let old = self.load_value(&base, 0, ty)?;
        let old_atoms = self.atoms(&old)?;
        let operand = self.atoms(&value)?;
        let new: Vec<Atom> = match rmw {
            RmwOp::Xchg => operand,
            RmwOp::Add => self.scalar_binary(BinOp::Add, ty, &old_atoms, &operand)?,
            RmwOp::Sub => self.scalar_binary(BinOp::Sub, ty, &old_atoms, &operand)?,
            RmwOp::And => self.scalar_binary(BinOp::And, ty, &old_atoms, &operand)?,
            RmwOp::Or => self.scalar_binary(BinOp::Or, ty, &old_atoms, &operand)?,
            RmwOp::Xor => self.scalar_binary(BinOp::Xor, ty, &old_atoms, &operand)?,
            RmwOp::Nand => {
                let both = self.scalar_binary(BinOp::And, ty, &old_atoms, &operand)?;
                let Ty::Int(bits) = ty else {
                    bail!("an atomic nand on {}", self.types.describe(ty));
                };
                let ones = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let mask = if bits <= 32 {
                    Atom::I32(ones as u32 as i32)
                } else {
                    Atom::I64(ones as i64)
                };
                self.scalar_binary(BinOp::Xor, ty, &both, &[mask])?
            }
            RmwOp::Max | RmwOp::Min | RmwOp::UMax | RmwOp::UMin => {
                let pred = match rmw {
                    RmwOp::Max => IPred::Sgt,
                    RmwOp::Min => IPred::Slt,
                    RmwOp::UMax => IPred::Ugt,
                    _ => IPred::Ult,
                };
                let wide = self.width(ty)?;
                if wide == super::ops::Width::W128 {
                    bail!("an atomic min or max on a 128-bit integer");
                }
                let condition = self.icmp(
                    pred,
                    ty,
                    Val::Atoms(old_atoms.clone()),
                    Val::Atoms(operand.clone()),
                )?;
                match self.select(
                    super::super::types::I1,
                    condition,
                    ty,
                    Val::Atoms(old_atoms.clone()),
                    Val::Atoms(operand),
                )? {
                    Val::Atoms(atoms) => atoms,
                    _ => bail!("an atomic min or max that is not a scalar"),
                }
            }
        };
        self.store_value(&base, 0, ty, &Val::Atoms(new))?;
        Ok(old)
    }

    pub(super) fn cmpxchg(&mut self, ty: Ty, ptr: Val, cmp: Val, new: Val) -> Res<Val> {
        let base = self.single(&ptr)?;
        let old = self.load_value(&base, 0, ty)?;
        let old_atoms = self.atoms(&old)?;
        let equal = self.icmp(IPred::Eq, ty, Val::Atoms(old_atoms.clone()), cmp)?;
        let success = self.single(&equal)?;
        self.push(&success)?;
        self.op(op::IF);
        self.code.push(0x40);
        self.store_value(&base, 0, ty, &new)?;
        self.op(op::END);
        let mut atoms = old_atoms;
        atoms.push(success);
        Ok(Val::Atoms(atoms))
    }
}
