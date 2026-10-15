declare i64 @tsuzuri_cpu_utf8_to_utf16(ptr, i64, ptr)
declare i32 @tsuzuri_cpu_utf8_validate(ptr, i64)

define internal i1 @tz.string.validate_utf8(ptr %source, i64 %length) nounwind {
entry:
  %large = icmp uge i64 %length, 32
  br i1 %large, label %cpu, label %portable
portable:
  %small = call i1 @tz.string.validate_utf8.portable(ptr %source, i64 %length)
  ret i1 %small
cpu:
  %valid = call i32 @tsuzuri_cpu_utf8_validate(ptr %source, i64 %length)
  %result = icmp ne i32 %valid, 0
  ret i1 %result
}

define internal %tz.string @tz.string.from_utf8(ptr %source, i64 %length) nounwind alwaysinline {
entry:
  %short = icmp ult i64 %length, 16
  br i1 %short, label %scalar, label %width
scalar:
  %original = call %tz.string @tz.string.from_utf8.short(ptr %source, i64 %length)
  ret %tz.string %original
width:
  %large = icmp uge i64 %length, 32
  br i1 %large, label %count, label %portable
portable:
  %small = call %tz.string @tz.string.from_utf8.portable(ptr %source, i64 %length)
  ret %tz.string %small
count:
  %units = call i64 @tsuzuri_cpu_utf8_to_utf16(ptr %source, i64 %length, ptr null)
  %valid = icmp sge i64 %units, 0
  br i1 %valid, label %allocate, label %fail
fail:
  %legacy = call %tz.string @tz.string.from_utf8.portable(ptr %source, i64 %length)
  ret %tz.string %legacy
allocate:
  %result = call %tz.string @tz.string.allocate(i64 %units)
  %data = extractvalue %tz.string %result, 0
  %written = call i64 @tsuzuri_cpu_utf8_to_utf16(ptr %source, i64 %length, ptr %data)
  ret %tz.string %result
}
