declare i32 @putchar(i32)

define internal i32 @tz.console.write(ptr %data, i64 %length) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %newline, label %byte
byte:
  %p = getelementptr inbounds i8, ptr %data, i64 %i
  %b = load i8, ptr %p
  %c = zext i8 %b to i32
  %r = call i32 @putchar(i32 %c)
  %failed = icmp slt i32 %r, 0
  br i1 %failed, label %error, label %advance
advance:
  %next = add i64 %i, 1
  br label %loop
newline:
  %last = call i32 @putchar(i32 10)
  ret i32 %last
error:
  ret i32 -1
}
