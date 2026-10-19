; PM03: the string functions for `TSUZURI_STRINGS=compact`. A string whose length word has bit 62 set is
; Latin-1, and the low 62 bits count its bytes; any other string is UTF-16, and its word is its
; length (see src/runtime/string_latin1.ll). The functions below keep the names and signatures that
; the generated code calls, take the words as they are, and tell the two forms apart themselves.
;
; src/llvm_strings.rs renames the definitions of the UTF-16 runtime that these replace to `<name>.utf16`
; when it appends this file. A function that is only given UTF-16 strings calls its `.utf16`
; original, so a program with no Latin-1 string runs the code that it ran before. A Latin-1 result
; is made when every input is Latin-1 or empty; a UTF-16 string next to a Latin-1 one makes a UTF-16
; result, with the Latin-1 side widened. @tz.string.allocate and @tz.string.copy stay as they were:
; they make and copy UTF-16 units, and a caller passes them a count, not a word.

; Copies the text of a string that is Latin-1 or UTF-16, given its word, as UTF-16 units.
define internal void @tz.string.copy_as_utf16(ptr %to, ptr %from, i64 %word) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  %length = and i64 %word, 4611686018427387903
  br i1 %latin1, label %bytes, label %units
bytes:
  call void @tz.string.widen(ptr %to, ptr %from, i64 %length)
  ret void
units:
  call void @tz.string.copy(ptr %to, ptr %from, i64 %length)
  ret void
}

define internal %tz.string @tz.string.new(ptr %source, i64 %word) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %length = and i64 %word, 4611686018427387903
  %result = call %tz.string @tz.string.allocate_latin1(i64 %length)
  %data = extractvalue %tz.string %result, 0
  call void @tz.string.copy_bytes(ptr %data, ptr %source, i64 %length)
  ret %tz.string %result
units:
  %legacy = call %tz.string @tz.string.new.utf16(ptr %source, i64 %word)
  ret %tz.string %legacy
}

; ASCII text is Latin-1 text.
define internal %tz.string @tz.string.from_ascii(ptr %source, i64 %length) nounwind {
entry:
  %result = call %tz.string @tz.string.allocate_latin1(i64 %length)
  %data = extractvalue %tz.string %result, 0
  call void @tz.string.copy_bytes(ptr %data, ptr %source, i64 %length)
  ret %tz.string %result
}

define internal %tz.string @tz.string.concat(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %aw = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bw = extractvalue %tz.string %right, 1
  %a_marked = and i64 %aw, 4611686018427387904
  %b_marked = and i64 %bw, 4611686018427387904
  %any_marked = or i64 %a_marked, %b_marked
  %none = icmp eq i64 %any_marked, 0
  br i1 %none, label %utf16, label %compact
utf16:
  %legacy = call %tz.string @tz.string.concat.utf16(%tz.string %left, %tz.string %right)
  ret %tz.string %legacy
compact:
  %an = and i64 %aw, 4611686018427387903
  %bn = and i64 %bw, 4611686018427387903
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %form
fail:
  call void @llvm.trap()
  unreachable
form:
  %a_plain = icmp eq i64 %a_marked, 0
  %a_full = icmp ne i64 %aw, 0
  %a_utf16 = and i1 %a_plain, %a_full
  %b_plain = icmp eq i64 %b_marked, 0
  %b_full = icmp ne i64 %bw, 0
  %b_utf16 = and i1 %b_plain, %b_full
  %any_utf16 = or i1 %a_utf16, %b_utf16
  br i1 %any_utf16, label %widen, label %bytes
bytes:
  %latin1 = call %tz.string @tz.string.allocate_latin1(i64 %length)
  %latin1_data = extractvalue %tz.string %latin1, 0
  call void @tz.string.copy_bytes(ptr %latin1_data, ptr %a, i64 %an)
  %latin1_second = getelementptr i8, ptr %latin1_data, i64 %an
  call void @tz.string.copy_bytes(ptr %latin1_second, ptr %b, i64 %bn)
  ret %tz.string %latin1
widen:
  %result = call %tz.string @tz.string.allocate(i64 %length)
  %data = extractvalue %tz.string %result, 0
  call void @tz.string.copy_as_utf16(ptr %data, ptr %a, i64 %aw)
  %second = getelementptr i16, ptr %data, i64 %an
  call void @tz.string.copy_as_utf16(ptr %second, ptr %b, i64 %bw)
  ret %tz.string %result
}

