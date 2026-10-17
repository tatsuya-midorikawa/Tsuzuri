; PM03: the character functions for `TSUZURI_STRINGS=compact`; see src/runtime/string_compact.ll. The
; UTF-16 originals are renamed to `<name>.utf16` when this file is appended.

; A character up to U+00FF is a one-byte Latin-1 string; any other is the UTF-16 string that it was.
define internal %tz.string @tz.character.display(i32 %value) nounwind {
entry:
  %small = icmp ule i32 %value, 255
  br i1 %small, label %latin1, label %utf16
latin1:
  %result = call %tz.string @tz.string.allocate_latin1(i64 1)
  %data = extractvalue %tz.string %result, 0
  %byte = trunc i32 %value to i8
  store i8 %byte, ptr %data
  ret %tz.string %result
utf16:
  %legacy = call %tz.string @tz.character.display.utf16(i32 %value)
  ret %tz.string %legacy
}

; `%source` points at a string. One Latin-1 byte is a character that is no surrogate; two Latin-1
; units are never a surrogate pair.
define internal i32 @tz.character.parse(ptr %source, i1 %scalar) nounwind {
entry:
  %text = load %tz.string, ptr %source
  %data = extractvalue %tz.string %text, 0
  %word = extractvalue %tz.string %text, 1
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %single = icmp eq i64 %word, 4611686018427387905
  br i1 %single, label %byte, label %invalid
byte:
  %unit = load i8, ptr %data
  %value = zext i8 %unit to i32
  ret i32 %value
invalid:
  ret i32 -1
units:
  %legacy = call i32 @tz.character.parse.utf16(ptr %source, i1 %scalar)
  ret i32 %legacy
}
