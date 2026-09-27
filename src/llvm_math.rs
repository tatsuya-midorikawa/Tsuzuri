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