; Appends `%right` to `%left`, which the call consumes: its buffer is resized in place when it can be.
define internal %tz.string @tz.string.append(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %aw = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bw = extractvalue %tz.string %right, 1
  %a_marked = and i64 %aw, 4611686018427387904
  %b_marked = and i64 %bw, 4611686018427387904
  %any_marked = or i64 %a_marked, %b_marked
  %none = icmp eq i64 %any_marked, 0
  br i1 %none, label %utf16, label %compact
utf16:
  %legacy = call %tz.string @tz.string.append.utf16(%tz.string %left, %tz.string %right)
  ret %tz.string %legacy
compact:
  %an = and i64 %aw, 4611686018427387903
  %bn = and i64 %bw, 4611686018427387903
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %form
fail:
  call void @llvm.trap()
  unreachable
form:
  call void @tz.string.allocate.check(i64 %length)
  %a_plain = icmp eq i64 %a_marked, 0
  %a_full = icmp ne i64 %aw, 0
  %a_utf16 = and i1 %a_plain, %a_full
  %b_plain = icmp eq i64 %b_marked, 0
  %b_full = icmp ne i64 %bw, 0
  %b_utf16 = and i1 %b_plain, %b_full
  %any_utf16 = or i1 %a_utf16, %b_utf16
  br i1 %any_utf16, label %widen, label %bytes
bytes:
  call void @tz.string.latin1_fits(i64 %length)
  %latin1_data = call ptr @tz.realloc(ptr %a, i64 %an, i64 %length)
  %latin1_second = getelementptr i8, ptr %latin1_data, i64 %an
  call void @tz.string.copy_bytes(ptr %latin1_second, ptr %b, i64 %bn)
  %latin1_word = or i64 %length, 4611686018427387904
  %latin1_a = insertvalue %tz.string zeroinitializer, ptr %latin1_data, 0
  %latin1 = insertvalue %tz.string %latin1_a, i64 %latin1_word, 1
  ret %tz.string %latin1
widen:
  %a_latin1 = icmp ne i64 %a_marked, 0
  %a_units = mul i64 %an, 2
  %old = select i1 %a_latin1, i64 %an, i64 %a_units
  %new = mul i64 %length, 2
  %data = call ptr @tz.realloc(ptr %a, i64 %old, i64 %new)
  br i1 %a_latin1, label %widen_left, label %append
widen_left:
  call void @tz.string.widen_in_place(ptr %data, i64 %an)
  br label %append
append:
  %second = getelementptr i16, ptr %data, i64 %an
  call void @tz.string.copy_as_utf16(ptr %second, ptr %b, i64 %bw)
  %result_a = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %result = insertvalue %tz.string %result_a, i64 %length, 1
  ret %tz.string %result
}

