define internal i1 @tz.string.validate_utf8(ptr %source, i64 %byte_length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %decode ]
  %done = icmp eq i64 %i, %byte_length
  br i1 %done, label %valid, label %decode
decode:
  %item = call { i32, i64 } @tz.string.try_decode_utf8(ptr %source, i64 %byte_length, i64 %i, i1 false)
  %value = extractvalue { i32, i64 } %item, 0
  %next = extractvalue { i32, i64 } %item, 1
  %legal = icmp sge i32 %value, 0
  br i1 %legal, label %loop, label %invalid
valid:
  ret i1 true
invalid:
  ret i1 false
}

define internal %tz.string @tz.string.from_ascii(ptr %source, i64 %length) nounwind {
entry:
  %result = call %tz.string @tz.string.allocate(i64 %length)
  %data = extractvalue %tz.string %result, 0
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %exit, label %copy
copy:
  %input = getelementptr inbounds i8, ptr %source, i64 %index
  %byte = load i8, ptr %input
  %unit = zext i8 %byte to i16
  %output = getelementptr inbounds i16, ptr %data, i64 %index
  store i16 %unit, ptr %output
  %next = add i64 %index, 1
  br label %loop
exit:
  ret %tz.string %result
}

define internal %tz.utf8string @tz.utf8string.from_ascii(ptr %source, i64 %length) nounwind {
entry:
  %result = call %tz.utf8string @tz.utf8string.allocate(i64 %length)
  %data = extractvalue %tz.utf8string %result, 0
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %exit, label %copy
copy:
  %input = getelementptr inbounds i16, ptr %source, i64 %index
  %unit = load i16, ptr %input
  %byte = trunc i16 %unit to i8
  %output = getelementptr inbounds i8, ptr %data, i64 %index
  store i8 %byte, ptr %output
  %next = add i64 %index, 1
  br label %loop
exit:
  ret %tz.utf8string %result
}

define internal i1 @tz.string.equal(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %an = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bn = extractvalue %tz.string %right, 1
  %same = icmp eq i64 %an, %bn
  br i1 %same, label %loop, label %different
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ], [ %wide_next, %wide_advance ]
  %done = icmp eq i64 %i, %an
  br i1 %done, label %equal, label %probe
probe:
  %remaining = sub i64 %an, %i
  %enough = icmp uge i64 %remaining, 4
  br i1 %enough, label %wide, label %compare
wide:
  %wide_ap = getelementptr inbounds i16, ptr %a, i64 %i
  %wide_bp = getelementptr inbounds i16, ptr %b, i64 %i
  %wide_av = load i64, ptr %wide_ap, align 2
  %wide_bv = load i64, ptr %wide_bp, align 2
  %wide_match = icmp eq i64 %wide_av, %wide_bv
  br i1 %wide_match, label %wide_advance, label %different
wide_advance:
  %wide_next = add i64 %i, 4
  br label %loop
compare:
  %ap = getelementptr inbounds i16, ptr %a, i64 %i
  %bp = getelementptr inbounds i16, ptr %b, i64 %i
  %av = load i16, ptr %ap
  %bv = load i16, ptr %bp
  %match = icmp eq i16 %av, %bv
  br i1 %match, label %advance, label %different
advance:
  %next = add i64 %i, 1
  br label %loop
equal:
  ret i1 true
different:
  ret i1 false
}

define internal i1 @tz.utf8string.equal(%tz.utf8string %left, %tz.utf8string %right) nounwind {
entry:
  %a = extractvalue %tz.utf8string %left, 0
  %an = extractvalue %tz.utf8string %left, 1
  %b = extractvalue %tz.utf8string %right, 0
  %bn = extractvalue %tz.utf8string %right, 1
  %same = icmp eq i64 %an, %bn
  br i1 %same, label %loop, label %different
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ], [ %wide_next, %wide_advance ]
  %done = icmp eq i64 %i, %an
  br i1 %done, label %equal, label %probe
probe:
  %remaining = sub i64 %an, %i
  %enough = icmp uge i64 %remaining, 8
  br i1 %enough, label %wide, label %compare
