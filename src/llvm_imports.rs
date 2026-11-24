use super::*;
use crate::abi::{Buffer, out_result};
use crate::check::HostImport;

pub(super) fn header(module: &CheckedModule) -> String {
    let mut output = String::new();
    for function in &module.functions {
        let TypedExprKind::HostCall(import, _) = &function.body.kind else {
            continue;
        };
        // An explicit symbol belongs to the host, whose own header declares it.
        if import.explicit {
            continue;
        }
        let parameters = host_abi::c_parameters(function, module);
        let viewed_result = crate::abi::c_view(&function.signature.result, &module.types());
        let _ = writeln!(
            output,
            "{} {}({parameters});",
            if out_result(&viewed_result) {
                "void".into()
            } else {
                c_type(&viewed_result)
            },
            import.native_symbol
        );
    }
    output
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn host_call(
        &mut self,
        import: &HostImport,
        arguments: &[TypedExpr],
        result: &Type,
    ) -> String {
        let mut types = Vec::new();
        let mut values = Vec::new();
        let out = if out_result(result) {
            let slot = self.host_result_slot(result);
            types.push("ptr".into());
            values.push(format!("ptr {slot}"));
            Some(slot)
        } else {
            None
        };
        // Compact strings that the host reads as UTF-16: where the pointer of each is in `values`,
        // optional `@count` length kind, and its descriptor.
        let mut widen: Vec<(usize, Option<crate::syntax::CountLength>, String)> = Vec::new();
        // `@count(..., i32 | i32u)`: `(slot_in_values_or_none_for_widen, length_ssa, max_limit)`.
        let mut count_checks: Vec<(Option<usize>, String, u64)> = Vec::new();
        // W13: `ref mut` records, scalars, and `copy` handles copied back immediately after the call.
        enum CopyOut {
            Record {
                slot: String,
                place: String,
                ty: Type,
            },
            Scalar {
                slot: String,
                place: String,
                ty: Type,
                c_llvm_ty: &'static str,
            },
        }
        let mut copy_outs: Vec<CopyOut> = Vec::new();
        // The widened copies of Latin-1 strings, with the length words that say which are copies.
        let mut copies: Vec<(String, String)> = Vec::new();
        let mut deferred_nullable_records: Vec<(usize, Type, String)> = Vec::new();
        let mut operands = Vec::new();
        for (arg_index, argument) in arguments.iter().enumerate() {
            if matches!(argument.ty, Type::Function(..)) {
                // A callback reaches the host as the address of its C-ABI wrapper.
                let mut target = argument;
                while let TypedExprKind::Cast(inner) = &target.kind {
                    target = inner;
                }
                let TypedExprKind::Function(FunctionRef::User(id)) = &target.kind else {
                    unreachable!("validated callback arguments name a function");
                };
                self.globals.callbacks.insert(*id);
                types.push("ptr".into());
                values.push(format!(
                    "ptr @tz.callback.{}",
                    self.module.functions[*id].qualified_name()
                ));
                continue;
            }
            let value = if let TypedExprKind::BorrowOperand(operand) = &argument.kind {
                self.operand_borrow(argument, operand, &mut operands)
            } else {
                self.expression(argument)
            };
            if argument.ty == Type::Unit {
                continue;
            }
            if let Some(nullable) = crate::abi::nullable(&argument.ty, &self.module.types()) {
                types.push("ptr".into());
                if matches!(
                    nullable,
                    crate::abi::Nullable::BorrowedRecord(_)
                        | crate::abi::Nullable::BorrowedZString(_)
                ) {
                    deferred_nullable_records.push((values.len(), argument.ty.clone(), value));
                    values.push(String::new());
                } else {
                    let converted = self.nullable_to_c(&argument.ty, &value);
                    values.push(format!("ptr {converted}"));
                }
                continue;
            }
            if let Type::Reference(inner, false) = &argument.ty
                && crate::abi::zstring(inner, &self.module.types())
            {
                let rec = self.value(format!("load {}, ptr {value}", self.ty(inner)));
                let arr = self.value(format!("extractvalue {} {rec}, 0", self.ty(inner)));
                let ptr = self.value(format!("extractvalue %tz.array {arr}, 0"));
                types.push("ptr".into());
                values.push(format!("ptr {ptr}"));
                continue;
            }
            if let Some((_, mutable)) = crate::abi::extern_buffer_parameter(&argument.ty) {
                let Type::Reference(inner, _) = &argument.ty else {
                    unreachable!("extern buffer parameter is a reference")
                };
                let count_len = import.count_at(arg_index);
                let (descriptor, desc_ty) = match (inner.as_ref(), mutable) {
                    (Type::Array(_), false) | (Type::ArrayView(_), true) => {
                        (value, "%tz.array".to_owned())
                    }
                    (Type::Array(_), true) => (
                        self.value(format!("load %tz.array, ptr {value}")),
                        "%tz.array".to_owned(),
                    ),
                    _ => {
                        let ty_str = self.ty(inner);
                        (self.value(format!("load {ty_str}, ptr {value}")), ty_str)
                    }
                };
                if **inner == Type::String && self.globals.strings == StringForm::Compact {
                    if let Some(limit) = count_len.and_then(|len| len.max_length()) {
                        let pre_len = self.string_length(&Type::String, &descriptor);
                        count_checks.push((None, pre_len, limit));
                    }
                    widen.push((values.len(), count_len, descriptor));
                    types.push("ptr".into());
                    values.push(String::new());
                    if let Some(len_llvm) =
                        count_len.map_or(Some("i64"), crate::syntax::CountLength::llvm_type)
                    {
                        types.push(len_llvm.into());
                        values.push(String::new());
                    }
                    continue;
                }
                let pointer = self.value(format!("extractvalue {desc_ty} {descriptor}, 0"));
                let length = self.value(format!("extractvalue {desc_ty} {descriptor}, 1"));
                types.push("ptr".into());
                values.push(format!("ptr {pointer}"));
                match count_len {
                    Some(crate::syntax::CountLength::None) => {}
                    Some(
                        kind @ (crate::syntax::CountLength::I32 | crate::syntax::CountLength::I32U),
                    ) => {
                        let slot = values.len();
                        types.push("i32".into());
                        values.push(String::new());
                        count_checks.push((
                            Some(slot),
                            length,
                            kind.max_length().expect("32-bit count has limit"),
                        ));
                    }
                    Some(crate::syntax::CountLength::I64 | crate::syntax::CountLength::I64U)
                    | Option::None => {
                        types.push("i64".into());
                        values.push(format!("i64 {length}"));
                    }
                }
                continue;
            }
            if let Type::Reference(inner, true) = &argument.ty {
                if matches!(inner.as_ref(), Type::Record(..)) {
                    let cur = self.value(format!("load {}, ptr {value}", self.ty(inner)));
                    let slot = self.host_result_slot(inner);
                    self.write_host_record(inner, &cur, &slot);
                    types.push("ptr".into());
                    values.push(format!("ptr {slot}"));
                    copy_outs.push(CopyOut::Record {
                        slot,
                        place: value,
                        ty: (**inner).clone(),
                    });
                    continue;
                }
                let (c_llvm_ty, align) = match inner.as_ref() {
                    Type::Bool | Type::Integer(32, _) => ("i32", 4),
                    Type::Integer(8, _) => ("i8", 1),
                    Type::Integer(16, _) => ("i16", 2),
                    Type::Integer(64, _) => ("i64", 8),
                    Type::Binary(32) => ("float", 4),
                    Type::Binary(64) => ("double", 8),
                    Type::Handle(..) => ("ptr", 8),
                    _ => unreachable!("validated mutable extern scalar: {inner:?}"),
                };
                let cur = self.value(format!("load {}, ptr {value}", self.ty(inner)));
                let encoded = if **inner == Type::Bool {
                    self.value(format!("zext i1 {cur} to i32"))
                } else {
                    cur
                };
                let slot = self.fresh();
                self.allocas
                    .push(format!("{slot} = alloca {c_llvm_ty}, align {align}"));
                self.instruction(format!("store {c_llvm_ty} {encoded}, ptr {slot}"));
                types.push("ptr".into());
                values.push(format!("ptr {slot}"));
                copy_outs.push(CopyOut::Scalar {
                    slot,
                    place: value,
                    ty: (**inner).clone(),
                    c_llvm_ty,
                });
                continue;
            }
            let inner = if let Type::Reference(inner, false) = &argument.ty {
                inner.as_ref()
            } else {
                &argument.ty
            };
            if matches!(inner, Type::Record(..)) {
                let value = if matches!(argument.ty, Type::Reference(..)) {
                    self.value(format!("load {}, ptr {value}", self.ty(inner)))
                } else {
                    value
                };
                let slot = self.host_result_slot(inner);
                self.write_host_record(inner, &value, &slot);
                types.push("ptr".into());
                values.push(format!("ptr {slot}"));
                continue;
            }
            if matches!(argument.ty, Type::Reference(..)) && matches!(inner, Type::Handle(..)) {
                // The host takes the handle itself, not the address of the borrowed slot.
                let handle = self.value(format!("load ptr, ptr {value}"));
                types.push("ptr".into());
                values.push(format!("ptr {handle}"));
                continue;
            }
            let input = self.ty(&argument.ty);
            let abi = abi_type(&argument.ty);
            let value = match argument.ty {
                Type::Bool | Type::Integer(8 | 16, false) => {
                    self.value(format!("zext {input} {value} to {abi}"))
                }
                Type::Integer(8 | 16, true) => self.value(format!("sext {input} {value} to {abi}")),
                _ => value,
            };
            types.push(abi.clone());
            values.push(format!("{abi} {value}"));
        }
        // W13: validate `@count(..., i32 | i32u)` bounds after all arguments are evaluated and
        // before widening compact strings so a trap never leaks a widened copy.
        for (slot, length, limit) in count_checks {
            let valid = self.value(format!("icmp ule i64 {length}, {limit}"));
            self.guard(&valid, TrapKind::BoundsCheck);
            if let Some(slot) = slot {
                let narrowed = self.value(format!("trunc i64 {length} to i32"));
                values[slot] = format!("i32 {narrowed}");
            }
        }
        // The host reads UTF-16 units; a Latin-1 string is widened into a copy that is freed after the
        // call. No argument is evaluated between a copy and the call that frees it.
        for (position, count_len, descriptor) in widen {
            let word = self.string_word(&Type::String, &descriptor);
            let widened = self.value(format!(
                "call %tz.string @tz.string.borrow_utf16(%tz.string {descriptor})"
            ));
            let copy = self.value(format!("extractvalue %tz.string {widened}, 0"));
            let length = self.value(format!("extractvalue %tz.string {widened}, 1"));
            values[position] = format!("ptr {copy}");
            match count_len {
                Some(crate::syntax::CountLength::None) => {}
                Some(crate::syntax::CountLength::I32 | crate::syntax::CountLength::I32U) => {
                    let narrowed = self.value(format!("trunc i64 {length} to i32"));
                    values[position + 1] = format!("i32 {narrowed}");
                }
                Some(crate::syntax::CountLength::I64 | crate::syntax::CountLength::I64U)
                | Option::None => {
                    values[position + 1] = format!("i64 {length}");
                }
            }
            copies.push((copy, word));
        }
        for (position, ty, value) in deferred_nullable_records {
            let converted = self.nullable_to_c(&ty, &value);
            values[position] = format!("ptr {converted}");
        }
        let result_nullable = crate::abi::nullable(result, &self.module.types()).is_some();
        let mut output = if *result == Type::Unit || out.is_some() {
            "void".to_owned()
        } else {
            abi_type(&crate::abi::c_view(result, &self.module.types()))
        };
        let mut convert_ptr_result = false;
        if !self.globals.wasm
            && import.explicit
            && let Some(canonical) = self.module.extern_abis.get(&import.native_symbol)
        {
            for (index, slot_type) in types.iter_mut().enumerate() {
                let target = canonical.parameters[index].llvm();
                if slot_type != target {
                    let raw = values[index]
                        .strip_prefix("ptr ")
                        .expect("only ptr/i64 slots differ in Word64");
                    let cast = self.value(format!("ptrtoint ptr {raw} to i64"));
                    *slot_type = target.to_owned();
                    values[index] = format!("{target} {cast}");
                }
            }
            let target_result = canonical.result.llvm();
            if output != target_result {
                debug_assert_eq!((output.as_str(), target_result), ("ptr", "i64"));
                output = target_result.to_owned();
                convert_ptr_result = true;
            }
        }
        let attributes = if self.globals.wasm {
            format!(
                " \"wasm-import-module\"=\"{}\" \"wasm-import-name\"=\"{}\"",
                import.wasm_module, import.wasm_name
            )
        } else {
            String::new()
        };
        self.intrinsics.insert(format!(
            "declare {output} @{}({}){attributes}",
            import.native_symbol,
            types.join(", ")
        ));
        let call = format!(
            "call {output} @{}({})",
            import.native_symbol,
            values.join(", ")
        );
        let finish_copy_outs = |emitter: &mut Self, copy_outs: &[CopyOut]| {
            for item in copy_outs {
                match item {
                    CopyOut::Record { slot, place, ty } => {
                        let updated = emitter.read_host_record(ty, slot);
                        emitter.instruction(format!(
                            "store {} {updated}, ptr {place}",
                            emitter.ty(ty)
                        ));
                    }
                    CopyOut::Scalar {
                        slot,
                        place,
                        ty,
                        c_llvm_ty,
                    } => {
                        let raw = emitter.value(format!("load {c_llvm_ty}, ptr {slot}"));
                        if !emitter.globals.wasm && matches!(ty, Type::Handle(..)) {
                            let valid = emitter.value(format!("icmp ne ptr {raw}, null"));
                            emitter.guard(&valid, TrapKind::NullHandle);
                        }
                        let decoded = if *ty == Type::Bool {
                            emitter.value(format!("icmp ne i32 {raw}, 0"))
                        } else {
                            raw
                        };
                        emitter.instruction(format!(
                            "store {} {decoded}, ptr {place}",
                            emitter.ty(ty)
                        ));
                    }
                }
            }
        };
        if *result == Type::Unit || out.is_some() {
            self.instruction(call);
            finish_copy_outs(self, &copy_outs);
            self.free_copies(&copies);
            let output = if let Some(slot) = out {
                self.read_host_result(result, &slot)
            } else {
                "0".into()
            };
            self.release_operands(operands);
            return output;
        }
        let value = self.value(call);
        finish_copy_outs(self, &copy_outs);
        let value = if convert_ptr_result {
            self.value(format!("inttoptr i64 {value} to ptr"))
        } else {
            value
        };
        self.free_copies(&copies);
        let output = if result_nullable {
            self.nullable_from_c(result, &value)
        } else {
            if !self.globals.wasm && matches!(result, Type::Handle(..)) {
                let valid = self.value(format!("icmp ne ptr {value}, null"));
                self.guard(&valid, TrapKind::NullHandle);
            }
            match result {
                Type::Bool => self.value(format!("icmp ne i32 {value}, 0")),
                Type::Integer(8 | 16, _) => {
                    self.value(format!("trunc i32 {value} to {}", self.ty(result)))
                }
                _ => value,
            }
        };
        self.release_operands(operands);
        output
    }

    /// Frees the widened copies that `host_call` made of Latin-1 strings: a string that was UTF-16 is
    /// the original, and its buffer stays.
    fn free_copies(&mut self, copies: &[(String, String)]) {
        for (copy, word) in copies {
            let latin1 = self.string_is_latin1(word);
            let target = self.value(format!("select i1 {latin1}, ptr {copy}, ptr null"));
            self.instruction(format!("call void @tz.free(ptr {target})"));
        }
    }

    pub(super) fn host_result_slot(&mut self, ty: &Type) -> String {
        let llvm = if matches!(ty, Type::Record(..)) {
            format!("%{}", host_abi::record_name(ty, self.module))
        } else {
            "%tz.abi.buffer".into()
        };
        let slot = self.fresh();
        self.allocas
            .push(format!("{slot} = alloca {llvm}, align 16"));
        self.instruction(format!("store {llvm} zeroinitializer, ptr {slot}"));
        if Buffer::of(ty).is_some() {
            let length = self.value(format!(
                "getelementptr inbounds %tz.abi.buffer, ptr {slot}, i32 0, i32 1"
            ));
            self.instruction(format!("store i64 -1, ptr {length}"));
        }
        slot
    }

    pub(super) fn read_host_result(&mut self, ty: &Type, slot: &str) -> String {
        if matches!(ty, Type::Record(..)) {
            return self.read_host_record(ty, slot);
        }
        let buffer = Buffer::of(ty).expect("validated host result");
        let pointer = self.value(format!("load ptr, ptr {slot}"));
        let length_slot = self.value(format!(
            "getelementptr inbounds %tz.abi.buffer, ptr {slot}, i32 0, i32 1"
        ));
        let length = self.value(format!("load i64, ptr {length_slot}"));
        let valid = self.value(format!(
            "icmp ule i64 {length}, {}",
            i64::MAX as u64 / buffer.width() as u64
        ));
        self.guard(&valid, TrapKind::AllocationSize);
        if buffer == Buffer::String {
            let valid = self.value(format!("icmp ule i64 {length}, 9007199254740991"));
            self.guard(&valid, TrapKind::StringConcatOverflow);
        }
        let bytes = self.value(format!("mul i64 {length}, {}", buffer.width()));
        self.host_pointer(&pointer, &bytes, buffer.width(), true);
        if buffer == Buffer::Utf8String {
            self.validate_host_utf8(&pointer, &length);
        }
        let result = self.value(format!(
            "insertvalue {} zeroinitializer, ptr {pointer}, 0",
            self.ty(ty)
        ));
        self.value(format!(
            "insertvalue {} {result}, i64 {length}, 1",
            self.ty(ty)
        ))
    }
}
