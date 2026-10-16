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

; The bytes of the blocks in use: the heap from its start to its end, less every free block and every block
; that only waits in the cache. The free blocks are in 33 lists (the large one, list 0, and the 32 small ones,
; lists 1 to 32); the cached blocks are in 32 more (lists 33 to 64), whose next link is at +16 instead of +4.
define i64 @tsuzuri_thread_heap_live_bytes() {
entry:
  call void @tsuzuri_threads_heap_lock()
  %end = load i32, ptr @tz.heap.end
  %base = ptrtoint ptr @__heap_base to i32
  %rounded = add i32 %base, 15
  %aligned = and i32 %rounded, -16
  %allocated = sub i32 %end, %aligned
  %large_head = load i32, ptr @tz.heap.free
  br label %lists
lists:
  %list = phi i32 [ 0, %entry ], [ %next_list, %walked ]
  %live = phi i32 [ %allocated, %entry ], [ %left, %walked ]
  %finished = icmp eq i32 %list, 65
  br i1 %finished, label %exit, label %pick
pick:
  %is_large = icmp eq i32 %list, 0
  %is_fast = icmp uge i32 %list, 33
  %small_index = sub i32 %list, 1
  %fast_index = sub i32 %list, 33
  %list_index = select i1 %is_fast, i32 %fast_index, i32 %small_index
  %safe_index = select i1 %is_large, i32 0, i32 %list_index
  %small_slot = getelementptr [32 x i32], ptr @tz.heap.small, i32 0, i32 %safe_index
  %fast_slot = getelementptr [32 x i32], ptr @tz.heap.fast, i32 0, i32 %safe_index
  %small_head = load i32, ptr %small_slot
  %fast_head = load i32, ptr %fast_slot
  %list_head = select i1 %is_fast, i32 %fast_head, i32 %small_head
  %head = select i1 %is_large, i32 %large_head, i32 %list_head
  %link = select i1 %is_fast, i32 16, i32 4
  br label %loop
loop:
  %block = phi i32 [ %head, %pick ], [ %next, %body ]
  %left = phi i32 [ %live, %pick ], [ %remaining, %body ]
  %done = icmp eq i32 %block, 0
  br i1 %done, label %walked, label %body
body:
  %pointer = inttoptr i32 %block to ptr
  %flags = load i32, ptr %pointer
  %capacity = and i32 %flags, -16
  %next_pointer = getelementptr i8, ptr %pointer, i32 %link
  %next = load i32, ptr %next_pointer
  %remaining = sub i32 %left, %capacity
  br label %loop
walked:
  %next_list = add i32 %list, 1
  br label %lists
exit:
  call void @tsuzuri_threads_heap_unlock()
  %result = zext i32 %live to i64
  ret i64 %result
}
