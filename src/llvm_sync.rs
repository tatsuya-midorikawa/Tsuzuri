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
        if instance.builtin.is_channel() {
            return self.channel_builtin(instance, ty);
        }
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

    /// A value of the std `Maybe` or `Result` union `result` with the named case.
    fn channel_case(
        &mut self,
        result: &Type,
        case: &str,
        payload: Option<(&Type, &str)>,
    ) -> String {
        let Type::Union(id, _) = result else {
            unreachable!("a channel result is a std union")
        };
        let index = self.module.unions[*id]
            .cases
            .iter()
            .position(|(name, _)| name == case)
            .expect("std case exists");
        let payload = payload.map(|(ty, value)| (value.to_owned(), self.ty(ty)));
        self.construct_value(result, index, payload)
    }

    /// The address of the field at `offset` of a channel block.
    fn channel_field(&mut self, block: &str, offset: u64) -> String {
        self.value(format!(
            "getelementptr inbounds i8, ptr {block}, i64 {offset}"
        ))
    }

    fn channel_load(&mut self, block: &str, offset: u64) -> String {
        let field = self.channel_field(block, offset);
        self.value(format!("load i64, ptr {field}, align 8"))
    }

    fn channel_store(&mut self, block: &str, offset: u64, value: &str) {
        let field = self.channel_field(block, offset);
        self.instruction(format!("store i64 {value}, ptr {field}, align 8"));
    }

    /// The block of the channel that the `Sender` or `Receiver` behind the pointer `reference`
    /// holds: the address is the record's one field.
    fn channel_block(&mut self, record: &Type, reference: &str) -> (String, String) {
        let record = self.ty(record);
        let field = self.value(format!(
            "getelementptr inbounds {record}, ptr {reference}, i32 0, i32 0"
        ));
        let address = self.value(format!("load i64, ptr {field}"));
        let block = self.value(format!("inttoptr i64 {address} to ptr"));
        (block, address)
    }

    /// Traps unless this thread holds no `Mutex` lock: a critical section never waits, so a
    /// channel operation, which may wait, is refused inside one on every schedule (F10 D7).
    fn channel_unlocked(&mut self) {
        let status = self.value("call i32 @tsuzuri_mutex_wait_ok()".to_owned());
        let free = self.value(format!("icmp ne i32 {status}, 0"));
        self.guard(&free, TrapKind::Assert);
    }

    /// Traps when the runtime found that every task is waiting (status 2).
    fn channel_not_deadlocked(&mut self, status: &str) {
        let progress = self.value(format!("icmp ne i32 {status}, 2"));
        self.guard(&progress, TrapKind::Assert);
    }

    /// Defines the bodies of `Channel.*` (F10 Phase 2). A channel is one block that the runtime
    /// of the target operates on under its own lock: a header of nine words, then the ring of
    /// items, which the runtime copies in and out. This code allocates and initializes the
    /// block, moves items through stack slots, and drops what is left when the last owner of
    /// the block closes it.
    fn channel_builtin(&mut self, instance: &BuiltinInstance, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        let count = instance.builtin.scheme().parameters.len();
        let result = ty.after_arguments(count);
        let item = instance.types[0].clone();
        match instance.builtin {
            Builtin::ChannelBounded => self.channel_bounded(&item, &result),
            Builtin::ChannelSend => {
                let Type::Reference(record, false) = &parameters[0] else {
                    unreachable!("Channel.send borrows the sender")
                };
                let (block, _) = self.channel_block(record, "%arg0");
                self.channel_unlocked();
                let slot = self.spill(&item, "%arg1");
                let status = self.value(format!(
                    "call i32 @tsuzuri_channel_send(ptr {block}, ptr {slot})"
                ));
                self.channel_not_deadlocked(&status);
                let sent = self.value(format!("icmp eq i32 {status}, 0"));
                let accepted = self.label();
                let refused = self.label();
                let done = self.label();
                self.branch(&sent, &accepted, &refused);
                self.begin(&accepted);
                let ok = self.channel_case(&result, "Ok", Some((&Type::Unit, "0")));
                let accepted_block = self.block.clone();
                self.jump(&done);
                // Nobody will receive the item: it goes back to the caller.
                self.begin(&refused);
                let returned = self.value(format!("load {}, ptr {slot}", self.ty(&item)));
                let error = self.channel_case(&result, "Error", Some((&item, &returned)));
                let refused_block = self.block.clone();
                self.jump(&done);
                self.begin(&done);
                self.value(format!(
                    "phi {} [ {ok}, %{accepted_block} ], [ {error}, %{refused_block} ]",
                    self.ty(&result)
                ))
            }
            Builtin::ChannelRecv => {
                let Type::Reference(record, false) = &parameters[0] else {
                    unreachable!("Channel.recv borrows the receiver")
                };
                let (block, _) = self.channel_block(record, "%arg0");
                self.channel_unlocked();
                let slot = self.slot(&item);
                let status = self.value(format!(
                    "call i32 @tsuzuri_channel_recv(ptr {block}, ptr {slot})"
                ));
                self.channel_not_deadlocked(&status);
                let received = self.value(format!("icmp eq i32 {status}, 0"));
                let some = self.label();
                let none = self.label();
                let done = self.label();
                self.branch(&received, &some, &none);
                self.begin(&some);
                let taken = self.value(format!("load {}, ptr {slot}", self.ty(&item)));
                let present = self.channel_case(&result, "Some", Some((&item, &taken)));
                let some_block = self.block.clone();
                self.jump(&done);
                self.begin(&none);
                let absent = self.channel_case(&result, "None", None);
                let none_block = self.block.clone();
                self.jump(&done);
                self.begin(&done);
                self.value(format!(
                    "phi {} [ {present}, %{some_block} ], [ {absent}, %{none_block} ]",
                    self.ty(&result)
                ))
            }
            Builtin::ChannelCloneSender => {
                let Type::Reference(record, false) = &parameters[0] else {
                    unreachable!("Channel.clone_sender borrows the sender")
                };
                let (block, address) = self.channel_block(record, "%arg0");
                self.instruction(format!(
                    "call void @tsuzuri_channel_clone_sender(ptr {block})"
                ));
                let sender = self.value(format!(
                    "insertvalue {} zeroinitializer, i64 {address}, 0",
                    self.ty(&result)
                ));
                // A new value of a type with a `Drop` instance is live, so that its drop runs.
                self.mark_live(&result, sender)
            }
            Builtin::ChannelCloseSender | Builtin::ChannelCloseReceiver => {
                let Type::Reference(record, false) = &parameters[0] else {
                    unreachable!("a close primitive borrows the end")
                };
                let kind = u32::from(instance.builtin == Builtin::ChannelCloseReceiver);
                let (block, _) = self.channel_block(record, "%arg0");
                self.channel_close(&item, &block, kind);
                "0".to_owned()
            }
            _ => unreachable!("not a Channel builtin"),
        }
    }

    /// `Channel.bounded`: the block and its header, and the two ends that hold its address.
    fn channel_bounded(&mut self, item: &Type, result: &Type) -> String {
        let positive = self.value("icmp sgt i64 %arg0, 0".to_owned());
        self.guard(&positive, TrapKind::Assert);
        let element = self.ty(item);
        let size = format!("ptrtoint (ptr getelementptr ({element}, ptr null, i32 1) to i64)");
        let empty = self.value(format!("icmp eq i64 {size}, 0"));
        let stride = self.value(format!("select i1 {empty}, i64 1, i64 {size}"));
        let limit = self.value(format!(
            "udiv i64 {}, {stride}",
            i64::MAX as u64 - CHANNEL_ITEMS
        ));
        // Unsigned comparison rejects the capacities that do not fit in a signed word.
        let fits = self.value(format!("icmp ule i64 %arg0, {limit}"));
        self.guard(&fits, TrapKind::AllocationSize);
        let bytes = self.value(format!("mul i64 %arg0, {stride}"));
        let total = self.value(format!("add i64 {bytes}, {CHANNEL_ITEMS}"));
        let block = self.value(format!("call ptr @tz.alloc(i64 {total})"));
        for (offset, value) in [
            (CHANNEL_CAPACITY, "%arg0"),
            (CHANNEL_ITEM_SIZE, size.as_str()),
            (CHANNEL_HEAD, "0"),
            (CHANNEL_COUNT, "0"),
            (CHANNEL_SENDERS, "1"),
            (CHANNEL_RECEIVERS, "1"),
            (CHANNEL_OWNERS, "2"),
            (CHANNEL_WAITERS, "0"),
            (CHANNEL_WAITERS + 8, "0"),
        ] {
            self.channel_store(&block, offset, value);
        }
        let address = self.value(format!("ptrtoint ptr {block} to i64"));
        let Type::Tuple(ends) = result else {
            unreachable!("Channel.bounded returns the two ends")
        };
        let sender = self.value(format!(
            "insertvalue {} zeroinitializer, i64 {address}, 0",
            self.ty(&ends[0])
        ));
        let sender = self.mark_live(&ends[0], sender);
        let receiver = self.value(format!(
            "insertvalue {} zeroinitializer, i64 {address}, 0",
            self.ty(&ends[1])
        ));
        let receiver = self.mark_live(&ends[1], receiver);
        let pair = self.value(format!(
            "insertvalue {} zeroinitializer, {} {sender}, 0",
            self.ty(result),
            self.ty(&ends[0])
        ));
        self.value(format!(
            "insertvalue {} {pair}, {} {receiver}, 1",
            self.ty(result),
            self.ty(&ends[1])
        ))
    }

    /// Releases one `Sender` (kind 0) or `Receiver` (kind 1). The owner that releases the last
    /// handle drops the items that were never received, each exactly once, and frees the block.
    fn channel_close(&mut self, item: &Type, block: &str, kind: u32) {
        let last = self.value(format!(
            "call i32 @tsuzuri_channel_close(ptr {block}, i32 {kind})"
        ));
        let destroy = self.value(format!("icmp ne i32 {last}, 0"));
        let free = self.label();
        let done = self.label();
        self.branch(&destroy, &free, &done);
        self.begin(&free);
        let count = self.channel_load(block, CHANNEL_COUNT);
        let head = self.channel_load(block, CHANNEL_HEAD);
        let capacity = self.channel_load(block, CHANNEL_CAPACITY);
        let size = self.channel_load(block, CHANNEL_ITEM_SIZE);
        let empty = self.value(format!("icmp eq i64 {size}, 0"));
        let stride = self.value(format!("select i1 {empty}, i64 1, i64 {size}"));
        let items = self.channel_field(block, CHANNEL_ITEMS);
        let item = item.clone();
        self.array_loop(&count, |emitter, offset| {
            let position = emitter.value(format!("add i64 {head}, {offset}"));
            let index = emitter.value(format!("urem i64 {position}, {capacity}"));
            let at = emitter.value(format!("mul i64 {index}, {stride}"));
            let slot = emitter.value(format!("getelementptr inbounds i8, ptr {items}, i64 {at}"));
            let left = emitter.value(format!("load {}, ptr {slot}", emitter.ty(&item)));
            emitter.drop_value(&item, &left);
        });
        self.instruction(format!("call void @tz.free(ptr {block})"));
        self.jump(&done);
        self.begin(&done);
    }
}

/// Byte offsets in a channel block. The runtimes (`task.c`, `task-wasm-threads.c`,
/// `channel-wasm.ll`) lay it out the same way: the capacity, the size of an item, the index of
/// the oldest item, the number of items, the live senders, the live receivers, the live handles
/// of either kind, two words for the runtime's waiting lists, and then the ring of items.
const CHANNEL_CAPACITY: u64 = 0;
const CHANNEL_ITEM_SIZE: u64 = 8;
const CHANNEL_HEAD: u64 = 16;
const CHANNEL_COUNT: u64 = 24;
const CHANNEL_SENDERS: u64 = 32;
const CHANNEL_RECEIVERS: u64 = 40;
const CHANNEL_OWNERS: u64 = 48;
const CHANNEL_WAITERS: u64 = 64;
const CHANNEL_ITEMS: u64 = 80;
