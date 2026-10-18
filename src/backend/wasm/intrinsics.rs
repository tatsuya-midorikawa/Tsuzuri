//! The `llvm.*` intrinsics that generated code and the runtime use.
//!
//! An intrinsic is lowered from the types of its arguments, not from the suffix of its name, so a
//! vector overload is the scalar one on every lane. One that this file does not know is E2002; it
//! is never approximated.

use super::super::error::{Res, bail};
use super::super::func::{BinOp, CallArg, IPred};
use super::super::types::Ty;
use super::bin::{self, Vt, op};
use super::lower::{Atom, Lower, Val};
use super::object::Kind;
use super::ops::Width;

/// The name without `llvm.`, if it is `base` or `base.<suffix>`.
fn is(name: &str, base: &str) -> bool {
    name.strip_prefix(base)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// One step of a reduction of the lanes of a vector.
#[derive(Clone, Copy)]
enum Reduction {
    Binary(BinOp),
    /// The lane that the comparison says is better replaces the one that is kept.
    Pick(IPred),
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
        if is(name, "frameaddress") {
            // The stack pointer after the static frame, which the stack checks compare with the
            // bounds of the stack; only the current frame has one.
            let depth = arg(self, 0)?;
            if !matches!(self.atoms(&depth)?.as_slice(), [Atom::I32(0)]) {
                bail!("llvm.frameaddress of a frame other than the current one");
            }
            self.op(op::GLOBAL_GET);
            let sp = self.shared.sp_symbol;
            self.reloc(Kind::GlobalIndex, sp, 0);
            bin::uleb_padded(&mut self.code, 0);
            return Ok(Val::Atoms(vec![self.save(Vt::I32)]));
        }
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
                let atoms = self.lanes(args[0].ty, &[a, b], |lower, ty, parts| {
                    lower.saturating(kind, ty, &parts[0], &parts[1])
                })?;
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
                // The choice is made for each lane of a vector.
                let (x, y) = (self.atoms(&a)?, self.atoms(&b)?);
                let atoms = self.lanes(args[0].ty, &[x, y], |lower, ty, parts| {
                    let (x, y) = (Val::Atoms(parts[0].clone()), Val::Atoms(parts[1].clone()));
                    let better = lower.icmp(pred, ty, x.clone(), y.clone())?;
                    let picked = lower.select(super::super::types::I1, better, ty, x, y)?;
                    lower.atoms(&picked)
                })?;
                return Ok(Val::Atoms(atoms));
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
        // The reductions of the lanes of a vector of integers, folded from the first lane.
        for (base, step) in [
            ("vector.reduce.add", Reduction::Binary(BinOp::Add)),
            ("vector.reduce.mul", Reduction::Binary(BinOp::Mul)),
            ("vector.reduce.and", Reduction::Binary(BinOp::And)),
            ("vector.reduce.or", Reduction::Binary(BinOp::Or)),
            ("vector.reduce.xor", Reduction::Binary(BinOp::Xor)),
            ("vector.reduce.smax", Reduction::Pick(IPred::Sgt)),
            ("vector.reduce.smin", Reduction::Pick(IPred::Slt)),
            ("vector.reduce.umax", Reduction::Pick(IPred::Ugt)),
            ("vector.reduce.umin", Reduction::Pick(IPred::Ult)),
        ] {
            if is(name, base) {
                let a = arg(self, 0)?;
                let Some((count, elem)) = self.vector_shape(args[0].ty) else {
                    bail!("the intrinsic {full} of a value that is not a vector");
                };
                let atoms = self.atoms(&a)?;
                let width = self.flat(elem)?.len();
                let mut folded = atoms[..width].to_vec();
                for lane in 1..count as usize {
                    let next = atoms[lane * width..(lane + 1) * width].to_vec();
                    folded = match step {
                        Reduction::Binary(bop) => self.scalar_binary(bop, elem, &folded, &next)?,
                        Reduction::Pick(pred) => {
                            let better = self.icmp(
                                pred,
                                elem,
                                Val::Atoms(folded.clone()),
                                Val::Atoms(next.clone()),
                            )?;
                            let picked = self.select(
                                super::super::types::I1,
                                better,
                                elem,
                                Val::Atoms(folded),
                                Val::Atoms(next),
                            )?;
                            self.atoms(&picked)?
                        }
                    };
                }
                return Ok(Val::Atoms(folded));
            }
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
        for (base, f32_op, f64_op) in [
            ("maximum", op::F32_MAX, op::F64_MAX),
            ("minimum", op::F32_MIN, op::F64_MIN),
        ] {
            // The `fmax` and `fmin` of WebAssembly are the IEEE 754-2019 operations that these are:
            // a NaN operand gives a NaN, and -0 is less than +0.
            if is(name, base) {
                let (a, b) = (arg(self, 0)?, arg(self, 1)?);
                let (a, b) = (self.atoms(&a)?, self.atoms(&b)?);
                let atoms = self.lanes(args[0].ty, &[a, b], |lower, ty, parts| {
                    let single = ty == Ty::Float;
                    lower.push_all(&parts[0])?;
                    lower.push_all(&parts[1])?;
                    lower.op(if single { f32_op } else { f64_op });
                    Ok(vec![lower.save(if single { Vt::F32 } else { Vt::F64 })])
                })?;
                return Ok(Val::Atoms(atoms));
            }
        }
        if is(name, "fmuladd") {
            // The product and the sum may be fused or not; the instruction set has no fused one.
            let (a, b, c) = (arg(self, 0)?, arg(self, 1)?, arg(self, 2)?);
            let (a, b, c) = (self.atoms(&a)?, self.atoms(&b)?, self.atoms(&c)?);
            let atoms = self.lanes(args[0].ty, &[a, b, c], |lower, ty, parts| {
                let single = ty == Ty::Float;
                lower.push_all(&parts[0])?;
                lower.push_all(&parts[1])?;
                lower.op(if single { op::F32_MUL } else { op::F64_MUL });
                lower.push_all(&parts[2])?;
                lower.op(if single { op::F32_ADD } else { op::F64_ADD });
                Ok(vec![lower.save(if single { Vt::F32 } else { Vt::F64 })])
            })?;
            return Ok(Val::Atoms(atoms));
        }
        if is(name, "bswap") {
            let a = arg(self, 0)?;
            let value = self.atoms(&a)?;
            let atoms = self.lanes(args[0].ty, &[value], |lower, ty, parts| {
                lower.bswap(ty, &parts[0])
            })?;
            return Ok(Val::Atoms(atoms));
        }
        if is(name, "bitreverse") {
            let a = arg(self, 0)?;
            let value = self.atoms(&a)?;
            let atoms = self.lanes(args[0].ty, &[value], |lower, ty, parts| {
                lower.bitreverse(ty, &parts[0])
            })?;
            return Ok(Val::Atoms(atoms));
        }
        if is(name, "is.fpclass") {
            let a = arg(self, 0)?;
            let mask = match arg(self, 1)? {
                Val::Atoms(atoms) => match atoms.as_slice() {
                    [Atom::I32(mask)] => *mask as u32,
                    _ => bail!("llvm.is.fpclass with a mask that is not a constant"),
                },
                _ => bail!("llvm.is.fpclass with a mask that is not a constant"),
            };
            let value = self.atoms(&a)?;
            let atoms = self.lanes(args[0].ty, &[value], |lower, ty, parts| {
                lower.fpclass(ty, &parts[0], mask)
            })?;
            return Ok(Val::Atoms(atoms));
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
            let (Some(lo), Some(hi)) = (a.first(), a.get(1)) else {
                bail!("a 128-bit operand that is not two words");
            };
            // The negation, taken when the sign bit is set; the minimum value stays as it is.
            let negated = self.binary128(BinOp::Sub, &[Atom::I64(0), Atom::I64(0)], a)?;
            self.push(hi)?;
            self.i64_const(0);
            self.op(op::I64_LT_S);
            let negative = self.save(Vt::I32);
            let mut words = Vec::with_capacity(2);
            for (kept, flipped) in [(lo, &negated[0]), (hi, &negated[1])] {
                self.push(flipped)?;
                self.push(kept)?;
                self.push(&negative)?;
                self.op(op::SELECT);
                words.push(self.save(Vt::I64));
            }
            return Ok(words);
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
            Width::W128 => {
                let (Some(lo), Some(hi)) = (a.first(), a.get(1)) else {
                    bail!("a 128-bit operand that is not two words");
                };
                let count = |lower: &mut Self, word: &Atom, opcode: u8| -> Res<Atom> {
                    lower.push(word)?;
                    lower.op(opcode);
                    Ok(lower.save(Vt::I64))
                };
                let total = match which {
                    "ctpop" => {
                        let (low, high) = (
                            count(self, lo, op::I64_POPCNT)?,
                            count(self, hi, op::I64_POPCNT)?,
                        );
                        self.push(&low)?;
                        self.push(&high)?;
                        self.op(op::I64_ADD);
                        self.save(Vt::I64)
                    }
                    // The count of the high word when it has a bit set, else 64 and the low word's:
                    // the count of a word of zeros is 64.
                    _ => {
                        let (first, second, opcode) = if which == "ctlz" {
                            (hi, lo, op::I64_CLZ)
                        } else {
                            (lo, hi, op::I64_CTZ)
                        };
                        let near = count(self, first, opcode)?;
                        let far = count(self, second, opcode)?;
                        self.push(&near)?;
                        self.push(&far)?;
                        self.i64_const(64);
                        self.op(op::I64_ADD);
                        self.push(first)?;
                        self.i64_const(0);
                        self.op(op::I64_NE);
                        self.op(op::SELECT);
                        self.save(Vt::I64)
                    }
                };
                return Ok(vec![total, Atom::I64(0)]);
            }
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
            Width::W32(bits) => (i64::from(bits), false),
            Width::W64(bits) => (i64::from(bits), true),
            Width::W128 => bail!("a funnel shift of {}", self.types.describe(ty)),
        };
        let full = bits == 32 || bits == 64;
        if a == b && full {
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
        if bits & (bits - 1) == 0 {
            self.int_const(wide, bits - 1);
            self.op(and);
        } else {
            self.int_const(wide, bits);
            self.op(if wide { op::I64_REM_U } else { op::I32_REM_U });
        }
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
        // What is shifted above the width is not part of the result.
        self.canon(width);
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

    /// `x` of 32 bits with its bytes in the other order.
    fn bswap32(&mut self, x: &Atom) -> Res<Atom> {
        // The bytes 1 and 3 come from the rotation to the left, the bytes 0 and 2 from the one to the right.
        self.push(x)?;
        self.i32_const(8);
        self.op(op::I32_ROTL);
        self.i32_const(0x00ff_00ff);
        self.op(op::I32_AND);
        self.push(x)?;
        self.i32_const(8);
        self.op(op::I32_ROTR);
        self.i32_const(0xff00_ff00u32 as i32);
        self.op(op::I32_AND);
        self.op(op::I32_OR);
        Ok(self.save(Vt::I32))
    }

    /// `x` of 64 bits with its bytes in the other order: each half swapped, and the halves exchanged.
    fn bswap64(&mut self, x: &Atom) -> Res<Atom> {
        self.push(x)?;
        self.op(op::I32_WRAP_I64);
        let low = self.save(Vt::I32);
        self.push(x)?;
        self.i64_const(32);
        self.op(op::I64_SHR_U);
        self.op(op::I32_WRAP_I64);
        let high = self.save(Vt::I32);
        let (swapped_low, swapped_high) = (self.bswap32(&low)?, self.bswap32(&high)?);
        self.push(&swapped_low)?;
        self.op(op::I64_EXTEND_I32_U);
        self.i64_const(32);
        self.op(op::I64_SHL);
        self.push(&swapped_high)?;
        self.op(op::I64_EXTEND_I32_U);
        self.op(op::I64_OR);
        Ok(self.save(Vt::I64))
    }

    fn bswap(&mut self, ty: Ty, a: &[Atom]) -> Res<Vec<Atom>> {
        Ok(match self.width(ty)? {
            Width::W32(8) => a.to_vec(),
            Width::W32(16) => {
                self.push_all(a)?;
                self.i32_const(0xff);
                self.op(op::I32_AND);
                self.i32_const(8);
                self.op(op::I32_SHL);
                self.push_all(a)?;
                self.i32_const(8);
                self.op(op::I32_SHR_U);
                self.op(op::I32_OR);
                vec![self.save(Vt::I32)]
            }
            Width::W32(32) => vec![self.bswap32(&a[0])?],
            Width::W64(64) => vec![self.bswap64(&a[0])?],
            Width::W128 => {
                let (Some(lo), Some(hi)) = (a.first(), a.get(1)) else {
                    bail!("a 128-bit operand that is not two words");
                };
                vec![self.bswap64(hi)?, self.bswap64(lo)?]
            }
            _ => bail!("a byte swap of {}", self.types.describe(ty)),
        })
    }

    /// The bits of the 32 or 64 of `x` in the other order.
    fn reverse_bits(&mut self, x: &Atom, wide: bool) -> Res<Atom> {
        let mut value = x.clone();
        for (shift, mask) in [
            (1, 0x5555_5555_5555_5555u64),
            (2, 0x3333_3333_3333_3333),
            (4, 0x0f0f_0f0f_0f0f_0f0f),
        ] {
            // The groups of `shift` bits swap places with their neighbors.
            let mask = if wide {
                mask as i64
            } else {
                i64::from(mask as u32)
            };
            self.push(&value)?;
            self.int_const(wide, shift);
            self.op(if wide { op::I64_SHR_U } else { op::I32_SHR_U });
            self.int_const(wide, mask);
            self.op(if wide { op::I64_AND } else { op::I32_AND });
            self.push(&value)?;
            self.int_const(wide, mask);
            self.op(if wide { op::I64_AND } else { op::I32_AND });
            self.int_const(wide, shift);
            self.op(if wide { op::I64_SHL } else { op::I32_SHL });
            self.op(if wide { op::I64_OR } else { op::I32_OR });
            value = self.save(if wide { Vt::I64 } else { Vt::I32 });
        }
        if wide {
            self.bswap64(&value)
        } else {
            self.bswap32(&value)
        }
    }

    fn bitreverse(&mut self, ty: Ty, a: &[Atom]) -> Res<Vec<Atom>> {
        Ok(match self.width(ty)? {
            Width::W128 => {
                let (Some(lo), Some(hi)) = (a.first(), a.get(1)) else {
                    bail!("a 128-bit operand that is not two words");
                };
                vec![self.reverse_bits(hi, true)?, self.reverse_bits(lo, true)?]
            }
            Width::W32(bits) => {
                let reversed = self.reverse_bits(&a[0], false)?;
                if bits == 32 {
                    vec![reversed]
                } else {
                    // The bits of the narrow value went to the top of the register.
                    self.push(&reversed)?;
                    self.i32_const(i32::from(32 - bits));
                    self.op(op::I32_SHR_U);
                    vec![self.save(Vt::I32)]
                }
            }
            Width::W64(bits) => {
                let reversed = self.reverse_bits(&a[0], true)?;
                if bits == 64 {
                    vec![reversed]
                } else {
                    self.push(&reversed)?;
                    self.i64_const(i64::from(64 - bits));
                    self.op(op::I64_SHR_U);
                    vec![self.save(Vt::I64)]
                }
            }
        })
    }

    /// `llvm.is.fpclass`: whether the float is in one of the classes of `mask` (1 signaling NaN,
    /// 2 quiet NaN, 4 -infinity, 8 -normal, 16 -subnormal, 32 -zero, 64 +zero, 128 +subnormal,
    /// 256 +normal, 512 +infinity), from its bits.
    fn fpclass(&mut self, ty: Ty, x: &[Atom], mask: u32) -> Res<Vec<Atom>> {
        let single = match ty {
            Ty::Float => true,
            Ty::Double => false,
            other => bail!("is.fpclass of {}", self.types.describe(other)),
        };
        // The fields of the format, in the width of its integer.
        let (exponent_shift, exponent_all, fraction_all, quiet): (i64, i64, i64, i64) = if single {
            (23, 0xff, 0x7f_ffff, 0x40_0000)
        } else {
            (52, 0x7ff, (1 << 52) - 1, 1 << 51)
        };
        self.push_all(x)?;
        self.op(if single {
            op::I32_REINTERPRET_F32
        } else {
            op::I64_REINTERPRET_F64
        });
        let bits = self.save(if single { Vt::I32 } else { Vt::I64 });
        let wide = !single;
        let (and, eq, shr) = if wide {
            (op::I64_AND, op::I64_EQ, op::I64_SHR_U)
        } else {
            (op::I32_AND, op::I32_EQ, op::I32_SHR_U)
        };
        // Each of these is an `i32` that is 1 or 0.
        self.push(&bits)?;
        self.int_const(wide, if single { 31 } else { 63 });
        self.op(shr);
        if wide {
            self.op(op::I32_WRAP_I64);
        }
        let negative = self.save(Vt::I32);
        self.push(&bits)?;
        self.int_const(wide, exponent_shift);
        self.op(shr);
        self.int_const(wide, exponent_all);
        self.op(and);
        let exponent = self.save(if wide { Vt::I64 } else { Vt::I32 });
        self.push(&bits)?;
        self.int_const(wide, fraction_all);
        self.op(and);
        let fraction = self.save(if wide { Vt::I64 } else { Vt::I32 });
        let test = |lower: &mut Self, value: &Atom, constant: i64| -> Res<Atom> {
            lower.push(value)?;
            lower.int_const(wide, constant);
            lower.op(eq);
            Ok(lower.save(Vt::I32))
        };
        let exponent_is_all = test(self, &exponent, exponent_all)?;
        let exponent_is_zero = test(self, &exponent, 0)?;
        let fraction_is_zero = test(self, &fraction, 0)?;
        // `a & b` of two flags, or `a & !b`.
        let both = |lower: &mut Self, a: &Atom, b: &Atom, not_b: bool| -> Res<Atom> {
            lower.push(a)?;
            lower.push(b)?;
            if not_b {
                lower.op(op::I32_EQZ);
            }
            lower.op(op::I32_AND);
            Ok(lower.save(Vt::I32))
        };
        let nan = both(self, &exponent_is_all, &fraction_is_zero, true)?;
        let infinite = both(self, &exponent_is_all, &fraction_is_zero, false)?;
        let zero = both(self, &exponent_is_zero, &fraction_is_zero, false)?;
        let subnormal = both(self, &exponent_is_zero, &fraction_is_zero, true)?;
        // Neither all ones nor zero in the exponent.
        self.push(&exponent_is_all)?;
        self.push(&exponent_is_zero)?;
        self.op(op::I32_OR);
        self.op(op::I32_EQZ);
        let normal = self.save(Vt::I32);
        // The fraction has its top bit set: a quiet NaN.
        self.push(&fraction)?;
        self.int_const(wide, quiet);
        self.op(and);
        self.int_const(wide, 0);
        self.op(if wide { op::I64_NE } else { op::I32_NE });
        let quiet_bit = self.save(Vt::I32);
        let signaling = both(self, &nan, &quiet_bit, true)?;
        let quiet_nan = both(self, &nan, &quiet_bit, false)?;
        let mut result = Atom::I32(0);
        for (class_mask, class, sign) in [
            (1u32, &signaling, None),
            (2, &quiet_nan, None),
            (4, &infinite, Some(true)),
            (8, &normal, Some(true)),
            (16, &subnormal, Some(true)),
            (32, &zero, Some(true)),
            (64, &zero, Some(false)),
            (128, &subnormal, Some(false)),
            (256, &normal, Some(false)),
            (512, &infinite, Some(false)),
        ] {
            if mask & class_mask == 0 {
                continue;
            }
            self.push(&result)?;
            self.push(class)?;
            if let Some(wanted) = sign {
                self.push(&negative)?;
                if !wanted {
                    self.op(op::I32_EQZ);
                }
                self.op(op::I32_AND);
            }
            self.op(op::I32_OR);
            result = self.save(Vt::I32);
        }
        Ok(vec![result])
    }

    /// The saturating sum or difference of two 128-bit integers.
    fn saturating128(&mut self, kind: Overflow, a: &[Atom], b: &[Atom]) -> Res<Vec<Atom>> {
        let (Some(a_hi), Some(_)) = (a.get(1), a.first()) else {
            bail!("a 128-bit operand that is not two words");
        };
        let (value, flag) = self.with_overflow128(kind, a, b)?;
        // The limit that an overflow gives: the bounds of the type, the one on the side of the first
        // operand's sign for a signed operation.
        let (limit_low, limit_high) = match kind {
            Overflow::UAdd => (Atom::I64(-1), Atom::I64(-1)),
            Overflow::USub => (Atom::I64(0), Atom::I64(0)),
            _ => {
                self.push(a_hi)?;
                self.i64_const(0);
                self.op(op::I64_LT_S);
                let negative = self.save(Vt::I32);
                let limit = |lower: &mut Self, below: i64, above: i64| -> Res<Atom> {
                    lower.i64_const(below);
                    lower.i64_const(above);
                    lower.push(&negative)?;
                    lower.op(op::SELECT);
                    Ok(lower.save(Vt::I64))
                };
                (limit(self, 0, -1)?, limit(self, i64::MIN, i64::MAX)?)
            }
        };
        let mut words = Vec::with_capacity(2);
        for (limit, kept) in [(&limit_low, &value[0]), (&limit_high, &value[1])] {
            self.push(limit)?;
            self.push(kept)?;
            self.push(&flag)?;
            self.op(op::SELECT);
            words.push(self.save(Vt::I64));
        }
        Ok(words)
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
            Width::W128 => return self.saturating128(kind, a, b),
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