; The result is written into `%buffer`, which holds `%capacity` UTF-16 units (or twice as many
; Latin-1 bytes), when it fits, and is allocated otherwise.
define internal %tz.string @tz.string.concat_into(ptr %buffer, i64 %capacity, %tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %aw = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bw = extractvalue %tz.string %right, 1
  %a_marked = and i64 %aw, 4611686018427387904
  %b_marked = and i64 %bw, 4611686018427387904
  %any_marked = or i64 %a_marked, %b_marked
  %none = icmp eq i64 %any_marked, 0
  br i1 %none, label %utf16, label %compact
utf16:
  %legacy = call %tz.string @tz.string.concat_into.utf16(ptr %buffer, i64 %capacity, %tz.string %left, %tz.string %right)
  ret %tz.string %legacy
compact:
  %an = and i64 %aw, 4611686018427387903
  %bn = and i64 %bw, 4611686018427387903
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %form
fail:
  call void @llvm.trap()
  unreachable
form:
  call void @tz.string.allocate.check(i64 %length)
  %a_plain = icmp eq i64 %a_marked, 0
  %a_full = icmp ne i64 %aw, 0
  %a_utf16 = and i1 %a_plain, %a_full
  %b_plain = icmp eq i64 %b_marked, 0
  %b_full = icmp ne i64 %bw, 0
  %b_utf16 = and i1 %b_plain, %b_full
  %any_utf16 = or i1 %a_utf16, %b_utf16
  br i1 %any_utf16, label %widen, label %bytes
bytes:
  %byte_capacity = mul i64 %capacity, 2
  %byte_fits = icmp ule i64 %length, %byte_capacity
  br i1 %byte_fits, label %byte_stack, label %byte_heap
byte_stack:
  br label %byte_copy
byte_heap:
  %byte_allocated = call %tz.string @tz.string.allocate_latin1(i64 %length)
  %byte_heap_data = extractvalue %tz.string %byte_allocated, 0
  br label %byte_copy
byte_copy:
  %byte_data = phi ptr [ %buffer, %byte_stack ], [ %byte_heap_data, %byte_heap ]
  call void @tz.string.copy_bytes(ptr %byte_data, ptr %a, i64 %an)
  %byte_second = getelementptr i8, ptr %byte_data, i64 %an
  call void @tz.string.copy_bytes(ptr %byte_second, ptr %b, i64 %bn)
  %byte_word = or i64 %length, 4611686018427387904
  %byte_a = insertvalue %tz.string zeroinitializer, ptr %byte_data, 0
  %byte_result = insertvalue %tz.string %byte_a, i64 %byte_word, 1
  ret %tz.string %byte_result
widen:
  %fits = icmp ule i64 %length, %capacity
  br i1 %fits, label %stack, label %heap
stack:
  br label %copy
heap:
  %allocated = call %tz.string @tz.string.allocate(i64 %length)
  %heap_data = extractvalue %tz.string %allocated, 0
  br label %copy
copy:
  %data = phi ptr [ %buffer, %stack ], [ %heap_data, %heap ]
  call void @tz.string.copy_as_utf16(ptr %data, ptr %a, i64 %aw)
  %second = getelementptr i16, ptr %data, i64 %an
  call void @tz.string.copy_as_utf16(ptr %second, ptr %b, i64 %bw)
  %result_a = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %result = insertvalue %tz.string %result_a, i64 %length, 1
  ret %tz.string %result
}

; The ASCII bytes of a string, as @tz.string.to_ascii writes them.
define internal i1 @tz.string.to_ascii(ptr %destination, ptr %source, i64 %word) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %length = and i64 %word, 4611686018427387903
  %result = call i1 @tz.string.to_ascii_latin1(ptr %destination, ptr %source, i64 %length)
  ret i1 %result
units:
  %legacy = call i1 @tz.string.to_ascii.utf16(ptr %destination, ptr %source, i64 %word)
  ret i1 %legacy
}

; Two Latin-1 strings are the UTF-8 comparison of their bytes, since a byte is a unit. A Latin-1 string
; and a UTF-16 one are compared unit by unit, whichever the form of a string with the same units.
define internal i1 @tz.string.equal(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %aw = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bw = extractvalue %tz.string %right, 1
  %a_marked = and i64 %aw, 4611686018427387904
  %b_marked = and i64 %bw, 4611686018427387904
  %any_marked = or i64 %a_marked, %b_marked
  %none = icmp eq i64 %any_marked, 0
  br i1 %none, label %utf16, label %compact
utf16:
  %legacy = call i1 @tz.string.equal.utf16(%tz.string %left, %tz.string %right)
  ret i1 %legacy
compact:
  %an = and i64 %aw, 4611686018427387903
  %bn = and i64 %bw, 4611686018427387903
  %same = icmp eq i64 %an, %bn
  br i1 %same, label %forms, label %different
forms:
  %both_marked = and i64 %a_marked, %b_marked
  %both = icmp ne i64 %both_marked, 0
  br i1 %both, label %bytes, label %mixed
bytes:
  %left_bytes_a = insertvalue %tz.utf8string zeroinitializer, ptr %a, 0
  %left_bytes = insertvalue %tz.utf8string %left_bytes_a, i64 %an, 1
  %right_bytes_a = insertvalue %tz.utf8string zeroinitializer, ptr %b, 0
  %right_bytes = insertvalue %tz.utf8string %right_bytes_a, i64 %bn, 1
  %bytes_equal = call i1 @tz.utf8string.equal(%tz.utf8string %left_bytes, %tz.utf8string %right_bytes)
  ret i1 %bytes_equal
mixed:
  %a_latin1 = icmp ne i64 %a_marked, 0
  %latin1_pointer = select i1 %a_latin1, ptr %a, ptr %b
  %units_pointer = select i1 %a_latin1, ptr %b, ptr %a
  %mixed_equal = call i1 @tz.string.equal_mixed(ptr %latin1_pointer, ptr %units_pointer, i64 %an)
  ret i1 %mixed_equal
different:
  ret i1 false
}

