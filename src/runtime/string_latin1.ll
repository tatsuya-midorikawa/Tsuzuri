; PM03: the kernels of Latin-1 strings, for `TSUZURI_STRINGS=compact`.
;
; A string is { ptr, i64 }. The second field, the length word, is the number of 16-bit code units
; of a UTF-16 string, as it always was. When bit 62 is set, the string is Latin-1: each unit is one
; byte (a code unit of at most 0xFF) and the low 62 bits are the number of units. Bit 63 is 0 in
; every string, because PM01 uses a negative length for the absent case of Maybe<string>. The empty
; string is the word 0 and is never Latin-1.
;
; These kernels take a pointer and a count, never a word: src/runtime/string_compact.ll takes the
; words apart and calls them. They read and write units one byte at a time or, to test or compare a
; run of units, eight bytes at a time. A little-endian target is assumed, as everywhere in the runtime
; that loads a vector of units.

; A Latin-1 string of `%length` units, with nothing written. 0 gives the empty string. Like
; @tz.string.allocate, a length above 2^53 - 1 is a trap.
define internal %tz.string @tz.string.allocate_latin1(i64 %length) nounwind {
entry:
  %fits = icmp ule i64 %length, 9007199254740991
  br i1 %fits, label %checkempty, label %fail
fail:
  call void @llvm.trap()
  unreachable
checkempty:
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %zero, label %allocate
zero:
  ret %tz.string zeroinitializer
allocate:
  %data = call ptr @tz.alloc(i64 %length)
  %word = or i64 %length, 4611686018427387904
  %a = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %b = insertvalue %tz.string %a, i64 %word, 1
  ret %tz.string %b
}

; Copies `%length` bytes forwards.
define internal void @tz.string.copy_bytes(ptr %to, ptr %from, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %exit, label %copy
copy:
  %source = getelementptr inbounds i8, ptr %from, i64 %i
  %destination = getelementptr inbounds i8, ptr %to, i64 %i
  %byte = load i8, ptr %source
  store i8 %byte, ptr %destination
  %next = add i64 %i, 1
  br label %loop
exit:
  ret void
}

; Writes `%length` Latin-1 bytes as UTF-16 units. The two buffers do not overlap.
define internal void @tz.string.widen(ptr %to, ptr %from, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %exit, label %copy
copy:
  %source = getelementptr inbounds i8, ptr %from, i64 %i
  %byte = load i8, ptr %source
  %unit = zext i8 %byte to i16
  %destination = getelementptr inbounds i16, ptr %to, i64 %i
  store i16 %unit, ptr %destination
  %next = add i64 %i, 1
  br label %loop
exit:
  ret void
}

; Widens `%length` Latin-1 bytes at the start of a buffer of 2 * `%length` bytes, in place. It goes
; from the end: unit i is written over bytes 2i and 2i + 1, which no byte of a lower index is.
define internal void @tz.string.widen_in_place(ptr %data, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %remaining = phi i64 [ %length, %entry ], [ %index, %copy ]
  %done = icmp eq i64 %remaining, 0
  br i1 %done, label %exit, label %copy
copy:
  %index = sub i64 %remaining, 1
  %source = getelementptr inbounds i8, ptr %data, i64 %index
  %byte = load i8, ptr %source
  %unit = zext i8 %byte to i16
  %destination = getelementptr inbounds i16, ptr %data, i64 %index
  store i16 %unit, ptr %destination
  br label %loop
exit:
  ret void
}

; Narrows `%length` UTF-16 units that are all at most 0xFF to bytes, in place, from the start: byte i
; is written over a byte that no unit of a higher index uses.
define internal void @tz.string.narrow_in_place(ptr %data, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %exit, label %copy
copy:
  %source = getelementptr inbounds i16, ptr %data, i64 %i
  %unit = load i16, ptr %source
  %byte = trunc i16 %unit to i8
  %destination = getelementptr inbounds i8, ptr %data, i64 %i
  store i8 %byte, ptr %destination
  %next = add i64 %i, 1
  br label %loop
exit:
  ret void
}