wide:
  %wide_ap = getelementptr inbounds i8, ptr %a, i64 %i
  %wide_bp = getelementptr inbounds i8, ptr %b, i64 %i
  %wide_av = load i64, ptr %wide_ap, align 1
  %wide_bv = load i64, ptr %wide_bp, align 1
  %wide_match = icmp eq i64 %wide_av, %wide_bv
  br i1 %wide_match, label %wide_advance, label %different
wide_advance:
  %wide_next = add i64 %i, 8
  br label %loop
compare:
  %ap = getelementptr inbounds i8, ptr %a, i64 %i
  %bp = getelementptr inbounds i8, ptr %b, i64 %i
  %av = load i8, ptr %ap
  %bv = load i8, ptr %bp
  %match = icmp eq i8 %av, %bv
  br i1 %match, label %advance, label %different
advance:
  %next = add i64 %i, 1
  br label %loop
equal:
  ret i1 true
different:
  ret i1 false
}

define internal i32 @tz.string.compare(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %an = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bn = extractvalue %tz.string %right, 1
  %shorter = icmp ult i64 %an, %bn
  %length = select i1 %shorter, i64 %an, i64 %bn
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %wide_next, %wide_advance ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %prefix, label %probe
probe:
  %remaining = sub i64 %length, %i
  %enough = icmp uge i64 %remaining, 4
  br i1 %enough, label %wide, label %scalar_loop
wide:
  %wide_ap = getelementptr inbounds i16, ptr %a, i64 %i
  %wide_bp = getelementptr inbounds i16, ptr %b, i64 %i
  %wide_av = load i64, ptr %wide_ap, align 2
  %wide_bv = load i64, ptr %wide_bp, align 2
  %wide_match = icmp eq i64 %wide_av, %wide_bv
  br i1 %wide_match, label %wide_advance, label %scalar_loop
wide_advance:
  %wide_next = add i64 %i, 4
  br label %loop
scalar_loop:
  %scalar_i = phi i64 [ %i, %probe ], [ %i, %wide ], [ %next, %advance ]
  %scalar_done = icmp eq i64 %scalar_i, %length
  br i1 %scalar_done, label %prefix, label %compare
compare:
  %ap = getelementptr inbounds i16, ptr %a, i64 %scalar_i
  %bp = getelementptr inbounds i16, ptr %b, i64 %scalar_i
  %av = load i16, ptr %ap
  %bv = load i16, ptr %bp
  %match = icmp eq i16 %av, %bv
  br i1 %match, label %advance, label %different
advance:
  %next = add i64 %scalar_i, 1
  br label %scalar_loop
different:
  %less = icmp ult i16 %av, %bv
  %order = select i1 %less, i32 -1, i32 1
  ret i32 %order
prefix:
  %same = icmp eq i64 %an, %bn
  %lengthorder = select i1 %shorter, i32 -1, i32 1
  %result = select i1 %same, i32 0, i32 %lengthorder
  ret i32 %result
}

