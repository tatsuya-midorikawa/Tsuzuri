; PM03: the integer parsers for `TSUZURI_STRINGS=compact`; see src/runtime/string_compact.ll. The UTF-16
; originals are renamed to `<name>.utf16` when this file is appended, and src/llvm_strings.rs adds
; `.latin1` copies of `scan`, `parse`, and `parse128` that read bytes. A unit above 0x7F is no digit
; whichever its width, so the copies change nothing but how a unit is loaded.

define internal i1 @tz.integer.parse(ptr %input, i64 %length, i32 %bits, i1 %signed, ptr %out) nounwind {
entry:
  %marked = and i64 %length, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %count = and i64 %length, 4611686018427387903
  %parsed = call i1 @tz.integer.parse.latin1(ptr %input, i64 %count, i32 %bits, i1 %signed, ptr %out)
  ret i1 %parsed
units:
  %legacy = call i1 @tz.integer.parse.utf16(ptr %input, i64 %length, i32 %bits, i1 %signed, ptr %out)
  ret i1 %legacy
}

define internal i1 @tz.integer.parse128(ptr %input, i64 %length, i1 %signed, ptr %out) nounwind {
entry:
  %marked = and i64 %length, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %count = and i64 %length, 4611686018427387903
  %parsed = call i1 @tz.integer.parse128.latin1(ptr %input, i64 %count, i1 %signed, ptr %out)
  ret i1 %parsed
units:
  %legacy = call i1 @tz.integer.parse128.utf16(ptr %input, i64 %length, i1 %signed, ptr %out)
  ret i1 %legacy
}
