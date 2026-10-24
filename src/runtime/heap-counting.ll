; --allocator counting (F13 Phase 2, F14): the target's allocator, renamed to @tz.alloc.base,
; @tz.free.base, and @tz.realloc.base, behind a 16-byte header that holds the requested size.
; @tz.heap.counts holds six counters: 0 allocations, 1 resizes, 2 frees, 3 requested bytes
; (an allocation counts its size, a resize its new size), 4 live bytes, 5 peak of the live bytes.
@tz.heap.counts = internal global [6 x i64] zeroinitializer, align 8

define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %large = icmp ugt i64 %size, 9223372036854775791
  br i1 %large, label %fail, label %request
request:
  %total = add i64 %size, 16
  %base = call ptr @tz.alloc.base(i64 %total)
  store i64 %size, ptr %base, align 16
  call void @tz.heap.change(i64 0, i64 %size, i64 %size)
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
  call void @tz.heap.change(i64 2, i64 %delta, i64 0)
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
  call void @tz.heap.change(i64 1, i64 %delta, i64 %new_size)
  %pointer = getelementptr inbounds i8, ptr %moved, i64 16
  ret ptr %pointer
fail:
  call void @llvm.trap()
  unreachable
}

; %slot (0 allocation, 1 resize, 2 free) counts one event, %requested adds to the requested
; bytes, and %delta moves the live bytes and the peak.
define internal void @tz.heap.change(i64 %slot, i64 %delta, i64 %requested) nounwind {
entry:
  %event_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 %slot
  %events = atomicrmw add ptr %event_slot, i64 1 monotonic, align 8
  %bytes_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 3
  %bytes = atomicrmw add ptr %bytes_slot, i64 %requested monotonic, align 8
  %live_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 4
  %before = atomicrmw add ptr %live_slot, i64 %delta monotonic, align 8
  %after = add i64 %before, %delta
  %peak_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 5
  %peak = atomicrmw umax ptr %peak_slot, i64 %after monotonic, align 8
  ret void
}

; Alloc.__counter and tsuzuri_alloc_stats read one counter by slot. A slot above 5 traps.
define internal i64 @tz.heap.counter(i64 %index) nounwind {
entry:
  %valid = icmp ult i64 %index, 6
  br i1 %valid, label %read, label %fail
read:
  %slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 %index
  %value = load atomic i64, ptr %slot monotonic, align 8
  ret i64 %value
fail:
  call void @llvm.trap()
  unreachable
}

; void tsuzuri_alloc_stats(tsuzuri_allocation_stats *stats)
define void @tsuzuri_alloc_stats(ptr %stats) nounwind {
entry:
  %allocations = call i64 @tz.heap.counter(i64 0)
  store i64 %allocations, ptr %stats, align 8
  %frees = call i64 @tz.heap.counter(i64 2)
  %frees_out = getelementptr inbounds i8, ptr %stats, i64 8
  store i64 %frees, ptr %frees_out, align 8
  %live = call i64 @tz.heap.counter(i64 4)
  %live_out = getelementptr inbounds i8, ptr %stats, i64 16
  store i64 %live, ptr %live_out, align 8
  %peak = call i64 @tz.heap.counter(i64 5)
  %peak_out = getelementptr inbounds i8, ptr %stats, i64 24
  store i64 %peak, ptr %peak_out, align 8
  ret void
}