define internal i32 @tz.utf8string.compare(%tz.utf8string %left, %tz.utf8string %right) nounwind {
entry:
  %left_data = extractvalue %tz.utf8string %left, 0
  %left_length = extractvalue %tz.utf8string %left, 1
  %right_data = extractvalue %tz.utf8string %right, 0
  %right_length = extractvalue %tz.utf8string %right, 1
  %shorter = icmp ult i64 %left_length, %right_length
  %length = select i1 %shorter, i64 %left_length, i64 %right_length
  br label %test
test:
  %index = phi i64 [ 0, %entry ], [ %wide_next, %wide_equal ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %prefix, label %width
width:
  %remaining = sub i64 %length, %index
  %wide = icmp uge i64 %remaining, 8
  br i1 %wide, label %compare_wide, label %scalar
compare_wide:
  %left_pointer = getelementptr inbounds i8, ptr %left_data, i64 %index
  %right_pointer = getelementptr inbounds i8, ptr %right_data, i64 %index
  %left_word = load i64, ptr %left_pointer, align 1
  %right_word = load i64, ptr %right_pointer, align 1
  %words_equal = icmp eq i64 %left_word, %right_word
  br i1 %words_equal, label %wide_equal, label %scalar
wide_equal:
  %wide_next = add i64 %index, 8
  br label %test
scalar:
  %offset = phi i64 [ %index, %width ], [ %index, %compare_wide ], [ %next, %equal ]
  %end = icmp eq i64 %offset, %length
  br i1 %end, label %prefix, label %compare
compare:
  %left_at = getelementptr inbounds i8, ptr %left_data, i64 %offset
  %right_at = getelementptr inbounds i8, ptr %right_data, i64 %offset
  %left_byte = load i8, ptr %left_at
  %right_byte = load i8, ptr %right_at
  %same = icmp eq i8 %left_byte, %right_byte
  br i1 %same, label %equal, label %different
equal:
  %next = add i64 %offset, 1
  br label %scalar
different:
  %less = icmp ult i8 %left_byte, %right_byte
  %order = select i1 %less, i32 -1, i32 1
  ret i32 %order
prefix:
  %same_length = icmp eq i64 %left_length, %right_length
  %length_order = select i1 %shorter, i32 -1, i32 1
  %result = select i1 %same_length, i32 0, i32 %length_order
  ret i32 %result
}

define internal %tz.string @tz.string.from_utf8(ptr %source, i64 %byte_length) nounwind {
entry:
  ; A valid UTF-8 sequence uses at most three bytes per UTF-16 code unit.
  %fits = icmp ule i64 %byte_length, 27021597764222973
  br i1 %fits, label %count, label %fail
count:
  %i = phi i64 [ 0, %entry ], [ %next, %countscalar ]
  %length = phi i64 [ 0, %entry ], [ %newlength, %countscalar ]
  %done = icmp eq i64 %i, %byte_length
  br i1 %done, label %allocate, label %countscalar
countscalar:
  %decoded = call { i32, i64 } @tz.string.decode_utf8(ptr %source, i64 %byte_length, i64 %i, i1 false)
  %scalar = extractvalue { i32, i64 } %decoded, 0
  %next = extractvalue { i32, i64 } %decoded, 1
  %pair = icmp uge i32 %scalar, 65536
  %width = select i1 %pair, i64 2, i64 1
  %newlength = add i64 %length, %width
  %within = icmp ule i64 %newlength, 9007199254740991
  br i1 %within, label %count, label %fail
allocate:
  %all_ascii = icmp eq i64 %length, %byte_length
  br i1 %all_ascii, label %ascii_only, label %unicode
ascii_only:
  %ascii_result = call %tz.string @tz.string.from_ascii(ptr %source, i64 %length)
  ret %tz.string %ascii_result
unicode:
  %result = call %tz.string @tz.string.allocate(i64 %length)
  %data = extractvalue %tz.string %result, 0
  br label %write
write:
  %input = phi i64 [ 0, %unicode ], [ %inputnext, %single ], [ %inputnext, %double ]
  %output = phi i64 [ 0, %unicode ], [ %singlenext, %single ], [ %doublenext, %double ]
  %finished = icmp eq i64 %input, %byte_length
  br i1 %finished, label %exit, label %decode
decode:
  %item = call { i32, i64 } @tz.string.decode_utf8(ptr %source, i64 %byte_length, i64 %input, i1 true)
  %value = extractvalue { i32, i64 } %item, 0
  %inputnext = extractvalue { i32, i64 } %item, 1
  %p = getelementptr inbounds i16, ptr %data, i64 %output
  %singlenext = add i64 %output, 1
  %supplementary = icmp uge i32 %value, 65536
  br i1 %supplementary, label %double, label %single
single:
  %unit = trunc i32 %value to i16
  store i16 %unit, ptr %p
  br label %write
double:
  %offset = sub i32 %value, 65536
  %highbits = lshr i32 %offset, 10
  %lowbits = and i32 %offset, 1023
  %high = add i32 %highbits, 55296
  %low = add i32 %lowbits, 56320
  %highunit = trunc i32 %high to i16
  %lowunit = trunc i32 %low to i16
  store i16 %highunit, ptr %p
  %lowp = getelementptr inbounds i16, ptr %data, i64 %singlenext
  store i16 %lowunit, ptr %lowp
  %doublenext = add i64 %output, 2
  br label %write
exit:
  ret %tz.string %result
fail:
  call void @llvm.trap()
  unreachable
}

define internal %tz.utf8string @tz.utf8string.from_string(ptr %source, i64 %unit_length) nounwind {
entry:
  %fits = icmp ule i64 %unit_length, 9007199254740991
  br i1 %fits, label %count, label %fail
count:
  %i = phi i64 [ 0, %entry ], [ %next, %countwidth ]
  %length = phi i64 [ 0, %entry ], [ %newlength, %countwidth ]
  %done = icmp eq i64 %i, %unit_length
  br i1 %done, label %allocate, label %countscalar
countscalar:
  %decoded = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %i, i1 false)
  %scalar = extractvalue { i32, i64 } %decoded, 0
  %next = extractvalue { i32, i64 } %decoded, 1
  %surrogateoffset = sub i32 %scalar, 55296
  %surrogate = icmp ult i32 %surrogateoffset, 2048
  br i1 %surrogate, label %fail, label %countwidth
countwidth:
  %one = icmp ult i32 %scalar, 128
  %two = icmp ult i32 %scalar, 2048
  %three = icmp ult i32 %scalar, 65536
  %widewidth = select i1 %three, i64 3, i64 4
  %multiwidth = select i1 %two, i64 2, i64 %widewidth
  %width = select i1 %one, i64 1, i64 %multiwidth
  %newlength = add i64 %length, %width
  br label %count
allocate:
  %all_ascii = icmp eq i64 %length, %unit_length
  br i1 %all_ascii, label %ascii_only, label %unicode
ascii_only:
  %ascii_result = call %tz.utf8string @tz.utf8string.from_ascii(ptr %source, i64 %length)
  ret %tz.utf8string %ascii_result
unicode:
  %result = call %tz.utf8string @tz.utf8string.allocate(i64 %length)
  %data = extractvalue %tz.utf8string %result, 0
  br label %write
write:
  %input = phi i64 [ 0, %unicode ], [ %inputnext, %single ], [ %inputnext, %lead ]
  %output = phi i64 [ 0, %unicode ], [ %singlenext, %single ], [ %multinext, %lead ]
  %finished = icmp eq i64 %input, %unit_length
  br i1 %finished, label %exit, label %decode
decode:
  %item = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %input, i1 true)
  %value = extractvalue { i32, i64 } %item, 0
  %inputnext = extractvalue { i32, i64 } %item, 1
  %p = getelementptr inbounds i8, ptr %data, i64 %output
  %ascii = icmp ult i32 %value, 128
  br i1 %ascii, label %single, label %multi
