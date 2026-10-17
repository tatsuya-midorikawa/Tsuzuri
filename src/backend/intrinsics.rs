//! The `llvm.*` intrinsics that generated code and the runtime use.
//!
//! An intrinsic is lowered from the types of its arguments, not from the suffix of its name, so a
//! vector overload (`llvm.ctpop.v16i8`) is the scalar one on every lane. One that this file does not
//! know is E2002; it is never approximated.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{self, InstBuilder, Value, types};

use super::error::{Res, bail};
use super::func::{CallArg, Const, Op};
use super::lower::{Lower, Val};
use super::types::Ty;

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
            return self.memory_copy(is(name, "memmove"), args);
        }
        if is(name, "memset") {
            return self.memory_set(args);
        }
        if is(name, "debugtrap") {
            self.b.ins().debugtrap();
            return Ok(Val::Void);
        }
        if is(name, "expect") || is(name, "expect.with.probability") {
            return arg(self, 0);
        }
        if is(name, "threadlocal.address") {
            let pointer = arg(self, 0)?;
            return Ok(Val::S(self.scalar(pointer)?));
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
                return self.with_overflow(kind, args[0].ty, a, b);
            }
        }
        for (base, kind, saturating_sub) in [
            ("sadd.sat", Overflow::SAdd, false),
            ("uadd.sat", Overflow::UAdd, false),
            ("ssub.sat", Overflow::SSub, true),
            ("usub.sat", Overflow::USub, true),
        ] {
            let _ = saturating_sub;
            if is(name, base) {
                let (a, b) = (arg(self, 0)?, arg(self, 1)?);
                return self.map2(args[0].ty, a, b, |lower, elem, x, y| {
                    lower.saturating(kind, elem, x, y)
                });
            }
        }
        if is(name, "scmp") || is(name, "ucmp") {
            let signed = is(name, "scmp");
            let (a, b) = (arg(self, 0)?, arg(self, 1)?);
            let result = ret;
            return self.map2(args[0].ty, a, b, |lower, elem, x, y| {
                lower.three_way(signed, elem, result, x, y)
            });
        }
        for (base, unary) in [
            ("ctpop", Unary::Popcnt),
            ("bswap", Unary::Bswap),
            ("bitreverse", Unary::Bitrev),
            ("fabs", Unary::FAbs),
            ("sqrt", Unary::Sqrt),
            ("floor", Unary::Floor),
            ("ceil", Unary::Ceil),
            ("trunc", Unary::Trunc),
            ("rint", Unary::Nearest),
            ("nearbyint", Unary::Nearest),
            ("roundeven", Unary::Nearest),
        ] {
            if is(name, base) {
                let a = arg(self, 0)?;
                return self.map1(args[0].ty, a, |lower, elem, x| lower.unary(unary, elem, x));
            }
        }
        if is(name, "abs") {
            let a = arg(self, 0)?;
            return self.map1(args[0].ty, a, |lower, elem, x| {
                lower.unary(Unary::Abs, elem, x)
            });
        }
        if is(name, "ctlz") || is(name, "cttz") {
            let leading = is(name, "ctlz");
            let a = arg(self, 0)?;
            return self.map1(args[0].ty, a, |lower, elem, x| {
                lower.count_zeros(leading, elem, x)
            });
        }
        for (base, kind) in [
            ("smax", MinMax::SMax),
            ("smin", MinMax::SMin),
            ("umax", MinMax::UMax),
            ("umin", MinMax::UMin),
        ] {
            if is(name, base) {
                let (a, b) = (arg(self, 0)?, arg(self, 1)?);
                return self.map2(args[0].ty, a, b, |lower, elem, x, y| {
                    lower.min_max(kind, elem, x, y)
                });
            }
        }
        if is(name, "copysign") {
            let (a, b) = (arg(self, 0)?, arg(self, 1)?);
            return self.map2(args[0].ty, a, b, |lower, _, x, y| {
                Ok(lower.b.ins().fcopysign(x, y))
            });
        }
        if is(name, "minimum") || is(name, "maximum") {
            let minimum = is(name, "minimum");
            let (a, b) = (arg(self, 0)?, arg(self, 1)?);
            return self.map2(args[0].ty, a, b, |lower, _, x, y| {
                Ok(if minimum {
                    lower.b.ins().fmin(x, y)
                } else {
                    lower.b.ins().fmax(x, y)
                })
            });
        }
        if is(name, "fma") || is(name, "fmuladd") {
            let (a, b, c) = (arg(self, 0)?, arg(self, 1)?, arg(self, 2)?);
            let (la, lb, lc) = (self.leaves(a)?, self.leaves(b)?, self.leaves(c)?);
            let mut out = Vec::with_capacity(la.len());
            for index in 0..la.len() {
                out.push(self.b.ins().fma(la[index], lb[index], lc[index]));
            }
            return Ok(if self.vector_shape(args[0].ty).is_some() {
                self.multi(&out)
            } else {
                Val::S(out[0])
            });
        }
        if is(name, "fshl") || is(name, "fshr") {
            let left = is(name, "fshl");
            let same = args[0].op == args[1].op;
            let (a, b, c) = (arg(self, 0)?, arg(self, 1)?, arg(self, 2)?);
            let (la, lb, lc) = (self.leaves(a)?, self.leaves(b)?, self.leaves(c)?);
            let elem = self
                .vector_shape(args[0].ty)
                .map_or(args[0].ty, |(_, elem)| elem);
            let mut out = Vec::with_capacity(la.len());
            for index in 0..la.len() {
                out.push(self.funnel(left, same, elem, la[index], lb[index], lc[index])?);
            }
            return Ok(if self.vector_shape(args[0].ty).is_some() {
                self.multi(&out)
            } else {
                Val::S(out[0])
            });
        }
        if is(name, "fptosi.sat") || is(name, "fptoui.sat") {
            let signed = is(name, "fptosi.sat");
            let a = arg(self, 0)?;
            let to = self.vector_shape(ret).map_or(ret, |(_, elem)| elem);
            return self.map1(args[0].ty, a, |lower, _, x| {
                lower.float_to_int_sat(signed, to, x)
            });
        }
        if is(name, "is.fpclass") {
            let a = arg(self, 0)?;
            let Some(Op::Const(id)) = args.get(1).map(|a| a.op) else {
                bail!("llvm.is.fpclass with a variable mask");
            };
            let Const::Int(mask) = self.func.consts[id as usize] else {
                bail!("llvm.is.fpclass with a mask that is not an integer");
            };
            return self.map1(args[0].ty, a, |lower, elem, x| {
                lower.is_fpclass(elem, x, mask as u32)
            });
        }
        if is(name, "vector.reduce.add") {
            let a = arg(self, 0)?;
            let lanes = self.leaves(a)?;
            let mut sum = lanes[0];
            for &lane in &lanes[1..] {
                sum = self.b.ins().iadd(sum, lane);
            }
            return Ok(Val::S(sum));
        }
        bail!("the intrinsic {full}")
    }

    /// Applies a scalar operation to a value, or to every lane of a vector.
    fn map1(
        &mut self,
        ty: Ty,
        a: Val,
        mut op: impl FnMut(&mut Self, Ty, Value) -> Res<Value>,
    ) -> Res<Val> {
        if let Some((_, elem)) = self.vector_shape(ty) {
            let lanes = self.leaves(a)?;
            let mut out = Vec::with_capacity(lanes.len());
            for &lane in &lanes {
                out.push(op(self, elem, lane)?);
            }
            return Ok(self.multi(&out));
        }
        let x = self.scalar(a)?;
        Ok(Val::S(op(self, ty, x)?))
    }

    fn map2(
        &mut self,
        ty: Ty,
        a: Val,
        b: Val,
        mut op: impl FnMut(&mut Self, Ty, Value, Value) -> Res<Value>,
    ) -> Res<Val> {
        if let Some((_, elem)) = self.vector_shape(ty) {
            let (la, lb) = (self.leaves(a)?, self.leaves(b)?);
            let mut out = Vec::with_capacity(la.len());
            for (&x, &y) in la.iter().zip(&lb) {
                out.push(op(self, elem, x, y)?);
            }
            return Ok(self.multi(&out));
        }
        let (x, y) = (self.scalar(a)?, self.scalar(b)?);
        Ok(Val::S(op(self, ty, x, y)?))
    }

    fn exact_int(&self, ty: Ty) -> Res<u16> {
        match ty {
            Ty::Int(bits) if matches!(bits, 8 | 16 | 32 | 64 | 128) => Ok(bits),
            other => bail!("an intrinsic on {}", self.types.describe(other)),
        }
    }

    // ---- memory ----

    fn size_value(&mut self, arg: &CallArg) -> Res<Value> {
        let value = self.scalar_op(arg.op, arg.ty)?;
        Ok(match self.b.func.dfg.value_type(value) {
            kind if kind == types::I64 => value,
            kind if kind == types::I128 => self.b.ins().ireduce(types::I64, value),
            _ => self.b.ins().uextend(types::I64, value),
        })
    }

    fn constant_of(&self, arg: &CallArg) -> Option<u64> {
        match arg.op {
            Op::Const(id) => match &self.func.consts[id as usize] {
                Const::Int(value) => u64::try_from(*value).ok(),
                Const::Zero => Some(0),
                _ => None,
            },
            Op::Local(_) => None,
        }
    }

    fn memory_copy(&mut self, overlapping: bool, args: &[CallArg]) -> Res<Val> {
        let (dest, src) = (
            self.scalar_op(args[0].op, args[0].ty)?,
            self.scalar_op(args[1].op, args[1].ty)?,
        );
        let config = self.frontend;
        if let Some(size) = self.constant_of(&args[2]) {
            self.b.emit_small_memory_copy(
                config,
                dest,
                src,
                size,
                1,
                1,
                !overlapping,
                super::lower::plain(),
            );
            return Ok(Val::Void);
        }
        let size = self.size_value(&args[2])?;
        if overlapping {
            self.b.call_memmove(config, dest, src, size);
        } else {
            self.b.call_memcpy(config, dest, src, size);
        }
        Ok(Val::Void)
    }

    fn memory_set(&mut self, args: &[CallArg]) -> Res<Val> {
        let dest = self.scalar_op(args[0].op, args[0].ty)?;
        let config = self.frontend;
        if let (Some(size), Some(byte)) = (self.constant_of(&args[2]), self.constant_of(&args[1])) {
            self.b
                .emit_small_memset(config, dest, byte as u8, size, 1, super::lower::plain());
            return Ok(Val::Void);
        }
        let byte = self.scalar_op(args[1].op, args[1].ty)?;
        let size = self.size_value(&args[2])?;
        self.b.call_memset(config, dest, byte, size);
        Ok(Val::Void)
    }

    // ---- integers ----

    fn with_overflow(&mut self, kind: Overflow, ty: Ty, a: Val, b: Val) -> Res<Val> {
        if let Some((_, elem)) = self.vector_shape(ty) {
            let (la, lb) = (self.leaves(a)?, self.leaves(b)?);
            let (mut values, mut flags) = (Vec::new(), Vec::new());
            for (&x, &y) in la.iter().zip(&lb) {
                let (value, flag) = self.overflow(kind, elem, x, y)?;
                values.push(value);
                flags.push(flag);
            }
            values.extend(flags);
            return Ok(self.multi(&values));
        }
        let (x, y) = (self.scalar(a)?, self.scalar(b)?);
        let (value, flag) = self.overflow(kind, ty, x, y)?;
        Ok(self.multi(&[value, flag]))
    }

    fn overflow(&mut self, kind: Overflow, ty: Ty, x: Value, y: Value) -> Res<(Value, Value)> {
        let bits = self.exact_int(ty)?;
        if bits == 128 && matches!(kind, Overflow::SMul | Overflow::UMul) {
            bail!("a checked multiplication of 128-bit integers");
        }
        Ok(match kind {
            Overflow::SAdd => self.b.ins().sadd_overflow(x, y),
            Overflow::UAdd => self.b.ins().uadd_overflow(x, y),
            Overflow::SSub => self.b.ins().ssub_overflow(x, y),
            Overflow::USub => self.b.ins().usub_overflow(x, y),
            Overflow::SMul => self.b.ins().smul_overflow(x, y),
            Overflow::UMul => self.b.ins().umul_overflow(x, y),
        })
    }

    fn saturating(&mut self, kind: Overflow, ty: Ty, x: Value, y: Value) -> Res<Value> {
        let bits = self.exact_int(ty)?;
        let cty = self.clif_type(ty)?;
        let (result, flag) = self.overflow(kind, ty, x, y)?;
        Ok(match kind {
            Overflow::UAdd => {
                let max = self.iconst(cty, -1);
                self.b.ins().select(flag, max, result)
            }
            Overflow::USub => {
                let zero = self.iconst(cty, 0);
                self.b.ins().select(flag, zero, result)
            }
            _ => {
                // Overflow runs toward the sign of the first operand's side.
                let max = self.iconst(cty, i128::MAX >> (128 - bits));
                let min = self.iconst(cty, i128::MIN >> (128 - bits));
                let zero = self.iconst(cty, 0);
                let negative = self.b.ins().icmp(IntCC::SignedLessThan, x, zero);
                let bound = self.b.ins().select(negative, min, max);
                self.b.ins().select(flag, bound, result)
            }
        })
    }

    fn three_way(&mut self, signed: bool, ty: Ty, result: Ty, x: Value, y: Value) -> Res<Value> {
        self.exact_int(ty)?;
        let out = self.clif_type(result)?;
        let (greater, less) = if signed {
            (IntCC::SignedGreaterThan, IntCC::SignedLessThan)
        } else {
            (IntCC::UnsignedGreaterThan, IntCC::UnsignedLessThan)
        };
        let g = self.b.ins().icmp(greater, x, y);
        let l = self.b.ins().icmp(less, x, y);
        let (g, l) = if out == types::I8 {
            (g, l)
        } else {
            (self.b.ins().uextend(out, g), self.b.ins().uextend(out, l))
        };
        Ok(self.b.ins().isub(g, l))
    }

    fn count_zeros(&mut self, leading: bool, ty: Ty, x: Value) -> Res<Value> {
        let bits = self.exact_int(ty)?;
        Ok(if leading {
            self.b.ins().clz(x)
        } else {
            let _ = bits;
            self.b.ins().ctz(x)
        })
    }

    fn min_max(&mut self, kind: MinMax, ty: Ty, x: Value, y: Value) -> Res<Value> {
        let bits = self.exact_int(ty)?;
        if bits == 128 {
            let cc = match kind {
                MinMax::SMax => IntCC::SignedGreaterThan,
                MinMax::SMin => IntCC::SignedLessThan,
                MinMax::UMax => IntCC::UnsignedGreaterThan,
                MinMax::UMin => IntCC::UnsignedLessThan,
            };
            let better = self.b.ins().icmp(cc, x, y);
            return Ok(self.b.ins().select(better, x, y));
        }
        Ok(match kind {
            MinMax::SMax => self.b.ins().smax(x, y),
            MinMax::SMin => self.b.ins().smin(x, y),
            MinMax::UMax => self.b.ins().umax(x, y),
            MinMax::UMin => self.b.ins().umin(x, y),
        })
    }

    fn unary(&mut self, op: Unary, ty: Ty, x: Value) -> Res<Value> {
        match op {
            Unary::Popcnt | Unary::Bswap | Unary::Bitrev | Unary::Abs => {
                let bits = self.exact_int(ty)?;
                Ok(match op {
                    Unary::Popcnt => self.b.ins().popcnt(x),
                    Unary::Bswap if bits == 8 => x,
                    Unary::Bswap => self.b.ins().bswap(x),
                    Unary::Bitrev => self.b.ins().bitrev(x),
                    _ => self.b.ins().iabs(x),
                })
            }
            _ => {
                if !matches!(ty, Ty::Float | Ty::Double) {
                    bail!("a floating-point intrinsic on {}", self.types.describe(ty));
                }
                Ok(match op {
                    Unary::FAbs => self.b.ins().fabs(x),
                    Unary::Sqrt => self.b.ins().sqrt(x),
                    Unary::Floor => self.b.ins().floor(x),
                    Unary::Ceil => self.b.ins().ceil(x),
                    Unary::Trunc => self.b.ins().trunc(x),
                    _ => self.b.ins().nearest(x),
                })
            }
        }
    }

    /// `fshl`/`fshr`: shifts the two words as one and takes one word of the result.
    fn funnel(
        &mut self,
        left: bool,
        same: bool,
        ty: Ty,
        a: Value,
        b: Value,
        amount: Value,
    ) -> Res<Value> {
        let bits = self.exact_int(ty)?;
        if same && bits != 128 {
            return Ok(if left {
                self.b.ins().rotl(a, amount)
            } else {
                self.b.ins().rotr(a, amount)
            });
        }
        let cty = self.clif_type(ty)?;
        let mask = self.b.ins().band_imm(amount, i64::from(bits) - 1);
        // fshl: (a << s) | ((b >> 1) >> (bits - 1 - s)); fshr: ((a << 1) << (bits - 1 - s)) | (b >> s).
        let inverse = {
            let top = self.iconst(cty, i128::from(bits) - 1);
            self.b.ins().isub(top, mask)
        };
        Ok(if left {
            let shifted = self.b.ins().ishl(a, mask);
            let half = self.b.ins().ushr_imm(b, 1);
            let rest = self.b.ins().ushr(half, inverse);
            self.b.ins().bor(shifted, rest)
        } else {
            let half = self.b.ins().ishl_imm(a, 1);
            let upper = self.b.ins().ishl(half, inverse);
            let lower = self.b.ins().ushr(b, mask);
            self.b.ins().bor(upper, lower)
        })
    }

    fn float_to_int_sat(&mut self, signed: bool, to: Ty, x: Value) -> Res<Value> {
        let bits = self.exact_int(to)?;
        let cty = self.clif_type(to)?;
        if bits == 128 {
            bail!("a saturating conversion to a 128-bit integer");
        }
        let wide = if bits < 32 { types::I32 } else { cty };
        let converted = if signed {
            self.b.ins().fcvt_to_sint_sat(wide, x)
        } else {
            self.b.ins().fcvt_to_uint_sat(wide, x)
        };
        if wide == cty {
            return Ok(converted);
        }
        let (low, high) = if signed {
            (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
        } else {
            (0, (1i128 << bits) - 1)
        };
        let (low, high) = (self.iconst(wide, low), self.iconst(wide, high));
        let clamped = if signed {
            let above = self.b.ins().smax(converted, low);
            self.b.ins().smin(above, high)
        } else {
            self.b.ins().umin(converted, high)
        };
        Ok(self.b.ins().ireduce(cty, clamped))
    }

    /// `llvm.is.fpclass`: which of the ten classes of the mask a number belongs to, from its bits.
    fn is_fpclass(&mut self, ty: Ty, x: Value, mask: u32) -> Res<Value> {
        let (int_ty, exponent_bits, mantissa_bits) = match ty {
            Ty::Float => (types::I32, 8u32, 23u32),
            Ty::Double => (types::I64, 11, 52),
            other => bail!("llvm.is.fpclass on {}", self.types.describe(other)),
        };
        let total = exponent_bits + mantissa_bits + 1;
        let bits = self.b.ins().bitcast(int_ty, ir::MemFlags::new(), x);
        let sign_mask = 1i128 << (total - 1);
        let magnitude = self.b.ins().band_imm(bits, (sign_mask - 1) as i64);
        let infinity = ((1i128 << exponent_bits) - 1) << mantissa_bits;
        let quiet = 1i128 << (mantissa_bits - 1);
        let min_normal = 1i128 << mantissa_bits;
        let inf = self.iconst(int_ty, infinity);
        let sign_bit = self.b.ins().ushr_imm(bits, i64::from(total - 1));
        let negative = self.b.ins().icmp_imm(IntCC::NotEqual, sign_bit, 0);
        let positive = self.b.ins().icmp_imm(IntCC::Equal, sign_bit, 0);
        let is_nan = self
            .b
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, magnitude, inf);
        let quiet_bit = self.b.ins().band_imm(magnitude, quiet as i64);
        let is_quiet = self.b.ins().icmp_imm(IntCC::NotEqual, quiet_bit, 0);
        let is_signaling = {
            let not_quiet = self.b.ins().bxor_imm(is_quiet, 1);
            self.b.ins().band(is_nan, not_quiet)
        };
        let is_quiet_nan = self.b.ins().band(is_nan, is_quiet);
        let is_inf = self.b.ins().icmp(IntCC::Equal, magnitude, inf);
        let is_zero = self.b.ins().icmp_imm(IntCC::Equal, magnitude, 0);
        let minimum = self.iconst(int_ty, min_normal);
        let below_normal = self
            .b
            .ins()
            .icmp(IntCC::UnsignedLessThan, magnitude, minimum);
        let nonzero = self.b.ins().icmp_imm(IntCC::NotEqual, magnitude, 0);
        let is_subnormal = self.b.ins().band(below_normal, nonzero);
        let not_nan = self.b.ins().bxor_imm(is_nan, 1);
        let not_inf = self.b.ins().bxor_imm(is_inf, 1);
        let not_small = self.b.ins().bxor_imm(below_normal, 1);
        let finite = self.b.ins().band(not_nan, not_inf);
        let is_normal = self.b.ins().band(finite, not_small);
        let mut result = self.b.ins().iconst(types::I8, 0);
        let classes: [(u32, Value, Option<bool>); 10] = [
            (0, is_signaling, None),
            (1, is_quiet_nan, None),
            (2, is_inf, Some(true)),
            (3, is_normal, Some(true)),
            (4, is_subnormal, Some(true)),
            (5, is_zero, Some(true)),
            (6, is_zero, Some(false)),
            (7, is_subnormal, Some(false)),
            (8, is_normal, Some(false)),
            (9, is_inf, Some(false)),
        ];
        for (bit, class, sign) in classes {
            if mask & (1 << bit) == 0 {
                continue;
            }
            let matched = match sign {
                None => class,
                Some(true) => self.b.ins().band(class, negative),
                Some(false) => self.b.ins().band(class, positive),
            };
            result = self.b.ins().bor(result, matched);
        }
        Ok(result)
    }
}

#[derive(Clone, Copy)]
enum Unary {
    Popcnt,
    Bswap,
    Bitrev,
    Abs,
    FAbs,
    Sqrt,
    Floor,
    Ceil,
    Trunc,
    Nearest,
}

#[derive(Clone, Copy)]
enum MinMax {
    SMax,
    SMin,
    UMax,
    UMin,
}
