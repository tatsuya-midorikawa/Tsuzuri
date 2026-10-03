; Padding helpers of interpolated strings. Text widths count Unicode scalars:
; a UTF-16 surrogate pair is one scalar and a lone surrogate is one.
define internal i64 @tz.format.scalars.utf16(ptr %data, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %advance ]
  %pairs = phi i64 [ 0, %entry ], [ %counted, %advance ]
  %done = icmp uge i64 %index, %length
  br i1 %done, label %exit, label %inspect
inspect:
  %at = getelementptr inbounds i16, ptr %data, i64 %index
  %unit = load i16, ptr %at
  %high_bits = and i16 %unit, -1024
  %is_high = icmp eq i16 %high_bits, -10240
  %following = add i64 %index, 1
  %has_next = icmp ult i64 %following, %length
  %both = and i1 %is_high, %has_next
  br i1 %both, label %second, label %single
second:
  %next_at = getelementptr inbounds i16, ptr %data, i64 %following
  %next_unit = load i16, ptr %next_at
  %low_bits = and i16 %next_unit, -1024
  %is_low = icmp eq i16 %low_bits, -9216
  br i1 %is_low, label %pair, label %single
pair:
  %skipped = add i64 %index, 2
  %paired = add i64 %pairs, 1
  br label %advance
single:
  br label %advance
advance:
  %next = phi i64 [ %skipped, %pair ], [ %following, %single ]
  %counted = phi i64 [ %paired, %pair ], [ %pairs, %single ]
  br label %loop
exit:
  %scalars = sub i64 %length, %pairs
  ret i64 %scalars
}

define internal i64 @tz.format.scalars.utf8(ptr %data, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %step ]
  %count = phi i64 [ 0, %entry ], [ %counted, %step ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %exit, label %step
step:
  %at = getelementptr inbounds i8, ptr %data, i64 %index
  %byte = load i8, ptr %at
  %tail_bits = and i8 %byte, -64
  %continuation = icmp eq i8 %tail_bits, -128
  %increment = select i1 %continuation, i64 0, i64 1
  %counted = add i64 %count, %increment
  %next = add i64 %index, 1
  br label %loop
exit:
  ret i64 %count
}

; Writes `count` copies of a fill scalar encoded as `width` UTF-16 units
; (`packed` holds the first unit in the low half) and returns the units written.
define internal i64 @tz.format.fill.u16(ptr %out, i64 %count, i32 %packed, i64 %width) nounwind {
entry:
  %first = trunc i32 %packed to i16
  %upper = lshr i32 %packed, 16
  %second = trunc i32 %upper to i16
  %pair = icmp eq i64 %width, 2
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %write ]
  %done = icmp eq i64 %index, %count
  br i1 %done, label %exit, label %write
write:
  %base = mul i64 %index, %width
  %slot = getelementptr inbounds i16, ptr %out, i64 %base
  store i16 %first, ptr %slot
  %after = add i64 %base, 1
  %extra = getelementptr i16, ptr %out, i64 %after
  %target = select i1 %pair, ptr %extra, ptr %slot
  %unit = select i1 %pair, i16 %second, i16 %first
  store i16 %unit, ptr %target
  %next = add i64 %index, 1
  br label %loop
exit:
  %units = mul i64 %count, %width
  ret i64 %units
}

; The same for UTF-8: `packed` holds the encoded bytes of the fill, lowest first.
define internal i64 @tz.format.fill.u8(ptr %out, i64 %count, i32 %packed, i64 %width) nounwind {
entry:
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %copied ]
  %done = icmp eq i64 %index, %count
  br i1 %done, label %exit, label %copy
copy:
  %base = mul i64 %index, %width
  br label %byte
byte:
  %offset = phi i64 [ 0, %copy ], [ %advanced, %store ]
  %finished = icmp eq i64 %offset, %width
  br i1 %finished, label %copied, label %store
store:
  %shift64 = mul i64 %offset, 8
  %shift = trunc i64 %shift64 to i32
  %shifted = lshr i32 %packed, %shift
  %value = trunc i32 %shifted to i8
  %position = add i64 %base, %offset
  %slot = getelementptr inbounds i8, ptr %out, i64 %position
  store i8 %value, ptr %slot
  %advanced = add i64 %offset, 1
  br label %byte
copied:
  %next = add i64 %index, 1
  br label %loop
exit:
  %units = mul i64 %count, %width
  ret i64 %units
}

; Widens ASCII bytes to UTF-16 units.
define internal void @tz.format.widen(ptr %to, ptr %from, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %exit, label %copy
copy:
  %source = getelementptr inbounds i8, ptr %from, i64 %index
  %byte = load i8, ptr %source
  %unit = zext i8 %byte to i16
  %destination = getelementptr inbounds i16, ptr %to, i64 %index
  store i16 %unit, ptr %destination
  %next = add i64 %index, 1
  br label %loop
exit:
  ret void
}
