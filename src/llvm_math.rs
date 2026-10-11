use super::*;

impl FunctionEmitter<'_, '_> {
    fn math_intrinsic(&mut self, name: &str, ty: &Type, values: &[&str]) -> String {
        let Type::Binary(bits) = ty else {
            unreachable!()
        };
        let lowered = self.ty(ty);
        let signature = vec![lowered.clone(); values.len()].join(", ");
        self.intrinsics.insert(format!(
            "declare {lowered} @llvm.{name}.f{bits}({signature})"
        ));
        let arguments = values
            .iter()
            .map(|value| format!("{lowered} {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.value(format!("call {lowered} @llvm.{name}.f{bits}({arguments})"))
    }

    fn math_bits(&mut self, ty: &Type, value: &str) -> (u16, String) {
        let bits = match ty {
            Type::Binary(bits) | Type::Decimal(bits) => *bits,
            _ => unreachable!(),
        };
        let value = if matches!(ty, Type::Binary(32 | 64)) {
            self.value(format!("bitcast {} {value} to i{bits}", self.ty(ty)))
        } else {
            value.into()
        };
        (bits, value)
    }

    fn math_from_bits(&mut self, ty: &Type, value: &str) -> String {
        if matches!(ty, Type::Binary(32 | 64)) {
            let Type::Binary(bits) = ty else {
                unreachable!()
            };
            self.value(format!("bitcast i{bits} {value} to {}", self.ty(ty)))
        } else {
            value.into()
        }
    }

    fn math_minmax(&mut self, ty: &Type, left: &str, right: &str, maximum: bool) -> String {
        if matches!(ty, Type::Binary(32 | 64)) {
            return self.math_intrinsic(
                if maximum { "maximum" } else { "minimum" },
                ty,
                &[left, right],
            );
        }
        let left = self.spill(ty, left);
        let right = self.spill(ty, right);
        let output = self.slot(ty);
        self.instruction(format!(
            "call void @tz_soft_math_binary(ptr {output}, ptr {left}, ptr {right}, i32 {}, i32 {})",
            numeric_kind(ty),
            u8::from(maximum)
        ));
        self.value(format!("load {}, ptr {output}", self.ty(ty)))
    }

    pub(super) fn math_builtin(&mut self, instance: &BuiltinInstance) -> String {
        use Builtin::*;
        let ty = &instance.types[0];
        let lowered = self.ty(ty);
        let operation = instance.builtin;
        if matches!(
            operation,
            MathSin
                | MathCos
                | MathTan
                | MathAsin
                | MathAcos
                | MathAtan
                | MathAtan2
                | MathExp
                | MathExp2
                | MathLog
                | MathLog2
                | MathLog10
                | MathPow
                | MathCbrt
                | MathHypot
        ) {
            let Type::Binary(bits) = ty else {
                unreachable!()
            };
            let name = operation.name().strip_prefix("Math.").unwrap();
            let arguments = (0..operation.scheme().parameters.len())
                .map(|index| format!("{lowered} %arg{index}"))
                .collect::<Vec<_>>()
                .join(", ");
            return self.value(format!(
                "call {lowered} @tz_math_{name}_f{bits}({arguments})"
            ));
        }
        if matches!(operation, MathPi | MathE) {
            return crate::numeric::float_literal(if operation == MathPi {
                "3.141592653589793238462643383279502884197169399375105820974944592307816406286208998628034825342117067982148086513282306647"
            } else {
                "2.718281828459045235360287471352662497757247093699959574966967627724076630353547594571382178525166427427466391932003059922"
            }, ty, Span::default()).unwrap();
        }
        if operation == MathFma {
            if crate::target::current_platform().hardware_fma()
                && !self.globals.wasm
                && matches!(ty, Type::Binary(32 | 64))
            {
                return self.math_intrinsic("fma", ty, &["%arg0", "%arg1", "%arg2"]);
            }
            let left = self.spill(ty, "%arg0");
            let right = self.spill(ty, "%arg1");
            let addend = self.spill(ty, "%arg2");
            let output = self.slot(ty);
            self.instruction(format!("call void @tz_soft_fma(ptr {output}, ptr {left}, ptr {right}, ptr {addend}, i32 {})", numeric_kind(ty)));
            return self.value(format!("load {lowered}, ptr {output}"));
        }
        if matches!(
            operation,
            MathAbs | MathCopysign | MathIsNan | MathIsInfinite | MathIsFinite
        ) {
            let (bits, value) = self.math_bits(ty, "%arg0");
            let magnitude = self.value(format!(
                "and i{bits} {value}, {}",
                (1u128 << (bits - 1)) - 1
            ));
            if operation == MathAbs {
                return self.math_from_bits(ty, &magnitude);
            }
            if operation == MathCopysign {
                let (_, other) = self.math_bits(ty, "%arg1");
                let sign = self.value(format!("and i{bits} {other}, {}", 1u128 << (bits - 1)));
                let result = self.value(format!("or i{bits} {magnitude}, {sign}"));
                return self.math_from_bits(ty, &result);
            }
            let (classified, infinity) = if matches!(ty, Type::Decimal(_)) {
                (
                    self.value(format!("lshr i{bits} {magnitude}, {}", bits - 6)),
                    30,
                )
            } else {
                let fraction = match bits {
                    16 => 10,
                    32 => 23,
                    64 => 52,
                    128 => 112,
                    _ => unreachable!(),
                };
                (
                    magnitude,
                    ((1u128 << (bits - 1)) - 1) & !((1u128 << fraction) - 1),
                )
            };
            let predicate = match operation {
                MathIsNan => "ugt",
                MathIsInfinite => "eq",
                MathIsFinite => "ult",
                _ => unreachable!(),
            };
            return self.value(format!("icmp {predicate} i{bits} {classified}, {infinity}"));
        }
        if matches!(operation, MathMin | MathMax) {
            return self.math_minmax(ty, "%arg0", "%arg1", operation == MathMax);
        }
        if operation == MathClamp {
            let valid = if matches!(ty, Type::Binary(32 | 64)) {
                self.value(format!("fcmp ole {lowered} %arg1, %arg2"))
            } else {
                let left = self.spill(ty, "%arg1");
                let right = self.spill(ty, "%arg2");
                let order = self.value(format!(
                    "call i32 @tz_soft_cmp(ptr {left}, ptr {right}, i32 {})",
                    numeric_kind(ty)
                ));
                self.value(format!("icmp sle i32 {order}, 0"))
            };
            let largest = self.math_minmax(ty, "%arg0", "%arg1", true);
            let result = self.math_minmax(ty, &largest, "%arg2", false);
            let bits = match ty {
                Type::Binary(bits) | Type::Decimal(bits) => *bits,
                _ => unreachable!(),
            };
            let nan = if matches!(ty, Type::Decimal(_)) {
                (31u128 << (bits - 6)).to_string()
            } else if matches!(ty, Type::Binary(32 | 64)) {
                "0x7FF8000000000000".into()
            } else {
                let fraction = if bits == 16 { 10 } else { 112 };
                ((((1u128 << (bits - 1)) - 1) & !((1u128 << fraction) - 1))
                    | (1u128 << (fraction - 1)))
                    .to_string()
            };
            return self.value(format!(
                "select i1 {valid}, {lowered} {result}, {lowered} {nan}"
            ));
        }
        if matches!(ty, Type::Binary(32 | 64)) {
            let intrinsic = match operation {
                MathSqrt => "sqrt",
                MathFloor => "floor",
                MathCeil => "ceil",
                MathTrunc | MathRound | MathRoundEven => "trunc",
                _ => unreachable!(),
            };
            let truncated = self.math_intrinsic(intrinsic, ty, &["%arg0"]);
            if !matches!(operation, MathRound | MathRoundEven) {
                return truncated;
            }
            let fraction = self.value(format!("fsub {lowered} %arg0, {truncated}"));
            let absolute = self.math_intrinsic("fabs", ty, &[&fraction]);
            let mut increment = self.value(format!("fcmp ogt {lowered} {absolute}, 0.5"));
            let half = self.value(format!("fcmp oeq {lowered} {absolute}, 0.5"));
            let tie = if operation == MathRound {
                half
            } else {
                let half_integral = self.value(format!("fmul {lowered} {truncated}, 0.5"));
                let truncated_half = self.math_intrinsic("trunc", ty, &[&half_integral]);
                let odd = self.value(format!(
                    "fcmp one {lowered} {half_integral}, {truncated_half}"
                ));
                self.value(format!("and i1 {half}, {odd}"))
            };
            increment = self.value(format!("or i1 {increment}, {tie}"));
            let negative = self.value(format!("fcmp olt {lowered} %arg0, 0.0"));
            let step = self.value(format!(
                "select i1 {negative}, {lowered} -1.0, {lowered} 1.0"
            ));
            let rounded = self.value(format!("fadd {lowered} {truncated}, {step}"));
            return self.value(format!(
                "select i1 {increment}, {lowered} {rounded}, {lowered} {truncated}"
            ));
        }
        if operation == MathSqrt && matches!(ty, Type::Binary(16)) && self.globals.f16.arithmetic()
        {
            return self.half_sqrt("%arg0");
        }
        let opcode = match operation {
            MathSqrt => 0,
            MathFloor => 1,
            MathCeil => 2,
            MathTrunc => 3,
            MathRound => 4,
            MathRoundEven => 5,
            _ => unreachable!(),
        };
        let input = self.spill(ty, "%arg0");
        let output = self.slot(ty);
        self.instruction(format!(
            "call void @tz_soft_math_unary(ptr {output}, ptr {input}, i32 {}, i32 {opcode})",
            numeric_kind(ty)
        ));
        self.value(format!("load {lowered}, ptr {output}"))
    }
}

/// f16 as LLVM `half` (D10).
///
/// An f16 stays an `i16` between operations, so the storage, the ABI, and the debug information
/// are those of the software path. An operation bitcasts to `half`, computes, and bitcasts back.
/// Each is one IEEE 754 operation on `half` with the default rounding, and no instruction carries
/// a fast-math flag, so no result is fused or reassociated. A target without half arithmetic gets
/// one binary32 operation between exact conversions for each, which rounds once for `+ - * /` and
/// `sqrt`: binary32 has 24 bits, at least 2 × 11 + 2.
///
/// The software runtime (`numeric.c`) returns a canonical NaN and LLVM leaves the sign and the
/// payload of a NaN result open, so every NaN result is replaced by the bits the runtime returns,
/// taken from the operands' bits and never from the result's. A result that is not a NaN is the
/// IEEE result in both paths.
impl FunctionEmitter<'_, '_> {
    /// Whether the f16 bits `bits` are a NaN: a magnitude above that of infinity, `0x7C00`.
    fn half_is_nan(&mut self, bits: &str) -> String {
        let magnitude = self.value(format!("and i16 {bits}, 32767"));
        self.value(format!("icmp ugt i16 {magnitude}, 31744"))
    }

    /// The quiet NaN `0x7E00` with the sign bit of the f16 bits `sign_bits`.
    fn half_canonical_nan(&mut self, sign_bits: &str) -> String {
        let sign = self.value(format!("and i16 {sign_bits}, -32768"));
        self.value(format!("or i16 {sign}, 32256"))
    }

    /// `left op right` for f16 bits: the result bits, or an `i1` for a comparison. A NaN result
    /// is `0x7E00` with the sign of `left` for `+ -` and of `left xor right` for `* /`.
    pub(super) fn half_binary(&mut self, operator: BinaryOp, left: &str, right: &str) -> String {
        use BinaryOp::*;
        let a = self.value(format!("bitcast i16 {left} to half"));
        let b = self.value(format!("bitcast i16 {right} to half"));
        let (instruction, product) = match operator {
            Add => ("fadd", false),
            Subtract => ("fsub", false),
            Multiply => ("fmul", true),
            Divide => ("fdiv", true),
            Equal => return self.value(format!("fcmp oeq half {a}, {b}")),
            NotEqual => return self.value(format!("fcmp une half {a}, {b}")),
            Less => return self.value(format!("fcmp olt half {a}, {b}")),
            LessEqual => return self.value(format!("fcmp ole half {a}, {b}")),
            Greater => return self.value(format!("fcmp ogt half {a}, {b}")),
            GreaterEqual => return self.value(format!("fcmp oge half {a}, {b}")),
            _ => unreachable!("half operator checked"),
        };
        let result = self.value(format!("{instruction} half {a}, {b}"));
        let bits = self.value(format!("bitcast half {result} to i16"));
        let nan = self.half_is_nan(&bits);
        let signs = if product {
            self.value(format!("xor i16 {left}, {right}"))
        } else {
            left.to_owned()
        };
        let canonical = self.half_canonical_nan(&signs);
        self.value(format!("select i1 {nan}, i16 {canonical}, i16 {bits}"))
    }

    /// `Math.sqrt` of f16 bits. A NaN operand comes back as it is, a signaling NaN included, and
    /// the NaN of a negative operand is the positive `0x7E00`.
    pub(super) fn half_sqrt(&mut self, input: &str) -> String {
        self.intrinsics
            .insert("declare half @llvm.sqrt.f16(half)".into());
        let value = self.value(format!("bitcast i16 {input} to half"));
        let root = self.value(format!("call half @llvm.sqrt.f16(half {value})"));
        let bits = self.value(format!("bitcast half {root} to i16"));
        let nan = self.half_is_nan(&bits);
        let result = self.value(format!("select i1 {nan}, i16 32256, i16 {bits}"));
        let input_nan = self.half_is_nan(input);
        self.value(format!("select i1 {input_nan}, i16 {input}, i16 {result}"))
    }

    /// `value as to` between f16 and f32 or f64 when the path computes it in `half`, otherwise
    /// `None`. A NaN input gives the canonical quiet NaN of the target with the input's sign, and
    /// the payload is dropped.
    pub(super) fn half_cast(&mut self, from: &Type, to: &Type, value: &str) -> Option<String> {
        let path = self.globals.f16;
        match (from, to) {
            (Type::Binary(16), Type::Binary(bits @ (32 | 64))) if path.widens() => {
                Some(self.half_widen(value, *bits))
            }
            (Type::Binary(bits @ (32 | 64)), Type::Binary(16)) if path.narrows_from(*bits) => {
                Some(self.half_narrow(value, *bits))
            }
            _ => None,
        }
    }

    fn half_widen(&mut self, value: &str, bits: u16) -> String {
        let (float, integer, shift, quiet_nan) = if bits == 32 {
            ("float", "i32", 16, 0x7FC0_0000_u64)
        } else {
            ("double", "i64", 48, 0x7FF8_0000_0000_0000)
        };
        let half = self.value(format!("bitcast i16 {value} to half"));
        let wide = self.value(format!("fpext half {half} to {float}"));
        let nan = self.half_is_nan(value);
        let sign = self.value(format!("and i16 {value}, -32768"));
        let sign = self.value(format!("zext i16 {sign} to {integer}"));
        let sign = self.value(format!("shl {integer} {sign}, {shift}"));
        let canonical = self.value(format!("or {integer} {sign}, {quiet_nan}"));
        let canonical = self.value(format!("bitcast {integer} {canonical} to {float}"));
        self.value(format!(
            "select i1 {nan}, {float} {canonical}, {float} {wide}"
        ))
    }

    fn half_narrow(&mut self, value: &str, bits: u16) -> String {
        let (float, integer) = if bits == 32 {
            ("float", "i32")
        } else {
            ("double", "i64")
        };
        let half = self.value(format!("fptrunc {float} {value} to half"));
        let narrow = self.value(format!("bitcast half {half} to i16"));
        let nan = self.value(format!("fcmp uno {float} {value}, {value}"));
        let raw = self.value(format!("bitcast {float} {value} to {integer}"));
        let top = self.value(format!("lshr {integer} {raw}, {}", bits - 16));
        let top = self.value(format!("trunc {integer} {top} to i16"));
        let canonical = self.half_canonical_nan(&top);
        self.value(format!("select i1 {nan}, i16 {canonical}, i16 {narrow}"))
    }
}
