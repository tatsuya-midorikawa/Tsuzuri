use super::*;
use crate::abi::{Buffer, RecordPassing, extern_record_passing, out_result};
use crate::check::HostImport;
use byvalue::{ByValue, CLayoutLeaves, Position};

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
            if extern_record_passing(&viewed_result, &module.types())
                == Some(RecordPassing::ByValue)
            {
                // W36: the C prototype returns the struct itself, on every target.
                host_abi::record_name(&viewed_result, module)
            } else if out_result(&viewed_result) {
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
                        Type::Handle(_, kind) => {
                            let raw = self.value(format!("load ptr, ptr {global}, align 8"));
                            // M01: an object that is read is retained; retaining nil is nothing.
                            let raw = if kind.objc() {
                                self.objc_retain(&raw)
                            } else {
                                raw
                            };
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
                Type::Handle(_, kind) => {
                    let raw = self.value(format!("load ptr, ptr {global}, align 8"));
                    let valid = self.value(format!("icmp ne ptr {raw}, null"));
                    self.guard(&valid, TrapKind::NullHandle);
                    // M01: an object that is read is retained, so the value owns a reference.
                    if kind.objc() {
                        self.objc_retain(&raw)
                    } else {
                        raw
                    }
                }
                Type::Record(..) => self.read_host_record(result, &global),
                _ => {
                    let raw = self.value(format!("load ptr, ptr {global}, align 8"));
                    // M01: `Maybe<C>` of an object is retained too; retaining nil is nothing.
                    let raw = if crate::abi::objc_object_result(result, &self.module.types()) {
                        self.objc_retain(&raw)
                    } else {
                        raw
                    };
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
        let platform = crate::target::current_platform();
        // M02: a message send. The arguments lower as those of an extern function; only the
        // callee (`objc_msgSend` with the receiver and the selector first) and the widths of
        // `bool` and of the small integers differ.
        let send = import.objc.as_deref();
        // W36: a `@c_layout` record result is the C struct by value. Its class does not depend on
        // the parameters, and a result in memory has its temporary before any argument is evaluated.
        let by_value_result =
            extern_record_passing(result, &self.module.types()) == Some(RecordPassing::ByValue);
        let result_layout = by_value_result.then(|| self.by_value_layout(result));
        let result_class = result_layout.as_ref().map(|layout| {
            byvalue::classify(layout, platform, true)
                .expect("byvalue::validate checked the by-value records of reachable externs")
        });
        let sret = result_layout
            .as_ref()
            .filter(|_| result_class == Some(ByValue::Memory))
            .map(|layout| self.by_value_slot(layout, &ByValue::Memory));
        let out = if !by_value_result && out_result(result) {
            let slot = self.host_result_slot(result);
            types.push("ptr".into());
            values.push(format!("ptr {slot}"));
            Some(slot)
        } else {
            None
        };
        // W36: the by-value record arguments: where each stands in `values`, its temporary and its
        // layout. `types` holds one empty entry for each until the whole call is lowered, because
        // System V counts the registers of every parameter before it.
        let mut by_values: Vec<(usize, String, CLayoutLeaves)> = Vec::new();
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
                    // M02: Objective-C's `BOOL *` points to one byte.
                    Type::Bool if send.is_some() => ("i8", 1),
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
                    self.value(format!("zext i1 {cur} to {c_llvm_ty}"))
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
            if extern_record_passing(&argument.ty, &self.module.types())
                == Some(RecordPassing::ByValue)
            {
                // A fresh copy for this argument of this call, written as soon as the argument is
                // evaluated: the callee may write to a copy that it receives in memory.
                let layout = self.by_value_layout(inner);
                let alone = byvalue::classify(&layout, platform, false)
                    .expect("byvalue::validate checked the by-value records of reachable externs");
                let slot = self.by_value_slot(&layout, &alone);
                self.write_host_record(inner, &value, &slot);
                by_values.push((values.len(), slot, layout));
                types.push(String::new());
                values.push(String::new());
                continue;
            }
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
            if send.is_some()
                && let Some(extension) = objc::small_scalar_extension(&argument.ty)
            {
                // M02 spec 5: `bool` and the 8- and 16-bit integers keep their width, with the
                // extension that the callee may rely on (clang's `i1 zeroext`, `i8 signext`).
                types.push(format!("{input} {extension}"));
                values.push(format!("{input} {extension} {value}"));
                continue;
            }
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
        if let Some(send) = send {
            // M02 spec 6: the receiver and the selector are the first two arguments. An instance
            // method's receiver is its first argument, already lowered to the object's pointer;
            // a class method reads the reference of its class. The selector is read after every
            // argument is evaluated. The places behind the insertion move by its length.
            let at = usize::from(send.class.is_none());
            let mut inserted = Vec::new();
            if let Some(class) = &send.class {
                inserted.push(format!("ptr {}", self.objc_classref(class)));
            }
            inserted.push(format!("ptr {}", self.objc_selref(&send.selector)));
            let shift = inserted.len();
            for (offset, value) in inserted.into_iter().enumerate() {
                types.insert(at + offset, "ptr".into());
                values.insert(at + offset, value);
            }
            for (position, ..) in &mut by_values {
                if *position >= at {
                    *position += shift;
                }
            }
            for (position, ..) in &mut widen {
                if *position >= at {
                    *position += shift;
                }
            }
            for (slot, ..) in &mut count_checks {
                if let Some(slot) = slot
                    && *slot >= at
                {
                    *slot += shift;
                }
            }
            for (position, ..) in &mut deferred_nullable_records {
                if *position >= at {
                    *position += shift;
                }
            }
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
        let small_result = send.and_then(|_| objc::small_scalar_extension(result));
        let mut output = if let Some(class) = &result_class {
            class.result_type()
        } else if *result == Type::Unit || out.is_some() {
            "void".to_owned()
        } else if let Some(extension) = small_result {
            // M02 spec 5: `zeroext i1`, `signext i8`, ... as the callee returns them.
            format!("{extension} {}", self.ty(result))
        } else {
            abi_type(&crate::abi::c_view(result, &self.module.types()))
        };
        let mut convert_ptr_result = false;
        if !self.globals.wasm
            && import.explicit
            && let Some(canonical) = self.module.extern_abis.get(&import.native_symbol)
        {
            for (index, slot_type) in types.iter_mut().enumerate() {
                // W36: a struct by value has no scalar type to agree on; declarations of one symbol
                // name the same C shape there (`abi::c_signature`).
                let Some(target) = canonical.parameters[index].llvm() else {
                    continue;
                };
                if slot_type != target {
                    let raw = values[index]
                        .strip_prefix("ptr ")
                        .expect("only ptr/i64 slots differ in Word64");
                    let cast = self.value(format!("ptrtoint ptr {raw} to i64"));
                    *slot_type = target.to_owned();
                    values[index] = format!("{target} {cast}");
                }
            }
            if let Some(target_result) = canonical.result.llvm()
                && output != target_result
            {
                debug_assert_eq!((output.as_str(), target_result), ("ptr", "i64"));
                output = target_result.to_owned();
                convert_ptr_result = true;
            }
        }
        // W36: every parameter is lowered, so the registers of the call can be counted and each
        // by-value record becomes the IR values of its class. A result in memory is the first
        // parameter, with `sret`.
        if !by_values.is_empty() || sret.is_some() {
            let positions: Vec<Position<'_>> = types
                .iter()
                .enumerate()
                .map(|(index, ty)| {
                    match by_values.iter().find(|(position, ..)| *position == index) {
                        Some((_, _, layout)) => Position::Record(layout),
                        None if matches!(ty.as_str(), "float" | "double") => Position::Sse,
                        None => Position::Integer,
                    }
                })
                .collect();
            let classes = byvalue::classify_call(&positions, result_layout.as_ref(), platform)
                .expect("byvalue::validate checked the by-value records of reachable externs");
            let mut lowered_types = Vec::new();
            let mut lowered_values = Vec::new();
            if let (Some(slot), Some(layout)) = (&sret, &result_layout) {
                let parameter = ByValue::sret_parameter(layout);
                lowered_values.push(format!("{parameter} {slot}"));
                lowered_types.push(parameter);
            }
            let mut records = by_values.iter().zip(&classes).peekable();
            for (index, (ty, value)) in types.into_iter().zip(values).enumerate() {
                match records.next_if(|((position, ..), _)| *position == index) {
                    Some(((_, slot, layout), class)) => {
                        for (ty, value) in self.by_value_to_abi(class, false, slot, layout) {
                            lowered_values.push(format!("{ty} {value}"));
                            lowered_types.push(ty);
                        }
                    }
                    None => {
                        lowered_types.push(ty);
                        lowered_values.push(value);
                    }
                }
            }
            types = lowered_types;
            values = lowered_values;
        }
        if send.is_some() {
            // M02 D11: `objc_msgSend` has one variadic declaration; each call names its own
            // function type, so the arguments go to the registers of that type.
            self.globals.objc.msg_send();
        } else {
            self.intrinsics.insert(extern_declare(
                self.module,
                self.globals.wasm,
                &import.wasm_module,
                &import.wasm_name,
                &import.native_symbol,
                &output,
                &types,
            ));
        }
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
                        .and_then(|sig| sig.result.llvm())
                        .unwrap_or("ptr");
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
        // M01: an Objective-C object result comes at +0 unless the declaration says
        // `@returns_retained`. The attached-call bundle makes the backend retain it right after
        // the call; a result that the canonical C type lowers to `i64` (W14) cannot carry the
        // bundle and is retained by a plain call after its `inttoptr`.
        let objc_result =
            !self.globals.wasm && crate::abi::objc_object_result(result, &self.module.types());
        if objc_result {
            self.globals.objc.used = true;
        }
        let retain_result = objc_result && !import.returns_retained;
        let bundle = if retain_result && !convert_ptr_result {
            self.globals.objc.attached_call();
            objc::ATTACHED_CALL
        } else {
            ""
        };
        // M09 D15: a message to a `main` class is sent on the main thread, which holds no P: the
        // pass that makes extern calls blocking calls leaves the call as it is.
        let main_thread =
            if self.globals.blocking_externs && send.is_some_and(|send| send.main_thread) {
                MAIN_THREAD_CALL
            } else {
                ""
            };
        let call = format!(
            "call {output} @{}({}){main_thread}{bundle}",
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
                            emitter.value(format!("icmp ne {c_llvm_ty} {raw}, 0"))
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
        if *result == Type::Unit || out.is_some() || sret.is_some() {
            self.instruction(call);
            let captured = capture_last_error(self, errno_ptr.as_deref());
            finish_copy_outs(self, &copy_outs);
            self.free_copies(&copies);
            let output = if let Some(slot) = out {
                self.read_host_result(result, &slot)
            } else if let Some(slot) = sret {
                self.read_host_record(result, &slot)
            } else {
                "0".into()
            };
            self.release_operands(operands);
            return pack_last_error(self, output, captured);
        }
        let value = self.value(call);
        let captured = capture_last_error(self, errno_ptr.as_deref());
        finish_copy_outs(self, &copy_outs);
        if let (Some(class), Some(layout)) = (&result_class, &result_layout) {
            // W36: the registers of the result go to a temporary, and the record is read from it
            // like any `@c_layout` record that the host wrote.
            self.free_copies(&copies);
            let slot = self.by_value_slot(layout, class);
            self.by_value_from_abi(class, &[(output, value)], &slot);
            let record = self.read_host_record(result, &slot);
            self.release_operands(operands);
            return pack_last_error(self, record, captured);
        }
        let value = if convert_ptr_result {
            let pointer = self.value(format!("inttoptr i64 {value} to ptr"));
            if retain_result {
                self.objc_retain_autoreleased(&pointer)
            } else {
                pointer
            }
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
                // M02: a message returns `bool` and the small integers in their own width.
                _ if small_result.is_some() => value,
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

    /// W36: the layout that `byvalue::classify` reads of the `@c_layout` record `record`.
    pub(crate) fn by_value_layout(&self, record: &Type) -> CLayoutLeaves {
        CLayoutLeaves::of(record, self.module).expect("a by-value record is a '@c_layout' record")
    }

    /// W36: a zeroed temporary for one by-value record of one call: as large as the record and as
    /// the IR values of `class` (an AArch64 `i64` argument of a 3-byte record reads 8 bytes), and
    /// aligned for every type that a class loads or stores. A temporary made for the class that
    /// `byvalue::classify` gives a record also fits the class that `classify_call` gives it.
    pub(crate) fn by_value_slot(&mut self, layout: &CLayoutLeaves, class: &ByValue) -> String {
        let bytes = class.storage(layout);
        let slot = self.fresh();
        self.allocas
            .push(format!("{slot} = alloca [{bytes} x i8], align 16"));
        self.instruction(format!("store [{bytes} x i8] zeroinitializer, ptr {slot}"));
        slot
    }

    /// W36: from the C struct in the temporary `slot` (`write_host_record` wrote it) to the IR
    /// values of `class`, as `(type with its attributes, value)`. With `result` false they are the
    /// arguments of a call, to append to its parameter types and values in order; a `Memory`
    /// argument is `slot` itself, so the temporary must be this argument's alone. With `result`
    /// true it is the one value that a function returns (W26: a callback that returns the record);
    /// `Memory` has none, because the callee writes through the `sret` pointer it received.
    pub(crate) fn by_value_to_abi(
        &mut self,
        class: &ByValue,
        result: bool,
        slot: &str,
        layout: &CLayoutLeaves,
    ) -> Vec<(String, String)> {
        let types = class.argument_types(layout, crate::target::current_platform());
        if result {
            let ty = class.result_type();
            return match class {
                ByValue::Memory => Vec::new(),
                ByValue::Eightbytes(parts) if parts.len() > 1 => {
                    let mut pair = "zeroinitializer".to_owned();
                    for (index, part) in types.iter().enumerate() {
                        let from = self.by_value_part(slot, index);
                        let loaded = self.value(format!("load {part}, ptr {from}"));
                        pair = self
                            .value(format!("insertvalue {ty} {pair}, {part} {loaded}, {index}"));
                    }
                    vec![(ty, pair)]
                }
                _ => {
                    let loaded = self.value(format!("load {ty}, ptr {slot}"));
                    vec![(ty, loaded)]
                }
            };
        }
        match class {
            ByValue::Memory => vec![(types[0].clone(), slot.to_owned())],
            ByValue::Eightbytes(_) => types
                .into_iter()
                .enumerate()
                .map(|(index, ty)| {
                    let from = self.by_value_part(slot, index);
                    let loaded = self.value(format!("load {ty}, ptr {from}"));
                    (ty, loaded)
                })
                .collect(),
            _ => {
                // The attribute (`alignstack(8)`) belongs to the parameter, not to the loaded type.
                let plain = types[0].split(" alignstack").next().unwrap_or(&types[0]);
                let loaded = self.value(format!("load {plain}, ptr {slot}"));
                vec![(types[0].clone(), loaded)]
            }
        }
    }

    /// W36: from the IR values of `class` to the C struct in the temporary `slot`, which
    /// `read_host_record` then reads. `values` are `(plain IR type, value)`: the one value that a
    /// call returned (`ByValue::result_type`), or the parameters of a callback that receives the
    /// record (W26). `Memory` stores nothing: the callee wrote through the `sret` pointer, and a
    /// `Memory` parameter is itself the address of the struct.
    pub(crate) fn by_value_from_abi(
        &mut self,
        class: &ByValue,
        values: &[(String, String)],
        slot: &str,
    ) {
        match (class, values) {
            (ByValue::Memory, _) => {}
            // One `{ a, b }` that a call returned: each eightbyte goes to its offset.
            (ByValue::Eightbytes(parts), [(pair, value)]) if parts.len() > 1 => {
                for (index, part) in parts.iter().enumerate() {
                    let ty = ByValue::Eightbytes(vec![*part]).result_type();
                    let extracted = self.value(format!("extractvalue {pair} {value}, {index}"));
                    let to = self.by_value_part(slot, index);
                    self.instruction(format!("store {ty} {extracted}, ptr {to}"));
                }
            }
            _ => {
                for (index, (ty, value)) in values.iter().enumerate() {
                    let to = self.by_value_part(slot, index);
                    self.instruction(format!("store {ty} {value}, ptr {to}"));
                }
            }
        }
    }

    /// The address of eightbyte `index` of a by-value temporary.
    fn by_value_part(&mut self, slot: &str, index: usize) -> String {
        if index == 0 {
            slot.to_owned()
        } else {
            self.value(format!(
                "getelementptr inbounds i8, ptr {slot}, i64 {}",
                index * 8
            ))
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
