//! Arithmetic, comparisons, conversions and `select`.
//!
//! An integer of fewer bits than its WebAssembly type is kept zero-extended, so that an unsigned
//! operation sees the value itself; a signed operation extends from the sign bit first, and an
//! operation that can set the unused bits clears them again. A 128-bit integer is two `i64` values,
//! low word first; what no instruction does for it is a call of the compiler runtime of the module.
//! A vector is its lanes, one by one.

use super::super::error::{Res, bail};
use super::super::func::{BinOp, CastOp, FPred, IPred};
use super::super::types::Ty;
use super::bin::{Vt, op};
use super::lower::{Atom, Lower, Val};

/// The WebAssembly type that holds an integer type, and its width in bits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Width {
    W32(u16),
    W64(u16),
    W128,
}

impl Width {
    pub(super) fn vt(self) -> Vt {
        match self {
            Width::W32(_) => Vt::I32,
            _ => Vt::I64,
        }
    }
}

fn width_of(ty: Ty) -> Res<Width> {
    Ok(match ty {
        Ty::Int(bits @ 1..=32) => Width::W32(bits),
        Ty::Int(bits @ 33..=64) => Width::W64(bits),
        Ty::Int(128) => Width::W128,
        Ty::Ptr => Width::W32(32),
        Ty::Int(bits) => bail!("an integer of {bits} bits"),
        other => bail!("{other:?} where an integer is expected"),
    })
}

impl<'w, 'a> Lower<'w, 'a> {
    pub(super) fn width(&self, ty: Ty) -> Res<Width> {
        width_of(ty)
    }

    /// Clears the bits of the integer on the stack above its width.
    pub(super) fn canon(&mut self, width: Width) {
        match width {
            Width::W32(bits) if bits < 32 => {
                self.i32_const(((1u64 << bits) - 1) as u32 as i32);
                self.op(op::I32_AND);
            }
            Width::W64(bits) if bits < 64 => {
                self.i64_const(((1u64 << bits) - 1) as i64);
                self.op(op::I64_AND);
            }
            _ => {}
        }
    }

    /// Extends the `i32` on the stack from its sign bit at `bits`.
    pub(super) fn sext32(&mut self, bits: u16) {
        match bits {
            32.. => {}
            8 => self.op(0xc0),
            16 => self.op(0xc1),
            _ => {
                let shift = i32::from(32 - bits);
                self.i32_const(shift);
                self.op(op::I32_SHL);
                self.i32_const(shift);
                self.op(op::I32_SHR_S);
            }
        }
    }

    /// Extends the `i64` on the stack from its sign bit at `bits`.
    pub(super) fn sext64(&mut self, bits: u16) {
        if bits < 64 {
            let shift = i64::from(64 - bits);
            self.i64_const(shift);
            self.op(op::I64_SHL);
            self.i64_const(shift);
            self.op(op::I64_SHR_S);
        }
    }

    /// Extends the integer on the stack from its sign bit at the width.
    pub(super) fn sext(&mut self, width: Width) {
        match width {
            Width::W32(bits) => self.sext32(bits),
            Width::W64(bits) => self.sext64(bits),
            Width::W128 => {}
        }
    }

    /// The number of atoms of a lane of a vector type.
    fn lane_atoms(&self, elem: Ty) -> Res<usize> {
        Ok(self.flat(elem)?.len())
    }

    /// Applies `scalar` to each lane of vectors, or once to scalars.
    pub(super) fn lanes<F>(&mut self, ty: Ty, values: &[Vec<Atom>], mut scalar: F) -> Res<Vec<Atom>>
    where
        F: FnMut(&mut Self, Ty, &[Vec<Atom>]) -> Res<Vec<Atom>>,
    {
        let Some((len, elem)) = self.vector_shape(ty) else {
            return scalar(self, ty, values);
        };
        let width = self.lane_atoms(elem)?;
        let mut out = Vec::new();
        for lane in 0..len as usize {
            let parts: Vec<Vec<Atom>> = values
                .iter()
                .map(|atoms| atoms[lane * width..(lane + 1) * width].to_vec())
                .collect();
            out.extend(scalar(self, elem, &parts)?);
        }
        Ok(out)
    }

    // ---- binary operations ----

