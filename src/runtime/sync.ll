; The parallel entries of a program that calls `Mutex.with_lock` (F10 D7): no work starts while this
; thread holds a lock, which would let another thread wait for it from a child. The call sites of
; `@tsuzuri_task_parallel` and `@tsuzuri_task_parallel_results` are renamed to these wrappers only
; in such a program, so other programs keep their IR.
define internal void @tz.mutex.parallel(ptr %run, ptr %context, i64 %length) nounwind {
entry:
  %free = call i32 @tsuzuri_mutex_parallel_ok()
  %start = icmp ne i32 %free, 0
  br i1 %start, label %go, label %locked
locked:
  call void @llvm.trap()
  unreachable
go:
  call void @tsuzuri_task_parallel(ptr %run, ptr %context, i64 %length)
  ret void
}

define internal i64 @tz.mutex.parallel_results(ptr %run, ptr %context, i64 %length) nounwind {
entry:
  %free = call i32 @tsuzuri_mutex_parallel_ok()
  %start = icmp ne i32 %free, 0
  br i1 %start, label %go, label %locked
locked:
  call void @llvm.trap()
  unreachable
go:
  %failure = call i64 @tsuzuri_task_parallel_results(ptr %run, ptr %context, i64 %length)
  ret i64 %failure
}
