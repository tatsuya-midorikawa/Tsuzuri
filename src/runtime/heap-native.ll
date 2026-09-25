declare ptr @malloc(i64)
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
