use super::*;

/// The bytes of a channel's header in the tzroutine runtime (`struct tzr_chan`, TZ01): the ring of
/// items follows it.
const CHANNEL_HEADER: u64 = 128;

impl FunctionEmitter<'_, '_> {
    /// Defines the bodies of `Channel.*` (TZ03), Go's two-way channel. The block is a header of the
    /// tzroutine runtime and the ring of items, which the runtime copies in and out under the
    /// channel's lock; the handle is a record of the block's address, counted by `share` and
    /// released by its drop. Items move through stack slots of this function, and the drop of the
    /// last handle drops the items that were never received.
    pub(super) fn go_channel_builtin(&mut self, instance: &BuiltinInstance, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        let count = instance.builtin.scheme().parameters.len();
        let result = ty.after_arguments(count);
        let item = instance.types[0].clone();
        if instance.builtin == Builtin::ChannelMake {
            return self.channel_make(&item, &result);
        }
        let Type::Reference(record, false) = &parameters[0] else {
            unreachable!("a Channel operation borrows the channel")
        };
        let (block, address) = self.channel_block(record, "%arg0");
        let declare = |emitter: &mut Self, declaration: &str| {
            emitter.intrinsics.insert(declaration.into());
        };
        match instance.builtin {
            Builtin::ChannelSend => {
                self.routine_unlocked();
                declare(self, "declare i32 @tsuzuri_tz_chan_send(ptr, ptr, i32)");
                let slot = self.spill(&item, "%arg1");
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_chan_send(ptr {block}, ptr {slot}, i32 1)"
                ));
                self.channel_trap_unless_ok(&status, 1);
                "0".to_owned()
            }
            Builtin::ChannelRecv => {
                self.routine_unlocked();
                declare(self, "declare i32 @tsuzuri_tz_chan_recv(ptr, ptr, i32)");
                let slot = self.slot(&item);
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_chan_recv(ptr {block}, ptr {slot}, i32 1)"
                ));
                self.channel_not_deadlocked(&status);
                // Closed and empty (status 1) leaves the slot as it was: it is read only on 0.
                let received = self.value(format!("icmp eq i32 {status}, 0"));
                self.channel_choice(
                    &result,
                    &received,
                    |emitter| {
                        let taken =
                            emitter.value(format!("load {}, ptr {slot}", emitter.ty(&item)));
                        emitter.channel_case(&result, "Some", Some((&item, &taken)))
                    },
                    |emitter| emitter.channel_case(&result, "None", None),
                )
            }
            Builtin::ChannelTrySend => {
                declare(self, "declare i32 @tsuzuri_tz_chan_send(ptr, ptr, i32)");
                let slot = self.spill(&item, "%arg1");
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_chan_send(ptr {block}, ptr {slot}, i32 0)"
                ));
                let Type::Union(_, arguments) = &result else {
                    unreachable!("Channel.try_send returns a Result")
                };
                let error = arguments[1].clone();
                let sent = self.value(format!("icmp eq i32 {status}, 0"));
                // A refused item is still in the slot: it goes back to the caller in the error.
                self.channel_choice(
                    &result,
                    &sent,
                    |emitter| emitter.channel_case(&result, "Ok", Some((&Type::Unit, "0"))),
                    |emitter| {
                        let returned =
                            emitter.value(format!("load {}, ptr {slot}", emitter.ty(&item)));
                        let full = emitter.value(format!("icmp eq i32 {status}, 3"));
                        let reason = emitter.channel_choice(
                            &error,
                            &full,
                            |emitter| {
                                emitter.channel_case(&error, "Full", Some((&item, &returned)))
                            },
                            |emitter| {
                                emitter.channel_case(&error, "Closed", Some((&item, &returned)))
                            },
                        );
                        emitter.channel_case(&result, "Error", Some((&error, &reason)))
                    },
                )
            }
            Builtin::ChannelTryRecv => {
                declare(self, "declare i32 @tsuzuri_tz_chan_recv(ptr, ptr, i32)");
                let slot = self.slot(&item);
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_chan_recv(ptr {block}, ptr {slot}, i32 0)"
                ));
                let received = self.value(format!("icmp eq i32 {status}, 0"));
                self.channel_choice(
                    &result,
                    &received,
                    |emitter| {
                        let taken =
                            emitter.value(format!("load {}, ptr {slot}", emitter.ty(&item)));
                        emitter.channel_case(&result, "Item", Some((&item, &taken)))
                    },
                    |emitter| {
                        let empty = emitter.value(format!("icmp eq i32 {status}, 3"));
                        emitter.channel_choice(
                            &result,
                            &empty,
                            |emitter| emitter.channel_case(&result, "Empty", None),
                            |emitter| emitter.channel_case(&result, "Disconnected", None),
                        )
                    },
                )
            }
            Builtin::ChannelClose => {
                declare(self, "declare i32 @tsuzuri_tz_chan_close(ptr)");
                let status = self.value(format!("call i32 @tsuzuri_tz_chan_close(ptr {block})"));
                self.channel_trap_unless_ok(&status, 2);
                "0".to_owned()
            }
            Builtin::ChannelLength | Builtin::ChannelCapacity => {
                let function = if instance.builtin == Builtin::ChannelLength {
                    "tsuzuri_tz_chan_length"
                } else {
                    "tsuzuri_tz_chan_capacity"
                };
                declare(self, &format!("declare i64 @{function}(ptr)"));
                self.value(format!("call i64 @{function}(ptr {block})"))
            }
            Builtin::ChannelShare => {
                declare(self, "declare void @tsuzuri_tz_chan_share(ptr)");
                self.instruction(format!("call void @tsuzuri_tz_chan_share(ptr {block})"));
                let handle = self.value(format!(
                    "insertvalue {} zeroinitializer, i64 {address}, 0",
                    self.ty(&result)
                ));
                // A new value of a type with a `Drop` instance is live, so that its drop runs.
                self.mark_live(&result, handle)
            }
            Builtin::ChannelRelease => {
                self.channel_release(&item, &block);
                "0".to_owned()
            }
            _ => unreachable!("not a Channel builtin"),
        }
    }

    /// `Channel.make`: the block, its header, and the one handle that holds its address.
    fn channel_make(&mut self, item: &Type, result: &Type) -> String {
        let valid = self.value("icmp sge i64 %arg0, 0".to_owned());
        self.guard(&valid, TrapKind::Assert);
        let element = self.ty(item);
        let size = format!("ptrtoint (ptr getelementptr ({element}, ptr null, i32 1) to i64)");
        let empty = self.value(format!("icmp eq i64 {size}, 0"));
        let stride = self.value(format!("select i1 {empty}, i64 1, i64 {size}"));
        let limit = self.value(format!(
            "udiv i64 {}, {stride}",
            i64::MAX as u64 - CHANNEL_HEADER
        ));
        let fits = self.value(format!("icmp ule i64 %arg0, {limit}"));
        self.guard(&fits, TrapKind::AllocationSize);
        let bytes = self.value(format!("mul i64 %arg0, {size}"));
        let total = self.value(format!("add i64 {bytes}, {CHANNEL_HEADER}"));
        let block = self.value(format!("call ptr @tz.alloc(i64 {total})"));
        self.intrinsics
            .insert("declare void @tsuzuri_tz_chan_init(ptr, i64, i64)".into());
        self.instruction(format!(
            "call void @tsuzuri_tz_chan_init(ptr {block}, i64 {size}, i64 %arg0)"
        ));
        let address = self.value(format!("ptrtoint ptr {block} to i64"));
        let handle = self.value(format!(
            "insertvalue {} zeroinitializer, i64 {address}, 0",
            self.ty(result)
        ));
        self.mark_live(result, handle)
    }

    /// The drop of a handle. The last one takes out the items that were never received and drops
    /// each once; nobody else holds the block then, so the runtime takes them without its lock.
    fn channel_release(&mut self, item: &Type, block: &str) {
        self.intrinsics
            .insert("declare i32 @tsuzuri_tz_chan_release(ptr)".into());
        let last = self.value(format!("call i32 @tsuzuri_tz_chan_release(ptr {block})"));
        let destroy = self.value(format!("icmp ne i32 {last}, 0"));
        let free = self.label();
        let done = self.label();
        self.branch(&destroy, &free, &done);
        self.begin(&free);
        if item.needs_drop(&self.module.types()) {
            self.intrinsics
                .insert("declare i32 @tsuzuri_tz_chan_drain(ptr, ptr)".into());
            let slot = self.slot(item);
            let drain = self.label();
            let left = self.label();
            let empty = self.label();
            self.jump(&drain);
            self.begin(&drain);
            let taken = self.value(format!(
                "call i32 @tsuzuri_tz_chan_drain(ptr {block}, ptr {slot})"
            ));
            let more = self.value(format!("icmp ne i32 {taken}, 0"));
            self.branch(&more, &left, &empty);
            self.begin(&left);
            let value = self.value(format!("load {}, ptr {slot}", self.ty(item)));
            self.drop_value(item, &value);
            self.jump(&drain);
            self.begin(&empty);
        }
        self.instruction(format!("call void @tz.free(ptr {block})"));
        self.jump(&done);
        self.begin(&done);
    }

    /// Traps unless the runtime's `status` is 0. A closed channel (1) first has the runtime write
    /// the message `report`; a deadlock (2) was reported by the detector.
    fn channel_trap_unless_ok(&mut self, status: &str, report: u32) {
        self.intrinsics
            .insert("declare void @tsuzuri_tz_report(i32)".into());
        let closed = self.value(format!("icmp eq i32 {status}, 1"));
        let tell = self.label();
        let check = self.label();
        self.branch(&closed, &tell, &check);
        self.begin(&tell);
        self.instruction(format!("call void @tsuzuri_tz_report(i32 {report})"));
        self.jump(&check);
        self.begin(&check);
        let ok = self.value(format!("icmp eq i32 {status}, 0"));
        self.guard(&ok, TrapKind::Assert);
    }

    /// The value of `ty` that `yes` or `no` builds, as `condition` says.
    fn channel_choice(
        &mut self,
        ty: &Type,
        condition: &str,
        yes: impl FnOnce(&mut Self) -> String,
        no: impl FnOnce(&mut Self) -> String,
    ) -> String {
        let first = self.label();
        let second = self.label();
        let done = self.label();
        self.branch(condition, &first, &second);
        self.begin(&first);
        let one = yes(self);
        let one_block = self.block.clone();
        self.jump(&done);
        self.begin(&second);
        let other = no(self);
        let other_block = self.block.clone();
        self.jump(&done);
        self.begin(&done);
        self.value(format!(
            "phi {} [ {one}, %{one_block} ], [ {other}, %{other_block} ]",
            self.ty(ty)
        ))
    }
}
