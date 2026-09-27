declare void @tsuzuri_threads_heap_lock()
declare void @tsuzuri_threads_heap_unlock()

define i32 @tsuzuri_thread_stack_probe() {
entry:
  %slot = alloca i32, align 16
  store volatile i32 1, ptr %slot
  %address = ptrtoint ptr %slot to i32
  ret i32 %address
}

define internal ptr @tz.alloc(i64 %size) {
entry:
  call void @tsuzuri_threads_heap_lock()
  %result = call ptr @tz.heap.alloc.unlocked(i64 %size)
  call void @tsuzuri_threads_heap_unlock()
  ret ptr %result
}

define internal void @tz.free(ptr %pointer) {
entry:
  call void @tsuzuri_threads_heap_lock()
  call void @tz.heap.free.unlocked(ptr %pointer)
  call void @tsuzuri_threads_heap_unlock()
  ret void
}

define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size) {
entry:
  call void @tsuzuri_threads_heap_lock()
  %result = call ptr @tz.heap.realloc.unlocked(ptr %old, i64 %old_size, i64 %new_size)
  call void @tsuzuri_threads_heap_unlock()
  ret ptr %result
}

define i32 @tsuzuri_thread_stack_alloc() {
entry:
  %pointer = call ptr @tz.alloc(i64 262144)
  %address = ptrtoint ptr %pointer to i32
  ret i32 %address
}

define i64 @tsuzuri_thread_heap_live_bytes() {
entry:
  call void @tsuzuri_threads_heap_lock()
  %end = load i32, ptr @tz.heap.end
  %base = ptrtoint ptr @__heap_base to i32
  %rounded = add i32 %base, 15
  %aligned = and i32 %rounded, -16
  %size = sub i32 %end, %aligned
  %empty = icmp eq i32 %end, 0
  %allocated = select i1 %empty, i32 0, i32 %size
  %head = load i32, ptr @tz.heap.free
  br label %loop
loop:
  %block = phi i32 [ %head, %entry ], [ %next, %body ]
  %live = phi i32 [ %allocated, %entry ], [ %remaining, %body ]
  %done = icmp eq i32 %block, 0
  br i1 %done, label %exit, label %body
body:
  %pointer = inttoptr i32 %block to ptr
  %capacity = load i32, ptr %pointer
  %next_pointer = getelementptr i8, ptr %pointer, i32 4
  %next = load i32, ptr %next_pointer
  %remaining = sub i32 %live, %capacity
  br label %loop
exit:
  call void @tsuzuri_threads_heap_unlock()
  %result = zext i32 %live to i64
  ret i64 %result
}