    pub(super) fn binary(&mut self, bop: BinOp, ty: Ty, a: Val, b: Val) -> Res<Val> {
        let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
        let atoms = self.lanes(ty, &[a, b], |lower, ty, parts| {
            lower.scalar_binary(bop, ty, &parts[0], &parts[1])
        })?;
        Ok(Val::Atoms(atoms))
    }

    pub(super) fn scalar_binary(
        &mut self,
        bop: BinOp,
        ty: Ty,
        a: &[Atom],
        b: &[Atom],
    ) -> Res<Vec<Atom>> {
        if matches!(ty, Ty::Float | Ty::Double) {
            let f32 = ty == Ty::Float;
            let opcode = match bop {
                BinOp::FAdd => Some(if f32 { op::F32_ADD } else { op::F64_ADD }),
                BinOp::FSub => Some(if f32 { op::F32_SUB } else { op::F64_SUB }),
                BinOp::FMul => Some(if f32 { op::F32_MUL } else { op::F64_MUL }),
                BinOp::FDiv => Some(if f32 { op::F32_DIV } else { op::F64_DIV }),
                BinOp::FRem => None,
                _ => bail!("an integer operation on {}", self.types.describe(ty)),
            };
            let Some(opcode) = opcode else {
                // `frem` is the C library's `fmod`, which is also what Clang calls at `-O0`.
                let name = if f32 { "fmodf" } else { "fmod" };
                let arguments = vec![(ty, Val::Atoms(a.to_vec())), (ty, Val::Atoms(b.to_vec()))];
                return match self.call_library(name, arguments, ty)? {
                    Val::Atoms(atoms) => Ok(atoms),
                    _ => bail!("{name} does not return a scalar"),
                };
            };
            self.push_all(a)?;
            self.push_all(b)?;
            self.op(opcode);
            return Ok(vec![self.save(if f32 { Vt::F32 } else { Vt::F64 })]);
        }
        let width = self.width(ty)?;
        if width == Width::W128 {
            return self.binary128(bop, a, b);
        }
        let wide = width.vt() == Vt::I64;
        let pick = |narrow: u8, wide_op: u8| if wide { wide_op } else { narrow };
        self.push_all(a)?;
        match bop {
            BinOp::Add => {
                self.push_all(b)?;
                self.op(pick(op::I32_ADD, op::I64_ADD));
                self.canon(width);
            }
            BinOp::Sub => {
                self.push_all(b)?;
                self.op(pick(op::I32_SUB, op::I64_SUB));
                self.canon(width);
            }
            BinOp::Mul => {
                self.push_all(b)?;
                self.op(pick(op::I32_MUL, op::I64_MUL));
                self.canon(width);
            }
            BinOp::And => {
                self.push_all(b)?;
                self.op(pick(op::I32_AND, op::I64_AND));
            }
            BinOp::Or => {
                self.push_all(b)?;
                self.op(pick(op::I32_OR, op::I64_OR));
            }
            BinOp::Xor => {
                self.push_all(b)?;
                self.op(pick(op::I32_XOR, op::I64_XOR));
            }
            BinOp::Shl => {
                self.push_all(b)?;
                self.op(pick(op::I32_SHL, op::I64_SHL));
                self.canon(width);
            }
            BinOp::LShr => {
                self.push_all(b)?;
                self.op(pick(op::I32_SHR_U, op::I64_SHR_U));
            }
            BinOp::AShr => {
                self.sext(width);
                self.push_all(b)?;
                self.op(pick(op::I32_SHR_S, op::I64_SHR_S));
                self.canon(width);
            }
            BinOp::UDiv => {
                self.push_all(b)?;
                self.op(pick(op::I32_DIV_U, op::I64_DIV_U));
            }
            BinOp::URem => {
                self.push_all(b)?;
                self.op(pick(op::I32_REM_U, op::I64_REM_U));
            }
            BinOp::SDiv => {
                self.sext(width);
                self.push_all(b)?;
                self.sext(width);
                self.op(pick(op::I32_DIV_S, op::I64_DIV_S));
                self.canon(width);
            }
            BinOp::SRem => {
                self.sext(width);
                self.push_all(b)?;
                self.sext(width);
                self.op(pick(op::I32_REM_S, op::I64_REM_S));
                self.canon(width);
            }
            other => bail!("the floating-point operation {other:?} on an integer"),
        }
        Ok(vec![self.save(width.vt())])
    }

