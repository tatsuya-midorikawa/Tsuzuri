@tz.debug.newline = private unnamed_addr constant [1 x i8] c"\0A"

declare i64 @write(i32, ptr, i64)

define internal void @tz.debug.write(%tz.utf8string %text) nounwind {
entry:
  %data = extractvalue %tz.utf8string %text, 0
  %length = extractvalue %tz.utf8string %text, 1
  br label %body
body:
  %offset = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %offset, %length
  br i1 %done, label %newline, label %bytes
bytes:
  %pointer = getelementptr inbounds i8, ptr %data, i64 %offset
  %remaining = sub i64 %length, %offset
  %written = call i64 @write(i32 2, ptr %pointer, i64 %remaining)
  %progress = icmp sgt i64 %written, 0
  br i1 %progress, label %advance, label %bad
advance:
  %next = add i64 %offset, %written
  br label %body
newline:
  %line = call i64 @write(i32 2, ptr @tz.debug.newline, i64 1)
  %line_ok = icmp eq i64 %line, 1
  br i1 %line_ok, label %finish, label %bad
finish:
  call void @tz.free(ptr %data)
  ret void
bad:
  call void @llvm.trap()
  unreachable
}