//! The `llvm.*` intrinsics that generated code and the runtime use.
//!
//! An intrinsic is lowered from the types of its arguments, not from the suffix of its name, so a
//! vector overload is the scalar one on every lane. One that this file does not know is E2002; it
//! is never approximated.

use super::super::error::{Res, bail};
use super::super::func::{CallArg, IPred};
use super::super::types::Ty;
use super::bin::{Vt, op};
use super::lower::{Atom, Lower, Val};
use super::ops::Width;

/// The name without `llvm.`, if it is `base` or `base.<suffix>`.
fn is(name: &str, base: &str) -> bool {
    name.strip_prefix(base)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overflow {
    SAdd,
    UAdd,
    SSub,
    USub,
    SMul,
    UMul,
}

impl<'w, 'a> Lower<'w, 'a> {
    pub(super) fn intrinsic(&mut self, full: &str, ret: Ty, args: &[CallArg]) -> Res<Val> {
        let name = full.strip_prefix("llvm.").unwrap_or(full);
        let arg = |lower: &mut Self, index: usize| -> Res<Val> {
            match args.get(index) {
                Some(argument) => lower.val(argument.op, argument.ty),
                None => bail!("the intrinsic {full} with too few arguments"),
            }
        };
        if is(name, "memcpy") || is(name, "memmove") {
            let (dst, src) = (arg(self, 0)?, arg(self, 1)?);
            let length = arg(self, 2)?;
            let (dst, src, length) = (
                self.single(&dst)?,
                self.single(&src)?,
                self.single(&length)?,
            );
            self.push(&dst)?;
            self.push(&src)?;
            self.push(&length)?;
            if self.atom_vt(&length) == Vt::I64 {
                self.op(op::I32_WRAP_I64);
            }
            self.op(op::PREFIX);
            self.uleb(u64::from(op::MEMORY_COPY));
            self.code.extend_from_slice(&[0x00, 0x00]);
            return Ok(Val::Void);
        }
        if is(name, "memset") {
            let (dst, byte) = (arg(self, 0)?, arg(self, 1)?);
            let length = arg(self, 2)?;
            let (dst, byte, length) = (
                self.single(&dst)?,
                self.single(&byte)?,
                self.single(&length)?,
            );
            self.push(&dst)?;
            self.push(&byte)?;
            self.push(&length)?;
            if self.atom_vt(&length) == Vt::I64 {
                self.op(op::I32_WRAP_I64);
            }
            self.op(op::PREFIX);
            self.uleb(u64::from(op::MEMORY_FILL));
            self.code.push(0x00);
            return Ok(Val::Void);
        }
        if is(name, "trap") || is(name, "debugtrap") {
            self.op(op::UNREACHABLE);
            return Ok(Val::Void);
        }
        if is(name, "expect")
            || is(name, "expect.with.probability")
            || is(name, "threadlocal.address")
        {
            return arg(self, 0);
        }
        if is(name, "wasm.memory.size") {
            self.op(op::MEMORY_SIZE);
            self.code.push(0x00);
            return Ok(Val::Atoms(vec![self.save(Vt::I32)]));
        }
        if is(name, "wasm.memory.grow") {
            let delta = arg(self, 1)?;
            let delta = self.single(&delta)?;
            self.push(&delta)?;
            self.op(op::MEMORY_GROW);
            self.code.push(0x00);
            return Ok(Val::Atoms(vec![self.save(Vt::I32)]));
        }
        for (base, kind) in [
            ("sadd.with.overflow", Overflow::SAdd),
            ("uadd.with.overflow", Overflow::UAdd),
            ("ssub.with.overflow", Overflow::SSub),
            ("usub.with.overflow", Overflow::USub),
            ("smul.with.overflow", Overflow::SMul),
            ("umul.with.overflow", Overflow::UMul),
        ] {
            if is(name, base) {
                let (a, b) = (arg(self, 0)?, arg(self, 1)?);
                let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
                let (value, flag) = self.with_overflow(kind, args[0].ty, &a, &b)?;
                let mut atoms = value;
                atoms.push(flag);
                return Ok(Val::Atoms(atoms));
            }
        }
        for (base, kind) in [
            ("sadd.sat", Overflow::SAdd),
            ("uadd.sat", Overflow::UAdd),
            ("ssub.sat", Overflow::SSub),
            ("usub.sat", Overflow::USub),
        ] {
            if is(name, base) {
                let (a, b) = (arg(self, 0)?, arg(self, 1)?);
                let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
                let atoms = self.saturating(kind, args[0].ty, &a, &b)?;
                return Ok(Val::Atoms(atoms));
            }
        }
        if is(name, "scmp") || is(name, "ucmp") {
            let signed = is(name, "scmp");
            let (a, b) = (arg(self, 0)?, arg(self, 1)?);
            let (greater, less) = if signed {
                (IPred::Sgt, IPred::Slt)
            } else {
                (IPred::Ugt, IPred::Ult)
            };
            let gt = self.icmp(greater, args[0].ty, a.clone(), b.clone())?;
            let lt = self.icmp(less, args[0].ty, a, b)?;
            let (gt, lt) = (self.atoms(&gt)?, self.atoms(&lt)?);
            let mut out = Vec::new();
            for (g, l) in gt.iter().zip(&lt) {
                self.push(g)?;
                self.push(l)?;
                self.op(op::I32_SUB);
                let Ty::Int(bits) = ret else {
                    bail!(
                        "a three-way comparison that returns {}",
                        self.types.describe(ret)
                    );
                };
                if bits > 32 {
                    self.op(op::I64_EXTEND_I32_S);
                }
                let width = self.width(Ty::Int(bits))?;
                self.canon(width);
                out.push(self.save(width.vt()));
            }
            return Ok(Val::Atoms(out));
        }
        for (base, signed, max) in [
            ("smax", true, true),
            ("smin", true, false),
            ("umax", false, true),
            ("umin", false, false),
        ] {
            if is(name, base) {
                let (a, b) = (arg(self, 0)?, arg(self, 1)?);
                let pred = match (signed, max) {
                    (true, true) => IPred::Sgt,
                    (true, false) => IPred::Slt,
                    (false, true) => IPred::Ugt,
                    (false, false) => IPred::Ult,
                };
                let condition = self.icmp(pred, args[0].ty, a.clone(), b.clone())?;
                return self.select(super::super::types::I1, condition, args[0].ty, a, b);
            }
        }
        if is(name, "abs") {
            let a = arg(self, 0)?;
            let value = self.atoms(&a)?;
            let atoms = self.lanes(args[0].ty, &[value], |lower, ty, parts| {
                lower.abs(ty, &parts[0])
            })?;
            return Ok(Val::Atoms(atoms));
        }
        for base in ["ctpop", "ctlz", "cttz"] {
            if is(name, base) {
                let a = arg(self, 0)?;
                let value = self.atoms(&a)?;
                let atoms = self.lanes(args[0].ty, &[value], |lower, ty, parts| {
                    lower.count_bits(base, ty, &parts[0])
                })?;
                return Ok(Val::Atoms(atoms));
            }
        }
        for (base, f32_op, f64_op) in [
            ("fabs", op::F32_ABS, op::F64_ABS),
            ("sqrt", op::F32_SQRT, op::F64_SQRT),
            ("floor", op::F32_FLOOR, op::F64_FLOOR),
            ("ceil", op::F32_CEIL, op::F64_CEIL),
            ("trunc", op::F32_TRUNC, op::F64_TRUNC),
            ("rint", op::F32_NEAREST, op::F64_NEAREST),
            ("nearbyint", op::F32_NEAREST, op::F64_NEAREST),
            ("roundeven", op::F32_NEAREST, op::F64_NEAREST),
        ] {
            if is(name, base) {
                let a = arg(self, 0)?;
                let value = self.atoms(&a)?;
                let atoms = self.lanes(args[0].ty, &[value], |lower, ty, parts| {
                    let single = ty == Ty::Float;
                    lower.push_all(&parts[0])?;
                    lower.op(if single { f32_op } else { f64_op });
                    Ok(vec![lower.save(if single { Vt::F32 } else { Vt::F64 })])
                })?;
                return Ok(Val::Atoms(atoms));
            }
        }
        if is(name, "copysign") {
            let (a, b) = (arg(self, 0)?, arg(self, 1)?);
            let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
            let atoms = self.lanes(args[0].ty, &[a, b], |lower, ty, parts| {
                let single = ty == Ty::Float;
                lower.push_all(&parts[0])?;
                lower.push_all(&parts[1])?;
                lower.op(if single {
                    op::F32_COPYSIGN
                } else {
                    op::F64_COPYSIGN
                });
                Ok(vec![lower.save(if single { Vt::F32 } else { Vt::F64 })])
            })?;
            return Ok(Val::Atoms(atoms));
        }
        if is(name, "fshl") || is(name, "fshr") {
            let left = is(name, "fshl");
            let (a, b, c) = (arg(self, 0)?, arg(self, 1)?, arg(self, 2)?);
            let (a, b, c) = (self.atoms(&a)?, self.atoms(&b)?, self.atoms(&c)?);
            let atoms = self.lanes(args[0].ty, &[a, b, c], |lower, ty, parts| {
                lower.funnel(left, ty, &parts[0], &parts[1], &parts[2])
            })?;
            return Ok(Val::Atoms(atoms));
        }
        if is(name, "fptosi.sat") || is(name, "fptoui.sat") {
            let signed = is(name, "fptosi.sat");
            let a = arg(self, 0)?;
            let value = self.atoms(&a)?;
            let atoms = self.lanes(args[0].ty, &[value], |lower, ty, parts| {
                let target = match ret {
                    Ty::Int(bits) => bits,
                    other => bail!("a saturating conversion to {}", lower.types.describe(other)),
                };
                lower.saturating_convert(signed, ty, target, &parts[0])
            })?;
            return Ok(Val::Atoms(atoms));
        }
        bail!("the intrinsic {full}")
    }

    /// `|x|` of an integer; the minimum value stays as it is.
    fn abs(&mut self, ty: Ty, a: &[Atom]) -> Res<Vec<Atom>> {
        let width = self.width(ty)?;
        if width == Width::W128 {
            bail!("the absolute value of a 128-bit integer");
        }
        let wide = width.vt() == Vt::I64;
        let (sub, lt_s) = if wide {
            (op::I64_SUB, op::I64_LT_S)
        } else {
            (op::I32_SUB, op::I32_LT_S)
        };
        // negative ? -x : x, with x extended from its sign bit.
        let signed = {
            self.push_all(a)?;
            match width {
                Width::W32(bits) => self.sext32(bits),
                Width::W64(bits) => self.sext64(bits),
                Width::W128 => {}
            }
            self.save(width.vt())
        };
        let zero = if wide { Atom::I64(0) } else { Atom::I32(0) };
        self.push(&zero)?;
        self.push(&signed)?;
        self.op(sub);
        self.push(&signed)?;
        self.push(&signed)?;
        self.push(&zero)?;
        self.op(lt_s);
        self.op(op::SELECT);
        self.canon(width);
        Ok(vec![self.save(width.vt())])
    }

    fn count_bits(&mut self, which: &str, ty: Ty, a: &[Atom]) -> Res<Vec<Atom>> {
        let width = self.width(ty)?;
        let (bits, wide) = match width {
            Width::W32(bits) => (bits, false),
            Width::W64(bits) => (bits, true),
            Width::W128 => bail!("{which} of a 128-bit integer"),
        };
        let container = if wide { 64 } else { 32 };
        self.push_all(a)?;
        match which {
            "ctpop" => self.op(if wide { op::I64_POPCNT } else { op::I32_POPCNT }),
            "ctlz" => {
                self.op(if wide { op::I64_CLZ } else { op::I32_CLZ });
                if bits < container {
                    let extra = i64::from(container - bits);
                    if wide {
                        self.i64_const(extra);
                        self.op(op::I64_SUB);
                    } else {
                        self.i32_const(extra as i32);
                        self.op(op::I32_SUB);
                    }
                }
            }
            _ => {
                if bits < container {
                    // A bit past the width stops the count at the width.
                    if wide {
                        self.i64_const(1i64 << bits);
                        self.op(op::I64_OR);
                    } else {
                        self.i32_const((1u64 << bits) as u32 as i32);
                        self.op(op::I32_OR);
                    }
                }
                self.op(if wide { op::I64_CTZ } else { op::I32_CTZ });
            }
        }
        Ok(vec![self.save(width.vt())])
    }

    /// The funnel shift `fshl` or `fshr` of the full width of an integer.
    fn funnel(&mut self, left: bool, ty: Ty, a: &[Atom], b: &[Atom], c: &[Atom]) -> Res<Vec<Atom>> {
        let width = self.width(ty)?;
        let (bits, wide) = match width {
            Width::W32(32) => (32i64, false),
            Width::W64(64) => (64i64, true),
            _ => bail!("a funnel shift of {}", self.types.describe(ty)),
        };
        if a == b {
            self.push_all(a)?;
            self.push_all(c)?;
            self.op(match (left, wide) {
                (true, false) => op::I32_ROTL,
                (true, true) => op::I64_ROTL,
                (false, false) => op::I32_ROTR,
                (false, true) => op::I64_ROTR,
            });
            return Ok(vec![self.save(width.vt())]);
        }
        let (shl, shr_u, sub, and, or, ne) = if wide {
            (
                op::I64_SHL,
                op::I64_SHR_U,
                op::I64_SUB,
                op::I64_AND,
                op::I64_OR,
                op::I64_NE,
            )
        } else {
            (
                op::I32_SHL,
                op::I32_SHR_U,
                op::I32_SUB,
                op::I32_AND,
                op::I32_OR,
                op::I32_NE,
            )
        };
        // k = c mod bits.
        self.push_all(c)?;
        self.int_const(wide, bits - 1);
        self.op(and);
        let k = self.save(width.vt());
        // fshl: k == 0 ? a : (a << k) | (b >> (bits - k)).
        // fshr: k == 0 ? b : (a << (bits - k)) | (b >> k).
        let (left_by_k, right_by_k, unshifted) = if left {
            (true, false, a)
        } else {
            (false, true, b)
        };
        let shifted = |lower: &mut Self, source: &[Atom], by_k: bool, down: bool| -> Res<Atom> {
            lower.push_all(source)?;
            if by_k {
                lower.push(&k)?;
            } else {
                lower.int_const(wide, bits);
                lower.push(&k)?;
                lower.op(sub);
            }
            lower.op(if down { shr_u } else { shl });
            Ok(lower.save(width.vt()))
        };
        let up = shifted(self, a, left_by_k, false)?;
        let down = shifted(self, b, right_by_k, true)?;
        self.push(&up)?;
        self.push(&down)?;
        self.op(or);
        self.push_all(unshifted)?;
        // `select(x, y, cond)` is x when cond: the combined value when k is not zero.
        self.push(&k)?;
        self.int_const(wide, 0);
        self.op(ne);
        self.op(op::SELECT);
        Ok(vec![self.save(width.vt())])
    }

    pub(super) fn int_const(&mut self, wide: bool, value: i64) {
        if wide {
            self.i64_const(value);
        } else {
            self.i32_const(value as i32);
        }
    }

    /// A conversion of a float to an integer of `target` bits that saturates.
    fn saturating_convert(
        &mut self,
        signed: bool,
        from: Ty,
        target: u16,
        a: &[Atom],
    ) -> Res<Vec<Atom>> {
        let from_f64 = match from {
            Ty::Float => false,
            Ty::Double => true,
            other => bail!(
                "a saturating conversion from {}",
                self.types.describe(other)
            ),
        };
        if target > 64 {
            bail!("a saturating conversion to {target} bits");
        }
        let wide = target > 32;
        self.push_all(a)?;
        self.trunc_sat(from_f64, wide, signed);
        let width = if wide {
            Width::W64(target)
        } else {
            Width::W32(target)
        };
        let container: u16 = if wide { 64 } else { 32 };
        if target < container {
            let value = self.save(width.vt());
            let (low, high): (i64, i64) = if signed {
                (-(1i64 << (target - 1)), (1i64 << (target - 1)) - 1)
            } else {
                (0, (1i64 << target) - 1)
            };
            let (lt, gt) = match (wide, signed) {
                (false, true) => (op::I32_LT_S, op::I32_GT_S),
                (false, false) => (op::I32_LT_U, op::I32_GT_U),
                (true, true) => (op::I64_LT_S, op::I64_GT_S),
                (true, false) => (op::I64_LT_U, op::I64_GT_U),
            };
            let constant = |lower: &mut Self, v: i64| {
                if wide {
                    lower.i64_const(v);
                } else {
                    lower.i32_const(v as i32);
                }
            };
            // max(x, low)
            constant(self, low);
            self.push(&value)?;
            self.push(&value)?;
            constant(self, low);
            self.op(lt);
            self.op(op::SELECT);
            let raised = self.save(width.vt());
            // min(x, high)
            constant(self, high);
            self.push(&raised)?;
            self.push(&raised)?;
            constant(self, high);
            self.op(gt);
            self.op(op::SELECT);
            self.canon(width);
        }
        Ok(vec![self.save(width.vt())])
    }

    /// The sum or difference of two 128-bit integers with its overflow flag. A product that
    /// overflows needs 256 bits, which this lowering does not compute.
    fn with_overflow128(
        &mut self,
        kind: Overflow,
        a: &[Atom],
        b: &[Atom],
    ) -> Res<(Vec<Atom>, Atom)> {
        let (Some(a_lo), Some(a_hi), Some(b_lo), Some(b_hi)) =
            (a.first(), a.get(1), b.first(), b.get(1))
        else {
            bail!("a 128-bit operand that is not two words");
        };
        let (subtract, signed) = match kind {
            Overflow::SAdd => (false, true),
            Overflow::UAdd => (false, false),
            Overflow::SSub => (true, true),
            Overflow::USub => (true, false),
            Overflow::SMul | Overflow::UMul => bail!("an overflow check of a 128-bit product"),
        };
        let arithmetic = if subtract { op::I64_SUB } else { op::I64_ADD };
        self.push(a_lo)?;
        self.push(b_lo)?;
        self.op(arithmetic);
        let lo = self.save(Vt::I64);
        // The carry out of the low word of a sum, the borrow from the high word of a difference.
        if subtract {
            self.push(a_lo)?;
            self.push(b_lo)?;
        } else {
            self.push(&lo)?;
            self.push(a_lo)?;
        }
        self.op(op::I64_LT_U);
        self.op(op::I64_EXTEND_I32_U);
        let carry = self.save(Vt::I64);
        self.push(a_hi)?;
        self.push(b_hi)?;
        self.op(arithmetic);
        let partial = self.save(Vt::I64);
        self.push(&partial)?;
        self.push(&carry)?;
        self.op(arithmetic);
        let hi = self.save(Vt::I64);
        match (signed, subtract) {
            (false, false) => {
                self.push(&partial)?;
                self.push(a_hi)?;
                self.op(op::I64_LT_U);
                self.push(&hi)?;
                self.push(&partial)?;
                self.op(op::I64_LT_U);
                self.op(op::I32_OR);
            }
            (false, true) => {
                self.push(a_hi)?;
                self.push(b_hi)?;
                self.op(op::I64_LT_U);
                self.push(&partial)?;
                self.push(&carry)?;
                self.op(op::I64_LT_U);
                self.op(op::I32_OR);
            }
            // The operands have the same sign and the result another (a sum), or the operands have
            // different signs and the result is not the first one's (a difference).
            (true, false) => {
                self.push(a_hi)?;
                self.push(&hi)?;
                self.op(op::I64_XOR);
                self.push(b_hi)?;
                self.push(&hi)?;
                self.op(op::I64_XOR);
                self.op(op::I64_AND);
                self.i64_const(0);
                self.op(op::I64_LT_S);
            }
            (true, true) => {
                self.push(a_hi)?;
                self.push(b_hi)?;
                self.op(op::I64_XOR);
                self.push(a_hi)?;
                self.push(&hi)?;
                self.op(op::I64_XOR);
                self.op(op::I64_AND);
                self.i64_const(0);
                self.op(op::I64_LT_S);
            }
        }
        let flag = self.save(Vt::I32);
        Ok((vec![lo, hi], flag))
    }

    fn with_overflow(
        &mut self,
        kind: Overflow,
        ty: Ty,
        a: &[Atom],
        b: &[Atom],
    ) -> Res<(Vec<Atom>, Atom)> {
        let width = self.width(ty)?;
        if width == Width::W128 {
            return self.with_overflow128(kind, a, b);
        }
        let (bits, wide) = match width {
            Width::W32(bits) => (bits, false),
            Width::W64(bits) => (bits, true),
            Width::W128 => unreachable!("handled above"),
        };
        let container: u16 = if wide { 64 } else { 32 };
        let signed = matches!(kind, Overflow::SAdd | Overflow::SSub | Overflow::SMul);
        let multiply = matches!(kind, Overflow::SMul | Overflow::UMul);
        // A product of more than 32 bits of operands does not fit its container: the compiler
        // runtime multiplies in 128 bits.
        if multiply && bits > 16 {
            return self.multiply_overflow(signed, bits, a, b);
        }
        let (add, sub, mul) = if wide {
            (op::I64_ADD, op::I64_SUB, op::I64_MUL)
        } else {
            (op::I32_ADD, op::I32_SUB, op::I32_MUL)
        };
        let arithmetic = match kind {
            Overflow::SAdd | Overflow::UAdd => add,
            Overflow::SSub | Overflow::USub => sub,
            Overflow::SMul | Overflow::UMul => mul,
        };
        let operand = |lower: &mut Self, atoms: &[Atom]| -> Res<()> {
            lower.push_all(atoms)?;
            if signed {
                lower.sext(width);
            }
            Ok(())
        };
        operand(self, a)?;
        operand(self, b)?;
        self.op(arithmetic);
        let raw = self.save(width.vt());
        // The result: the low bits of the exact value.
        self.push(&raw)?;
        self.canon(width);
        let value = self.save(width.vt());
        let flag = if bits < container || multiply {
            // The exact value fits the container: it is in range if the width holds it.
            if signed {
                self.push(&value)?;
                self.sext(width);
                self.push(&raw)?;
                self.op(if wide { op::I64_NE } else { op::I32_NE });
            } else {
                self.push(&raw)?;
                if wide {
                    self.i64_const(((1u64 << bits) - 1) as i64);
                    self.op(op::I64_GT_U);
                } else {
                    self.i32_const(((1u64 << bits) - 1) as u32 as i32);
                    self.op(op::I32_GT_U);
                }
            }
            self.save(Vt::I32)
        } else {
            // The width is the container's: compare as the instruction set allows.
            let (xor, and, lt_s, lt_u) = if wide {
                (op::I64_XOR, op::I64_AND, op::I64_LT_S, op::I64_LT_U)
            } else {
                (op::I32_XOR, op::I32_AND, op::I32_LT_S, op::I32_LT_U)
            };
            let zero = if wide { Atom::I64(0) } else { Atom::I32(0) };
            match kind {
                Overflow::UAdd => {
                    self.push(&value)?;
                    self.push_all(a)?;
                    self.op(lt_u);
                }
                Overflow::USub => {
                    self.push_all(a)?;
                    self.push_all(b)?;
                    self.op(lt_u);
                }
                Overflow::SAdd => {
                    // The signs of both operands differ from the sign of the sum.
                    self.push_all(a)?;
                    self.push(&value)?;
                    self.op(xor);
                    self.push_all(b)?;
                    self.push(&value)?;
                    self.op(xor);
                    self.op(and);
                    self.push(&zero)?;
                    self.op(lt_s);
                }
                _ => {
                    self.push_all(a)?;
                    self.push_all(b)?;
                    self.op(xor);
                    self.push_all(a)?;
                    self.push(&value)?;
                    self.op(xor);
                    self.op(and);
                    self.push(&zero)?;
                    self.op(lt_s);
                }
            }
            self.save(Vt::I32)
        };
        Ok((vec![value], flag))
    }

    /// A product of integers of more than 16 bits, through the 128-bit multiplication of the runtime.
    fn multiply_overflow(
        &mut self,
        signed: bool,
        bits: u16,
        a: &[Atom],
        b: &[Atom],
    ) -> Res<(Vec<Atom>, Atom)> {
        let wide_ty = Ty::Int(if bits > 32 { 64 } else { 32 });
        let extend = |lower: &mut Self, atoms: &[Atom]| -> Res<Vec<Atom>> {
            let mut value = atoms.to_vec();
            if bits <= 32 {
                lower.push_all(atoms)?;
                if signed {
                    lower.sext32(bits);
                    lower.op(op::I64_EXTEND_I32_S);
                } else {
                    lower.op(op::I64_EXTEND_I32_U);
                }
                value = vec![lower.save(Vt::I64)];
            } else if signed {
                lower.push_all(atoms)?;
                lower.sext64(bits);
                value = vec![lower.save(Vt::I64)];
            }
            let low = value[0].clone();
            let high = if signed {
                lower.push(&low)?;
                lower.i64_const(63);
                lower.op(op::I64_SHR_S);
                lower.save(Vt::I64)
            } else {
                Atom::I64(0)
            };
            Ok(vec![low, high])
        };
        let _ = wide_ty;
        let (x, y) = (extend(self, a)?, extend(self, b)?);
        let product = self.call_library(
            "__multi3",
            vec![(Ty::Int(128), Val::Atoms(x)), (Ty::Int(128), Val::Atoms(y))],
            Ty::Int(128),
        )?;
        let product = self.atoms(&product)?;
        let [low, high] = &product[..] else {
            bail!("a 128-bit product that is not two words");
        };
        // The low word holds the value; it overflows when the exact product needs more bits.
        let value;
        let flag;
        if bits >= 33 {
            self.push(low)?;
            value = self.save(Vt::I64);
            // 64 bits: the high word must be the extension of the low one.
            self.push(high)?;
            if signed {
                self.push(low)?;
                self.i64_const(63);
                self.op(op::I64_SHR_S);
            } else {
                self.i64_const(0);
            }
            self.op(op::I64_NE);
            flag = self.save(Vt::I32);
            let width = Width::W64(bits);
            self.push(&value)?;
            self.canon(width);
            return Ok((vec![self.save(Vt::I64)], flag));
        }
        // At most 32 bits: the exact product fits the low word.
        self.push(low)?;
        let (value_atom, container) = (low.clone(), Width::W32(bits));
        self.op(op::I32_WRAP_I64);
        self.canon(container);
        value = self.save(Vt::I32);
        self.push(&value_atom)?;
        if signed {
            self.push(&value)?;
            self.sext32(bits);
            self.op(op::I64_EXTEND_I32_S);
            self.op(op::I64_NE);
        } else {
            self.i64_const(((1u64 << bits) - 1) as i64);
            self.op(op::I64_GT_U);
        }
        flag = self.save(Vt::I32);
        Ok((vec![value], flag))
    }

    fn saturating(&mut self, kind: Overflow, ty: Ty, a: &[Atom], b: &[Atom]) -> Res<Vec<Atom>> {
        let width = self.width(ty)?;
        let (bits, wide) = match width {
            Width::W32(bits) => (bits, false),
            Width::W64(bits) => (bits, true),
            Width::W128 => bail!("a saturating operation on a 128-bit integer"),
        };
        let (value, flag) = self.with_overflow(kind, ty, a, b)?;
        let mask = if bits >= 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let (max_signed, min_signed) = (mask >> 1, !(mask >> 1) & mask);
        // The value to take when the operation overflows.
        match kind {
            Overflow::UAdd => self.int_const(wide, mask as i64),
            Overflow::USub => self.int_const(wide, 0),
            _ => {
                // The sign of the first operand says which end was passed: below zero, the minimum.
                let lt_s = if wide { op::I64_LT_S } else { op::I32_LT_S };
                self.int_const(wide, min_signed as i64);
                self.int_const(wide, max_signed as i64);
                self.push_all(a)?;
                self.sext(width);
                self.int_const(wide, 0);
                self.op(lt_s);
                self.op(op::SELECT);
            }
        }
        // `select(x, y, cond)` is x when cond: the limit when it overflowed, the value when not.
        self.push(&value[0])?;
        self.push(&flag)?;
        self.op(op::SELECT);
        Ok(vec![self.save(width.vt())])
    }
}
