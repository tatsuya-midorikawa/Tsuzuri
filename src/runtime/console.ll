declare i64 @write(i32, ptr, i64)

define internal i32 @tz.console.write(ptr %data, i64 %length) {
entry:
  %newline = alloca i8, align 1
  store i8 10, ptr %newline, align 1
  %body = call i32 @tz.console.put(ptr %data, i64 %length)
  %failed = icmp slt i32 %body, 0
  br i1 %failed, label %error, label %end
end:
  %last = call i32 @tz.console.put(ptr %newline, i64 1)
  ret i32 %last
error:
  ret i32 -1
}

define internal i32 @tz.console.put(ptr %data, i64 %length) {
entry:
  br label %loop
loop:
  %offset = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %offset, %length
  br i1 %done, label %ok, label %step
step:
  %pointer = getelementptr inbounds i8, ptr %data, i64 %offset
  %remaining = sub i64 %length, %offset
  %written = call i64 @write(i32 1, ptr %pointer, i64 %remaining)
  %progress = icmp sgt i64 %written, 0
  br i1 %progress, label %advance, label %fail
advance:
  %next = add i64 %offset, %written
  br label %loop
ok:
  ret i32 0
fail:
  ret i32 -1
}
