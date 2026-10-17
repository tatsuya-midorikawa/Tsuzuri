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

; UTF-8 text that is all ASCII becomes a Latin-1 string, copied once; any other text is decoded by
; the UTF-16 function, and so is every invalid one.
define internal %tz.string @tz.string.from_utf8(ptr %source, i64 %byte_length) nounwind {
entry:
  %ascii = call i1 @tz.string.is_ascii(ptr %source, i64 %byte_length)
  br i1 %ascii, label %latin1, label %utf16
latin1:
  %result = call %tz.string @tz.string.from_ascii(ptr %source, i64 %byte_length)
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
