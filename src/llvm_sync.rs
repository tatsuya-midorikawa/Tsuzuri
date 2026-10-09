use super::*;

/// The LLVM integer type that holds an `AtomicValue`, and its size in bytes. A `bool` is a byte in
/// the cell and an `i1` everywhere else, because an atomic instruction does not take `i1`.
fn atomic_storage(value: &Type) -> (String, u32) {
    match value {
        Type::Integer(bits, _) => (format!("i{bits}"), u32::from(*bits) / 8),
        Type::Bool => ("i8".into(), 1),
        _ => unreachable!("an Atomic holds an integer or a bool"),
    }
}

impl FunctionEmitter<'_, '_> {
    /// The cell of an `Atomic` or `Mutex` that the shared borrow `reference` points to: the data
    /// pointer of the record's one-element array. The cell lives in the buffer, so it stays where
    /// it is however the record is copied or moved (F10 D3).
    fn shared_cell(&mut self, record: &Type, reference: &str) -> String {
        let record = self.ty(record);
        let field = self.value(format!(
            "getelementptr inbounds {record}, ptr {reference}, i32 0, i32 0"
        ));
        let array = self.value(format!("load %tz.array, ptr {field}"));
        self.value(format!("extractvalue %tz.array {array}, 0"))
    }

    /// Defines the body of `Atomic.*`, `Mutex.create`, `Mutex.with_lock` and `Mutex.into_inner` and
    /// returns the value they return (F10). Every atomic operation is one sequentially consistent
    /// instruction, so the portable result is the same on every target; the default WASM
    /// target lowers them to plain operations because it runs one thread.
    pub(super) fn sync_builtin(&mut self, instance: &BuiltinInstance, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        let count = instance.builtin.scheme().parameters.len();
        let result = ty.after_arguments(count);
        let value = instance.types[0].clone();
        match instance.builtin {
            Builtin::AtomicCreate => {
                let (array, data) = self.allocate_array(&value, "1");
                self.instruction(format!("store {} %arg0, ptr {data}", self.ty(&value)));
                let record = self.ty(&result);
                self.value(format!(
                    "insertvalue {record} zeroinitializer, %tz.array {array}, 0"
                ))
            }
            Builtin::AtomicIntoInner => {
                let record = self.ty(&parameters[0]);
                let array = self.value(format!("extractvalue {record} %arg0, 0"));
                let data = self.value(format!("extractvalue %tz.array {array}, 0"));
                let inner = self.value(format!("load {}, ptr {data}", self.ty(&value)));
                self.instruction(format!("call void @tz.free(ptr {data})"));
                inner
            }
            builtin if builtin.is_atomic() => {
                self.atomic_operation(builtin, &value, parameters, &result)
            }
            Builtin::MutexCreate => {
                let tuple = Type::Tuple(vec![Type::Integer(32, false), value.clone()]);
                let (array, data) = self.allocate_array(&tuple, "1");
                let tuple_type = self.ty(&tuple);
                let cell = self.value(format!(
                    "insertvalue {tuple_type} zeroinitializer, i32 0, 0"
                ));
                let cell = self.value(format!(
                    "insertvalue {tuple_type} {cell}, {} %arg0, 1",
                    self.ty(&value)
                ));
                self.instruction(format!("store {tuple_type} {cell}, ptr {data}"));
                let record = self.ty(&result);
                self.value(format!(
                    "insertvalue {record} zeroinitializer, %tz.array {array}, 0"
                ))
            }
            Builtin::MutexIntoInner => {
                let record = self.ty(&parameters[0]);
                let tuple = Type::Tuple(vec![Type::Integer(32, false), value.clone()]);
                let array = self.value(format!("extractvalue {record} %arg0, 0"));
                let data = self.value(format!("extractvalue %tz.array {array}, 0"));
                let cell = self.value(format!("load {}, ptr {data}", self.ty(&tuple)));
                self.instruction(format!("call void @tz.free(ptr {data})"));
                self.value(format!("extractvalue {} {cell}, 1", self.ty(&tuple)))
            }
            Builtin::MutexWith => {
                let Type::Reference(record, false) = &parameters[0] else {
                    unreachable!("Mutex.with_lock borrows the mutex")
                };
                let tuple = Type::Tuple(vec![Type::Integer(32, false), value.clone()]);
                let cell = self.shared_cell(record, "%arg0");
                // The status is zero unless this thread holds a mutex already; the runtime has
                // told the user why, and the trap goes through the usual path so that a
                // boundary of `--trap-mode return` still catches it.
                let status = self.value(format!("call i32 @tsuzuri_mutex_lock(ptr {cell})"));
                let acquired = self.value(format!("icmp eq i32 {status}, 0"));
                self.guard(&acquired, TrapKind::Assert);
                let pointer = self.value(format!(
                    "getelementptr inbounds {}, ptr {cell}, i32 0, i32 1",
                    self.ty(&tuple)
                ));
                let borrow = Type::Reference(Box::new(value), true);
                let (output, _) =
                    self.apply_value("%arg1", &parameters[1], Some((&borrow, &pointer)), false);
                self.instruction(format!("call void @tsuzuri_mutex_unlock(ptr {cell})"));
                output
            }
            _ => unreachable!("not an Atomic or Mutex builtin"),
        }
    }

