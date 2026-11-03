use super::*;

/// The block of a `WaitGroup`, `Once` or `Cond` handle: the count of its handles and the word that
/// the runtime waits on. A `Semaphore`'s block also keeps its capacity.
const HANDLE_BYTES: u64 = 16;
const SEMAPHORE_BYTES: u64 = 24;
const WORD: u64 = 8;
const CAPACITY: u64 = 16;

/// The numbers of `tsuzuri_tz_report` (src/runtime/tzroutine.c).
const NEGATIVE_WAIT_GROUP: u32 = 3;
const MUTEX_AGAIN: u32 = 4;
const RW_MUTEX_AGAIN: u32 = 6;
const SEMAPHORE_OVER: u32 = 7;
const SEMAPHORE_WEIGHT: u32 = 8;

/// The start function of `WaitGroup.go`: runs the boxed owned function once (as `Owned.call`, the
/// environment borrowed), drops it, takes one from the count (a count below zero reports 3 and
/// traps), lets go of its handle of the group and frees the box. The box is the closure and the
/// group's block.
const WAIT_GROUP_GO_START: &str = "define internal void @tz.waitgroup.go(ptr %box) {\nentry:\n  %closure = load %tz.closure, ptr %box\n  %desc = extractvalue %tz.closure %closure, 0\n  %code = load ptr, ptr %desc\n  %env = extractvalue %tz.closure %closure, 1\n  %unit = call i8 %code(i8 0, ptr %env, i1 true)\n  call void @tz.closure.drop(%tz.closure %closure)\n  %field = getelementptr inbounds i8, ptr %box, i64 16\n  %block = load ptr, ptr %field\n  call void @tz.free(ptr %box)\n  %word = getelementptr inbounds i8, ptr %block, i64 8\n  %status = call i32 @tsuzuri_tz_waitgroup_add(ptr %word, i64 -1)\n  %ok = icmp eq i32 %status, 0\n  br i1 %ok, label %release, label %negative\nnegative:\n  call void @tsuzuri_tz_report(i32 3)\n  call void @llvm.trap()\n  unreachable\nrelease:\n  %previous = atomicrmw sub ptr %block, i64 1 acq_rel\n  %last = icmp eq i64 %previous, 1\n  br i1 %last, label %free, label %done\nfree:\n  call void @tz.free(ptr %block)\n  br label %done\ndone:\n  ret void\n}\n";

