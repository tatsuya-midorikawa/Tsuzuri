; Decimal text of an integer, for to_string, Display, and the result of a top-level expression.
; The functions write ASCII without a terminator, from the end of the text backwards: `%end` is the
; address after the last byte, and the text, which has the returned length, starts at `%end` minus
; that length. A caller provides 40 bytes, the length of the longest text,
; "-170141183460469231731687303715884105728", and passes the address after them.
; Integers of 64 bits or less take the 64-bit entry point: the caller extends the value to i64.
; Only the 128-bit entry point divides an i128, so a program that shows no i128 or u128
; value never needs the 128-bit division routine.

; Writes the decimal digits of an unsigned number before `%end`, with leading zeros up to `%least`
; digits (at least 1), and returns how many bytes it wrote. Once the number is used up, the
; division yields zeros, which is the padding.
define internal i32 @tz.integer.digits(ptr %end, i64 %value, i32 %least) noinline nounwind {
entry:
  br label %loop
loop:
  %rest = phi i64 [ %value, %entry ], [ %quotient, %loop ]
  %at = phi ptr [ %end, %entry ], [ %slot, %loop ]
  %count = phi i32 [ 0, %entry ], [ %count.next, %loop ]
  %quotient = udiv i64 %rest, 10
  %times = mul i64 %quotient, 10
  %digit = sub i64 %rest, %times
  %small = trunc i64 %digit to i8
  %char = add i8 %small, 48
  %slot = getelementptr inbounds i8, ptr %at, i32 -1
  store i8 %char, ptr %slot
  %count.next = add i32 %count, 1
  %left = icmp ne i64 %quotient, 0
  %short = icmp ult i32 %count.next, %least
  %more = or i1 %left, %short
  br i1 %more, label %loop, label %exit
exit:
  ret i32 %count.next
}

; The text of an integer of 64 bits or less; `%signed` says whether the number is negative when
; its top bit is set.
define internal i32 @tz.integer.format(ptr %end, i64 %value, i1 %signed) nounwind {
entry:
  %below = icmp slt i64 %value, 0
  %negative = and i1 %signed, %below
  %negated = sub i64 0, %value
  %magnitude = select i1 %negative, i64 %negated, i64 %value
  %count = call i32 @tz.integer.digits(ptr %end, i64 %magnitude, i32 1)
  br i1 %negative, label %minus, label %exit
minus:
  %back = sub i32 0, %count
  %slot = getelementptr inbounds i8, ptr %end, i32 %back
  %sign = getelementptr inbounds i8, ptr %slot, i32 -1
  store i8 45, ptr %sign
  %signed.count = add i32 %count, 1
  ret i32 %signed.count
exit:
  ret i32 %count
}

; The text of an i128 or u128. A magnitude above 2^64 - 1 splits by 10^19 twice, into
; top * 10^38 + middle * 10^19 + low with top at most 3; each part below 10^19 is written with
; 19 digits when something precedes it. Wrapping 64-bit arithmetic recovers the remainders from
; the low halves, because the true values fit in 64 bits.
define internal i32 @tz.integer.format128(ptr %end, i128 %value, i1 %signed) nounwind {
entry:
  %below = icmp slt i128 %value, 0
  %negative = and i1 %signed, %below
  %negated = sub i128 0, %value
  %magnitude = select i1 %negative, i128 %negated, i128 %value
  %wide = icmp ugt i128 %magnitude, 18446744073709551615
  br i1 %wide, label %split, label %narrow
narrow:
  %narrow.value = trunc i128 %magnitude to i64
  %narrow.count = call i32 @tz.integer.digits(ptr %end, i64 %narrow.value, i32 1)
  br label %sign
split:
  %quotient = udiv i128 %magnitude, 10000000000000000000
  %quotient.low = trunc i128 %quotient to i64
  %magnitude.low = trunc i128 %magnitude to i64
  %quotient.times = mul i64 %quotient.low, -8446744073709551616
  %low = sub i64 %magnitude.low, %quotient.times
  %upper = udiv i128 %quotient, 10000000000000000000
  %top = trunc i128 %upper to i64
  %top.times = mul i64 %top, -8446744073709551616
  %middle = sub i64 %quotient.low, %top.times
  %has.top = icmp ne i64 %top, 0
  %middle.least = select i1 %has.top, i32 19, i32 1
  %low.count = call i32 @tz.integer.digits(ptr %end, i64 %low, i32 19)
  %middle.end = getelementptr inbounds i8, ptr %end, i32 -19
  %middle.count = call i32 @tz.integer.digits(ptr %middle.end, i64 %middle, i32 %middle.least)
  %parts = add i32 %low.count, %middle.count
  br i1 %has.top, label %with.top, label %joined
with.top:
  %top.back = sub i32 0, %parts
  %top.at = getelementptr inbounds i8, ptr %end, i32 %top.back
  %top.slot = getelementptr inbounds i8, ptr %top.at, i32 -1
  %top.byte = trunc i64 %top to i8
  %top.char = add i8 %top.byte, 48
  store i8 %top.char, ptr %top.slot
  %with.count = add i32 %parts, 1
  br label %joined
joined:
  %wide.count = phi i32 [ %with.count, %with.top ], [ %parts, %split ]
  br label %sign
sign:
  %count = phi i32 [ %narrow.count, %narrow ], [ %wide.count, %joined ]
  br i1 %negative, label %minus, label %exit
minus:
  %back = sub i32 0, %count
  %slot = getelementptr inbounds i8, ptr %end, i32 %back
  %minus.slot = getelementptr inbounds i8, ptr %slot, i32 -1
  store i8 45, ptr %minus.slot
  %signed.count = add i32 %count, 1
  ret i32 %signed.count
exit:
  ret i32 %count
}
