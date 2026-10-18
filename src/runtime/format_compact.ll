; PM03: the helpers of string interpolation for `TSUZURI_STRINGS=compact`; see src/runtime/string_compact.ll.
; The result of an interpolation is Latin-1 when every piece of it is Latin-1 or empty and every fill is
; at most U+00FF, and UTF-16 otherwise. The generated code adds up the pieces, tells these helpers which
; form to make with `%wide`, and gives each piece as a pointer and a length word, whatever its form.

; The number of Unicode scalars of a text given as a pointer and a length word.
define internal i64 @tz.format.scalars(ptr %data, i64 %word) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %count = and i64 %word, 4611686018427387903
  ret i64 %count
units:
  %scalars = call i64 @tz.format.scalars.utf16(ptr %data, i64 %word)
  ret i64 %scalars
}

; A string of `%total` units, UTF-16 when `%wide` and Latin-1 otherwise, with nothing written.
define internal %tz.string @tz.format.allocate(i64 %total, i1 %wide) nounwind {
entry:
  br i1 %wide, label %units, label %bytes
units:
  %utf16 = call %tz.string @tz.string.allocate(i64 %total)
  ret %tz.string %utf16
bytes:
  %latin1 = call %tz.string @tz.string.allocate_latin1(i64 %total)
  ret %tz.string %latin1
}

; Copies the text of a string, given as a pointer and a length word, to unit `%offset` of the data of
; a result of the form `%wide`. A Latin-1 result only takes Latin-1 text.
define internal void @tz.format.put(ptr %output, i64 %offset, ptr %source, i64 %word, i1 %wide) nounwind {
entry:
  br i1 %wide, label %units, label %bytes
units:
  %unit_target = getelementptr inbounds i16, ptr %output, i64 %offset
  call void @tz.string.copy_as_utf16(ptr %unit_target, ptr %source, i64 %word)
  ret void
bytes:
  %byte_target = getelementptr inbounds i8, ptr %output, i64 %offset
  %count = and i64 %word, 4611686018427387903
  call void @tz.string.copy_bytes(ptr %byte_target, ptr %source, i64 %count)
  ret void
}

; Writes `%count` copies of a fill scalar at unit `%offset` and returns the units written. A Latin-1
; result only has fills of one byte.
define internal i64 @tz.format.fill(ptr %output, i64 %offset, i64 %count, i32 %packed, i64 %width, i1 %wide) nounwind {
entry:
  br i1 %wide, label %units, label %bytes
units:
  %unit_target = getelementptr inbounds i16, ptr %output, i64 %offset
  %unit_count = call i64 @tz.format.fill.u16(ptr %unit_target, i64 %count, i32 %packed, i64 %width)
  ret i64 %unit_count
bytes:
  %byte_target = getelementptr inbounds i8, ptr %output, i64 %offset
  %byte_count = call i64 @tz.format.fill.u8(ptr %byte_target, i64 %count, i32 %packed, i64 1)
  ret i64 %byte_count
}