single:
  %byte = trunc i32 %value to i8
  store i8 %byte, ptr %p
  %singlenext = add i64 %output, 1
  br label %write
multi:
  %istwo = icmp ult i32 %value, 2048
  %isthree = icmp ult i32 %value, 65536
  %widecount = select i1 %isthree, i64 3, i64 4
  %countbytes = select i1 %istwo, i64 2, i64 %widecount
  %widetag = select i1 %isthree, i32 224, i32 240
  %leadtag = select i1 %istwo, i32 192, i32 %widetag
  %multinext = add i64 %output, %countbytes
  br label %continuations
continuations:
  %position = phi i64 [ %multinext, %multi ], [ %previous, %continuation ]
  %bits = phi i32 [ %value, %multi ], [ %remaining, %continuation ]
  %previous = sub i64 %position, 1
  %islead = icmp eq i64 %previous, %output
  br i1 %islead, label %lead, label %continuation
continuation:
  %payload = and i32 %bits, 63
  %tagged = or i32 %payload, 128
  %encoded = trunc i32 %tagged to i8
  %destination = getelementptr inbounds i8, ptr %data, i64 %previous
  store i8 %encoded, ptr %destination
  %remaining = lshr i32 %bits, 6
  br label %continuations
lead:
  %first = or i32 %bits, %leadtag
  %firstbyte = trunc i32 %first to i8
  store i8 %firstbyte, ptr %p
  br label %write
exit:
  ret %tz.utf8string %result
fail:
  call void @llvm.trap()
  unreachable
}

define internal i1 @tz.string.is_well_formed(ptr %source, i64 %unit_length) nounwind {
entry:
  %fits = icmp ule i64 %unit_length, 9007199254740991
  br i1 %fits, label %loop, label %invalid
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %decode ]
  %done = icmp eq i64 %i, %unit_length
  br i1 %done, label %valid, label %decode