    /// A 128-bit integer operation: the two words of each operand, low first.
    pub(super) fn binary128(&mut self, bop: BinOp, a: &[Atom], b: &[Atom]) -> Res<Vec<Atom>> {
        let (Some(a_lo), Some(a_hi), Some(b_lo), Some(b_hi)) =
            (a.first(), a.get(1), b.first(), b.get(1))
        else {
            bail!("a 128-bit operand that is not two words");
        };
        match bop {
            BinOp::And | BinOp::Or | BinOp::Xor => {
                let opcode = match bop {
                    BinOp::And => op::I64_AND,
                    BinOp::Or => op::I64_OR,
                    _ => op::I64_XOR,
                };
                self.push(a_lo)?;
                self.push(b_lo)?;
                self.op(opcode);
                let lo = self.save(Vt::I64);
                self.push(a_hi)?;
                self.push(b_hi)?;
                self.op(opcode);
                let hi = self.save(Vt::I64);
                Ok(vec![lo, hi])
            }
            BinOp::Add => {
                self.push(a_lo)?;
                self.push(b_lo)?;
                self.op(op::I64_ADD);
                let lo = self.save(Vt::I64);
                self.push(a_hi)?;
                self.push(b_hi)?;
                self.op(op::I64_ADD);
                // The carry out of the low word.
                self.push(&lo)?;
                self.push(a_lo)?;
                self.op(op::I64_LT_U);
                self.op(op::I64_EXTEND_I32_U);
                self.op(op::I64_ADD);
                let hi = self.save(Vt::I64);
                Ok(vec![lo, hi])
            }
            BinOp::Sub => {
                self.push(a_lo)?;
                self.push(b_lo)?;
                self.op(op::I64_SUB);
                let lo = self.save(Vt::I64);
                self.push(a_hi)?;
                self.push(b_hi)?;
                self.op(op::I64_SUB);
                // The borrow from the high word.
                self.push(a_lo)?;
                self.push(b_lo)?;
                self.op(op::I64_LT_U);
                self.op(op::I64_EXTEND_I32_U);
                self.op(op::I64_SUB);
                let hi = self.save(Vt::I64);
                Ok(vec![lo, hi])
            }
            BinOp::Shl | BinOp::LShr | BinOp::AShr => {
                // A constant count is a few word operations. The routines of the runtime that a
                // variable count calls are written with such shifts, so this is also what ends their recursion.
                if let (Atom::I64(count), Atom::I64(0)) = (b_lo, b_hi) {
                    return self.shift128_constant(bop, a_lo, a_hi, (*count & 127) as u32);
                }
                let name = match bop {
                    BinOp::Shl => "__ashlti3",
                    BinOp::LShr => "__lshrti3",
                    _ => "__ashrti3",
                };
                self.push(b_lo)?;
                self.op(op::I32_WRAP_I64);
                let count = self.save(Vt::I32);
                let arguments = vec![
                    (Ty::Int(128), Val::Atoms(a.to_vec())),
                    (Ty::Int(32), Val::Atoms(vec![count])),
                ];
                self.library_scalar(name, arguments, Ty::Int(128))
            }
            BinOp::Mul | BinOp::UDiv | BinOp::SDiv | BinOp::URem | BinOp::SRem => {
                let name = match bop {
                    BinOp::Mul => "__multi3",
                    BinOp::UDiv => "__udivti3",
                    BinOp::SDiv => "__divti3",
                    BinOp::URem => "__umodti3",
                    _ => "__modti3",
                };
                let arguments = vec![
                    (Ty::Int(128), Val::Atoms(a.to_vec())),
                    (Ty::Int(128), Val::Atoms(b.to_vec())),
                ];
                self.library_scalar(name, arguments, Ty::Int(128))
            }
            other => bail!("the operation {other:?} on a 128-bit integer"),
        }
    }

