use super::*;
use crate::simd::{SimdKind, SimdType};

impl FunctionEmitter<'_, '_> {
    fn vector_constant(&self, vector: SimdType, value: &str) -> String {
        let element = self.ty(&vector.element());
        format!(
            "<{}>",
            vec![format!("{element} {value}"); usize::from(vector.lanes())].join(", ")
        )
    }

    pub(super) fn simd_unary(
        &mut self,
        operator: UnaryOp,
        vector: SimdType,
        value: &str,
    ) -> String {
        let ty = self.ty(&Type::Simd(vector));
        if operator == UnaryOp::Negate {
            self.value(if vector.kind == SimdKind::Float {
                format!("fneg {ty} {value}")
            } else {
                format!("sub {ty} zeroinitializer, {value}")
            })
        } else {
            let ones = self.vector_constant(
                vector,
                if vector.kind == SimdKind::Mask {
                    "true"
                } else {
                    "-1"
                },
            );
            self.value(format!("xor {ty} {value}, {ones}"))
        }
    }

    pub(super) fn simd_binary(
        &mut self,
        operator: BinaryOp,
        vector: SimdType,
        left: &str,
        right: &str,
    ) -> String {
        use BinaryOp::*;
        let ty = self.ty(&Type::Simd(vector));
        let float = vector.kind == SimdKind::Float;
        let opcode = match operator {
            Add => {
                if float {
                    "fadd"
                } else {
                    "add"
                }
            }
            Subtract => {
                if float {
                    "fsub"
                } else {
                    "sub"
                }
            }
            Multiply => {
                if float {
                    "fmul"
                } else {
                    "mul"
                }
            }
            Divide => "fdiv",
            BitAnd => "and",
            BitOr => "or",
            BitXor => "xor",
            ShiftLeft => "shl",
            ShiftRight if vector.kind == SimdKind::Signed => "ashr",
            ShiftRight | ShiftRightUnsigned => "lshr",
            _ => unreachable!("SIMD arithmetic validated"),
        };
        let right = if matches!(operator, ShiftLeft | ShiftRight | ShiftRightUnsigned) {
            let bits = if vector.kind == SimdKind::Mask {
                1
            } else {
                vector.bits
            };
            let counts = self.vector_constant(vector, &(bits - 1).to_string());
            self.value(format!("and {ty} {right}, {counts}"))
        } else {
            right.to_owned()
        };
        self.value(format!("{opcode} {ty} {left}, {right}"))
    }

    pub(super) fn simd_builtin(&mut self, instance: &BuiltinInstance) -> String {
        use Builtin::*;
        let Type::Simd(vector) = instance.types[0] else {
            unreachable!("SIMD marker and family validated");
        };
        let ty = self.ty(&Type::Simd(vector));
        let element = vector.element();
        let scalar = self.ty(&element);
        let lanes = vector.lanes();
        match instance.builtin {
            SimdSplat => {
                let first = self.value(format!("insertelement {ty} poison, {scalar} %arg0, i32 0"));
                self.value(format!(
                    "shufflevector {ty} {first}, {ty} poison, <{lanes} x i32> zeroinitializer"
                ))
            }
            SimdOfLanes2 | SimdOfLanes4 | SimdOfLanes8 | SimdOfLanes16 => {
                let mut result = "poison".to_owned();
                for index in 0..lanes {
                    result = self.value(format!(
                        "insertelement {ty} {result}, {scalar} %arg{index}, i32 {index}"
                    ));
                }
                result
            }
            SimdExtract | SimdReplace => {
                let valid = self.value(format!("icmp ult i64 %arg1, {lanes}"));
                self.guard(&valid, TrapKind::BoundsCheck);
                if instance.builtin == SimdExtract {
                    self.value(format!("extractelement {ty} %arg0, i64 %arg1"))
                } else {
                    self.value(format!(
                        "insertelement {ty} %arg0, {scalar} %arg2, i64 %arg1"
                    ))
                }
            }
            SimdLoad => {
                let length = self.value("extractvalue %tz.array %arg0, 1");
                let enough = self.value(format!("icmp uge i64 {length}, {lanes}"));
                let maximum = self.value(format!("sub i64 {length}, {lanes}"));
                let offset = self.value(format!("icmp ule i64 %arg1, {maximum}"));
                let valid = self.value(format!("and i1 {enough}, {offset}"));
                self.guard(&valid, TrapKind::BoundsCheck);
                let data = self.value("extractvalue %tz.array %arg0, 0");
                let pointer = self.value(format!(
                    "getelementptr inbounds {scalar}, ptr {data}, i64 %arg1"
                ));
                self.value(format!("load {ty}, ptr {pointer}, align 1"))
            }
            SimdSum | SimdAll | SimdAny => {
                let mut total = self.value(format!("extractelement {ty} %arg0, i32 0"));
                let opcode = match instance.builtin {
                    SimdAll => "and",
                    SimdAny => "or",
                    _ if vector.kind == SimdKind::Float => "fadd",
                    _ => "add",
                };
                for index in 1..lanes {
                    let next = self.value(format!("extractelement {ty} %arg0, i32 {index}"));
                    total = self.value(format!("{opcode} {scalar} {total}, {next}"));
                }
                total
            }
            SimdSelect => {
                let mask = self.ty(&Type::Simd(vector.mask()));
                self.value(format!("select {mask} %arg0, {ty} %arg1, {ty} %arg2"))
            }
            operation => {
                let float = vector.kind == SimdKind::Float;
                let prefix = if float {
                    "o"
                } else if vector.kind == SimdKind::Signed {
                    "s"
                } else {
                    "u"
                };
                let predicate = match operation {
                    SimdEq => {
                        if float {
                            "oeq".to_owned()
                        } else {
                            "eq".to_owned()
                        }
                    }
                    SimdNe => {
                        if float {
                            "une".to_owned()
                        } else {
                            "ne".to_owned()
                        }
                    }
                    SimdLt => format!("{prefix}lt"),
                    SimdLe => format!("{prefix}le"),
                    SimdGt => format!("{prefix}gt"),
                    SimdGe => format!("{prefix}ge"),
                    _ => unreachable!("SIMD builtin"),
                };
                self.value(format!(
                    "{} {predicate} {ty} %arg0, %arg1",
                    if float { "fcmp" } else { "icmp" }
                ))
            }
        }
    }
}
