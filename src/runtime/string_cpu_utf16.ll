; SPDX-License-Identifier: Apache-2.0

declare i64 @tsuzuri_cpu_utf16_to_utf8(ptr, i64, ptr)

define internal %tz.utf8string @tz.utf8string.from_string(ptr %source, i64 %length) nounwind {
entry:
  %large = icmp uge i64 %length, 32
  br i1 %large, label %count, label %portable
portable:
  %small = call %tz.utf8string @tz.utf8string.from_string.portable(ptr %source, i64 %length)
  ret %tz.utf8string %small
count:
  %bytes = call i64 @tsuzuri_cpu_utf16_to_utf8(ptr %source, i64 %length, ptr null)
  %valid = icmp sge i64 %bytes, 0
  br i1 %valid, label %allocate, label %fail
fail:
  call void @llvm.trap()
  unreachable
allocate:
  %result = call %tz.utf8string @tz.utf8string.allocate(i64 %bytes)
  %data = extractvalue %tz.utf8string %result, 0
  call void @tz.utf8string.write_utf16(ptr %source, i64 %length, ptr %data)
  ret %tz.utf8string %result
}