    /// `x << n`, `x >> n` of the 128-bit integer (`lo`, `hi`) by the constant `n` below 128.
    fn shift128_constant(&mut self, bop: BinOp, lo: &Atom, hi: &Atom, n: u32) -> Res<Vec<Atom>> {
        if n == 0 {
            return Ok(vec![lo.clone(), hi.clone()]);
        }
        let arithmetic = bop == BinOp::AShr;
        let shr = if arithmetic {
            op::I64_SHR_S
        } else {
            op::I64_SHR_U
        };
        // `value op by` of one word, saved.
        let word = |lower: &mut Self, value: &Atom, opcode: u8, by: u32| -> Res<Atom> {
            lower.push(value)?;
            lower.i64_const(i64::from(by));
            lower.op(opcode);
            Ok(lower.save(Vt::I64))
        };
        Ok(match (bop, n) {
            (BinOp::Shl, 1..=63) => {
                let carried = word(self, lo, op::I64_SHR_U, 64 - n)?;
                let high = word(self, hi, op::I64_SHL, n)?;
                self.push(&high)?;
                self.push(&carried)?;
                self.op(op::I64_OR);
                let high = self.save(Vt::I64);
                vec![word(self, lo, op::I64_SHL, n)?, high]
            }
            (BinOp::Shl, 64) => vec![Atom::I64(0), lo.clone()],
            (BinOp::Shl, _) => vec![Atom::I64(0), word(self, lo, op::I64_SHL, n - 64)?],
            (_, 1..=63) => {
                let carried = word(self, hi, op::I64_SHL, 64 - n)?;
                let low = word(self, lo, op::I64_SHR_U, n)?;
                self.push(&low)?;
                self.push(&carried)?;
                self.op(op::I64_OR);
                let low = self.save(Vt::I64);
                vec![low, word(self, hi, shr, n)?]
            }
            (_, 64) => vec![
                hi.clone(),
                if arithmetic {
                    word(self, hi, shr, 63)?
                } else {
                    Atom::I64(0)
                },
            ],
            (_, _) => vec![
                word(self, hi, shr, n - 64)?,
                if arithmetic {
                    word(self, hi, shr, 63)?
                } else {
                    Atom::I64(0)
                },
            ],
        })
    }

    fn library_scalar(&mut self, name: &str, arguments: Vec<(Ty, Val)>, ret: Ty) -> Res<Vec<Atom>> {
        match self.call_library(name, arguments, ret)? {
            Val::Atoms(atoms) => Ok(atoms),
            _ => bail!("{name} does not return a scalar"),
        }
    }

    pub(super) fn fneg(&mut self, ty: Ty, a: Val) -> Res<Val> {
        let a = self.atoms(&a)?;
        let atoms = self.lanes(ty, &[a], |lower, ty, parts| {
            lower.push_all(&parts[0])?;
            lower.op(if ty == Ty::Float {
                op::F32_NEG
            } else {
                op::F64_NEG
            });
            Ok(vec![lower.save(if ty == Ty::Float {
                Vt::F32
            } else {
                Vt::F64
            })])
        })?;
        Ok(Val::Atoms(atoms))
    }

    // ---- comparisons ----

    pub(super) fn icmp(&mut self, pred: IPred, ty: Ty, a: Val, b: Val) -> Res<Val> {
        let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
        let atoms = self.lanes(ty, &[a, b], |lower, ty, parts| {
            lower.scalar_icmp(pred, ty, &parts[0], &parts[1])
        })?;
        Ok(Val::Atoms(atoms))
    }

    fn scalar_icmp(&mut self, pred: IPred, ty: Ty, a: &[Atom], b: &[Atom]) -> Res<Vec<Atom>> {
        let width = self.width(ty)?;
        let signed = matches!(pred, IPred::Sgt | IPred::Sge | IPred::Slt | IPred::Sle);
        if width == Width::W128 {
            return self.icmp128(pred, a, b);
        }
        self.push_all(a)?;
        if signed {
            self.sext(width);
        }
        self.push_all(b)?;
        if signed {
            self.sext(width);
        }
        let base = if width.vt() == Vt::I64 {
            op::I64_EQ
        } else {
            op::I32_EQ
        };
        // The comparisons come in the order eq, ne, lt_s, lt_u, gt_s, gt_u, le_s, le_u, ge_s, ge_u.
        let offset = match pred {
            IPred::Eq => 0,
            IPred::Ne => 1,
            IPred::Slt => 2,
            IPred::Ult => 3,
            IPred::Sgt => 4,
            IPred::Ugt => 5,
            IPred::Sle => 6,
            IPred::Ule => 7,
            IPred::Sge => 8,
            IPred::Uge => 9,
        };
        self.op(base + offset);
        Ok(vec![self.save(Vt::I32)])
    }