; Whether every UTF-16 unit is at most 0xFF, four at a time.
define internal i1 @tz.string.is_latin1_units(ptr %data, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %wide_next, %wide ], [ %next, %scalar ]
  %remaining = sub i64 %length, %i
  %enough = icmp uge i64 %remaining, 4
  br i1 %enough, label %wide, label %tail
wide:
  %wide_pointer = getelementptr inbounds i16, ptr %data, i64 %i
  %wide_word = load i64, ptr %wide_pointer, align 2
  %wide_high = and i64 %wide_word, -71777214294589696
  %wide_clean = icmp eq i64 %wide_high, 0
  %wide_next = add i64 %i, 4
  br i1 %wide_clean, label %loop, label %no
tail:
  %done = icmp eq i64 %i, %length
  br i1 %done, label %yes, label %scalar
scalar:
  %pointer = getelementptr inbounds i16, ptr %data, i64 %i
  %unit = load i16, ptr %pointer
  %fits = icmp ule i16 %unit, 255
  %next = add i64 %i, 1
  br i1 %fits, label %loop, label %no
yes:
  ret i1 true
no:
  ret i1 false
}

; Whether `%length` Latin-1 bytes and `%length` UTF-16 units are the same code units.
define internal i1 @tz.string.equal_mixed(ptr %bytes, ptr %units, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %equal, label %compare
compare:
  %byte_pointer = getelementptr inbounds i8, ptr %bytes, i64 %i
  %unit_pointer = getelementptr inbounds i16, ptr %units, i64 %i
  %byte = load i8, ptr %byte_pointer
  %unit = load i16, ptr %unit_pointer
  %widened = zext i8 %byte to i16
  %match = icmp eq i16 %widened, %unit
  br i1 %match, label %advance, label %different
advance:
  %next = add i64 %i, 1
  br label %loop
equal:
  ret i1 true
different:
  ret i1 false
}

; The order of Latin-1 bytes against UTF-16 units, as @tz.string.compare orders two UTF-16 strings:
; the first unit that differs decides, as an unsigned number, and otherwise the shorter text is
; smaller. Returns -1, 0, or 1 for `bytes` before, equal to, or after `units`.
define internal i32 @tz.string.compare_mixed(ptr %bytes, i64 %byte_length, ptr %units, i64 %unit_length) nounwind {
entry:
  %shorter = icmp ult i64 %byte_length, %unit_length
  %length = select i1 %shorter, i64 %byte_length, i64 %unit_length
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %prefix, label %compare
compare:
  %byte_pointer = getelementptr inbounds i8, ptr %bytes, i64 %i
  %unit_pointer = getelementptr inbounds i16, ptr %units, i64 %i
  %byte = load i8, ptr %byte_pointer
  %unit = load i16, ptr %unit_pointer
  %widened = zext i8 %byte to i16
  %match = icmp eq i16 %widened, %unit
  br i1 %match, label %advance, label %different
advance:
  %next = add i64 %i, 1
  br label %loop
different:
  %less = icmp ult i16 %widened, %unit
  %order = select i1 %less, i32 -1, i32 1
  ret i32 %order
prefix:
  %same = icmp eq i64 %byte_length, %unit_length
  %length_order = select i1 %shorter, i32 -1, i32 1
  %result = select i1 %same, i32 0, i32 %length_order
  ret i32 %result
}