; The same order as @tz.string.compare: -1, 0, or 1, by the first unit that differs and then by length.
define internal i32 @tz.string.compare(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %aw = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bw = extractvalue %tz.string %right, 1
  %a_marked = and i64 %aw, 4611686018427387904
  %b_marked = and i64 %bw, 4611686018427387904
  %any_marked = or i64 %a_marked, %b_marked
  %none = icmp eq i64 %any_marked, 0
  br i1 %none, label %utf16, label %compact
utf16:
  %legacy = call i32 @tz.string.compare.utf16(%tz.string %left, %tz.string %right)
  ret i32 %legacy
compact:
  %an = and i64 %aw, 4611686018427387903
  %bn = and i64 %bw, 4611686018427387903
  %both_marked = and i64 %a_marked, %b_marked
  %both = icmp ne i64 %both_marked, 0
  br i1 %both, label %bytes, label %mixed
bytes:
  %left_bytes_a = insertvalue %tz.utf8string zeroinitializer, ptr %a, 0
  %left_bytes = insertvalue %tz.utf8string %left_bytes_a, i64 %an, 1
  %right_bytes_a = insertvalue %tz.utf8string zeroinitializer, ptr %b, 0
  %right_bytes = insertvalue %tz.utf8string %right_bytes_a, i64 %bn, 1
  %bytes_order = call i32 @tz.utf8string.compare(%tz.utf8string %left_bytes, %tz.utf8string %right_bytes)
  ret i32 %bytes_order
mixed:
  %a_latin1 = icmp ne i64 %a_marked, 0
  br i1 %a_latin1, label %left_bytes_first, label %right_bytes_first
left_bytes_first:
  %forward = call i32 @tz.string.compare_mixed(ptr %a, i64 %an, ptr %b, i64 %bn)
  ret i32 %forward
right_bytes_first:
  %backward = call i32 @tz.string.compare_mixed(ptr %b, i64 %bn, ptr %a, i64 %an)
  %reversed = sub i32 0, %backward
  ret i32 %reversed
}

; UTF-8 text of which every scalar is at most U+00FF (ASCII and the Latin-1 letters) becomes a Latin-1 string,
; counted by one scan and written once; any other text is decoded by the UTF-16 function, and so is every
; invalid one. A length that no text of UTF-16 units could have is a trap before anything is read, as in
; the UTF-16 function.
define internal %tz.string @tz.string.from_utf8(ptr %source, i64 %byte_length) nounwind {
entry:
  %fits = icmp ule i64 %byte_length, 27021597764222973
  br i1 %fits, label %scan, label %fail
fail:
  call void @llvm.trap()
  unreachable
scan:
  %scanned = call { i64, i32 } @tz.string.utf8_scan(ptr %source, i64 %byte_length)
  %units = extractvalue { i64, i32 } %scanned, 0
  %latin1 = icmp sge i64 %units, 0
  br i1 %latin1, label %make, label %utf16
make:
  %result = call %tz.string @tz.string.allocate_latin1(i64 %units)
  %data = extractvalue %tz.string %result, 0
  %ascii = icmp eq i64 %units, %byte_length
  br i1 %ascii, label %copy, label %transcode
copy:
  call void @tz.string.copy_bytes(ptr %data, ptr %source, i64 %units)
  ret %tz.string %result
transcode:
  call void @tz.string.utf8_to_latin1(ptr %source, i64 %byte_length, ptr %data)
  ret %tz.string %result
utf16:
  %legacy = call %tz.string @tz.string.from_utf8.utf16(ptr %source, i64 %byte_length)
  ret %tz.string %legacy
}

define internal %tz.utf8string @tz.utf8string.from_string(ptr %source, i64 %word) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %length = and i64 %word, 4611686018427387903
  %result = call %tz.utf8string @tz.utf8string.from_latin1(ptr %source, i64 %length)
  ret %tz.utf8string %result
units:
  %legacy = call %tz.utf8string @tz.utf8string.from_string.utf16(ptr %source, i64 %word)
  ret %tz.utf8string %legacy
}

; Latin-1 text has no surrogate.
define internal i1 @tz.string.is_well_formed(ptr %source, i64 %word) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %yes, label %units
yes:
  ret i1 true
units:
  %legacy = call i1 @tz.string.is_well_formed.utf16(ptr %source, i64 %word)
  ret i1 %legacy
}