    fn icmp128(&mut self, pred: IPred, a: &[Atom], b: &[Atom]) -> Res<Vec<Atom>> {
        let (Some(a_lo), Some(a_hi), Some(b_lo), Some(b_hi)) =
            (a.first(), a.get(1), b.first(), b.get(1))
        else {
            bail!("a 128-bit operand that is not two words");
        };
        match pred {
            IPred::Eq | IPred::Ne => {
                self.push(a_lo)?;
                self.push(b_lo)?;
                self.op(op::I64_XOR);
                self.push(a_hi)?;
                self.push(b_hi)?;
                self.op(op::I64_XOR);
                self.op(op::I64_OR);
                self.op(op::I64_EQZ);
                if pred == IPred::Ne {
                    self.op(op::I32_EQZ);
                }
            }
            _ => {
                // `less` of (x, y); the other orders are its mirror or its complement.
                let (signed, swap, negate) = match pred {
                    IPred::Ult => (false, false, false),
                    IPred::Slt => (true, false, false),
                    IPred::Ugt => (false, true, false),
                    IPred::Sgt => (true, true, false),
                    IPred::Uge => (false, false, true),
                    IPred::Sge => (true, false, true),
                    IPred::Ule => (false, true, true),
                    _ => (true, true, true),
                };
                let ((x_lo, x_hi), (y_lo, y_hi)) = if swap {
                    ((b_lo, b_hi), (a_lo, a_hi))
                } else {
                    ((a_lo, a_hi), (b_lo, b_hi))
                };
                self.push(x_hi)?;
                self.push(y_hi)?;
                self.op(if signed { op::I64_LT_S } else { op::I64_LT_U });
                self.push(x_hi)?;
                self.push(y_hi)?;
                self.op(op::I64_EQ);
                self.push(x_lo)?;
                self.push(y_lo)?;
                self.op(op::I64_LT_U);
                self.op(op::I32_AND);
                self.op(op::I32_OR);
                if negate {
                    self.op(op::I32_EQZ);
                }
            }
        }
        Ok(vec![self.save(Vt::I32)])
    }

    pub(super) fn fcmp(&mut self, pred: FPred, ty: Ty, a: Val, b: Val) -> Res<Val> {
        let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
        let atoms = self.lanes(ty, &[a, b], |lower, ty, parts| {
            lower.scalar_fcmp(pred, ty, &parts[0], &parts[1])
        })?;
        Ok(Val::Atoms(atoms))
    }

    fn scalar_fcmp(&mut self, pred: FPred, ty: Ty, a: &[Atom], b: &[Atom]) -> Res<Vec<Atom>> {
        let f32 = match ty {
            Ty::Float => true,
            Ty::Double => false,
            other => bail!(
                "a floating-point comparison of {}",
                self.types.describe(other)
            ),
        };
        let [x] = a else {
            bail!("a floating-point comparison of more than one scalar");
        };
        let [y] = b else {
            bail!("a floating-point comparison of more than one scalar");
        };
        let (eq, ne, lt, gt, le, ge) = if f32 {
            (
                op::F32_EQ,
                op::F32_NE,
                op::F32_LT,
                op::F32_GT,
                op::F32_LE,
                op::F32_GE,
            )
        } else {
            (
                op::F64_EQ,
                op::F64_NE,
                op::F64_LT,
                op::F64_GT,
                op::F64_LE,
                op::F64_GE,
            )
        };
        let compare = |lower: &mut Self, opcode: u8| -> Res<()> {
            lower.push(x)?;
            lower.push(y)?;
            lower.op(opcode);
            Ok(())
        };
        match pred {
            FPred::False => return Ok(vec![Atom::I32(0)]),
            FPred::True => return Ok(vec![Atom::I32(1)]),
            FPred::Oeq => compare(self, eq)?,
            FPred::Ogt => compare(self, gt)?,
            FPred::Oge => compare(self, ge)?,
            FPred::Olt => compare(self, lt)?,
            FPred::Ole => compare(self, le)?,
            FPred::One | FPred::Ueq => {
                compare(self, lt)?;
                compare(self, gt)?;
                self.op(op::I32_OR);
                if pred == FPred::Ueq {
                    self.op(op::I32_EQZ);
                }
            }
            FPred::Ord => {
                self.push(x)?;
                self.push(x)?;
                self.op(eq);
                self.push(y)?;
                self.push(y)?;
                self.op(eq);
                self.op(op::I32_AND);
            }
            FPred::Uno => {
                self.push(x)?;
                self.push(x)?;
                self.op(ne);
                self.push(y)?;
                self.push(y)?;
                self.op(ne);
                self.op(op::I32_OR);
            }
            FPred::Ugt => {
                compare(self, le)?;
                self.op(op::I32_EQZ);
            }
            FPred::Uge => {
                compare(self, lt)?;
                self.op(op::I32_EQZ);
            }
            FPred::Ult => {
                compare(self, ge)?;
                self.op(op::I32_EQZ);
            }
            FPred::Ule => {
                compare(self, gt)?;
                self.op(op::I32_EQZ);
            }
            FPred::Une => compare(self, ne)?,
        }
        Ok(vec![self.save(Vt::I32)])
    }