decode:
  %item = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %i, i1 false)
  %value = extractvalue { i32, i64 } %item, 0
  %next = extractvalue { i32, i64 } %item, 1
  %surrogateoffset = sub i32 %value, 55296
  %surrogate = icmp ult i32 %surrogateoffset, 2048
  br i1 %surrogate, label %invalid, label %loop
valid:
  ret i1 true
invalid:
  ret i1 false
}

define internal %tz.string @tz.string.to_well_formed(ptr %source, i64 %unit_length) nounwind {
entry:
  %result = call %tz.string @tz.string.new(ptr %source, i64 %unit_length)
  %data = extractvalue %tz.string %result, 0
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %decode ], [ %next, %replace ]
  %done = icmp eq i64 %i, %unit_length
  br i1 %done, label %exit, label %decode
decode:
  %item = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %i, i1 false)
  %value = extractvalue { i32, i64 } %item, 0
  %next = extractvalue { i32, i64 } %item, 1
  %surrogateoffset = sub i32 %value, 55296
  %surrogate = icmp ult i32 %surrogateoffset, 2048
  br i1 %surrogate, label %replace, label %loop
replace:
  %p = getelementptr inbounds i16, ptr %data, i64 %i
  store i16 -3, ptr %p
  br label %loop
exit:
  ret %tz.string %result
}

define internal i64 @tz.string.utf8_to_utf16_length(ptr %source, i64 %byte_length) nounwind {
entry:
  ; A valid UTF-8 sequence uses at most three bytes per UTF-16 code unit.
  %fits = icmp ule i64 %byte_length, 27021597764222973
  br i1 %fits, label %count, label %fail
count:
  %i = phi i64 [ 0, %entry ], [ %next, %countvalid ]
  %length = phi i64 [ 0, %entry ], [ %newlength, %countvalid ]
  %done = icmp eq i64 %i, %byte_length
  br i1 %done, label %allocate, label %countscalar
countscalar:
  %decoded = call { i32, i64 } @tz.string.try_decode_utf8(ptr %source, i64 %byte_length, i64 %i, i1 false)
  %scalar = extractvalue { i32, i64 } %decoded, 0
  %next = extractvalue { i32, i64 } %decoded, 1
  %valid = icmp sge i32 %scalar, 0
  br i1 %valid, label %countvalid, label %fail
countvalid:
  %pair = icmp uge i32 %scalar, 65536
  %width = select i1 %pair, i64 2, i64 1
  %newlength = add i64 %length, %width
  %within = icmp ule i64 %newlength, 9007199254740991
  br i1 %within, label %count, label %fail
allocate:
  ret i64 %length
fail:
  ret i64 -1
}

define internal i64 @tz.utf8string.utf16_to_utf8_length(ptr %source, i64 %unit_length) nounwind {
entry:
  %fits = icmp ule i64 %unit_length, 9007199254740991
  br i1 %fits, label %count, label %fail
count:
  %i = phi i64 [ 0, %entry ], [ %next, %countwidth ]
  %length = phi i64 [ 0, %entry ], [ %newlength, %countwidth ]
  %done = icmp eq i64 %i, %unit_length
  br i1 %done, label %allocate, label %countscalar
countscalar:
  %decoded = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %i, i1 false)
  %scalar = extractvalue { i32, i64 } %decoded, 0
  %next = extractvalue { i32, i64 } %decoded, 1
  %surrogateoffset = sub i32 %scalar, 55296
  %surrogate = icmp ult i32 %surrogateoffset, 2048
  br i1 %surrogate, label %fail, label %countwidth
countwidth:
  %one = icmp ult i32 %scalar, 128
  %two = icmp ult i32 %scalar, 2048
  %three = icmp ult i32 %scalar, 65536
  %widewidth = select i1 %three, i64 3, i64 4
  %multiwidth = select i1 %two, i64 2, i64 %widewidth
  %width = select i1 %one, i64 1, i64 %multiwidth
  %newlength = add i64 %length, %width
  br label %count
allocate:
  ret i64 %length
fail:
  ret i64 -1
}
