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

define internal void @tz.string.allocate.check(i64 %length) nounwind {
entry:
  %fits = icmp ule i64 %length, 9007199254740991
  br i1 %fits, label %done, label %fail
fail:
  call void @llvm.trap()
  unreachable
done:
  ret void
}

define internal %tz.string @tz.string.append(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %an = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bn = extractvalue %tz.string %right, 1
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %grow
fail:
  call void @llvm.trap()
  unreachable
grow:
  call void @tz.string.allocate.check(i64 %length)
  %old = mul i64 %an, 2
  %new = mul i64 %length, 2
  %data = call ptr @tz.realloc(ptr %a, i64 %old, i64 %new)
  %second = getelementptr i16, ptr %data, i64 %an
  call void @tz.string.copy(ptr %second, ptr %b, i64 %bn)
  %result0 = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %result = insertvalue %tz.string %result0, i64 %length, 1
  ret %tz.string %result
}

define internal %tz.string @tz.string.concat_into(ptr %buffer, i64 %capacity, %tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %an = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bn = extractvalue %tz.string %right, 1
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %size
fail:
  call void @llvm.trap()
  unreachable
size:
  call void @tz.string.allocate.check(i64 %length)
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %zero, label %choose
zero:
  ret %tz.string zeroinitializer
choose:
  %fits = icmp ule i64 %length, %capacity
  br i1 %fits, label %stack, label %heap
stack:
  br label %copy
heap:
  %allocated = call %tz.string @tz.string.allocate(i64 %length)
  %heapdata = extractvalue %tz.string %allocated, 0
  br label %copy
copy:
  %data = phi ptr [ %buffer, %stack ], [ %heapdata, %heap ]
  call void @tz.string.copy(ptr %data, ptr %a, i64 %an)
  %second = getelementptr i16, ptr %data, i64 %an
  call void @tz.string.copy(ptr %second, ptr %b, i64 %bn)
  %result0 = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %result = insertvalue %tz.string %result0, i64 %length, 1
  ret %tz.string %result
}



; Return the scalar and next byte index, rejecting non-shortest UTF-8 and surrogates.
define internal { i32, i64 } @tz.string.decode_utf8(ptr %source, i64 %length, i64 %index, i1 %validated) nounwind {
entry:
  %decoded = call { i32, i64 } @tz.string.try_decode_utf8(ptr %source, i64 %length, i64 %index, i1 %validated)
  %value = extractvalue { i32, i64 } %decoded, 0
  %valid = icmp sge i32 %value, 0
  br i1 %valid, label %done, label %fail
fail:
  call void @llvm.trap()
  unreachable
done:
  ret { i32, i64 } %decoded
}

define internal { i32, i64 } @tz.string.try_decode_utf8(ptr %source, i64 %length, i64 %index, i1 %validated) nounwind {
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
  %lead_allowed = or i1 %validated, %validlead
  br i1 %lead_allowed, label %checklength, label %fail
checklength:
  %shortwidth = select i1 %two, i64 2, i64 3
  %width = select i1 %four, i64 4, i64 %shortwidth
  %remaining = sub i64 %length, %index
  %enough = icmp uge i64 %remaining, %width
  %length_allowed = or i1 %validated, %enough
  br i1 %length_allowed, label %start, label %fail
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
  %continuation_allowed = or i1 %validated, %valid
  br i1 %continuation_allowed, label %append, label %fail
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
  %scalar_allowed = or i1 %validated, %validscalar
  br i1 %scalar_allowed, label %result, label %fail
result:
  %a = insertvalue { i32, i64 } zeroinitializer, i32 %value, 0
  %b = insertvalue { i32, i64 } %a, i64 %end, 1
  ret { i32, i64 } %b
fail:
  ret { i32, i64 } { i32 -1, i64 -1 }
}


; Preserve lone surrogates as their code-unit value so callers choose rejection or repair.
define internal { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %length, i64 %index, i1 %validated) nounwind {
entry:
  %p = getelementptr inbounds i16, ptr %source, i64 %index
  %unit = load i16, ptr %p
  %value = zext i16 %unit to i32
  %next = add i64 %index, 1
  %highoffset = sub i32 %value, 55296
  %high = icmp ult i32 %highoffset, 1024
  %hasnext = icmp ult i64 %next, %length
  %next_allowed = or i1 %validated, %hasnext
  %possiblepair = and i1 %high, %next_allowed
  br i1 %possiblepair, label %checklow, label %single
checklow:
  %lowp = getelementptr inbounds i16, ptr %source, i64 %next
  %lowunit = load i16, ptr %lowp
  %lowvalue = zext i16 %lowunit to i32
  %lowoffset = sub i32 %lowvalue, 56320
  %low = icmp ult i32 %lowoffset, 1024
  %low_allowed = or i1 %validated, %low
  br i1 %low_allowed, label %pair, label %single
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
