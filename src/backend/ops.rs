//! Arithmetic, comparisons, conversions and `select`.
//!
//! An integer of fewer bits than its Cranelift type is kept zero-extended, so that an unsigned
//! operation sees the value itself; a signed operation extends from the sign bit first, and an
//! operation that can set the unused bits clears them again. A vector is its lanes, one by one.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{self, InstBuilder, MemFlags, Value, types};

use super::error::{Res, bail};
use super::func::{BinOp, CastOp, FPred, IPred};
use super::lower::{Lower, Val};
use super::types::{Compound, Ty};

impl<'w, 'a> Lower<'w, 'a> {
    /// The length and element type of a vector type.
    pub(super) fn vector_shape(&self, ty: Ty) -> Option<(u32, Ty)> {
        match self.types.shape(ty) {
            Some(Compound::Vector { len, elem }) => Some((*len, *elem)),
            _ => None,
        }
    }

    pub(super) fn vector_element(&self, ty: Ty) -> Res<Ty> {
        match self.vector_shape(ty) {
            Some((_, elem)) => Ok(elem),
            None => bail!("{} where a vector is expected", self.types.describe(ty)),
        }
    }

    fn value_type(&self, value: Value) -> ir::Type {
        self.b.func.dfg.value_type(value)
    }

    /// Clears the bits of an integer above its width, if the type keeps any.
    pub(super) fn canon(&mut self, value: Value, bits: u16) -> Value {
        let kind = self.value_type(value);
        if u32::from(bits) >= kind.bits() || kind == types::I128 {
            return value;
        }
        self.b.ins().band_imm(value, (1i64 << bits) - 1)
    }

    /// Extends an integer from its sign bit to the width of its type.
    pub(super) fn sext_in(&mut self, value: Value, bits: u16) -> Value {
        let kind = self.value_type(value);
        if u32::from(bits) >= kind.bits() {
            return value;
        }
        let shift = i64::from(kind.bits() - u32::from(bits));
        let up = self.b.ins().ishl_imm(value, shift);
        self.b.ins().sshr_imm(up, shift)
    }

    fn check_int(&self, ty: Ty) -> Res<u16> {
        match ty {
            Ty::Int(bits) if bits > 64 && bits < 128 => {
                bail!("an integer of {bits} bits")
            }
            Ty::Int(bits) => Ok(bits),
            other => bail!(
                "{} where an integer is expected",
                self.types.describe(other)
            ),
        }
    }

    // ---- binary operations ----

    pub(super) fn binary(&mut self, op: BinOp, ty: Ty, a: Val, b: Val) -> Res<Val> {
        if let Some((_, elem)) = self.vector_shape(ty) {
            let (la, lb) = (self.leaves(a)?, self.leaves(b)?);
            let mut out = Vec::with_capacity(la.len());
            for (&x, &y) in la.iter().zip(&lb) {
                out.push(self.scalar_binary(op, elem, x, y)?);
            }
            return Ok(self.multi(&out));
        }
        let (x, y) = (self.scalar(a)?, self.scalar(b)?);
        Ok(Val::S(self.scalar_binary(op, ty, x, y)?))
    }

