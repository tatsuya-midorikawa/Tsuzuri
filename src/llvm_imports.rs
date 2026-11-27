use super::*;
use crate::abi::{Buffer, out_result};
use crate::check::HostImport;

pub(super) fn header(module: &CheckedModule) -> String {
    let mut output = String::new();
    for function in &module.functions {
        let TypedExprKind::HostCall(import, _) = &function.body.kind else {
            continue;
        };
        // An explicit symbol belongs to the host; an extern view or data symbol links no generated prototype.
        if import.explicit || import.lowers_inline() {
            continue;
        }
        let parameters = host_abi::c_parameters(function, module);
        let viewed_result =
            crate::abi::c_view(import.c_result(&function.signature.result), &module.types());
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

pub(super) fn extern_declare(
    module: &CheckedModule,
    wasm: bool,
    wasm_module: &str,
    wasm_name: &str,
    symbol: &str,
    output: &str,
    types: &[String],
) -> String {
    let linkage = if !wasm && module.weak_symbols.contains(symbol) {
        "extern_weak "
    } else if !wasm
        && crate::target::current_platform().windows()
        && module.link_libraries.contains_key(symbol)
    {
        "dllimport "
    } else {
        ""
    };
    let attributes = if wasm {
        format!(" \"wasm-import-module\"=\"{wasm_module}\" \"wasm-import-name\"=\"{wasm_name}\"")
    } else {
        String::new()
    };
    format!(
        "declare {linkage}{output} @{symbol}({}){attributes}",
        types.join(", ")
    )
}

impl FunctionEmitter<'_, '_> {
    /// M06: lowers `extern "sym" view def` (`load` from `@sym`) and `extern "sym" address def` (`@sym`).
    pub(super) fn data_symbol(
        &mut self,
        data: &crate::check::DataSymbol,
        arguments: &[TypedExpr],
        result: &Type,
        decl_span: Span,
    ) -> String {
        if let Some(arg) = arguments.first() {
            let _ = self.expression(arg);
        }
        self.globals
            .data_symbols
            .entry(data.symbol.clone())
            .or_insert(decl_span);
        let global = format!("@{}", data.symbol);
        if self.module.weak_symbols.contains(&data.symbol) {
            return match data.access {
                crate::syntax::DataAccess::Address => {
                    let payload = crate::abi::std_maybe_payload(result, &self.module.types())
                        .expect("validated weak data symbol Maybe<H>")
                        .clone();
                    if crate::abi::extern_addr(&payload, &self.module.types()) {
                        let Type::Union(id, _) = result else {
                            unreachable!()
                        };
                        let some_case = self.module.unions[*id]
                            .cases
                            .iter()
                            .position(|(name, p)| name == "Some" && p.is_some())
                            .expect("std::Maybe has Some");
                        let present = self.value(format!("icmp ne ptr {global}, null"));
                        let ok_block = self.label();
                        let miss_block = self.label();
                        let done_block = self.label();
                        self.branch(&present, &ok_block, &miss_block);

                        self.begin(&miss_block);
                        let none_val = self.nullary_value(result, "None");
                        let from_miss = self.block.clone();
                        self.jump(&done_block);

                        self.begin(&ok_block);
                        let raw_i64 = self.value(format!("ptrtoint ptr {global} to i64"));
                        let pty = self.ty(&payload);
                        let ea = self.value(format!(
                            "insertvalue {pty} zeroinitializer, i64 {raw_i64}, 0"
                        ));
                        let ok_val = self.construct_value(result, some_case, Some((ea, pty)));
                        let from_ok = self.block.clone();
                        self.jump(&done_block);

                        self.begin(&done_block);
                        let llvm = self.ty(result);
                        self.value(format!(
                            "phi {llvm} [ {none_val}, %{from_miss} ], [ {ok_val}, %{from_ok} ]"
                        ))
                    } else {
                        self.nullable_from_c(result, &global)
                    }
                }
                crate::syntax::DataAccess::Read => {
                    let payload = crate::abi::std_maybe_payload(result, &self.module.types())
                        .expect("validated weak data symbol Maybe<T>")
                        .clone();
                    let Type::Union(id, _) = result else {
                        unreachable!()
                    };
                    let some_case = self.module.unions[*id]
                        .cases
                        .iter()
                        .position(|(name, p)| name == "Some" && p.is_some())
                        .expect("std::Maybe has Some");
                    let present = self.value(format!("icmp ne ptr {global}, null"));
                    let ok_block = self.label();
                    let miss_block = self.label();
                    let done_block = self.label();
                    self.branch(&present, &ok_block, &miss_block);

                    self.begin(&miss_block);
                    let none_val = self.nullary_value(result, "None");
                    let from_miss = self.block.clone();
                    self.jump(&done_block);

                    self.begin(&ok_block);
                    let ok_val = match &payload {
                        Type::Handle(..) => {
                            let raw = self.value(format!("load ptr, ptr {global}, align 8"));
                            self.nullable_from_c(result, &raw)
                        }
                        Type::Integer(bits, _) => {
                            let align = usize::from(*bits) / 8;
                            let v =
                                self.value(format!("load i{bits}, ptr {global}, align {align}"));
                            let pty = self.ty(&payload);
                            self.construct_value(result, some_case, Some((v, pty)))
                        }
                        Type::Binary(32) => {
                            let v = self.value(format!("load float, ptr {global}, align 4"));
                            self.construct_value(result, some_case, Some((v, "float".into())))
                        }
                        Type::Binary(64) => {
                            let v = self.value(format!("load double, ptr {global}, align 8"));
                            self.construct_value(result, some_case, Some((v, "double".into())))
                        }
                        Type::Record(..) => {
                            let v = self.read_host_record(&payload, &global);
                            let pty = self.ty(&payload);
                            self.construct_value(result, some_case, Some((v, pty)))
                        }
                        _ => unreachable!("validated weak data symbol read payload"),
                    };
                    let from_ok = self.block.clone();
                    self.jump(&done_block);

                    self.begin(&done_block);
                    let llvm = self.ty(result);
                    self.value(format!(
                        "phi {llvm} [ {none_val}, %{from_miss} ], [ {ok_val}, %{from_ok} ]"
                    ))
                }
            };
        }
        match data.access {
            crate::syntax::DataAccess::Address => {
                if crate::abi::extern_addr(result, &self.module.types()) {
                    let raw_i64 = self.value(format!("ptrtoint ptr {global} to i64"));
                    self.value(format!(
                        "insertvalue {} zeroinitializer, i64 {raw_i64}, 0",
                        self.ty(result)
                    ))
                } else {
                    global
                }
            }
            crate::syntax::DataAccess::Read => match result {
                Type::Integer(bits, _) => {
                    let align = usize::from(*bits) / 8;
                    self.value(format!("load i{bits}, ptr {global}, align {align}"))
                }
                Type::Binary(32) => self.value(format!("load float, ptr {global}, align 4")),
                Type::Binary(64) => self.value(format!("load double, ptr {global}, align 8")),
                Type::Handle(..) => {
                    let raw = self.value(format!("load ptr, ptr {global}, align 8"));
                    let valid = self.value(format!("icmp ne ptr {raw}, null"));
                    self.guard(&valid, TrapKind::NullHandle);
                    raw
                }
                Type::Record(..) => self.read_host_record(result, &global),
                _ => {
                    let raw = self.value(format!("load ptr, ptr {global}, align 8"));
                    self.nullable_from_c(result, &raw)
                }
            },
        }
    }

    /// W24: lowers `extern view def` inline at the call site: validates non-null and C alignment
    /// via `host_pointer` (trapping at the call site with `BoundsCheck`), then reads or writes
    /// the record field-by-field using the C struct layout.
    pub(super) fn extern_view(
        &mut self,
        direction: crate::syntax::ViewDirection,
        arguments: &[TypedExpr],
        result: &Type,
    ) -> String {
        match direction {
            crate::syntax::ViewDirection::Read => {
                let ea = self.expression(&arguments[0]);
                let ea_ty = self.ty(&arguments[0].ty);
                let address = self.value(format!("extractvalue {ea_ty} {ea}, 0"));
                let ptr = self.value(format!("inttoptr i64 {address} to ptr"));
                let (size, align) = host_abi::record_layout(result, self.module);
                self.host_pointer(&ptr, &size.to_string(), align, false);
                self.read_host_record(result, &ptr)
            }
            crate::syntax::ViewDirection::Write => {
                let ea = self.expression(&arguments[0]);
                // M21: a borrowed temporary lives in a slot until the record is written, as in `host_call`.
                let mut operands = Vec::new();
                let source_ref = if let TypedExprKind::BorrowOperand(operand) = &arguments[1].kind {
                    self.operand_borrow(&arguments[1], operand, &mut operands)
                } else {
                    self.expression(&arguments[1])
                };
                let Type::Reference(record_ty, false) = &arguments[1].ty else {
                    unreachable!("validated extern view write parameter")
                };
                let ea_ty = self.ty(&arguments[0].ty);
                let address = self.value(format!("extractvalue {ea_ty} {ea}, 0"));
                let ptr = self.value(format!("inttoptr i64 {address} to ptr"));
                let (size, align) = host_abi::record_layout(record_ty, self.module);
                self.host_pointer(&ptr, &size.to_string(), align, false);
                let value = self.value(format!("load {}, ptr {source_ref}", self.ty(record_ty)));
                self.write_host_record(record_ty, &value, &ptr);
                self.release_operands(operands);
                "0".into()
            }
        }
    }

    pub(super) fn host_call(
        &mut self,
        import: &HostImport,
        arguments: &[TypedExpr],
        full_result: &Type,
    ) -> String {
        let result = import.c_result(full_result);
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
                        | crate::abi::Nullable::BorrowedZUtf8String(_)
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
                && crate::abi::any_zstring(inner, &self.module.types())
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
        self.intrinsics.insert(extern_declare(
            self.module,
            self.globals.wasm,
            &import.wasm_module,
            &import.wasm_name,
            &import.native_symbol,
            &output,
            &types,
        ));
        if !self.globals.wasm
            && import.explicit
            && self.module.weak_symbols.contains(&import.native_symbol)
        {
            let sym = &import.native_symbol;
            let present = self.value(format!("icmp ne ptr @{sym}, null"));
            let ok_block = self.label();
            let miss_block = self.label();
            self.branch(&present, &ok_block, &miss_block);
            self.begin(&miss_block);
            self.globals.weak_checked_symbols.insert(sym.clone());
            self.instruction(format!(
                "call void @tz.weak.missing(ptr @tz.weak.name.{sym}, i64 {})",
                sym.len()
            ));
            self.emit_trap(TrapKind::Assert);
            self.instruction("unreachable");
            self.begin(&ok_block);
        }
        let mut errno_ptr = None;
        if import.last_error && !self.globals.wasm {
            if self.globals.blocking_externs {
                self.instruction("call void @tsuzuri_tz_blocking_enter()");
            }
            match crate::target::current_platform().last_error_access() {
                crate::target::LastErrorAccess::Win32 => {
                    self.intrinsics.insert(extern_declare(
                        self.module,
                        false,
                        "",
                        "",
                        "SetLastError",
                        "void",
                        &[String::from("i32")],
                    ));
                    self.intrinsics.insert(extern_declare(
                        self.module,
                        false,
                        "",
                        "",
                        "GetLastError",
                        "i32",
                        &[],
                    ));
                    self.instruction("call void @SetLastError(i32 0)");
                }
                crate::target::LastErrorAccess::Errno(sym) => {
                    let ret_ty = self
                        .module
                        .extern_abis
                        .get(sym)
                        .map_or("ptr", |sig| sig.result.llvm());
                    self.intrinsics.insert(extern_declare(
                        self.module,
                        false,
                        "",
                        "",
                        sym,
                        ret_ty,
                        &[],
                    ));
                    let ptr_val = if ret_ty == "ptr" {
                        self.value(format!("call ptr @{sym}()"))
                    } else {
                        let raw = self.value(format!("call {ret_ty} @{sym}()"));
                        self.value(format!("inttoptr {ret_ty} {raw} to ptr"))
                    };
                    self.instruction(format!("store i32 0, ptr {ptr_val}"));
                    errno_ptr = Some(ptr_val);
                }
            }
        }
        let call = format!(
            "call {output} @{}({})",
            import.native_symbol,
            values.join(", ")
        );
        let capture_last_error = |emitter: &mut Self, errno_ptr: Option<&str>| -> Option<String> {
            if !import.last_error || emitter.globals.wasm {
                return None;
            }
            let err = match errno_ptr {
                Some(ptr_val) => emitter.value(format!("load i32, ptr {ptr_val}")),
                None => emitter.value("call i32 @GetLastError()"),
            };
            if emitter.globals.blocking_externs {
                emitter.instruction("call void @tsuzuri_tz_blocking_exit()");
            }
            Some(err)
        };
        let pack_last_error =
            |emitter: &mut Self, r_value: String, captured: Option<String>| -> String {
                if let Some(err) = captured {
                    let pair_ty = emitter.ty(full_result);
                    let r_ty = emitter.ty(result);
                    let first = emitter.value(format!(
                        "insertvalue {pair_ty} zeroinitializer, {r_ty} {r_value}, 0"
                    ));
                    emitter.value(format!("insertvalue {pair_ty} {first}, i32 {err}, 1"))
                } else {
                    r_value
                }
            };
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
            let captured = capture_last_error(self, errno_ptr.as_deref());
            finish_copy_outs(self, &copy_outs);
            self.free_copies(&copies);
            let output = if let Some(slot) = out {
                self.read_host_result(result, &slot)
            } else {
                "0".into()
            };
            self.release_operands(operands);
            return pack_last_error(self, output, captured);
        }
        let value = self.value(call);
        let captured = capture_last_error(self, errno_ptr.as_deref());
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
        pack_last_error(self, output, captured)
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
