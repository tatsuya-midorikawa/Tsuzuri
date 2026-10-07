; --allocator counting (F13 Phase 2): the target's allocator, renamed to @tz.alloc.base,
; @tz.free.base, and @tz.realloc.base, behind a 16-byte header that holds the requested
; size. The counts are allocations, frees, live bytes, and the peak of the live bytes.
@tz.heap.counts = internal global [4 x i64] zeroinitializer, align 8

define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %large = icmp ugt i64 %size, 9223372036854775791
  br i1 %large, label %fail, label %request
request:
  %total = add i64 %size, 16
  %base = call ptr @tz.alloc.base(i64 %total)
  store i64 %size, ptr %base, align 16
  call void @tz.heap.change(i64 0, i64 %size)
  %pointer = getelementptr inbounds i8, ptr %base, i64 16
  ret ptr %pointer
fail:
  call void @llvm.trap()
  unreachable
}

define internal void @tz.free(ptr %pointer) nounwind {
entry:
  %null = icmp eq ptr %pointer, null
  br i1 %null, label %done, label %release
release:
  %base = getelementptr inbounds i8, ptr %pointer, i64 -16
  %size = load i64, ptr %base, align 16
  %delta = sub i64 0, %size
  call void @tz.heap.change(i64 1, i64 %delta)
  call void @tz.free.base(ptr %base)
  br label %done
done:
  ret void
}

define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size) nounwind {
entry:
  %zero = icmp eq i64 %new_size, 0
  br i1 %zero, label %release, label %check
release:
  call void @tz.free(ptr %old)
  ret ptr null
check:
  %empty = icmp eq ptr %old, null
  br i1 %empty, label %allocate, label %bound
allocate:
  %fresh = call ptr @tz.alloc(i64 %new_size)
  ret ptr %fresh
bound:
  %large = icmp ugt i64 %new_size, 9223372036854775791
  br i1 %large, label %fail, label %resize
resize:
  %base = getelementptr inbounds i8, ptr %old, i64 -16
  %stored = load i64, ptr %base, align 16
  %old_total = add i64 %stored, 16
  %new_total = add i64 %new_size, 16
  %moved = call ptr @tz.realloc.base(ptr %base, i64 %old_total, i64 %new_total)
  store i64 %new_size, ptr %moved, align 16
  %delta = sub i64 %new_size, %stored
  call void @tz.heap.change(i64 2, i64 %delta)
  %pointer = getelementptr inbounds i8, ptr %moved, i64 16
  ret ptr %pointer
fail:
  call void @llvm.trap()
  unreachable
}

; Event 0 counts an allocation, 1 a free, and 2 a resize, which only moves the live bytes.
define internal void @tz.heap.change(i64 %event, i64 %delta) nounwind {
entry:
  %counted = icmp ult i64 %event, 2
  br i1 %counted, label %count, label %live
count:
  %slot = getelementptr inbounds [4 x i64], ptr @tz.heap.counts, i64 0, i64 %event
  %events = atomicrmw add ptr %slot, i64 1 monotonic, align 8
  br label %live
live:
  %live_slot = getelementptr inbounds [4 x i64], ptr @tz.heap.counts, i64 0, i64 2
  %before = atomicrmw add ptr %live_slot, i64 %delta monotonic, align 8
  %after = add i64 %before, %delta
  %peak_slot = getelementptr inbounds [4 x i64], ptr @tz.heap.counts, i64 0, i64 3
  %peak = atomicrmw umax ptr %peak_slot, i64 %after monotonic, align 8
  ret void
}

; void tsuzuri_alloc_stats(tsuzuri_allocation_stats *stats)
define void @tsuzuri_alloc_stats(ptr %stats) nounwind {
entry:
  %allocations = load atomic i64, ptr @tz.heap.counts monotonic, align 8
  store i64 %allocations, ptr %stats, align 8
  %frees_slot = getelementptr inbounds [4 x i64], ptr @tz.heap.counts, i64 0, i64 1
  %frees = load atomic i64, ptr %frees_slot monotonic, align 8
  %frees_out = getelementptr inbounds i8, ptr %stats, i64 8
  store i64 %frees, ptr %frees_out, align 8
  %live_slot = getelementptr inbounds [4 x i64], ptr @tz.heap.counts, i64 0, i64 2
  %live = load atomic i64, ptr %live_slot monotonic, align 8
  %live_out = getelementptr inbounds i8, ptr %stats, i64 16
  store i64 %live, ptr %live_out, align 8
  %peak_slot = getelementptr inbounds [4 x i64], ptr @tz.heap.counts, i64 0, i64 3
  %peak = load atomic i64, ptr %peak_slot monotonic, align 8
  %peak_out = getelementptr inbounds i8, ptr %stats, i64 24
  store i64 %peak, ptr %peak_out, align 8
  ret void
}
