; Standalone WASM has no host thread imports; complete the group in input order.
define internal void @tsuzuri_task_parallel(ptr %run, ptr %context, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %body ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %exit, label %body
body:
  call void %run(ptr %context, i64 %index)
  %next = add i64 %index, 1
  br label %loop
exit:
  ret void
}

define internal i64 @tsuzuri_task_parallel_results(ptr %run, ptr %context, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %ok ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %success, label %body
body:
  %failed = call i32 %run(ptr %context, i64 %index)
  %is_failed = icmp ne i32 %failed, 0
  br i1 %is_failed, label %failure, label %ok
ok:
  %next = add i64 %index, 1
  br label %loop
failure:
  ret i64 %index
success:
  ret i64 -1
}
