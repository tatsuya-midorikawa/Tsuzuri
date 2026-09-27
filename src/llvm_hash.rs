use super::*;

pub(super) const OFFSET: &str = "14695981039346656037";

impl FunctionEmitter<'_, '_> {
    fn hash_element(&mut self, method: &TypedExpr, pointer: &str) -> String {
        let Type::Function(parameters, _) = &method.ty else {
            unreachable!()
        };
        let Type::Reference(element, _) = &parameters[0] else {
            unreachable!()
        };
        let value = self.borrowed_element(element, pointer);
        self.comparison_values(method, &[value])
    }

    pub(super) fn structural_hash(&mut self, arguments: &[TypedExpr]) -> String {
        let Type::Reference(ty, _) = &arguments[0].ty else {
            unreachable!()
        };
        let input = self.expression(&arguments[0]);
        let methods = &arguments[1..];
        if let Type::Tuple(elements) = ty.as_ref() {
            let mut state = self.hash_word(OFFSET, "84");
            state = self.hash_word(&state, &elements.len().to_string());
            for (index, method) in methods.iter().enumerate() {
                let pointer = self.value(format!(
                    "getelementptr inbounds {}, ptr {input}, i32 0, i32 {index}",
                    self.ty(ty)
                ));
                state = self.hash_word(&state, &index.to_string());
                let hash = self.hash_element(method, &pointer);
                state = self.hash_word(&state, &hash);
            }
            return state;
        }
        let (element, linked) = match ty.as_ref() {
            Type::Array(element) => (element.as_ref(), false),
            Type::List(element) => (element.as_ref(), true),
            _ => unreachable!(),
        };
        let aggregate = self.ty(ty);
        let input = if linked {
            self.value(format!("load {aggregate}, ptr {input}"))
        } else {
            input
        };
        let data = self.value(format!("extractvalue {aggregate} {input}, 0"));
        let length = self.value(format!("extractvalue {aggregate} {input}, 1"));
        let state = self.hash_word(OFFSET, if linked { "76" } else { "65" });
        let state = self.hash_word(&state, &length);
        let state_slot = self.spill(&Type::Integer(64, false), &state);
        let cursor =
            linked.then(|| self.spill(&Type::Reference(Box::new(Type::Unit), false), &data));
        self.array_loop(&length, |emitter, index| {
            let pointer = if let Some(cursor) = &cursor {
                let node = emitter.value(format!("load ptr, ptr {cursor}"));
                let next = emitter.value(format!("load ptr, ptr {node}"));
                emitter.instruction(format!("store ptr {next}, ptr {cursor}"));
                emitter.list_element_pointer(element, &node)
            } else {
                emitter.element_pointer(element, &data, index)
            };
            let state = emitter.value(format!("load i64, ptr {state_slot}"));
            let state = emitter.hash_word(&state, index);
            let hash = emitter.hash_element(&methods[0], &pointer);
            let state = emitter.hash_word(&state, &hash);
            emitter.instruction(format!("store i64 {state}, ptr {state_slot}"));
        });
        self.value(format!("load i64, ptr {state_slot}"))
    }

    pub(super) fn hash_bits(
        &mut self,
        state: &str,
        value: &str,
        bits: u16,
        bytes: usize,
    ) -> String {
        let mut state = state.to_owned();
        for index in 0..bytes {
            let shifted = if index == 0 {
                value.to_owned()
            } else {
                self.value(format!("lshr i{bits} {value}, {}", index * 8))
            };
            let widened = match bits {
                1..=63 => self.value(format!("zext i{bits} {shifted} to i64")),
                64 => shifted,
                _ => self.value(format!("trunc i{bits} {shifted} to i64")),
            };
            let byte = self.value(format!("and i64 {widened}, 255"));
            let mixed = self.value(format!("xor i64 {state}, {byte}"));
            state = self.value(format!("mul i64 {mixed}, 1099511628211"));
        }
        state
    }

    pub(super) fn hash_word(&mut self, state: &str, value: &str) -> String {
        self.hash_bits(state, value, 64, 8)
    }

    pub(super) fn hash_primitive(&mut self, ty: &Type) -> String {
        let tag = match ty {
            Type::Bool => 0x01,
            Type::Unit => 0x02,
            Type::Integer(bits, _) => 0x10 + (bits / 8).ilog2(),
            Type::Binary(bits) => 0x20 + (bits / 8).ilog2(),
            Type::Decimal(bits) => 0x30 + (bits / 8).ilog2(),
            Type::Char => 0x43,
            Type::Utf8Char => 0x63,
            Type::String => 0x53,
            Type::Utf8String => 0x73,
            _ => unreachable!(),
        };
        let mut state = self.hash_word(OFFSET, &tag.to_string());
        if *ty == Type::Unit {
            return state;
        }
        let mut value = self.value(format!("load {}, ptr %arg0", self.ty(ty)));
        if ty.is_string() {
            let data = self.value(format!("extractvalue {} {value}, 0", self.ty(ty)));
            let length = self.value(format!("extractvalue {} {value}, 1", self.ty(ty)));
            state = self.hash_word(&state, &length);
            let state_slot = self.spill(&Type::Integer(64, false), &state);
            let bits = if *ty == Type::String { 16 } else { 8 };
            self.array_loop(&length, |emitter, index| {
                let pointer = emitter.value(format!(
                    "getelementptr inbounds i{bits}, ptr {data}, i64 {index}"
                ));
                let unit = emitter.value(format!("load i{bits}, ptr {pointer}"));
                let state = emitter.value(format!("load i64, ptr {state_slot}"));
                let state = emitter.hash_bits(&state, &unit, bits, (bits / 8) as usize);
                emitter.instruction(format!("store i64 {state}, ptr {state_slot}"));
            });
            return self.value(format!("load i64, ptr {state_slot}"));
        }
        let bits = match ty {
            Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => *bits,
            Type::Char => 16,
            Type::Utf8Char => 32,
            Type::Bool => 64,
            _ => unreachable!(),
        };
        if *ty == Type::Bool {
            value = self.value(format!("zext i1 {value} to i64"));
        }
        if matches!(ty, Type::Binary(32 | 64)) {
            value = self.value(format!("bitcast {} {value} to i{bits}", self.ty(ty)));
        }
        if matches!(ty, Type::Binary(_)) {
            let magnitude = self.value(format!(
                "and i{bits} {value}, {}",
                (1u128 << (bits - 1)) - 1
            ));
            let zero = self.value(format!("icmp eq i{bits} {magnitude}, 0"));
            value = self.value(format!("select i1 {zero}, i{bits} 0, i{bits} {value}"));
            let fraction = match bits {
                16 => 10,
                32 => 23,
                64 => 52,
                128 => 112,
                _ => unreachable!(),
            };
            let infinity = ((1u128 << (bits - 1)) - 1) & !((1u128 << fraction) - 1);
            let nan = self.value(format!("icmp ugt i{bits} {magnitude}, {infinity}"));
            value = self.value(format!(
                "select i1 {nan}, i{bits} {}, i{bits} {value}",
                infinity | (1u128 << (fraction - 1))
            ));
        }
        if matches!(ty, Type::Decimal(_)) {
            let output = self.slot(ty);
            self.instruction(format!(
                "call void @tz_soft_hash_canonical(ptr {output}, ptr %arg0, i32 {})",
                numeric_kind(ty)
            ));
            value = self.value(format!("load i{bits}, ptr {output}"));
        }
        self.hash_bits(&state, &value, bits, (bits / 8) as usize)
    }
}
