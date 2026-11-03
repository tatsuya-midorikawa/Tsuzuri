use super::*;

/// The bytes of a context's `done` channel: the runtime's 128-byte header, without a ring (the
/// channel has no buffer and nothing is sent on it).
const DONE_BYTES: u64 = 128;

/// No deadline (`INT64_MAX` in `src/runtime/tzroutine.c`).
const NO_DEADLINE: &str = "9223372036854775807";

const CONTEXT_NEW: &str = "declare ptr @tsuzuri_tz_ctx_new(ptr, i64, ptr, ptr)";

impl FunctionEmitter<'_, '_> {
    /// Defines the bodies of `Context` (TZ07): the runtime's contexts, which a `Context` or an
    /// `AfterFunc` record holds the address of. The `done` channel and an `AfterFunc`'s channel are
    /// allocated here, so that they are counted as the program's, and the runtime frees them with
    /// `@tz.free` when it lets go of the last reference.
    pub(super) fn context_builtin(&mut self, builtin: Builtin, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        let count = builtin.scheme().parameters.len();
        let result = ty.after_arguments(count);
        match builtin {
            Builtin::ContextBackground => {
                let context = self.context_new("null", NO_DEADLINE, false);
                self.timer_handle(&result, &context)
            }
            Builtin::ContextWithCancel => {
                let parent = self.timer_of(&parameters[0]);
                let context = self.context_new(&parent, NO_DEADLINE, false);
                self.timer_handle(&result, &context)
            }
            Builtin::ContextWithDeadline | Builtin::ContextWithTimeout => {
                let parent = self.timer_of(&parameters[0]);
                let context =
                    self.context_new(&parent, "%arg1", builtin == Builtin::ContextWithTimeout);
                self.timer_handle(&result, &context)
            }
            Builtin::ContextShare => {
                let context = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare void @tsuzuri_tz_ctx_share(ptr)".into());
                self.instruction(format!("call void @tsuzuri_tz_ctx_share(ptr {context})"));
                self.timer_handle(&result, &context)
            }
            Builtin::ContextDone => {
                let context = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare ptr @tsuzuri_tz_ctx_done(ptr)".into());
                let block = self.value(format!("call ptr @tsuzuri_tz_ctx_done(ptr {context})"));
                self.channel_handle(&result, &block)
            }
            Builtin::ContextErr => {
                let context = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare i32 @tsuzuri_tz_ctx_err(ptr)".into());
                let why = self.value(format!("call i32 @tsuzuri_tz_ctx_err(ptr {context})"));
                self.value(format!("sext i32 {why} to i64"))
            }
            Builtin::ContextDeadline => {
                let context = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare i64 @tsuzuri_tz_ctx_deadline(ptr)".into());
                self.value(format!("call i64 @tsuzuri_tz_ctx_deadline(ptr {context})"))
            }
            Builtin::ContextCancel | Builtin::ContextRelease | Builtin::ContextFreeAfter => {
                let handle = self.timer_of(&parameters[0]);
                let function = match builtin {
                    Builtin::ContextCancel => "tsuzuri_tz_ctx_cancel",
                    Builtin::ContextRelease => "tsuzuri_tz_ctx_release",
                    _ => "tsuzuri_tz_ctx_free_after",
                };
                self.intrinsics
                    .insert(format!("declare void @{function}(ptr)"));
                self.instruction(format!("call void @{function}(ptr {handle})"));
                "0".to_owned()
            }
            Builtin::ContextAfterFunc => {
                let context = self.timer_of(&parameters[0]);
                let owned = self.ty(&parameters[1]);
                let closure = self.value(format!("extractvalue {owned} %arg1, 0"));
                let boxed = self.value("call ptr @tz.alloc(i64 16)");
                self.instruction(format!("store %tz.closure {closure}, ptr {boxed}"));
                self.intrinsics.insert(super::ROUTINE_OWNED_START.into());
                self.intrinsics
                    .insert(super::timer::ROUTINE_OWNED_DROP.into());
                let block = self.value(format!(
                    "call ptr @tz.alloc(i64 {})",
                    super::timer::TIMER_CHANNEL_BYTES
                ));
                self.intrinsics.insert(
                    "declare ptr @tsuzuri_tz_ctx_after(ptr, ptr, ptr, ptr, ptr, ptr)".into(),
                );
                let after = self.value(format!(
                    "call ptr @tsuzuri_tz_ctx_after(ptr {context}, ptr {block}, ptr @tz.free, ptr @tz.tzroutine.owned, ptr @tz.tzroutine.owned.drop, ptr {boxed})"
                ));
                self.timer_handle(&result, &after)
            }
            Builtin::ContextStopAfter => {
                let after = self.timer_of(&parameters[0]);
                self.intrinsics
                    .insert("declare i32 @tsuzuri_tz_ctx_stop_after(ptr)".into());
                let pending =
                    self.value(format!("call i32 @tsuzuri_tz_ctx_stop_after(ptr {after})"));
                self.value(format!("icmp ne i32 {pending}, 0"))
            }
            _ => unreachable!("not a Context builtin"),
        }
    }

    /// Allocates a done channel and makes a context of the runtime with it: a child of `parent`
    /// (`null` for the root) whose deadline is `deadline`, or that many nanoseconds from now when
    /// `timeout`.
    fn context_new(&mut self, parent: &str, deadline: &str, timeout: bool) -> String {
        let block = self.value(format!("call ptr @tz.alloc(i64 {DONE_BYTES})"));
        let (function, declaration) = if timeout {
            (
                "tsuzuri_tz_ctx_timeout",
                "declare ptr @tsuzuri_tz_ctx_timeout(ptr, i64, ptr, ptr)",
            )
        } else {
            ("tsuzuri_tz_ctx_new", CONTEXT_NEW)
        };
        self.intrinsics.insert(declaration.into());
        self.value(format!(
            "call ptr @{function}(ptr {parent}, i64 {deadline}, ptr {block}, ptr @tz.free)"
        ))
    }
}