define internal %tz.string @tz.string.to_well_formed(ptr %source, i64 %word) nounwind {
entry:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %copy, label %units
copy:
  %clone = call %tz.string @tz.string.new(ptr %source, i64 %word)
  ret %tz.string %clone
units:
  %legacy = call %tz.string @tz.string.to_well_formed.utf16(ptr %source, i64 %word)
  ret %tz.string %legacy
}

; The units of a string that the caller owns, as UTF-16: a Latin-1 string is resized to twice its bytes
; and widened in place; a UTF-16 one is returned as it is.
define internal %tz.string @tz.string.to_utf16(%tz.string %text) nounwind {
entry:
  %data = extractvalue %tz.string %text, 0
  %word = extractvalue %tz.string %text, 1
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %widen, label %done
widen:
  %length = and i64 %word, 4611686018427387903
  %bytes = mul i64 %length, 2
  %grown = call ptr @tz.realloc(ptr %data, i64 %length, i64 %bytes)
  call void @tz.string.widen_in_place(ptr %grown, i64 %length)
  %a = insertvalue %tz.string zeroinitializer, ptr %grown, 0
  %b = insertvalue %tz.string %a, i64 %length, 1
  ret %tz.string %b
done:
  ret %tz.string %text
}

; The units of a string that the caller keeps, as UTF-16, for a call that only reads them: a Latin-1
; string is widened into a new buffer, which the caller frees when the word of the original is
; marked; a UTF-16 string is returned as it is.
define internal %tz.string @tz.string.borrow_utf16(%tz.string %text) nounwind {
entry:
  %data = extractvalue %tz.string %text, 0
  %word = extractvalue %tz.string %text, 1
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %widen, label %done
widen:
  %length = and i64 %word, 4611686018427387903
  %bytes = mul i64 %length, 2
  %copy = call ptr @tz.alloc(i64 %bytes)
  call void @tz.string.widen(ptr %copy, ptr %data, i64 %length)
  %a = insertvalue %tz.string zeroinitializer, ptr %copy, 0
  %b = insertvalue %tz.string %a, i64 %length, 1
  ret %tz.string %b
done:
  ret %tz.string %text
}

; A string from UTF-16 units that the caller owns and gives up. Units that are all at most 0xFF are
; narrowed in place to a Latin-1 string and the buffer is shrunk; any other units stay.
define internal %tz.string @tz.string.from_units(ptr %data, i64 %length) nounwind {
entry:
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %utf16, label %scan
scan:
  %fits = call i1 @tz.string.is_latin1_units(ptr %data, i64 %length)
  br i1 %fits, label %narrow, label %utf16
narrow:
  call void @tz.string.narrow_in_place(ptr %data, i64 %length)
  %old = mul i64 %length, 2
  %shrunk = call ptr @tz.realloc(ptr %data, i64 %old, i64 %length)
  %word = or i64 %length, 4611686018427387904
  %a = insertvalue %tz.string zeroinitializer, ptr %shrunk, 0
  %b = insertvalue %tz.string %a, i64 %word, 1
  ret %tz.string %b
utf16:
  %c = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %d = insertvalue %tz.string %c, i64 %length, 1
  ret %tz.string %d
}