    // ---- conversions ----

    pub(super) fn cast(&mut self, cop: CastOp, from: Ty, to: Ty, a: Val) -> Res<Val> {
        // A bitcast that changes the shape of a vector, or between a vector and an integer, reads the
        // bytes of the value in another type: through memory, which has the layout of both.
        if cop == CastOp::BitCast
            && from != to
            && (self.vector_shape(from).is_some() || self.vector_shape(to).is_some())
        {
            let size = self.types.size_of(from)?;
            if size != self.types.size_of(to)? {
                bail!("a bitcast between types of different sizes");
            }
            let align = self.types.align_of(from)?.max(self.types.align_of(to)?);
            let slot = Atom::Frame(self.slot(size, align)?);
            self.store_value(&slot, 0, from, &a)?;
            return self.load_value(&slot, 0, to);
        }
        let a = self.atoms(&a)?;
        if let (Some((lanes_from, elem_from)), Some((lanes_to, elem_to))) =
            (self.vector_shape(from), self.vector_shape(to))
        {
            if lanes_from != lanes_to || cop == CastOp::BitCast {
                if cop == CastOp::BitCast && from == to {
                    return Ok(Val::Atoms(a));
                }
                bail!("a {cop:?} between vectors of different shapes");
            }
            let width = self.lane_atoms(elem_from)?;
            let mut out = Vec::new();
            for lane in 0..lanes_from as usize {
                let part = a[lane * width..(lane + 1) * width].to_vec();
                out.extend(self.scalar_cast(cop, elem_from, elem_to, &part)?);
            }
            return Ok(Val::Atoms(out));
        }
        Ok(Val::Atoms(self.scalar_cast(cop, from, to, &a)?))
    }

