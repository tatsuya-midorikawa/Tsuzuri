declare ptr @malloc(i64)
declare ptr @realloc(ptr, i64)
declare void @free(ptr)

define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %pointer = call ptr @malloc(i64 %size)
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
  call void @free(ptr %pointer)
  ret void
}

define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size) nounwind {
entry:
  %zero = icmp eq i64 %new_size, 0
  br i1 %zero, label %release, label %resize
release:
  call void @free(ptr %old)
  ret ptr null
resize:
  %pointer = call ptr @realloc(ptr %old, i64 %new_size)
  %failed = icmp eq ptr %pointer, null
  br i1 %failed, label %fail, label %done
fail:
  call void @llvm.trap()
  unreachable
done:
  ret ptr %pointer
}
