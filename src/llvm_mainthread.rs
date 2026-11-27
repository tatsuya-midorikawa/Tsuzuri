use super::*;

/// `src/runtime/mainthread-darwin.c`: queues `run(box)` for the process main thread, or drops the
/// function with `drop(box)` when the boundary ends first.
pub(super) const MAIN_THREAD_POST: &str =
    "declare void @tsuzuri_mainthread_post(i64, ptr, ptr, ptr)";
pub(super) const MAIN_THREAD_IS_CURRENT: &str = "declare i32 @tsuzuri_mainthread_is_current()";
/// The call that a wrapper makes before it runs code that needs the main thread (M09 Phase 2).
pub(super) const MAIN_THREAD_CHECK_CALL: &str = "  call void @tz.main_thread.check()\n";
const MAIN_THREAD_WRONG: &str =
    "Tsuzuri runtime: a callback that uses main-thread handles was called on another thread\n";

/// `@tz.main_thread.check`: traps with a line on the standard error unless the calling thread is
/// the process main thread. A callback, an export, a test, a bench and a library's `tsuzuri_main`
/// call it first when their body has the effect `main_thread`: only there can a host choose the
/// thread. The `main` of an executable and a posted function need no check.
pub(super) fn entry_check_definition() -> String {
    let bytes: String = MAIN_THREAD_WRONG
        .bytes()
        .map(|byte| format!("\\{byte:02X}"))
        .collect();
    let length = MAIN_THREAD_WRONG.len();
    format!(
        "@tz.main_thread.wrong = private unnamed_addr constant [{length} x i8] c\"{bytes}\"\n\
define internal void @tz.main_thread.check() nounwind {{\n\
entry:\n  %current = call i32 @tsuzuri_mainthread_is_current()\n  %main = icmp ne i32 %current, 0\n  br i1 %main, label %ok, label %wrong\n\
wrong:\n  call void @tz.trap.write(ptr @tz.main_thread.wrong, i64 {length})\n  call void @llvm.trap()\n  unreachable\n\
ok:\n  ret void\n}}\n"
    )
}
pub(super) const MAIN_THREAD_BOUNDARY_DONE: &str =
    "declare void @tsuzuri_mainthread_boundary_done()";
pub(super) const MAIN_THREAD_BOUNDARY_OPEN: &str =
    "declare void @tsuzuri_mainthread_boundary_open()";
pub(super) const MAIN_THREAD_BOUNDARY_DONE_CALL: &str =
    "  call void @tsuzuri_mainthread_boundary_done()\n";
pub(super) const MAIN_THREAD_BOUNDARY_OPEN_CALL: &str =
    "  call void @tsuzuri_mainthread_boundary_open()\n";

/// What the main thread calls for a posted function (M09): the body of `@tz.tzroutine.owned`. Its
/// name does not start with `@tz.callback.`: it is no callback of an `extern`, the main thread is
/// never a scheduler thread, and the function was posted before `main` ended.
const MAIN_THREAD_RUN: &str = "define internal void @tz.mainthread.run(ptr %box) {\nentry:\n  %closure = load %tz.closure, ptr %box\n  %desc = extractvalue %tz.closure %closure, 0\n  %code = load ptr, ptr %desc\n  %env = extractvalue %tz.closure %closure, 1\n  %unit = call i8 %code(i8 0, ptr %env, i1 true)\n  call void @tz.closure.drop(%tz.closure %closure)\n  call void @tz.free(ptr %box)\n  ret void\n}\n";

/// The same in a program that spawns: the call follows the boundary rule of a static callback
/// (W12). Inside the boundary of `main` what it spawns belongs to `main`; outside every boundary
/// (a library whose host runs the main queue) the one call is a boundary of its own.
const MAIN_THREAD_RUN_SCOPED: &str = "define internal void @tz.mainthread.run(ptr %box) {\nentry:\n  %tz.host = call ptr @tsuzuri_tz_callback_scope_enter()\n  %closure = load %tz.closure, ptr %box\n  %desc = extractvalue %tz.closure %closure, 0\n  %code = load ptr, ptr %desc\n  %env = extractvalue %tz.closure %closure, 1\n  %unit = call i8 %code(i8 0, ptr %env, i1 true)\n  call void @tz.closure.drop(%tz.closure %closure)\n  call void @tz.free(ptr %box)\n  call void @tsuzuri_tz_callback_scope_leave(ptr %tz.host)\n  ret void\n}\n";

/// Whether the program posts to the main thread: its IR declares the runtime's `post`.
pub(super) fn posts(intrinsics: &BTreeSet<String>) -> bool {
    intrinsics.contains(MAIN_THREAD_POST)
}

/// Puts `call` before every `ret` of the function text `ir` (a runner or an entry wrapper).
pub(super) fn before_returns(ir: &str, call: &str) -> String {
    let mut output = String::with_capacity(ir.len() + call.len() * 2);
    for line in ir.split_inclusive('\n') {
        let body = line.trim_start();
        if body.starts_with("ret ") || body.trim_end() == "ret void" {
            output.push_str(call);
        }
        output.push_str(line);
    }
    output
}

impl FunctionEmitter<'_, '_> {
    /// Defines the bodies of `MainThread` (M09). `post` and `after` move the owned function into a
    /// box, as `Routine.go` and `Time.after_func` do, and hand it to the runtime, which calls
    /// `@tz.mainthread.run` on the main thread or `@tz.tzroutine.owned.drop` at the end of the
    /// boundary. The driver refuses the targets that have no main dispatch queue.
    pub(super) fn main_thread_builtin(&mut self, builtin: Builtin, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        match builtin {
            Builtin::MainThreadPost | Builtin::MainThreadAfter => {
                let (nanoseconds, function, argument) = if builtin == Builtin::MainThreadPost {
                    ("0", &parameters[0], "%arg0")
                } else {
                    ("%arg0", &parameters[1], "%arg1")
                };
                let owned = self.ty(function);
                let closure = self.value(format!("extractvalue {owned} {argument}, 0"));
                let boxed = self.value("call ptr @tz.alloc(i64 16)");
                self.instruction(format!("store %tz.closure {closure}, ptr {boxed}"));
                if self.globals.spawns && !self.globals.wasm {
                    self.intrinsics
                        .insert("declare ptr @tsuzuri_tz_callback_scope_enter()".into());
                    self.intrinsics
                        .insert("declare void @tsuzuri_tz_callback_scope_leave(ptr)".into());
                    self.intrinsics.insert(MAIN_THREAD_RUN_SCOPED.into());
                } else {
                    self.intrinsics.insert(MAIN_THREAD_RUN.into());
                }
                self.intrinsics.insert(timer::ROUTINE_OWNED_DROP.into());
                self.intrinsics.insert(MAIN_THREAD_POST.into());
                self.instruction(format!(
                    "call void @tsuzuri_mainthread_post(i64 {nanoseconds}, ptr @tz.mainthread.run, ptr @tz.tzroutine.owned.drop, ptr {boxed})"
                ));
                "0".to_owned()
            }
            Builtin::MainThreadIsCurrent => {
                self.intrinsics.insert(MAIN_THREAD_IS_CURRENT.into());
                let current = self.value("call i32 @tsuzuri_mainthread_is_current()");
                self.value(format!("icmp ne i32 {current}, 0"))
            }
            _ => unreachable!("not a MainThread builtin"),
        }
    }
}
