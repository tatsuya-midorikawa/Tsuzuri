define internal void @tz.string.copy(ptr %to, ptr %from, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %exit, label %copy
copy:
  %source = getelementptr inbounds i16, ptr %from, i64 %i
  %destination = getelementptr inbounds i16, ptr %to, i64 %i
  %unit = load i16, ptr %source
  store i16 %unit, ptr %destination
  %next = add i64 %i, 1
  br label %loop
exit:
  ret void
}

define internal %tz.string @tz.string.allocate(i64 %length) nounwind {
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
  %bytes = mul i64 %length, 2
  %data = call ptr @tz.alloc(i64 %bytes)
  %a = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %b = insertvalue %tz.string %a, i64 %length, 1
  ret %tz.string %b
}

define internal %tz.string @tz.string.new(ptr %source, i64 %length) nounwind {
entry:
  %result = call %tz.string @tz.string.allocate(i64 %length)
  %data = extractvalue %tz.string %result, 0
  call void @tz.string.copy(ptr %data, ptr %source, i64 %length)
  ret %tz.string %result
}

define internal %tz.string @tz.string.concat(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %an = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bn = extractvalue %tz.string %right, 1
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %allocate
fail:
  call void @llvm.trap()
  unreachable
allocate:
  %result = call %tz.string @tz.string.allocate(i64 %length)
  %data = extractvalue %tz.string %result, 0
  call void @tz.string.copy(ptr %data, ptr %a, i64 %an)
  %second = getelementptr i16, ptr %data, i64 %an
  call void @tz.string.copy(ptr %second, ptr %b, i64 %bn)
  ret %tz.string %result
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
  %i = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %i, %an
  br i1 %done, label %equal, label %compare
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
  %i = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %prefix, label %compare
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

; Return the scalar and next byte index, rejecting non-shortest UTF-8 and surrogates.
define internal { i32, i64 } @tz.string.decode_utf8(ptr %source, i64 %length, i64 %index) nounwind {
entry:
  %firstp = getelementptr inbounds i8, ptr %source, i64 %index
  %firstbyte = load i8, ptr %firstp
  %first = zext i8 %firstbyte to i32
  %next = add i64 %index, 1
  %ascii = icmp ult i32 %first, 128
  br i1 %ascii, label %single, label %classify
single:
  %singlea = insertvalue { i32, i64 } zeroinitializer, i32 %first, 0
  %singleb = insertvalue { i32, i64 } %singlea, i64 %next, 1
  ret { i32, i64 } %singleb
classify:
  %twooffset = sub i32 %first, 194
  %two = icmp ult i32 %twooffset, 30
  %threeoffset = sub i32 %first, 224
  %three = icmp ult i32 %threeoffset, 16
  %fouroffset = sub i32 %first, 240
  %four = icmp ult i32 %fouroffset, 5
  %twothree = or i1 %two, %three
  %validlead = or i1 %twothree, %four
  br i1 %validlead, label %checklength, label %fail
checklength:
  %shortwidth = select i1 %two, i64 2, i64 3
  %width = select i1 %four, i64 4, i64 %shortwidth
  %remaining = sub i64 %length, %index
  %enough = icmp uge i64 %remaining, %width
  br i1 %enough, label %start, label %fail
start:
  %shortmask = select i1 %two, i32 31, i32 15
  %mask = select i1 %four, i32 7, i32 %shortmask
  %initial = and i32 %first, %mask
  %shortminimum = select i1 %two, i32 128, i32 2048
  %minimum = select i1 %four, i32 65536, i32 %shortminimum
  %end = add i64 %index, %width
  br label %loop
loop:
  %i = phi i64 [ %next, %start ], [ %following, %append ]
  %value = phi i32 [ %initial, %start ], [ %combined, %append ]
  %done = icmp eq i64 %i, %end
  br i1 %done, label %validate, label %continuation
continuation:
  %p = getelementptr inbounds i8, ptr %source, i64 %i
  %byte = load i8, ptr %p
  %tag = and i8 %byte, -64
  %valid = icmp eq i8 %tag, -128
  br i1 %valid, label %append, label %fail
append:
  %wide = zext i8 %byte to i32
  %payload = and i32 %wide, 63
  %shifted = shl i32 %value, 6
  %combined = or i32 %shifted, %payload
  %following = add i64 %i, 1
  br label %loop
validate:
  %shortest = icmp uge i32 %value, %minimum
  %inrange = icmp ule i32 %value, 1114111
  %surrogateoffset = sub i32 %value, 55296
  %nonsurrogate = icmp uge i32 %surrogateoffset, 2048
  %scalar = and i1 %inrange, %nonsurrogate
  %validscalar = and i1 %shortest, %scalar
  br i1 %validscalar, label %result, label %fail
result:
  %a = insertvalue { i32, i64 } zeroinitializer, i32 %value, 0
  %b = insertvalue { i32, i64 } %a, i64 %end, 1
  ret { i32, i64 } %b
fail:
  call void @llvm.trap()
  unreachable
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
  %decoded = call { i32, i64 } @tz.string.decode_utf8(ptr %source, i64 %byte_length, i64 %i)
  %scalar = extractvalue { i32, i64 } %decoded, 0
  %next = extractvalue { i32, i64 } %decoded, 1
  %pair = icmp uge i32 %scalar, 65536
  %width = select i1 %pair, i64 2, i64 1
  %newlength = add i64 %length, %width
  %within = icmp ule i64 %newlength, 9007199254740991
  br i1 %within, label %count, label %fail
allocate:
  %result = call %tz.string @tz.string.allocate(i64 %length)
  %data = extractvalue %tz.string %result, 0
  br label %write
write:
  %input = phi i64 [ 0, %allocate ], [ %inputnext, %single ], [ %inputnext, %double ]
  %output = phi i64 [ 0, %allocate ], [ %singlenext, %single ], [ %doublenext, %double ]
  %finished = icmp eq i64 %input, %byte_length
  br i1 %finished, label %exit, label %decode
decode:
  %item = call { i32, i64 } @tz.string.decode_utf8(ptr %source, i64 %byte_length, i64 %input)
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

; Preserve lone surrogates as their code-unit value so callers choose rejection or repair.
define internal { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %length, i64 %index) nounwind {
entry:
  %p = getelementptr inbounds i16, ptr %source, i64 %index
  %unit = load i16, ptr %p
  %value = zext i16 %unit to i32
  %next = add i64 %index, 1
  %highoffset = sub i32 %value, 55296
  %high = icmp ult i32 %highoffset, 1024
  %hasnext = icmp ult i64 %next, %length
  %possiblepair = and i1 %high, %hasnext
  br i1 %possiblepair, label %checklow, label %single
checklow:
  %lowp = getelementptr inbounds i16, ptr %source, i64 %next
  %lowunit = load i16, ptr %lowp
  %lowvalue = zext i16 %lowunit to i32
  %lowoffset = sub i32 %lowvalue, 56320
  %low = icmp ult i32 %lowoffset, 1024
  br i1 %low, label %pair, label %single
pair:
  %shifted = shl i32 %highoffset, 10
  %offset = or i32 %shifted, %lowoffset
  %scalar = add i32 %offset, 65536
  %afterpair = add i64 %next, 1
  %paira = insertvalue { i32, i64 } zeroinitializer, i32 %scalar, 0
  %pairb = insertvalue { i32, i64 } %paira, i64 %afterpair, 1
  ret { i32, i64 } %pairb
single:
  %a = insertvalue { i32, i64 } zeroinitializer, i32 %value, 0
  %b = insertvalue { i32, i64 } %a, i64 %next, 1
  ret { i32, i64 } %b
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
  %decoded = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %i)
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
  %result = call %tz.utf8string @tz.utf8string.allocate(i64 %length)
  %data = extractvalue %tz.utf8string %result, 0
  br label %write
write:
  %input = phi i64 [ 0, %allocate ], [ %inputnext, %single ], [ %inputnext, %lead ]
  %output = phi i64 [ 0, %allocate ], [ %singlenext, %single ], [ %multinext, %lead ]
  %finished = icmp eq i64 %input, %unit_length
  br i1 %finished, label %exit, label %decode
decode:
  %item = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %input)
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
  %item = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %i)
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
  %item = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %unit_length, i64 %i)
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

define internal i1 @tz.string.to_ascii(ptr %destination, ptr %source, i64 %unit_length) nounwind {
entry:
  %fits = icmp ule i64 %unit_length, 4096
  br i1 %fits, label %loop, label %invalid
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %i, %unit_length
  br i1 %done, label %valid, label %check
check:
  %p = getelementptr inbounds i16, ptr %source, i64 %i
  %unit = load i16, ptr %p
  %ascii = icmp ule i16 %unit, 127
  br i1 %ascii, label %copy, label %invalid
copy:
  %byte = trunc i16 %unit to i8
  %output = getelementptr inbounds i8, ptr %destination, i64 %i
  store i8 %byte, ptr %output
  %next = add i64 %i, 1
  br label %loop
valid:
  ret i1 true
invalid:
  ret i1 false
}