; Copies the text of a string, given as a pointer and a length word, to unit `%offset` of the data of a
; result that is UTF-16 when `%wide` and Latin-1 otherwise. A Latin-1 result only takes Latin-1 text.
define internal void @tz.string.put(ptr %output, i64 %offset, ptr %source, i64 %word, i1 %wide) nounwind {
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

; The units [`%first`, `%last`) of a string, which the caller has checked to be in range, as a string of the
; same form: a slice of a Latin-1 string is Latin-1. This is what String.slice does, in one allocation.
define internal %tz.string @tz.string.slice(ptr %data, i64 %word, i64 %first, i64 %last) nounwind {
entry:
  %length = sub i64 %last, %first
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %zero, label %copy
zero:
  ret %tz.string zeroinitializer
copy:
  %marked = and i64 %word, 4611686018427387904
  %latin1 = icmp ne i64 %marked, 0
  br i1 %latin1, label %bytes, label %units
bytes:
  %byte_result = call %tz.string @tz.string.allocate_latin1(i64 %length)
  %byte_target = extractvalue %tz.string %byte_result, 0
  %byte_source = getelementptr inbounds i8, ptr %data, i64 %first
  call void @tz.string.copy_bytes(ptr %byte_target, ptr %byte_source, i64 %length)
  ret %tz.string %byte_result
units:
  %unit_result = call %tz.string @tz.string.allocate(i64 %length)
  %unit_target = extractvalue %tz.string %unit_result, 0
  %unit_source = getelementptr inbounds i16, ptr %data, i64 %first
  call void @tz.string.copy(ptr %unit_target, ptr %unit_source, i64 %length)
  ret %tz.string %unit_result
}

; The strings of an array of `%count` strings with `%separator` between them, as String.join makes
; them: Latin-1 when the separator, if it is used, and every part are Latin-1 or empty, and UTF-16
; otherwise. A total that no string can have is a trap. The parts are borrowed.
define internal %tz.string @tz.string.join(ptr %separator, i64 %separator_word, ptr %parts, i64 %count) nounwind {
entry:
  %separator_length = and i64 %separator_word, 4611686018427387903
  %separator_marked = and i64 %separator_word, 4611686018427387904
  %separator_plain = icmp eq i64 %separator_marked, 0
  %separator_full = icmp ne i64 %separator_word, 0
  %separator_utf16 = and i1 %separator_plain, %separator_full
  %several = icmp ugt i64 %count, 1
  %separator_wide = and i1 %separator_utf16, %several
  br label %size_test
size_test:
  %size_index = phi i64 [ 0, %entry ], [ %size_next, %size_step ]
  %total = phi i64 [ 0, %entry ], [ %grown, %size_step ]
  %wide = phi i1 [ %separator_wide, %entry ], [ %wide_next, %size_step ]
  %size_more = icmp ult i64 %size_index, %count
  br i1 %size_more, label %size_body, label %allocate
size_body:
  %size_pointer = getelementptr inbounds %tz.string, ptr %parts, i64 %size_index
  %size_part = load %tz.string, ptr %size_pointer
  %size_word = extractvalue %tz.string %size_part, 1
  %size_length = and i64 %size_word, 4611686018427387903
  %size_marked = and i64 %size_word, 4611686018427387904
  %size_plain = icmp eq i64 %size_marked, 0
  %size_full = icmp ne i64 %size_word, 0
  %size_utf16 = and i1 %size_plain, %size_full
  %wide_next = or i1 %wide, %size_utf16
  %not_first = icmp ne i64 %size_index, 0
  %extra = select i1 %not_first, i64 %separator_length, i64 0
  %with_separator = add i64 %total, %extra
  %grown = add i64 %with_separator, %size_length
  %fits = icmp ule i64 %grown, 9007199254740991
  br i1 %fits, label %size_step, label %fail
size_step:
  %size_next = add i64 %size_index, 1
  br label %size_test
fail:
  call void @llvm.trap()
  unreachable
allocate:
  br i1 %wide, label %allocate_units, label %allocate_bytes
allocate_units:
  %unit_result = call %tz.string @tz.string.allocate(i64 %total)
  br label %write
allocate_bytes:
  %byte_result = call %tz.string @tz.string.allocate_latin1(i64 %total)
  br label %write
write:
  %result = phi %tz.string [ %unit_result, %allocate_units ], [ %byte_result, %allocate_bytes ]
  %output = extractvalue %tz.string %result, 0
  br label %write_test
write_test:
  %index = phi i64 [ 0, %write ], [ %next, %write_step ]
  %offset = phi i64 [ 0, %write ], [ %after_part, %write_step ]
  %more = icmp ult i64 %index, %count
  br i1 %more, label %write_body, label %done
write_body:
  %first = icmp eq i64 %index, 0
  br i1 %first, label %write_part, label %write_separator
write_separator:
  call void @tz.string.put(ptr %output, i64 %offset, ptr %separator, i64 %separator_word, i1 %wide)
  %after_separator = add i64 %offset, %separator_length
  br label %write_part
write_part:
  %part_offset = phi i64 [ %offset, %write_body ], [ %after_separator, %write_separator ]
  %part_pointer = getelementptr inbounds %tz.string, ptr %parts, i64 %index
  %part = load %tz.string, ptr %part_pointer
  %part_data = extractvalue %tz.string %part, 0
  %part_word = extractvalue %tz.string %part, 1
  %part_length = and i64 %part_word, 4611686018427387903
  call void @tz.string.put(ptr %output, i64 %part_offset, ptr %part_data, i64 %part_word, i1 %wide)
  %after_part = add i64 %part_offset, %part_length
  br label %write_step
write_step:
  %next = add i64 %index, 1
  br label %write_test
done:
  ret %tz.string %result
}
