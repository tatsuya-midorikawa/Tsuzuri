declare ptr @tsuzuri_host_alloc(i64, i64)
declare void @tsuzuri_host_free(ptr, i64, i64)
declare ptr @tsuzuri_host_realloc(ptr, i64, i64, i64)

; --allocator host (F13): the host defines the three functions. Each block starts with a
; 16-byte header holding the requested size, so the host receives the same size and
; alignment when a block is allocated, resized, and freed, whichever path frees it.
define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %large = icmp ugt i64 %size, 9223372036854775791
  br i1 %large, label %fail, label %request
request:
  %total = add i64 %size, 16
  %base = call ptr @tsuzuri_host_alloc(i64 %total, i64 16)
  %address = ptrtoint ptr %base to i64
  %low = and i64 %address, 15
  %null = icmp eq ptr %base, null
  %misaligned = icmp ne i64 %low, 0
  %bad = or i1 %null, %misaligned
  br i1 %bad, label %fail, label %ok
fail:
  call void @llvm.trap()
  unreachable
ok:
  store i64 %size, ptr %base, align 16
  %pointer = getelementptr inbounds i8, ptr %base, i64 16
  ret ptr %pointer
}

define internal void @tz.free(ptr %pointer) nounwind {
entry:
  %null = icmp eq ptr %pointer, null
  br i1 %null, label %done, label %release
release:
  %base = getelementptr inbounds i8, ptr %pointer, i64 -16
  %size = load i64, ptr %base, align 16
  %total = add i64 %size, 16
  call void @tsuzuri_host_free(ptr %base, i64 %total, i64 16)
  br label %done
done:
  ret void
}

; The header, not %old_size, is the size the host allocated.
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
  %moved = call ptr @tsuzuri_host_realloc(ptr %base, i64 %old_total, i64 %new_total, i64 16)
  %address = ptrtoint ptr %moved to i64
  %low = and i64 %address, 15
  %null = icmp eq ptr %moved, null
  %misaligned = icmp ne i64 %low, 0
  %bad = or i1 %null, %misaligned
  br i1 %bad, label %fail, label %ok
fail:
  call void @llvm.trap()
  unreachable
ok:
  store i64 %new_size, ptr %moved, align 16
  %pointer = getelementptr inbounds i8, ptr %moved, i64 16
  ret ptr %pointer
}