    pub(super) fn scalar_binary(&mut self, op: BinOp, ty: Ty, x: Value, y: Value) -> Res<Value> {
        if matches!(ty, Ty::Float | Ty::Double) {
            return Ok(match op {
                BinOp::FAdd => self.b.ins().fadd(x, y),
                BinOp::FSub => self.b.ins().fsub(x, y),
                BinOp::FMul => self.b.ins().fmul(x, y),
                BinOp::FDiv => self.b.ins().fdiv(x, y),
                BinOp::FRem => self.frem(ty, x, y)?,
                _ => bail!("an integer operation on {}", self.types.describe(ty)),
            });
        }
        let bits = self.check_int(ty)?;
        Ok(match op {
            BinOp::Add => {
                let r = self.b.ins().iadd(x, y);
                self.canon(r, bits)
            }
            BinOp::Sub => {
                let r = self.b.ins().isub(x, y);
                self.canon(r, bits)
            }
            BinOp::Mul => {
                let r = self.b.ins().imul(x, y);
                self.canon(r, bits)
            }
            BinOp::And => self.b.ins().band(x, y),
            BinOp::Or => self.b.ins().bor(x, y),
            BinOp::Xor => self.b.ins().bxor(x, y),
            BinOp::Shl => {
                let r = self.b.ins().ishl(x, y);
                self.canon(r, bits)
            }
            BinOp::LShr => self.b.ins().ushr(x, y),
            BinOp::AShr => {
                let s = self.sext_in(x, bits);
                let r = self.b.ins().sshr(s, y);
                self.canon(r, bits)
            }
            BinOp::UDiv | BinOp::URem | BinOp::SDiv | BinOp::SRem if bits == 128 => {
                // No CPU divides 128-bit integers: the compiler runtime does, as for Clang.
                let name = match op {
                    BinOp::UDiv => "__udivti3",
                    BinOp::SDiv => "__divti3",
                    BinOp::URem => "__umodti3",
                    _ => "__modti3",
                };
                self.call_library(name, &[x, y])?
            }
            BinOp::UDiv => self.b.ins().udiv(x, y),
            BinOp::URem => self.b.ins().urem(x, y),
            BinOp::SDiv => {
                let (s, t) = (self.sext_in(x, bits), self.sext_in(y, bits));
                let r = self.b.ins().sdiv(s, t);
                self.canon(r, bits)
            }
            BinOp::SRem => {
                let (s, t) = (self.sext_in(x, bits), self.sext_in(y, bits));
                let r = self.b.ins().srem(s, t);
                self.canon(r, bits)
            }
            other => bail!("the floating-point operation {other:?} on an integer"),
        })
    }

    /// A call of a function that the lowering needs though the module does not name it.
    fn call_library(&mut self, name: &str, arguments: &[Value]) -> Res<Value> {
        let Some(&id) = self.module.by_name.get(name) else {
            bail!("a call of {name}, which the module does not provide");
        };
        let function = self.func_ref(id, true)?;
        let call = self.b.ins().call(function, arguments);
        Ok(self.b.inst_results(call)[0])
    }

    /// `frem` is the C library's `fmod`, which is also what Clang calls at `-O0`.
    fn frem(&mut self, ty: Ty, x: Value, y: Value) -> Res<Value> {
        let name = if ty == Ty::Float { "fmodf" } else { "fmod" };
        self.call_library(name, &[x, y])
    }

    pub(super) fn fneg(&mut self, ty: Ty, a: Val) -> Res<Val> {
        if self.vector_shape(ty).is_some() {
            let lanes = self.leaves(a)?;
            let out: Vec<Value> = lanes.iter().map(|&x| self.b.ins().fneg(x)).collect();
            return Ok(self.multi(&out));
        }
        let x = self.scalar(a)?;
        Ok(Val::S(self.b.ins().fneg(x)))
    }

    // ---- comparisons ----

    pub(super) fn icmp(&mut self, pred: IPred, ty: Ty, a: Val, b: Val) -> Res<Val> {
        if let Some((_, elem)) = self.vector_shape(ty) {
            let (la, lb) = (self.leaves(a)?, self.leaves(b)?);
            let mut out = Vec::with_capacity(la.len());
            for (&x, &y) in la.iter().zip(&lb) {
                out.push(self.scalar_icmp(pred, elem, x, y)?);
            }
            return Ok(self.multi(&out));
        }
        let (x, y) = (self.scalar(a)?, self.scalar(b)?);
        Ok(Val::S(self.scalar_icmp(pred, ty, x, y)?))
    }

    fn scalar_icmp(&mut self, pred: IPred, ty: Ty, x: Value, y: Value) -> Res<Value> {
        let (cc, signed) = match pred {
            IPred::Eq => (IntCC::Equal, false),
            IPred::Ne => (IntCC::NotEqual, false),
            IPred::Ugt => (IntCC::UnsignedGreaterThan, false),
            IPred::Uge => (IntCC::UnsignedGreaterThanOrEqual, false),
            IPred::Ult => (IntCC::UnsignedLessThan, false),
            IPred::Ule => (IntCC::UnsignedLessThanOrEqual, false),
            IPred::Sgt => (IntCC::SignedGreaterThan, true),
            IPred::Sge => (IntCC::SignedGreaterThanOrEqual, true),
            IPred::Slt => (IntCC::SignedLessThan, true),
            IPred::Sle => (IntCC::SignedLessThanOrEqual, true),
        };
        let (x, y) = match ty {
            Ty::Int(bits) if signed => {
                self.check_int(ty)?;
                (self.sext_in(x, bits), self.sext_in(y, bits))
            }
            Ty::Int(_) => {
                self.check_int(ty)?;
                (x, y)
            }
            Ty::Ptr => (x, y),
            other => bail!("an integer comparison of {}", self.types.describe(other)),
        };
        Ok(self.b.ins().icmp(cc, x, y))
    }