    /// One `Atomic` operation on the cell of the shared borrow `%arg0`.
    fn atomic_operation(
        &mut self,
        builtin: Builtin,
        value: &Type,
        parameters: &[Type],
        result: &Type,
    ) -> String {
        let (storage, align) = atomic_storage(value);
        let Type::Reference(record, false) = &parameters[0] else {
            unreachable!("an Atomic operation borrows the cell")
        };
        let cell = self.shared_cell(record, "%arg0");
        let boolean = *value == Type::Bool;
        // An operand as the cell holds it.
        let widen = |emitter: &mut Self, operand: &str| {
            if boolean {
                emitter.value(format!("zext i1 {operand} to i8"))
            } else {
                operand.to_owned()
            }
        };
        let narrow = |emitter: &mut Self, stored: String| {
            if boolean {
                emitter.value(format!("icmp ne i8 {stored}, 0"))
            } else {
                stored
            }
        };
        let read_modify_write = |emitter: &mut Self, operation: &str| {
            let operand = widen(emitter, "%arg1");
            let previous = emitter.value(format!(
                "atomicrmw {operation} ptr {cell}, {storage} {operand} seq_cst, align {align}"
            ));
            narrow(emitter, previous)
        };
        match builtin {
            Builtin::AtomicLoad => {
                let stored = self.value(format!(
                    "load atomic {storage}, ptr {cell} seq_cst, align {align}"
                ));
                narrow(self, stored)
            }
            Builtin::AtomicStore => {
                let operand = widen(self, "%arg1");
                self.instruction(format!(
                    "store atomic {storage} {operand}, ptr {cell} seq_cst, align {align}"
                ));
                "0".to_owned()
            }
            Builtin::AtomicSwap => read_modify_write(self, "xchg"),
            Builtin::AtomicFetchAdd => read_modify_write(self, "add"),
            Builtin::AtomicFetchSub => read_modify_write(self, "sub"),
            Builtin::AtomicFetchAnd => read_modify_write(self, "and"),
            Builtin::AtomicFetchOr => read_modify_write(self, "or"),
            Builtin::AtomicFetchXor => read_modify_write(self, "xor"),
            Builtin::AtomicCompareExchange => {
                let expected = widen(self, "%arg1");
                let desired = widen(self, "%arg2");
                let pair = self.value(format!(
                    "cmpxchg ptr {cell}, {storage} {expected}, {storage} {desired} seq_cst seq_cst, align {align}"
                ));
                let stored = self.value(format!("extractvalue {{ {storage}, i1 }} {pair}, 0"));
                let previous = narrow(self, stored);
                let exchanged = self.value(format!("extractvalue {{ {storage}, i1 }} {pair}, 1"));
                let (ok, error) = self.result_cases(result);
                let payload_type = self.ty(value);
                let written = self.construct_value(
                    result,
                    ok,
                    Some((previous.clone(), payload_type.clone())),
                );
                let kept = self.construct_value(result, error, Some((previous, payload_type)));
                self.value(format!(
                    "select i1 {exchanged}, {} {written}, {} {kept}",
                    self.ty(result),
                    self.ty(result)
                ))
            }
            _ => unreachable!("not an Atomic operation"),
        }
    }

    /// The tags of `Ok` and `Error` in a `Result` type.
    fn result_cases(&self, result: &Type) -> (usize, usize) {
        let Type::Union(id, _) = result else {
            unreachable!("a Result is a union")
        };
        let cases = &self.module.unions[*id].cases;
        let tag = |name: &str| {
            cases
                .iter()
                .position(|(case, _)| case == name)
                .expect("Result has Ok and Error")
        };
        (tag("Ok"), tag("Error"))
    }
}
