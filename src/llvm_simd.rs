use super::*;
use crate::simd::{SimdKind, SimdType};

/// The LLVM types of the 256-bit vectors (F08 Phase 2).
const WIDE_VECTORS: [&str; 6] = [
    "<32 x i8>",
    "<16 x i16>",
    "<8 x i32>",
    "<4 x i64>",
    "<8 x float>",
    "<4 x double>",
];

/// LLVM aligns a 256-bit vector to 32 bytes, but the heap, array elements, frames, and union
/// payloads guarantee 16. Every load and store of a type that holds such a vector states
/// `align 16` or less, so no access assumes more alignment than its storage has.
pub(super) fn with_vector_alignment(ir: String) -> String {
    if !WIDE_VECTORS.iter().any(|vector| ir.contains(vector)) {
        return ir;
    }
    let wide = wide_types(&ir);
    let mut output = String::with_capacity(ir.len());
    for line in ir.split_inclusive('\n') {
        match aligned_access(line, &wide) {
            Some(aligned) => output.push_str(&aligned),
            None => output.push_str(line),
        }
    }
    output
}

/// The named types of `ir` that hold a wide vector directly or through other named types.
pub(super) fn wide_types(ir: &str) -> BTreeSet<&str> {
    let mut wide = BTreeSet::new();
    if !WIDE_VECTORS.iter().any(|vector| ir.contains(vector)) {
        return wide;
    }
    let definitions: Vec<(&str, &str)> = ir
        .lines()
        .filter_map(|line| {
            let (name, body) = line.split_once(" = type ")?;
            name.starts_with('%').then_some((name, body))
        })
        .collect();
    loop {
        let before = wide.len();
        for (name, body) in &definitions {
            if !wide.contains(name) && holds_wide_vector(body, &wide) {
                wide.insert(*name);
            }
        }
        if wide.len() == before {
            return wide;
        }
    }
}

/// Whether the IR text names a wide vector type or one of the `wide` named types.
pub(super) fn holds_wide_vector(ty: &str, wide: &BTreeSet<&str>) -> bool {
    WIDE_VECTORS.iter().any(|vector| ty.contains(vector))
        || wide.iter().any(|name| {
            ty.match_indices(name).any(|(at, _)| {
                !ty[at + name.len()..]
                    .starts_with(|c: char| c.is_ascii_alphanumeric() || "._$-\"".contains(c))
            })
        })
}

/// The line with `align 16` when it loads or stores a type that holds a wide vector without
/// already stating an alignment of at most 16.
fn aligned_access(line: &str, wide: &BTreeSet<&str>) -> Option<String> {
    let trimmed = line.trim_start();
    let accessed = match trimmed.strip_prefix("store ") {
        Some(rest) => rest,
        None => trimmed.split_once(" = load ")?.1,
    };
    if !holds_wide_vector(leading_type(accessed)?, wide) {
        return None;
    }
    let (body, newline) = match line.strip_suffix('\n') {
        Some(body) => (body, "\n"),
        None => (line, ""),
    };
    let (code, metadata) = body.split_at(body.find(", !").unwrap_or(body.len()));
    let code = match code.rsplit_once(", align ") {
        Some((_, align)) if align.parse::<u64>().is_ok_and(|align| align <= 16) => return None,
        Some((head, _)) => head,
        None => code,
    };
    Some(format!("{code}, align 16{metadata}{newline}"))
}

/// The type at the start of a load or store operand list.
fn leading_type(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut quoted = false;
    for (at, character) in text.char_indices() {
        match character {
            '"' => quoted = !quoted,
            _ if quoted => {}
            '<' | '{' | '[' | '(' => depth += 1,
            '>' | '}' | ']' | ')' => depth = depth.checked_sub(1)?,
            ' ' | ',' if depth == 0 => return Some(&text[..at]),
            _ => {}
        }
    }
    None
}

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
            SimdOfLanes2 | SimdOfLanes4 | SimdOfLanes8 | SimdOfLanes16 | SimdOfLanes32 => {
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
            SimdLoad | SimdStore => {
                // Every lane is checked before the access, so a store never writes part of a vector.
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
                if instance.builtin == SimdLoad {
                    self.value(format!("load {ty}, ptr {pointer}, align 1"))
                } else {
                    self.instruction(format!("store {ty} %arg2, ptr {pointer}, align 1"));
                    "0".to_owned()
                }
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