    pub(super) fn fcmp(&mut self, pred: FPred, ty: Ty, a: Val, b: Val) -> Res<Val> {
        if let Some((_, elem)) = self.vector_shape(ty) {
            let (la, lb) = (self.leaves(a)?, self.leaves(b)?);
            let mut out = Vec::with_capacity(la.len());
            for (&x, &y) in la.iter().zip(&lb) {
                out.push(self.scalar_fcmp(pred, elem, x, y)?);
            }
            return Ok(self.multi(&out));
        }
        let (x, y) = (self.scalar(a)?, self.scalar(b)?);
        Ok(Val::S(self.scalar_fcmp(pred, ty, x, y)?))
    }

    fn scalar_fcmp(&mut self, pred: FPred, ty: Ty, x: Value, y: Value) -> Res<Value> {
        if !matches!(ty, Ty::Float | Ty::Double) {
            bail!("a floating-point comparison of {}", self.types.describe(ty));
        }
        // AArch64 has no condition for `one`, `ueq` and the unordered-or forms of `<` and `>`
        // (Cranelift stops with `unimplemented`), so they are built from the plain ones, on every
        // target alike: `ult` is "not `oge`", `ueq` is "unordered or `oeq`", and so on.
        let plain = |this: &mut Self, cc: FloatCC| this.b.ins().fcmp(cc, x, y);
        Ok(match pred {
            FPred::False => self.b.ins().iconst(types::I8, 0),
            FPred::True => self.b.ins().iconst(types::I8, 1),
            FPred::Oeq => plain(self, FloatCC::Equal),
            FPred::Ogt => plain(self, FloatCC::GreaterThan),
            FPred::Oge => plain(self, FloatCC::GreaterThanOrEqual),
            FPred::Olt => plain(self, FloatCC::LessThan),
            FPred::Ole => plain(self, FloatCC::LessThanOrEqual),
            FPred::Ord => plain(self, FloatCC::Ordered),
            FPred::Une => plain(self, FloatCC::NotEqual),
            FPred::Uno => plain(self, FloatCC::Unordered),
            FPred::One => {
                let (less, greater) = (
                    plain(self, FloatCC::LessThan),
                    plain(self, FloatCC::GreaterThan),
                );
                self.b.ins().bor(less, greater)
            }
            FPred::Ueq => {
                let (unordered, equal) =
                    (plain(self, FloatCC::Unordered), plain(self, FloatCC::Equal));
                self.b.ins().bor(unordered, equal)
            }
            FPred::Ugt => {
                let not = plain(self, FloatCC::LessThanOrEqual);
                self.b.ins().bxor_imm(not, 1)
            }
            FPred::Uge => {
                let not = plain(self, FloatCC::LessThan);
                self.b.ins().bxor_imm(not, 1)
            }
            FPred::Ult => {
                let not = plain(self, FloatCC::GreaterThanOrEqual);
                self.b.ins().bxor_imm(not, 1)
            }
            FPred::Ule => {
                let not = plain(self, FloatCC::GreaterThan);
                self.b.ins().bxor_imm(not, 1)
            }
        })
    }

    // ---- select ----

    pub(super) fn select(&mut self, cond_ty: Ty, cond: Val, ty: Ty, a: Val, b: Val) -> Res<Val> {
        if self.vector_shape(cond_ty).is_some() {
            let (lc, la, lb) = (self.leaves(cond)?, self.leaves(a)?, self.leaves(b)?);
            let mut out = Vec::with_capacity(la.len());
            for index in 0..la.len() {
                out.push(self.b.ins().select(lc[index], la[index], lb[index]));
            }
            return Ok(self.multi(&out));
        }
        let c = self.scalar(cond)?;
        match ty {
            Ty::Compound(_) if self.is_large(ty)? => {
                let (x, y) = (self.scalar(a)?, self.scalar(b)?);
                Ok(Val::Ref(self.b.ins().select(c, x, y)))
            }
            Ty::Compound(_) => {
                let (la, lb) = (self.leaves(a)?, self.leaves(b)?);
                let out: Vec<Value> = la
                    .iter()
                    .zip(&lb)
                    .map(|(&x, &y)| self.b.ins().select(c, x, y))
                    .collect();
                Ok(self.multi(&out))
            }
            _ => {
                let (x, y) = (self.scalar(a)?, self.scalar(b)?);
                Ok(Val::S(self.b.ins().select(c, x, y)))
            }
        }
    }