; The UTF-8 text of `%length` Latin-1 bytes: a byte below 0x80 is itself and any other is the two
; bytes 0xC2 or 0xC3, then 0x80 or more.
define internal %tz.utf8string @tz.utf8string.from_latin1(ptr %source, i64 %length) nounwind {
entry:
  br label %count
count:
  %counted = phi i64 [ 0, %entry ], [ %counted_next, %count_step ]
  %bytes = phi i64 [ 0, %entry ], [ %bytes_next, %count_step ]
  %count_done = icmp eq i64 %counted, %length
  br i1 %count_done, label %allocate, label %count_step
count_step:
  %count_pointer = getelementptr inbounds i8, ptr %source, i64 %counted
  %count_byte = load i8, ptr %count_pointer
  %count_high = icmp slt i8 %count_byte, 0
  %count_width = select i1 %count_high, i64 2, i64 1
  %bytes_next = add i64 %bytes, %count_width
  %counted_next = add i64 %counted, 1
  br label %count
allocate:
  %result = call %tz.utf8string @tz.utf8string.allocate(i64 %bytes)
  %data = extractvalue %tz.utf8string %result, 0
  %ascii = icmp eq i64 %bytes, %length
  br i1 %ascii, label %copy, label %write
copy:
  call void @tz.string.copy_bytes(ptr %data, ptr %source, i64 %length)
  ret %tz.utf8string %result
write:
  %index = phi i64 [ 0, %allocate ], [ %index_next, %advance ]
  %position = phi i64 [ 0, %allocate ], [ %position_next, %advance ]
  %write_done = icmp eq i64 %index, %length
  br i1 %write_done, label %exit, label %unit
unit:
  %from = getelementptr inbounds i8, ptr %source, i64 %index
  %byte = load i8, ptr %from
  %to = getelementptr inbounds i8, ptr %data, i64 %position
  %high = icmp slt i8 %byte, 0
  br i1 %high, label %pair, label %single
single:
  store i8 %byte, ptr %to
  %single_next = add i64 %position, 1
  br label %advance
pair:
  %lead_bits = lshr i8 %byte, 6
  %lead = or i8 %lead_bits, -64
  %tail_bits = and i8 %byte, 63
  %tail = or i8 %tail_bits, -128
  store i8 %lead, ptr %to
  %tail_to = getelementptr inbounds i8, ptr %to, i64 1
  store i8 %tail, ptr %tail_to
  %pair_next = add i64 %position, 2
  br label %advance
advance:
  %position_next = phi i64 [ %single_next, %single ], [ %pair_next, %pair ]
  %index_next = add i64 %index, 1
  br label %write
exit:
  ret %tz.utf8string %result
}

; Copies `%length` Latin-1 bytes to `%to` as ASCII, or says that the text cannot be converted:
; only 1 to 4096 units, none above 0x7F, the same as @tz.string.to_ascii.
define internal i1 @tz.string.to_ascii_latin1(ptr %to, ptr %from, i64 %length) nounwind {
entry:
  %fits = icmp ule i64 %length, 4096
  br i1 %fits, label %loop, label %invalid
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %valid, label %check
check:
  %source = getelementptr inbounds i8, ptr %from, i64 %i
  %byte = load i8, ptr %source
  %ascii = icmp sge i8 %byte, 0
  br i1 %ascii, label %copy, label %invalid
copy:
  %destination = getelementptr inbounds i8, ptr %to, i64 %i
  store i8 %byte, ptr %destination
  %next = add i64 %i, 1
  br label %loop
valid:
  ret i1 true
invalid:
  ret i1 false
}

; Reads `%byte_length` bytes of UTF-8 as text of which every scalar is at most U+00FF. When the text is valid
; UTF-8 and has no scalar above 0xFF, the result is the number of scalars, which is the number of Latin-1
; bytes and of UTF-16 units, and the largest scalar. Otherwise it is { -1, 0 }: the text is invalid or has
; a scalar above 0xFF, and the caller decodes it as UTF-16, which reports an invalid text as it always did.
; Text of that kind is ASCII bytes and pairs of a lead byte 0xC2 or 0xC3 and a continuation byte 0x80 to 0xBF.
define internal { i64, i32 } @tz.string.utf8_scan(ptr %source, i64 %byte_length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ]
  %units = phi i64 [ 0, %entry ], [ %units_next, %advance ]
  %max = phi i32 [ 0, %entry ], [ %max_next, %advance ]
  %done = icmp eq i64 %i, %byte_length
  br i1 %done, label %success, label %read