    fn scalar_cast(&mut self, cop: CastOp, from: Ty, to: Ty, a: &[Atom]) -> Res<Vec<Atom>> {
        match cop {
            CastOp::BitCast => self.bitcast(from, to, a),
            CastOp::FpTrunc | CastOp::FpExt => {
                self.push_all(a)?;
                match (from, to, cop) {
                    (Ty::Double, Ty::Float, CastOp::FpTrunc) => {
                        self.op(op::F32_DEMOTE_F64);
                        Ok(vec![self.save(Vt::F32)])
                    }
                    (Ty::Float, Ty::Double, CastOp::FpExt) => {
                        self.op(op::F64_PROMOTE_F32);
                        Ok(vec![self.save(Vt::F64)])
                    }
                    _ => bail!(
                        "a {cop:?} from {} to {}",
                        self.types.describe(from),
                        self.types.describe(to)
                    ),
                }
            }
            CastOp::FpToSi | CastOp::FpToUi => {
                let to_width = self.width(to)?;
                if to_width == Width::W128 {
                    bail!("a conversion from floating point to a 128-bit integer");
                }
                let source = match from {
                    Ty::Float => 0,
                    Ty::Double => 1,
                    other => bail!("a conversion from {}", self.types.describe(other)),
                };
                self.push_all(a)?;
                self.trunc_sat(source == 1, to_width.vt() == Vt::I64, cop == CastOp::FpToSi);
                self.canon(to_width);
                Ok(vec![self.save(to_width.vt())])
            }
            CastOp::SiToFp | CastOp::UiToFp => {
                let from_width = self.width(from)?;
                if from_width == Width::W128 {
                    bail!("a conversion from a 128-bit integer to floating point");
                }
                let signed = cop == CastOp::SiToFp;
                self.push_all(a)?;
                if signed {
                    self.sext(from_width);
                }
                let wide = from_width.vt() == Vt::I64;
                let opcode = match (to, wide, signed) {
                    (Ty::Float, false, true) => op::F32_CONVERT_I32_S,
                    (Ty::Float, false, false) => op::F32_CONVERT_I32_U,
                    (Ty::Float, true, true) => op::F32_CONVERT_I64_S,
                    (Ty::Float, true, false) => op::F32_CONVERT_I64_U,
                    (Ty::Double, false, true) => op::F64_CONVERT_I32_S,
                    (Ty::Double, false, false) => op::F64_CONVERT_I32_U,
                    (Ty::Double, true, true) => op::F64_CONVERT_I64_S,
                    (Ty::Double, true, false) => op::F64_CONVERT_I64_U,
                    _ => bail!("a conversion to {}", self.types.describe(to)),
                };
                self.op(opcode);
                Ok(vec![self.save(if to == Ty::Float {
                    Vt::F32
                } else {
                    Vt::F64
                })])
            }
            CastOp::Trunc | CastOp::ZExt | CastOp::SExt | CastOp::PtrToInt | CastOp::IntToPtr => {
                self.int_cast(cop, from, to, a)
            }
        }
    }

    /// `i32.trunc_sat_*` or `i64.trunc_sat_*` of the float on the stack.
    pub(super) fn trunc_sat(&mut self, from_f64: bool, to_i64: bool, signed: bool) {
        self.op(op::PREFIX);
        let code = (u8::from(to_i64) * 4) + (u8::from(from_f64) * 2) + u8::from(!signed);
        self.uleb(u64::from(code));
    }

    fn int_cast(&mut self, cop: CastOp, from: Ty, to: Ty, a: &[Atom]) -> Res<Vec<Atom>> {
        let from_width = self.width(from)?;
        let to_width = self.width(to)?;
        match cop {
            CastOp::Trunc => {
                let Some(lo) = a.first() else {
                    bail!("a truncation of nothing");
                };
                self.push(lo)?;
                if from_width.vt() == Vt::I64 && to_width.vt() == Vt::I32 {
                    self.op(op::I32_WRAP_I64);
                }
                self.canon(to_width);
                Ok(vec![self.save(to_width.vt())])
            }
            CastOp::ZExt => match (from_width.vt(), to_width) {
                (Vt::I32, Width::W32(_)) => Ok(a.to_vec()),
                (Vt::I64, Width::W64(_)) => Ok(a.to_vec()),
                (Vt::I32, Width::W64(_)) => {
                    self.push_all(a)?;
                    self.op(op::I64_EXTEND_I32_U);
                    Ok(vec![self.save(Vt::I64)])
                }
                (Vt::I32, Width::W128) => {
                    self.push_all(a)?;
                    self.op(op::I64_EXTEND_I32_U);
                    Ok(vec![self.save(Vt::I64), Atom::I64(0)])
                }
                (Vt::I64, Width::W128) => Ok(vec![a[0].clone(), Atom::I64(0)]),
                _ => bail!(
                    "a zero extension from {} to {}",
                    self.types.describe(from),
                    self.types.describe(to)
                ),
            },
            CastOp::SExt => {
                self.push_all(&a[..1])?;
                match from_width {
                    Width::W32(bits) => {
                        self.sext32(bits);
                        if to_width.vt() == Vt::I64 {
                            self.op(op::I64_EXTEND_I32_S);
                        }
                    }
                    Width::W64(bits) => self.sext64(bits),
                    Width::W128 => bail!("a sign extension from a 128-bit integer"),
                }
                match to_width {
                    Width::W128 => {
                        let lo = self.save(Vt::I64);
                        self.push(&lo)?;
                        self.i64_const(63);
                        self.op(op::I64_SHR_S);
                        let hi = self.save(Vt::I64);
                        Ok(vec![lo, hi])
                    }
                    _ => {
                        self.canon(to_width);
                        Ok(vec![self.save(to_width.vt())])
                    }
                }
            }
            CastOp::PtrToInt => match to_width {
                Width::W32(_) => {
                    self.push_all(a)?;
                    self.canon(to_width);
                    Ok(vec![self.save(Vt::I32)])
                }
                Width::W64(_) => {
                    self.push_all(a)?;
                    self.op(op::I64_EXTEND_I32_U);
                    self.canon(to_width);
                    Ok(vec![self.save(Vt::I64)])
                }
                Width::W128 => {
                    self.push_all(a)?;
                    self.op(op::I64_EXTEND_I32_U);
                    Ok(vec![self.save(Vt::I64), Atom::I64(0)])
                }
            },
            CastOp::IntToPtr => {
                self.push(&a[0])?;
                if from_width.vt() == Vt::I64 {
                    self.op(op::I32_WRAP_I64);
                }
                Ok(vec![self.save(Vt::I32)])
            }
            _ => bail!("the cast {cop:?}"),
        }
    }

