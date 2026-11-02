use super::*;

/// The bytes of a timer's channel: the runtime's 128-byte header and the ring of one `i64`.
const TIMER_CHANNEL_BYTES: u64 = 128 + 8;

/// The kinds of `tzr_timer` in `src/runtime/tzroutine.c`.
const SEND: u32 = 1;
const TICK: u32 = 2;
const FUNC: u32 = 3;

/// Drops an owned function that a timer never called, and frees its box: the counterpart of
/// `@tz.tzroutine.owned`, which calls it.
const ROUTINE_OWNED_DROP: &str = "define internal void @tz.tzroutine.owned.drop(ptr %box) {\nentry:\n  %closure = load %tz.closure, ptr %box\n  call void @tz.closure.drop(%tz.closure %closure)\n  call void @tz.free(ptr %box)\n  ret void\n}\n";

const TIMER_NEW: &str =
    "declare ptr @tsuzuri_tz_timer_new(ptr, ptr, i64, i64, i32, i32, ptr, ptr, ptr)";

impl FunctionEmitter<'_, '_> {
    /// Defines the bodies of the timers (TZ05): `Time.sleep`'s wait and the timers of `Time.after`,
    /// `Time.tick`, `Time.after_func`, `Timer` and `Ticker`, which the tzroutine runtime keeps in
    /// its heap and fires on its timer thread. A timer's channel is allocated here, so that it is
    /// counted as the program's, and the runtime frees it with `@tz.free` when it lets go of the last
    /// reference. A `Timer` or `Ticker` is a record of the runtime's timer.
    pub(super) fn timer_builtin(&mut self, builtin: Builtin, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        let count = builtin.scheme().parameters.len();
        let result = ty.after_arguments(count);
        match builtin {
            Builtin::TimeSleepNs => {
                self.time_sleep();
                "0".to_owned()
            }
            Builtin::TimeAfter => {
                let block = self.timer_new("%arg0", "0", SEND, false, None);
                self.channel_handle(&result, &block)
            }
            Builtin::TimeTick => {
                self.positive_period("%arg0");
                let block = self.timer_new("%arg0", "%arg0", TICK, false, None);
                self.channel_handle(&result, &block)
            }
            Builtin::TimerCreate | Builtin::TickerCreate => {
                let (period, kind) = if builtin == Builtin::TickerCreate {
                    self.positive_period("%arg0");
                    ("%arg0", TICK)
                } else {
                    ("0", SEND)
                };
                let timer = self.timer_new("%arg0", period, kind, true, None);
                self.timer_handle(&result, &timer)
            }
            Builtin::TimeAfterFunc => {
                let owned = self.ty(&parameters[1]);
                let closure = self.value(format!("extractvalue {owned} %arg1, 0"));
                let boxed = self.value("call ptr @tz.alloc(i64 16)");
                self.instruction(format!("store %tz.closure {closure}, ptr {boxed}"));
                self.intrinsics.insert(super::ROUTINE_OWNED_START.into());
                self.intrinsics.insert(ROUTINE_OWNED_DROP.into());
                let timer = self.timer_new("%arg0", "0", FUNC, true, Some(&boxed));
                self.timer_handle(&result, &timer)
            }
            Builtin::TimerChannel | Builtin::TickerChannel => {
                let timer = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare ptr @tsuzuri_tz_timer_chan(ptr)".into());
                let block = self.value(format!("call ptr @tsuzuri_tz_timer_chan(ptr {timer})"));
                self.channel_handle(&result, &block)
            }
            Builtin::TimerStop | Builtin::TickerStop => {
                let timer = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare i32 @tsuzuri_tz_timer_stop(ptr)".into());
                let pending = self.value(format!("call i32 @tsuzuri_tz_timer_stop(ptr {timer})"));
                if builtin == Builtin::TimerStop {
                    self.value(format!("icmp ne i32 {pending}, 0"))
                } else {
                    "0".to_owned()
                }
            }
            Builtin::TimerReset | Builtin::TickerReset => {
                if builtin == Builtin::TickerReset {
                    self.positive_period("%arg1");
                }
                let timer = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare i32 @tsuzuri_tz_timer_reset(ptr, i64)".into());
                let pending = self.value(format!(
                    "call i32 @tsuzuri_tz_timer_reset(ptr {timer}, i64 %arg1)"
                ));
                if builtin == Builtin::TimerReset {
                    self.value(format!("icmp ne i32 {pending}, 0"))
                } else {
                    "0".to_owned()
                }
            }
            Builtin::TimerFree | Builtin::TickerFree => {
                let timer = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare void @tsuzuri_tz_timer_free(ptr)".into());
                self.instruction(format!("call void @tsuzuri_tz_timer_free(ptr {timer})"));
                "0".to_owned()
            }
            _ => unreachable!("not a timer builtin"),
        }
    }

