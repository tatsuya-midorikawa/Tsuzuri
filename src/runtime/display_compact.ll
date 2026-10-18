; PM03: the display functions for `TSUZURI_STRINGS=compact`; see src/runtime/string_compact.ll. The
; UTF-16 originals are renamed to `<name>.utf16` when this file is appended.

; The same text as the UTF-16 join, written as a Latin-1 string, when every part is Latin-1 or empty.
; When a part is UTF-16, the Latin-1 parts, which the call owns like all of them, are widened in
; place and the UTF-16 join takes over.
define internal %tz.string @tz.display.join(%tz.array %parts, i32 %kind) nounwind {
entry:
  %data = extractvalue %tz.array %parts, 0
  %length = extractvalue %tz.array %parts, 1
  %tuple = icmp eq i32 %kind, 0
  %linked = icmp eq i32 %kind, 2
  %open = select i1 %tuple, i8 40, i8 91
  %close = select i1 %tuple, i8 41, i8 93
  %overhead = select i1 %linked, i64 4, i64 2
  br label %size_test
size_test:
  %size_index = phi i64 [ 0, %entry ], [ %size_next, %size_advance ]
  %total = phi i64 [ %overhead, %entry ], [ %total_next, %size_advance ]
  %size_done = icmp eq i64 %size_index, %length
  br i1 %size_done, label %allocate, label %size_body
size_body:
  %size_pointer = getelementptr inbounds %tz.string, ptr %data, i64 %size_index
  %size_part = load %tz.string, ptr %size_pointer
  %size_word = extractvalue %tz.string %size_part, 1
  %size_marked = and i64 %size_word, 4611686018427387904
  %size_plain = icmp eq i64 %size_marked, 0
  %size_full = icmp ne i64 %size_word, 0
  %size_utf16 = and i1 %size_plain, %size_full
  br i1 %size_utf16, label %widen, label %size_check
size_check:
  %size_length = and i64 %size_word, 4611686018427387903
  %first = icmp eq i64 %size_index, 0
  %separator = select i1 %first, i64 0, i64 2
  %available = sub i64 9007199254740991, %total
  %with_separator = icmp uge i64 %available, %separator
  %space = sub i64 %available, %separator
  %fits_part = icmp ule i64 %size_length, %space
  %fits = and i1 %with_separator, %fits_part
  br i1 %fits, label %size_advance, label %fail
size_advance:
  %separated = add i64 %total, %separator
  %total_next = add i64 %separated, %size_length
  %size_next = add i64 %size_index, 1
  br label %size_test
fail:
  call void @llvm.trap()
  unreachable
widen:
  br label %widen_test
widen_test:
  %widen_index = phi i64 [ 0, %widen ], [ %widen_next, %widen_body ]
  %widen_done = icmp eq i64 %widen_index, %length
  br i1 %widen_done, label %delegate, label %widen_body
widen_body:
  %widen_pointer = getelementptr inbounds %tz.string, ptr %data, i64 %widen_index
  %widen_part = load %tz.string, ptr %widen_pointer
  %widened = call %tz.string @tz.string.to_utf16(%tz.string %widen_part)
  store %tz.string %widened, ptr %widen_pointer
  %widen_next = add i64 %widen_index, 1
  br label %widen_test
delegate:
  %legacy = call %tz.string @tz.display.join.utf16(%tz.array %parts, i32 %kind)
  ret %tz.string %legacy
allocate:
  %result = call %tz.string @tz.string.allocate_latin1(i64 %total)
  %output = extractvalue %tz.string %result, 0
  store i8 %open, ptr %output
  br i1 %linked, label %open_bar, label %ready
open_bar:
  %bar_pointer = getelementptr inbounds i8, ptr %output, i64 1
  store i8 124, ptr %bar_pointer
  br label %ready
ready:
  %start = select i1 %linked, i64 2, i64 1
  br label %write_test
write_test:
  %index = phi i64 [ 0, %ready ], [ %next, %copy ]
  %position = phi i64 [ %start, %ready ], [ %next_position, %copy ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %finish, label %write_body
write_body:
  %part_pointer = getelementptr inbounds %tz.string, ptr %data, i64 %index
  %part = load %tz.string, ptr %part_pointer
  %part_data = extractvalue %tz.string %part, 0
  %part_word = extractvalue %tz.string %part, 1
  %part_length = and i64 %part_word, 4611686018427387903
  %write_first = icmp eq i64 %index, 0
  br i1 %write_first, label %copy, label %separate
separate:
  %comma_pointer = getelementptr inbounds i8, ptr %output, i64 %position
  store i8 44, ptr %comma_pointer
  %space_position = add i64 %position, 1
  %space_pointer = getelementptr inbounds i8, ptr %output, i64 %space_position
  store i8 32, ptr %space_pointer
  %after_separator = add i64 %position, 2
  br label %copy
copy:
  %part_start = phi i64 [ %position, %write_body ], [ %after_separator, %separate ]
  %target = getelementptr inbounds i8, ptr %output, i64 %part_start
  call void @tz.string.copy_bytes(ptr %target, ptr %part_data, i64 %part_length)
  call void @tz.free(ptr %part_data)
  %next_position = add i64 %part_start, %part_length
  %next = add i64 %index, 1
  br label %write_test
finish:
  br i1 %linked, label %close_bar, label %closing
close_bar:
  %last_bar = getelementptr inbounds i8, ptr %output, i64 %position
  store i8 124, ptr %last_bar
  br label %closing
closing:
  %bar_width = select i1 %linked, i64 1, i64 0
  %close_position = add i64 %position, %bar_width
  %close_pointer = getelementptr inbounds i8, ptr %output, i64 %close_position
  store i8 %close, ptr %close_pointer
  call void @tz.free(ptr %data)
  ret %tz.string %result
}

; A Latin-1 or empty text is quoted as its UTF-16 units and the result, which has only ASCII added to
; the units, becomes Latin-1 again. The escapes of a unit never look at its neighbours unless it is a
; surrogate, and no Latin-1 unit is one.
define internal %tz.string @tz.display.quote(ptr %input, i64 %word, i32 %kind) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  %empty = icmp eq i64 %word, 0
  %compact = or i1 %latin1, %empty
  br i1 %compact, label %bytes, label %units
bytes:
  %length = and i64 %word, 4611686018427387903
  br i1 %empty, label %nothing, label %widen
widen:
  %size = mul i64 %length, 2
  %wide = call ptr @tz.alloc(i64 %size)
  call void @tz.string.widen(ptr %wide, ptr %input, i64 %length)
  %quoted_text = call %tz.string @tz.display.quote.utf16(ptr %wide, i64 %length, i32 %kind)
  call void @tz.free(ptr %wide)
  br label %narrow
nothing:
  %quoted_nothing = call %tz.string @tz.display.quote.utf16(ptr %input, i64 0, i32 %kind)
  br label %narrow
narrow:
  %quoted = phi %tz.string [ %quoted_text, %widen ], [ %quoted_nothing, %nothing ]
  %quoted_data = extractvalue %tz.string %quoted, 0
  %quoted_length = extractvalue %tz.string %quoted, 1
  %result = call %tz.string @tz.string.from_units(ptr %quoted_data, i64 %quoted_length)
  ret %tz.string %result
units:
  %legacy = call %tz.string @tz.display.quote.utf16(ptr %input, i64 %word, i32 %kind)
  ret %tz.string %legacy
}