    fn bitcast(&mut self, from: Ty, to: Ty, a: &[Atom]) -> Res<Vec<Atom>> {
        if from == to {
            return Ok(a.to_vec());
        }
        let (source, target) = (a.first().map(|atom| self.atom_vt(atom)), self.flat(to)?);
        match (source, target.as_slice(), a.len()) {
            (Some(Vt::I32), [Vt::F32], 1) => {
                self.push_all(a)?;
                self.op(op::F32_REINTERPRET_I32);
                Ok(vec![self.save(Vt::F32)])
            }
            (Some(Vt::F32), [Vt::I32], 1) => {
                self.push_all(a)?;
                self.op(op::I32_REINTERPRET_F32);
                Ok(vec![self.save(Vt::I32)])
            }
            (Some(Vt::I64), [Vt::F64], 1) => {
                self.push_all(a)?;
                self.op(op::F64_REINTERPRET_I64);
                Ok(vec![self.save(Vt::F64)])
            }
            (Some(Vt::F64), [Vt::I64], 1) => {
                self.push_all(a)?;
                self.op(op::I64_REINTERPRET_F64);
                Ok(vec![self.save(Vt::I64)])
            }
            (Some(Vt::I32), [Vt::I32], 1) | (Some(Vt::I64), [Vt::I64], 1) => Ok(a.to_vec()),
            _ => bail!(
                "a bitcast from {} to {}",
                self.types.describe(from),
                self.types.describe(to)
            ),
        }
    }

    // ---- select ----

    pub(super) fn select(&mut self, cond_ty: Ty, cond: Val, ty: Ty, a: Val, b: Val) -> Res<Val> {
        if let (Val::Ref(x), Val::Ref(y)) = (&a, &b) {
            let condition = self.single(&cond)?;
            self.push(x)?;
            self.push(y)?;
            self.push(&condition)?;
            self.op(op::SELECT);
            return Ok(Val::Ref(self.save(Vt::I32)));
        }
        let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
        if a.len() != b.len() {
            bail!("a select of values of different shapes");
        }
        let conditions = self.atoms(&cond)?;
        let lanes = self.vector_shape(cond_ty).map(|(len, _)| len as usize);
        let per_lane = match lanes {
            Some(lanes) if lanes > 0 => a.len() / lanes,
            _ => 0,
        };
        let _ = ty;
        let mut out = Vec::with_capacity(a.len());
        for (index, (x, y)) in a.iter().zip(&b).enumerate() {
            let condition = if lanes.is_some() {
                conditions[index / per_lane.max(1)].clone()
            } else {
                conditions[0].clone()
            };
            self.push(x)?;
            self.push(y)?;
            self.push(&condition)?;
            self.op(op::SELECT);
            let vt = self.atom_vt(x);
            out.push(self.save(vt));
        }
        Ok(Val::Atoms(out))
    }
}
