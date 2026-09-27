define internal %tz.string @tz.character.display(i32 %value) nounwind {
entry:
  %buffer = alloca [2 x i16], align 4
  %bmp = icmp ule i32 %value, 65535
  br i1 %bmp, label %single, label %pair
single:
  %unit = trunc i32 %value to i16
  store i16 %unit, ptr %buffer
  br label %done
pair:
  %offset = sub i32 %value, 65536
  %upper = lshr i32 %offset, 10
  %high = or i32 %upper, 55296
  %lower = and i32 %offset, 1023
  %low = or i32 %lower, 56320
  %high_unit = trunc i32 %high to i16
  %low_unit = trunc i32 %low to i16
  store i16 %high_unit, ptr %buffer
  %second = getelementptr i16, ptr %buffer, i32 1
  store i16 %low_unit, ptr %second
  br label %done
done:
  %length = phi i64 [ 1, %single ], [ 2, %pair ]
  %result = call %tz.string @tz.string.new(ptr %buffer, i64 %length)
  ret %tz.string %result
}

define internal i32 @tz.character.parse(ptr %source, i1 %scalar) nounwind {
entry:
  %text = load %tz.string, ptr %source
  %data = extractvalue %tz.string %text, 0
  %length = extractvalue %tz.string %text, 1
  %one = icmp eq i64 %length, 1
  br i1 %one, label %single, label %check_pair
single:
  %unit = load i16, ptr %data
  %value = zext i16 %unit to i32
  %below = icmp ult i32 %value, 55296
  %above = icmp ugt i32 %value, 57343
  %outside = or i1 %below, %above
  %any_unit = xor i1 %scalar, true
  %valid = or i1 %outside, %any_unit
  %result = select i1 %valid, i32 %value, i32 -1
  ret i32 %result
check_pair:
  %two = icmp eq i64 %length, 2
  %pair = and i1 %two, %scalar
  br i1 %pair, label %decode, label %invalid
decode:
  %first = load i16, ptr %data
  %second_pointer = getelementptr i16, ptr %data, i32 1
  %second = load i16, ptr %second_pointer
  %high = zext i16 %first to i32
  %low = zext i16 %second to i32
  %upper = sub i32 %high, 55296
  %lower = sub i32 %low, 56320
  %valid_high = icmp ult i32 %upper, 1024
  %valid_low = icmp ult i32 %lower, 1024
  %valid_pair = and i1 %valid_high, %valid_low
  %shifted = shl i32 %upper, 10
  %combined = add i32 %shifted, %lower
  %code = add i32 %combined, 65536
  %decoded = select i1 %valid_pair, i32 %code, i32 -1
  ret i32 %decoded
invalid:
  ret i32 -1
}

define internal i32 @tz.character.utf8(ptr %buffer, i32 %value) nounwind {
entry:
  %below = icmp ult i32 %value, 55296
  %above = icmp ugt i32 %value, 57343
  %outside = or i1 %below, %above
  %within = icmp ule i32 %value, 1114111
  %valid = and i1 %outside, %within
  br i1 %valid, label %first, label %invalid
invalid:
  call void @llvm.trap()
  unreachable
first:
  %ascii = icmp ult i32 %value, 128
  br i1 %ascii, label %one, label %second
one:
  %ascii_byte = trunc i32 %value to i8
  store i8 %ascii_byte, ptr %buffer
  ret i32 1
second:
  %small = icmp ult i32 %value, 2048
  br i1 %small, label %two, label %third
two:
  %head2 = lshr i32 %value, 6
  %prefix2 = or i32 %head2, 192
  %tail2 = and i32 %value, 63
  %suffix2 = or i32 %tail2, 128
  %byte20 = trunc i32 %prefix2 to i8
  %byte21 = trunc i32 %suffix2 to i8
  store i8 %byte20, ptr %buffer
  %pointer21 = getelementptr i8, ptr %buffer, i32 1
  store i8 %byte21, ptr %pointer21
  ret i32 2
third:
  %bmp = icmp ult i32 %value, 65536
  br i1 %bmp, label %three, label %four
three:
  %head3 = lshr i32 %value, 12
  %prefix3 = or i32 %head3, 224
  %middle3 = lshr i32 %value, 6
  %middle_mask3 = and i32 %middle3, 63
  %middle_byte3 = or i32 %middle_mask3, 128
  %tail3 = and i32 %value, 63
  %suffix3 = or i32 %tail3, 128
  %byte30 = trunc i32 %prefix3 to i8
  %byte31 = trunc i32 %middle_byte3 to i8
  %byte32 = trunc i32 %suffix3 to i8
  store i8 %byte30, ptr %buffer
  %pointer31 = getelementptr i8, ptr %buffer, i32 1
  store i8 %byte31, ptr %pointer31
  %pointer32 = getelementptr i8, ptr %buffer, i32 2
  store i8 %byte32, ptr %pointer32
  ret i32 3
four:
  %head4 = lshr i32 %value, 18
  %prefix4 = or i32 %head4, 240
  %middle41 = lshr i32 %value, 12
  %middle_mask41 = and i32 %middle41, 63
  %middle_byte41 = or i32 %middle_mask41, 128
  %middle42 = lshr i32 %value, 6
  %middle_mask42 = and i32 %middle42, 63
  %middle_byte42 = or i32 %middle_mask42, 128
  %tail4 = and i32 %value, 63
  %suffix4 = or i32 %tail4, 128
  %byte40 = trunc i32 %prefix4 to i8
  %byte41 = trunc i32 %middle_byte41 to i8
  %byte42 = trunc i32 %middle_byte42 to i8
  %byte43 = trunc i32 %suffix4 to i8
  store i8 %byte40, ptr %buffer
  %pointer41 = getelementptr i8, ptr %buffer, i32 1
  store i8 %byte41, ptr %pointer41
  %pointer42 = getelementptr i8, ptr %buffer, i32 2
  store i8 %byte42, ptr %pointer42
  %pointer43 = getelementptr i8, ptr %buffer, i32 3
  store i8 %byte43, ptr %pointer43
  ret i32 4
}