impl FunctionEmitter<'_, '_> {
    /// Defines the bodies of the sync types of TZ06. A handle (`WaitGroup`, `Once`, `Cond`,
    /// `Semaphore`) is a record of the address of a block that this code allocates and counts:
    /// `share` adds one to the count and the drop of the last handle frees it, so the runtime only
    /// waits on the word of the block. `OnceValue` and `RwMutex` are cells in an array, as `Mutex`
    /// is, whose first part is the runtime's word.
    pub(super) fn wait_builtin(&mut self, instance: &BuiltinInstance, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        let builtin = instance.builtin;
        let count = builtin.scheme().parameters.len();
        let result = ty.after_arguments(count);
        match builtin {
            Builtin::WaitGroupCreate | Builtin::OnceCreate | Builtin::CondCreate => {
                self.new_handle(&result, HANDLE_BYTES, "0", None)
            }
            Builtin::SemaphoreCreate => {
                self.semaphore_weight_check("%arg0", None);
                self.new_handle(&result, SEMAPHORE_BYTES, "%arg0", Some("%arg0"))
            }
            Builtin::WaitGroupShare
            | Builtin::OnceShare
            | Builtin::CondShare
            | Builtin::SemaphoreShare => {
                let (block, address) = self.handle_of(&parameters[0]);
                self.instruction(format!("atomicrmw add ptr {block}, i64 1 monotonic"));
                let handle = self.value(format!(
                    "insertvalue {} zeroinitializer, i64 {address}, 0",
                    self.ty(&result)
                ));
                self.mark_live(&result, handle)
            }
            Builtin::WaitGroupDrop
            | Builtin::OnceDrop
            | Builtin::CondDrop
            | Builtin::SemaphoreDrop => {
                let (block, _) = self.handle_of(&parameters[0]);
                let previous = self.value(format!("atomicrmw sub ptr {block}, i64 1 acq_rel"));
                let last = self.value(format!("icmp eq i64 {previous}, 1"));
                let free = self.label();
                let done = self.label();
                self.branch(&last, &free, &done);
                self.begin(&free);
                self.instruction(format!("call void @tz.free(ptr {block})"));
                self.jump(&done);
                self.begin(&done);
                "0".to_owned()
            }
            Builtin::WaitGroupAdd => {
                let word = self.handle_word(&parameters[0]);
                self.declare_wait("declare i32 @tsuzuri_tz_waitgroup_add(ptr, i64)");
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_waitgroup_add(ptr {word}, i64 %arg1)"
                ));
                self.channel_trap_unless_ok(&status, NEGATIVE_WAIT_GROUP);
                "0".to_owned()
            }
            Builtin::WaitGroupGo => {
                let owned = self.ty(&parameters[1]);
                let closure = self.value(format!("extractvalue {owned} %arg1, 0"));
                let (block, _) = self.handle_of(&parameters[0]);
                let word = self.block_field(&block, WORD);
                self.declare_wait("declare i32 @tsuzuri_tz_waitgroup_add(ptr, i64)");
                self.declare_wait("declare void @tsuzuri_tz_report(i32)");
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_waitgroup_add(ptr {word}, i64 1)"
                ));
                self.channel_trap_unless_ok(&status, NEGATIVE_WAIT_GROUP);
                // The tzroutine's own handle of the group.
                self.instruction(format!("atomicrmw add ptr {block}, i64 1 monotonic"));
                let boxed = self.value("call ptr @tz.alloc(i64 24)");
                self.instruction(format!("store %tz.closure {closure}, ptr {boxed}"));
                let field = self.block_field(&boxed, 16);
                self.instruction(format!("store ptr {block}, ptr {field}"));
                self.intrinsics.insert(WAIT_GROUP_GO_START.into());
                self.declare_wait("declare void @tsuzuri_tz_go(ptr, ptr)");
                self.instruction(format!(
                    "call void @tsuzuri_tz_go(ptr @tz.waitgroup.go, ptr {boxed})"
                ));
                "0".to_owned()
            }
            Builtin::WaitGroupWait => {
                let word = self.handle_word(&parameters[0]);
                self.declare_wait("declare i32 @tsuzuri_tz_waitgroup_wait(ptr)");
                let status = self.value(format!("call i32 @tsuzuri_tz_waitgroup_wait(ptr {word})"));
                self.channel_not_deadlocked(&status);
                "0".to_owned()
            }
            Builtin::OnceBegin => {
                let word = self.handle_word(&parameters[0]);
                self.declare_wait("declare i32 @tsuzuri_tz_once_begin(ptr)");
                let status = self.value(format!("call i32 @tsuzuri_tz_once_begin(ptr {word})"));
                self.channel_not_deadlocked(&status);
                self.value(format!("icmp eq i32 {status}, 1"))
            }
            Builtin::OnceEnd => {
                let word = self.handle_word(&parameters[0]);
                self.declare_wait("declare void @tsuzuri_tz_once_end(ptr)");
                self.instruction(format!("call void @tsuzuri_tz_once_end(ptr {word})"));
                "0".to_owned()
            }
            Builtin::OnceDone => {
                let word = self.handle_word(&parameters[0]);
                let state = self.value(format!("load atomic i64, ptr {word} acquire, align 8"));
                self.value(format!("icmp eq i64 {state}, 2"))
            }
            Builtin::CondSignal | Builtin::CondBroadcast => {
                let word = self.handle_word(&parameters[0]);
                self.declare_wait("declare void @tsuzuri_tz_cond_wake(ptr, i64)");
                let woken = if builtin == Builtin::CondSignal {
                    1
                } else {
                    -1
                };
                self.instruction(format!(
                    "call void @tsuzuri_tz_cond_wake(ptr {word}, i64 {woken})"
                ));
                "0".to_owned()
            }
            Builtin::CondWaitWith => self.cond_wait_with(instance, parameters),
            Builtin::SemaphoreAcquire | Builtin::SemaphoreTryAcquire => {
                let (block, _) = self.handle_of(&parameters[0]);
                let capacity = self.block_load(&block, CAPACITY);
                self.semaphore_weight_check("%arg1", Some(&capacity));
                let word = self.block_field(&block, WORD);
                self.declare_wait("declare i32 @tsuzuri_tz_sem_acquire(ptr, i64, i32)");
                let wait = i32::from(builtin == Builtin::SemaphoreAcquire);
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_sem_acquire(ptr {word}, i64 %arg1, i32 {wait})"
                ));
                if builtin == Builtin::SemaphoreAcquire {
                    self.channel_not_deadlocked(&status);
                    "0".to_owned()
                } else {
                    self.value(format!("icmp eq i32 {status}, 0"))
                }
            }
            Builtin::SemaphoreRelease => {
                let (block, _) = self.handle_of(&parameters[0]);
                let capacity = self.block_load(&block, CAPACITY);
                self.semaphore_weight_check("%arg1", Some(&capacity));
                let word = self.block_field(&block, WORD);
                self.declare_wait("declare i32 @tsuzuri_tz_sem_release(ptr, i64, i64)");
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_sem_release(ptr {word}, i64 %arg1, i64 {capacity})"
                ));
                self.channel_trap_unless_ok(&status, SEMAPHORE_OVER);
                "0".to_owned()
            }
            Builtin::OnceValueCreate | Builtin::RwMutexCreate => {
                let value = instance.types[0].clone();
                let (stored, initial) = if builtin == Builtin::OnceValueCreate {
                    let maybe = self.once_value_maybe(&result);
                    let none = self.channel_case(&maybe, "None", None);
                    (maybe, none)
                } else {
                    (value.clone(), "%arg0".to_owned())
                };
                let tuple = Type::Tuple(vec![Type::Integer(64, false), stored.clone()]);
                let (array, data) = self.allocate_array(&tuple, "1");
                let tuple_type = self.ty(&tuple);
                let cell = self.value(format!(
                    "insertvalue {tuple_type} zeroinitializer, i64 0, 0"
                ));
                let cell = self.value(format!(
                    "insertvalue {tuple_type} {cell}, {} {initial}, 1",
                    self.ty(&stored)
                ));
                self.instruction(format!("store {tuple_type} {cell}, ptr {data}"));
                let record = self.ty(&result);
                self.value(format!(
                    "insertvalue {record} zeroinitializer, %tz.array {array}, 0"
                ))
            }
            Builtin::OnceValueIntoInner | Builtin::RwMutexIntoInner => {
                let stored = if builtin == Builtin::OnceValueIntoInner {
                    result.clone()
                } else {
                    instance.types[0].clone()
                };
                let record = self.ty(&parameters[0]);
                let tuple = Type::Tuple(vec![Type::Integer(64, false), stored]);
                let array = self.value(format!("extractvalue {record} %arg0, 0"));
                let data = self.value(format!("extractvalue %tz.array {array}, 0"));
                let cell = self.value(format!("load {}, ptr {data}", self.ty(&tuple)));
                self.instruction(format!("call void @tz.free(ptr {data})"));
                self.value(format!("extractvalue {} {cell}, 1", self.ty(&tuple)))
            }
            Builtin::OnceValueGet => self.once_value_get(instance, parameters),
            Builtin::SyncMapReplace => {
                let value = self.ty(&instance.types[0]);
                let old = self.value(format!("load {value}, ptr %arg1"));
                self.instruction(format!("store {value} %arg0, ptr %arg1"));
                old
            }
            Builtin::RwMutexWithRead | Builtin::RwMutexWithWrite => {
                let Type::Reference(record, false) = &parameters[0] else {
                    unreachable!("an RwMutex operation borrows the lock")
                };
                let value = instance.types[0].clone();
                let tuple = Type::Tuple(vec![Type::Integer(64, false), value.clone()]);
                let cell = self.shared_cell(record, "%arg0");
                let write = builtin == Builtin::RwMutexWithWrite;
                let (lock, unlock) = if write {
                    ("tsuzuri_tz_rw_write_lock", "tsuzuri_tz_rw_write_unlock")
                } else {
                    ("tsuzuri_tz_rw_read_lock", "tsuzuri_tz_rw_read_unlock")
                };
                self.runtime_lock(lock, &cell, RW_MUTEX_AGAIN);
                let pointer = self.value(format!(
                    "getelementptr inbounds {}, ptr {cell}, i32 0, i32 1",
                    self.ty(&tuple)
                ));
                // A shared borrow of an array is its slice value; an exclusive one is its place.
                let lent = if write {
                    pointer
                } else {
                    self.borrowed_element(&value, &pointer)
                };
                let borrow = Type::Reference(Box::new(value), write);
                let (output, _) =
                    self.apply_value("%arg1", &parameters[1], Some((&borrow, &lent)), false);
                self.runtime_unlock(unlock, &cell);
                output
            }
            _ => unreachable!("not a sync builtin of TZ06"),
        }
    }

    /// Takes the runtime's lock `function` of the word at `cell`: 1 is the lock taken again by
    /// its holder (the runtime writes the message `report` and this traps), 2 a deadlock.
    pub(super) fn runtime_lock(&mut self, function: &str, cell: &str, report: u32) {
        self.declare_wait(&format!("declare i32 @{function}(ptr)"));
        let status = self.value(format!("call i32 @{function}(ptr {cell})"));
        self.channel_trap_unless_ok(&status, report);
    }

    pub(super) fn runtime_unlock(&mut self, function: &str, cell: &str) {
        self.declare_wait(&format!("declare void @{function}(ptr)"));
        self.instruction(format!("call void @{function}(ptr {cell})"));
    }

    fn declare_wait(&mut self, declaration: &str) {
        self.intrinsics.insert(declaration.into());
    }

    /// A new handle of `bytes`: one owner, the word `word` and, for a `Semaphore`, its capacity.
    fn new_handle(
        &mut self,
        result: &Type,
        bytes: u64,
        word: &str,
        capacity: Option<&str>,
    ) -> String {
        let block = self.value(format!("call ptr @tz.alloc(i64 {bytes})"));
        self.instruction(format!("store i64 1, ptr {block}, align 8"));
        let field = self.block_field(&block, WORD);
        self.instruction(format!("store i64 {word}, ptr {field}, align 8"));
        if let Some(capacity) = capacity {
            let field = self.block_field(&block, CAPACITY);
            self.instruction(format!("store i64 {capacity}, ptr {field}, align 8"));
        }
        let address = self.value(format!("ptrtoint ptr {block} to i64"));
        let handle = self.value(format!(
            "insertvalue {} zeroinitializer, i64 {address}, 0",
            self.ty(result)
        ));
        // A new value of a type with a `Drop` instance is live, so that its drop runs.
        self.mark_live(result, handle)
    }

    /// The block and the address of the borrowed handle `%arg0`.
    fn handle_of(&mut self, parameter: &Type) -> (String, String) {
        let Type::Reference(record, false) = parameter else {
            unreachable!("a handle operation borrows the handle")
        };
        self.channel_block(record, "%arg0")
    }

    fn handle_word(&mut self, parameter: &Type) -> String {
        let (block, _) = self.handle_of(parameter);
        self.block_field(&block, WORD)
    }

    fn block_field(&mut self, block: &str, offset: u64) -> String {
        self.value(format!(
            "getelementptr inbounds i8, ptr {block}, i64 {offset}"
        ))
    }

    fn block_load(&mut self, block: &str, offset: u64) -> String {
        let field = self.block_field(block, offset);
        self.value(format!("load i64, ptr {field}, align 8"))
    }

    /// Traps, with the runtime's message, unless `weight` is not negative and (for an acquire or
    /// a release) at most the capacity.
    fn semaphore_weight_check(&mut self, weight: &str, capacity: Option<&str>) {
        let valid = match capacity {
            // Unsigned: a negative weight is larger than any capacity.
            Some(capacity) => self.value(format!("icmp ule i64 {weight}, {capacity}")),
            None => self.value(format!("icmp sge i64 {weight}, 0")),
        };
        let tell = self.label();
        let check = self.label();
        self.branch(&valid, &check, &tell);
        self.begin(&tell);
        self.declare_wait("declare void @tsuzuri_tz_report(i32)");
        self.instruction(format!(
            "call void @tsuzuri_tz_report(i32 {SEMAPHORE_WEIGHT})"
        ));
        self.jump(&check);
        self.begin(&check);
        self.guard(&valid, TrapKind::Assert);
    }

    /// The `Maybe<'a>` of the cell of the `OnceValue<'a>` record `record`.
    fn once_value_maybe(&self, record: &Type) -> Type {
        let record = match record {
            Type::Reference(inner, _) => inner,
            other => other,
        };
        let Type::Record(id, arguments) = record else {
            unreachable!("a OnceValue is a record")
        };
        let fields = self.module.types().record_fields(*id, arguments);
        let Type::Array(tuple) = &fields[0] else {
            unreachable!("the cell of a OnceValue is an array")
        };
        let Type::Tuple(parts) = tuple.as_ref() else {
            unreachable!("the cell of a OnceValue is a tuple")
        };
        parts[1].clone()
    }

    /// `OnceValue.get`: once the state says done, the value is there and is borrowed. Otherwise the
    /// runtime picks the one caller that makes it, which stores it as `Some` (the cell held `None`,
    /// which has nothing to drop) and publishes it; the others wait until it is done. The function
    /// is called at most once and dropped in every case.
    fn once_value_get(&mut self, instance: &BuiltinInstance, parameters: &[Type]) -> String {
        let Type::Reference(record, false) = &parameters[0] else {
            unreachable!("OnceValue.get borrows the cell")
        };
        let value = instance.types[0].clone();
        let maybe = self.once_value_maybe(record);
        let tuple = Type::Tuple(vec![Type::Integer(64, false), maybe.clone()]);
        let tuple_type = self.ty(&tuple);
        let cell = self.shared_cell(record, "%arg0");
        let state = self.value(format!("load atomic i64, ptr {cell} acquire, align 8"));
        let done = self.value(format!("icmp eq i64 {state}, 2"));
        let start = self.label();
        let make = self.label();
        let ready = self.label();
        self.branch(&done, &ready, &start);
        self.begin(&start);
        self.declare_wait("declare i32 @tsuzuri_tz_once_begin(ptr)");
        self.declare_wait("declare void @tsuzuri_tz_once_end(ptr)");
        let status = self.value(format!("call i32 @tsuzuri_tz_once_begin(ptr {cell})"));
        self.channel_not_deadlocked(&status);
        let first = self.value(format!("icmp eq i32 {status}, 1"));
        self.branch(&first, &make, &ready);
        self.begin(&make);
        let (made, _) = self.apply_value("%arg1", &parameters[1], Some((&Type::Unit, "0")), true);
        let some = self.channel_case(&maybe, "Some", Some((&value, &made)));
        let slot = self.value(format!(
            "getelementptr inbounds {tuple_type}, ptr {cell}, i32 0, i32 1"
        ));
        self.instruction(format!("store {} {some}, ptr {slot}", self.ty(&maybe)));
        self.instruction(format!("call void @tsuzuri_tz_once_end(ptr {cell})"));
        self.jump(&ready);
        self.begin(&ready);
        self.drop_value(&parameters[1], "%arg1");
        let slot = self.value(format!(
            "getelementptr inbounds {tuple_type}, ptr {cell}, i32 0, i32 1"
        ));
        let payload = self.payload_pointer(&maybe, &slot);
        self.borrowed_element(&value, &payload)
    }

    /// `Cond.wait_with cond lock ready body`: under the lock, reads the condition's sequence
    /// number and asks the predicate; while it is false, the runtime releases the lock, waits for
    /// the number to change and takes the lock again. Then the body runs under the lock. The
    /// predicate is borrowed for each call and dropped at the end.
    fn cond_wait_with(&mut self, instance: &BuiltinInstance, parameters: &[Type]) -> String {
        let Type::Reference(lock, false) = &parameters[1] else {
            unreachable!("Cond.wait_with borrows the mutex")
        };
        let value = instance.types[0].clone();
        let tuple = Type::Tuple(vec![Type::Integer(32, false), value.clone()]);
        let sequence = self.handle_word(&parameters[0]);
        let cell = self.shared_cell(lock, "%arg1");
        self.runtime_lock("tsuzuri_tz_mutex32_lock", &cell, MUTEX_AGAIN);
        let pointer = self.value(format!(
            "getelementptr inbounds {}, ptr {cell}, i32 0, i32 1",
            self.ty(&tuple)
        ));
        let check = self.label();
        let wait = self.label();
        let run = self.label();
        self.jump(&check);
        self.begin(&check);
        let observed = self.value(format!("load atomic i64, ptr {sequence} acquire, align 8"));
        let shared = Type::Reference(Box::new(value.clone()), false);
        let viewed = self.borrowed_element(&value, &pointer);
        let (holds, _) = self.apply_value("%arg2", &parameters[2], Some((&shared, &viewed)), true);
        self.branch(&holds, &run, &wait);
        self.begin(&wait);
        self.declare_wait("declare i32 @tsuzuri_tz_cond_wait(ptr, ptr, i64)");
        let status = self.value(format!(
            "call i32 @tsuzuri_tz_cond_wait(ptr {sequence}, ptr {cell}, i64 {observed})"
        ));
        self.channel_trap_unless_ok(&status, MUTEX_AGAIN);
        self.jump(&check);
        self.begin(&run);
        self.drop_value(&parameters[2], "%arg2");
        let exclusive = Type::Reference(Box::new(value), true);
        let (output, _) =
            self.apply_value("%arg3", &parameters[3], Some((&exclusive, &pointer)), false);
        self.runtime_unlock("tsuzuri_tz_mutex32_unlock", &cell);
        output
    }
}