    // ---- conversions ----

    pub(super) fn cast(&mut self, op: CastOp, from: Ty, to: Ty, a: Val) -> Res<Val> {
        if op == CastOp::BitCast {
            return self.bitcast(from, to, a);
        }
        if let (Some((_, from_elem)), Some((_, to_elem))) =
            (self.vector_shape(from), self.vector_shape(to))
        {
            let lanes = self.leaves(a)?;
            let mut out = Vec::with_capacity(lanes.len());
            for &x in &lanes {
                out.push(self.scalar_cast(op, from_elem, to_elem, x)?);
            }
            return Ok(self.multi(&out));
        }
        let x = self.scalar(a)?;
        Ok(Val::S(self.scalar_cast(op, from, to, x)?))
    }

    fn scalar_cast(&mut self, op: CastOp, from: Ty, to: Ty, x: Value) -> Res<Value> {
        let pointer = self.target.pointer;
        Ok(match op {
            CastOp::Trunc => {
                let (Ty::Int(_), Ty::Int(to_bits)) = (from, to) else {
                    bail!(
                        "a trunc from {} to {}",
                        self.types.describe(from),
                        self.types.describe(to)
                    );
                };
                self.check_int(to)?;
                let kind = self.clif_type(to)?;
                let narrowed = if self.value_type(x) == kind {
                    x
                } else {
                    self.b.ins().ireduce(kind, x)
                };
                self.canon(narrowed, to_bits)
            }
            CastOp::ZExt => {
                let kind = self.clif_type(to)?;
                self.check_int(from)?;
                self.check_int(to)?;
                if self.value_type(x) == kind {
                    x
                } else {
                    self.b.ins().uextend(kind, x)
                }
            }
            CastOp::SExt => {
                let (Ty::Int(from_bits), Ty::Int(to_bits)) = (from, to) else {
                    bail!(
                        "a sext from {} to {}",
                        self.types.describe(from),
                        self.types.describe(to)
                    );
                };
                self.check_int(from)?;
                self.check_int(to)?;
                let kind = self.clif_type(to)?;
                let signed = self.sext_in(x, from_bits);
                let wide = if self.value_type(signed) == kind {
                    signed
                } else {
                    self.b.ins().sextend(kind, signed)
                };
                self.canon(wide, to_bits)
            }
            CastOp::FpTrunc => {
                if (from, to) != (Ty::Double, Ty::Float) {
                    bail!(
                        "an fptrunc from {} to {}",
                        self.types.describe(from),
                        self.types.describe(to)
                    );
                }
                self.b.ins().fdemote(types::F32, x)
            }
            CastOp::FpExt => {
                if (from, to) != (Ty::Float, Ty::Double) {
                    bail!(
                        "an fpext from {} to {}",
                        self.types.describe(from),
                        self.types.describe(to)
                    );
                }
                self.b.ins().fpromote(types::F64, x)
            }
            CastOp::FpToSi | CastOp::FpToUi => {
                let Ty::Int(to_bits) = to else {
                    bail!("a float conversion to {}", self.types.describe(to));
                };
                self.check_int(to)?;
                let kind = self.clif_type(to)?;
                if kind == types::I128 {
                    bail!("a float converted to a 128-bit integer");
                }
                // Out-of-range results are poison in LLVM; saturating never traps.
                let wide = if kind.bits() < 32 { types::I32 } else { kind };
                let converted = if op == CastOp::FpToSi {
                    self.b.ins().fcvt_to_sint_sat(wide, x)
                } else {
                    self.b.ins().fcvt_to_uint_sat(wide, x)
                };
                let narrowed = if wide == kind {
                    converted
                } else {
                    self.b.ins().ireduce(kind, converted)
                };
                self.canon(narrowed, to_bits)
            }
            CastOp::SiToFp | CastOp::UiToFp => {
                let Ty::Int(bits) = from else {
                    bail!("an integer conversion from {}", self.types.describe(from));
                };
                self.check_int(from)?;
                let target = self.clif_type(to)?;
                if self.value_type(x) == types::I128 {
                    bail!("a 128-bit integer converted to a float");
                }
                let signed = op == CastOp::SiToFp;
                let value = if signed { self.sext_in(x, bits) } else { x };
                let value = if self.value_type(value).bits() < 32 {
                    if signed {
                        self.b.ins().sextend(types::I32, value)
                    } else {
                        self.b.ins().uextend(types::I32, value)
                    }
                } else {
                    value
                };
                if signed {
                    self.b.ins().fcvt_from_sint(target, value)
                } else {
                    self.b.ins().fcvt_from_uint(target, value)
                }
            }
            CastOp::PtrToInt => {
                let kind = self.clif_type(to)?;
                self.check_int(to)?;
                let Ty::Int(to_bits) = to else {
                    unreachable!("checked as an integer")
                };
                if kind == pointer {
                    x
                } else if kind == types::I128 {
                    self.b.ins().uextend(kind, x)
                } else {
                    let narrowed = self.b.ins().ireduce(kind, x);
                    self.canon(narrowed, to_bits)
                }
            }
            CastOp::IntToPtr => {
                self.check_int(from)?;
                let kind = self.value_type(x);
                if kind == pointer {
                    x
                } else if kind == types::I128 {
                    self.b.ins().ireduce(pointer, x)
                } else {
                    self.b.ins().uextend(pointer, x)
                }
            }
            CastOp::BitCast => unreachable!("handled before the lanes"),
        })
    }