read:
  %pointer = getelementptr inbounds i8, ptr %source, i64 %i
  %byte = load i8, ptr %pointer
  %ascii = icmp sge i8 %byte, 0
  br i1 %ascii, label %single, label %lead_test
single:
  %single_value = zext i8 %byte to i32
  %single_next = add i64 %i, 1
  br label %advance
lead_test:
  %lead_bits = and i8 %byte, -2
  %is_lead = icmp eq i8 %lead_bits, -62
  br i1 %is_lead, label %pair, label %fail
pair:
  %follow_index = add i64 %i, 1
  %has_follow = icmp ult i64 %follow_index, %byte_length
  br i1 %has_follow, label %follow, label %fail
follow:
  %follow_pointer = getelementptr inbounds i8, ptr %source, i64 %follow_index
  %follow_byte = load i8, ptr %follow_pointer
  %follow_bits = and i8 %follow_byte, -64
  %is_continuation = icmp eq i8 %follow_bits, -128
  br i1 %is_continuation, label %decoded, label %fail
decoded:
  %high_bits = and i8 %byte, 3
  %high_wide = zext i8 %high_bits to i32
  %high_shifted = shl i32 %high_wide, 6
  %low_bits = and i8 %follow_byte, 63
  %low_wide = zext i8 %low_bits to i32
  %pair_value = or i32 %high_shifted, %low_wide
  %pair_next = add i64 %i, 2
  br label %advance
advance:
  %value = phi i32 [ %single_value, %single ], [ %pair_value, %decoded ]
  %next = phi i64 [ %single_next, %single ], [ %pair_next, %decoded ]
  %units_next = add i64 %units, 1
  %greater = icmp ugt i32 %value, %max
  %max_next = select i1 %greater, i32 %value, i32 %max
  br label %loop
success:
  %result_units = insertvalue { i64, i32 } zeroinitializer, i64 %units, 0
  %result = insertvalue { i64, i32 } %result_units, i32 %max, 1
  ret { i64, i32 } %result
fail:
  ret { i64, i32 } { i64 -1, i32 0 }
}

; Writes the Latin-1 bytes of `%byte_length` bytes of UTF-8 that @tz.string.utf8_scan accepted. The
; buffer holds the number of bytes that the scan counted.
define internal void @tz.string.utf8_to_latin1(ptr %source, i64 %byte_length, ptr %data) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ]
  %o = phi i64 [ 0, %entry ], [ %o_next, %advance ]
  %done = icmp eq i64 %i, %byte_length
  br i1 %done, label %exit, label %read
read:
  %pointer = getelementptr inbounds i8, ptr %source, i64 %i
  %byte = load i8, ptr %pointer
  %ascii = icmp sge i8 %byte, 0
  br i1 %ascii, label %single, label %pair
single:
  %single_next = add i64 %i, 1
  br label %advance
pair:
  %follow_index = add i64 %i, 1
  %follow_pointer = getelementptr inbounds i8, ptr %source, i64 %follow_index
  %follow_byte = load i8, ptr %follow_pointer
  %high_bits = and i8 %byte, 3
  %high_shifted = shl i8 %high_bits, 6
  %low_bits = and i8 %follow_byte, 63
  %pair_value = or i8 %high_shifted, %low_bits
  %pair_next = add i64 %i, 2
  br label %advance
advance:
  %value = phi i8 [ %byte, %single ], [ %pair_value, %pair ]
  %next = phi i64 [ %single_next, %single ], [ %pair_next, %pair ]
  %destination = getelementptr inbounds i8, ptr %data, i64 %o
  store i8 %value, ptr %destination
  %o_next = add i64 %o, 1
  br label %loop
exit:
  ret void
}
