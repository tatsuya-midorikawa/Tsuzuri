use super::*;
use crate::abi::{Buffer, out_result};
use crate::check::HostImport;

pub(super) fn header(module: &CheckedModule) -> String {
    let mut output = String::new();
    for function in &module.functions {
        let TypedExprKind::HostCall(import, _) = &function.body.kind else {
            continue;
        };
        let parameters = host_abi::c_parameters(function, module);
        let _ = writeln!(
            output,
            "{} {}({parameters});",
            if out_result(&function.signature.result) {
                "void".into()
            } else {
                c_type(&function.signature.result)
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
        for argument in arguments {
            let value = self.expression(argument);
            if argument.ty == Type::Unit {
                continue;
            }
            let inner = if let Type::Reference(inner, false) = &argument.ty {
                inner.as_ref()
            } else {
                &argument.ty
            };
            if Buffer::of(inner).is_some() {
                let descriptor = if matches!(inner, Type::Array(_)) {
                    value
                } else {
                    self.value(format!("load {}, ptr {value}", self.ty(inner)))
                };
                let pointer =
                    self.value(format!("extractvalue {} {descriptor}, 0", self.ty(inner)));
                let length = self.value(format!("extractvalue {} {descriptor}, 1", self.ty(inner)));
                types.extend(["ptr".into(), "i64".into()]);
                values.extend([format!("ptr {pointer}"), format!("i64 {length}")]);
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
        let output = if *result == Type::Unit || out.is_some() {
            "void".to_owned()
        } else {
            abi_type(result)
        };
        let attributes = if self.globals.wasm {
            format!(
                " \"wasm-import-module\"=\"tsuzuri\" \"wasm-import-name\"=\"{}\"",
                import.wasm_name
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
        if *result == Type::Unit || out.is_some() {
            self.instruction(call);
            return if let Some(slot) = out {
                self.read_host_result(result, &slot)
            } else {
                "0".into()
            };
        }
        let value = self.value(call);
        match result {
            Type::Bool => self.value(format!("icmp ne i32 {value}, 0")),
            Type::Integer(8 | 16, _) => {
                self.value(format!("trunc i32 {value} to {}", self.ty(result)))
            }
            _ => value,
        }
    }

    fn host_result_slot(&mut self, ty: &Type) -> String {
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

    fn read_host_result(&mut self, ty: &Type, slot: &str) -> String {
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