    fn bitcast(&mut self, from: Ty, to: Ty, a: Val) -> Res<Val> {
        if from == to {
            return Ok(a);
        }
        match (from, to) {
            (Ty::Ptr, Ty::Ptr) => return Ok(a),
            (Ty::Int(_) | Ty::Float | Ty::Double, Ty::Int(_) | Ty::Float | Ty::Double) => {
                let kind = self.clif_type(to)?;
                let x = self.scalar(a)?;
                if self.value_type(x) == kind {
                    return Ok(Val::S(x));
                }
                if self.value_type(x).bits() != kind.bits() {
                    bail!("a bitcast between types of different sizes");
                }
                return Ok(Val::S(self.b.ins().bitcast(kind, MemFlags::new(), x)));
            }
            _ => {}
        }
        // A vector of booleans and the integer of its bits.
        if let (Some((len, Ty::Int(1))), Ty::Int(bits)) = (self.vector_shape(from), to)
            && u32::from(bits) == len
            && bits <= 64
        {
            let lanes = self.leaves(a)?;
            let kind = self.clif_type(to)?;
            let mut result = self.b.ins().iconst(kind, 0);
            for (index, &lane) in lanes.iter().enumerate() {
                let wide = if kind == types::I8 {
                    lane
                } else {
                    self.b.ins().uextend(kind, lane)
                };
                let shifted = self.b.ins().ishl_imm(wide, index as i64);
                result = self.b.ins().bor(result, shifted);
            }
            return Ok(Val::S(result));
        }
        if let (Ty::Int(bits), Some((len, Ty::Int(1)))) = (from, self.vector_shape(to))
            && u32::from(bits) == len
            && bits <= 64
        {
            let x = self.scalar(a)?;
            let mut out = Vec::with_capacity(len as usize);
            for index in 0..len {
                let shifted = self.b.ins().ushr_imm(x, i64::from(index));
                let bit = self.b.ins().band_imm(shifted, 1);
                out.push(if self.value_type(bit) == types::I8 {
                    bit
                } else {
                    self.b.ins().ireduce(types::I8, bit)
                });
            }
            return Ok(self.multi(&out));
        }
        // Anything else of the same size: write it as one type and read it as the other.
        let size = self.types.store_size_of(from)?;
        if size != self.types.store_size_of(to)? {
            bail!(
                "a bitcast from {} to {}",
                self.types.describe(from),
                self.types.describe(to)
            );
        }
        let slot = self.new_slot(size, 16)?;
        let place = super::lower::Place::Slot(slot, 0);
        self.store_value(place, from, a)?;
        self.load_value(place, to)
    }
}