    /// `Time.sleep`: the scheduler's timer, which parks a tzroutine without its thread (a sleep
    /// in a critical section traps, as a yield does). The default WebAssembly target has no timer
    /// (TZ09): it waits as `Time.sleep_ms` does, for the milliseconds rounded up.
    fn time_sleep(&mut self) {
        if self.globals.wasm {
            let quotient = self.value("sdiv i64 %arg0, 1000000");
            let remainder = self.value("srem i64 %arg0, 1000000");
            let partial = self.value(format!("icmp sgt i64 {remainder}, 0"));
            let extra = self.value(format!("zext i1 {partial} to i64"));
            let rounded = self.value(format!("add i64 {quotient}, {extra}"));
            let positive = self.value("icmp sgt i64 %arg0, 0");
            let milliseconds = self.value(format!("select i1 {positive}, i64 {rounded}, i64 0"));
            self.intrinsics
                .insert("declare i64 @tsuzuri_os_sleep(i64)".into());
            self.value(format!("call i64 @tsuzuri_os_sleep(i64 {milliseconds})"));
            return;
        }
        self.routine_unlocked();
        self.intrinsics
            .insert("declare i32 @tsuzuri_tz_sleep(i64)".into());
        self.value("call i32 @tsuzuri_tz_sleep(i64 %arg0)");
    }

    /// Traps unless the period is positive (Go panics).
    fn positive_period(&mut self, period: &str) {
        let positive = self.value(format!("icmp sgt i64 {period}, 0"));
        self.guard(&positive, TrapKind::Assert);
    }

    /// Allocates a timer's channel and hands it to the runtime with the timer. Returns the block
    /// (`handle` false: the caller gets a handle of the channel) or the timer's record.
    fn timer_new(
        &mut self,
        nanoseconds: &str,
        period: &str,
        kind: u32,
        handle: bool,
        function: Option<&str>,
    ) -> String {
        let block = self.value(format!("call ptr @tz.alloc(i64 {TIMER_CHANNEL_BYTES})"));
        self.intrinsics.insert(TIMER_NEW.into());
        let (run, drop, boxed) = match function {
            Some(boxed) => (
                "@tz.tzroutine.owned",
                "@tz.tzroutine.owned.drop",
                boxed.to_owned(),
            ),
            None => ("null", "null", "null".to_owned()),
        };
        let timer = self.value(format!(
            "call ptr @tsuzuri_tz_timer_new(ptr {block}, ptr @tz.free, i64 {nanoseconds}, i64 {period}, i32 {kind}, i32 {}, ptr {run}, ptr {drop}, ptr {boxed})",
            u32::from(handle)
        ));
        if handle { timer } else { block }
    }

    /// The `Channel<i64>` handle of a timer's channel block.
    fn channel_handle(&mut self, result: &Type, block: &str) -> String {
        let address = self.value(format!("ptrtoint ptr {block} to i64"));
        let handle = self.value(format!(
            "insertvalue {} zeroinitializer, i64 {address}, 0",
            self.ty(result)
        ));
        // A new value of a type with a `Drop` instance is live, so that its drop runs.
        self.mark_live(result, handle)
    }

    /// The `Timer` or `Ticker` record of a runtime timer.
    fn timer_handle(&mut self, result: &Type, timer: &str) -> String {
        let address = self.value(format!("ptrtoint ptr {timer} to i64"));
        let handle = self.value(format!(
            "insertvalue {} zeroinitializer, i64 {address}, 0",
            self.ty(result)
        ));
        self.mark_live(result, handle)
    }

    /// The runtime's timer of the borrowed `Timer` or `Ticker` `%arg0`.
    fn timer_of(&mut self, parameter: &Type) -> String {
        let Type::Reference(record, false) = parameter else {
            unreachable!("a timer operation borrows the handle")
        };
        self.channel_block(record, "%arg0").0
    }
}
