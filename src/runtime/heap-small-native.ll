; --allocator small (PM05 Phase 2): @tz.alloc, @tz.free, and @tz.realloc over the size-class allocator of
; heap-small.ll, with the same contract as heap-native.ll: a null result of the C library's allocator traps,
; a new size of 0 frees, and a null block is a new allocation. The old size is not needed; the block's class
; (or the C library) knows the size that is copied.
define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %pointer = call ptr @tz_small_alloc(i64 %size)
  %failed = icmp eq ptr %pointer, null
  br i1 %failed, label %fail, label %ok
fail:
  call void @llvm.trap()
  unreachable
ok:
  ret ptr %pointer
}

define internal void @tz.free(ptr %pointer) nounwind {
entry:
  call void @tz_small_free(ptr %pointer)
  ret void
}

define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size) nounwind {
entry:
  %zero = icmp eq i64 %new_size, 0
  br i1 %zero, label %release, label %resize
release:
  call void @tz_small_free(ptr %old)
  ret ptr null
resize:
  %pointer = call ptr @tz_small_realloc(ptr %old, i64 %new_size)
  %failed = icmp eq ptr %pointer, null
  br i1 %failed, label %fail, label %done
fail:
  call void @llvm.trap()
  unreachable
done:
  ret ptr %pointer
}
