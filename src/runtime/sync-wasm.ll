; Standalone WASM runs one thread, so a lock is a flag: it is held by the one `Mutex.with_lock`
; that is open, and a second one can only be nested in it (F10 D7). The status is zero when the
; lock is taken; there is no console to explain a refusal, and the caller traps.
@tsuzuri.mutex.held = internal global i32 0

define internal i32 @tsuzuri_mutex_lock(ptr %cell) nounwind {
entry:
  %held = load i32, ptr @tsuzuri.mutex.held
  %nested = icmp ne i32 %held, 0
  br i1 %nested, label %refuse, label %take
refuse:
  ret i32 1
take:
  store i32 1, ptr @tsuzuri.mutex.held
  ret i32 0
}

define internal void @tsuzuri_mutex_unlock(ptr %cell) nounwind {
entry:
  store i32 0, ptr @tsuzuri.mutex.held
  ret void
}

; Nonzero when work may start: no lock is held.
define internal i32 @tsuzuri_mutex_parallel_ok() nounwind {
entry:
  %held = load i32, ptr @tsuzuri.mutex.held
  %free = icmp eq i32 %held, 0
  %ok = zext i1 %free to i32
  ret i32 %ok
}
