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
