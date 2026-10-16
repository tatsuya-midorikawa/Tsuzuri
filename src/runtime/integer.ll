; PM08: integers as text and from text, for to_string, Display, string interpolation (with the format
; specs {n:+}, {n:x}, {n:X}, {n:o}, {n:b}), the result of a top-level expression, and Parse.parse of an
; integer type; none of it needs the numeric runtime.
;
; The functions that write text write ASCII without a terminator, from the end of the text
; backwards: `%end` is the address after the last byte, and the text, which has the returned
; length, starts at `%end` minus that length. A caller provides 40 bytes for a decimal text, the
; length of the longest one, "-170141183460469231731687303715884105728", and 160 for a format
; spec, which also has the 128 digits of a binary text. Integers of 64 bits or less take the
; 64-bit entry points: the caller extends the value to i64. Only the 128-bit entry points divide
; or multiply an i128, so a program that shows or parses no i128 or u128 value never needs the
; 128-bit division and multiplication routines.
;
; The parsers read the UTF-16 code units of a string directly. The numeric runtime's parser and
; formatters, which the tests compare these functions with, give the same results.

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

; The decimal digits of an unsigned 128-bit number. A number above 2^64 - 1 splits by 10^19
; twice, into top * 10^38 + middle * 10^19 + low with top at most 3; each part below 10^19 is
; written with 19 digits when something precedes it. Wrapping 64-bit arithmetic recovers the
; remainders from the low halves, because the true values fit in 64 bits.
define internal i32 @tz.integer.magnitude128(ptr %end, i128 %magnitude) nounwind {
entry:
  %wide = icmp ugt i128 %magnitude, 18446744073709551615
  br i1 %wide, label %split, label %narrow
narrow:
  %narrow.value = trunc i128 %magnitude to i64
  %narrow.count = call i32 @tz.integer.digits(ptr %end, i64 %narrow.value, i32 1)
  ret i32 %narrow.count
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
  ret i32 %wide.count
}

; Writes the digits of a 128-bit number, given as two halves, in base 2^`%shift` before `%end`:
; 16, 8, or 2 for a shift of 4, 3, or 1. The letters of base 16 are capitals when `%upper` says so.
; Returns how many bytes it wrote (at least 1).
define internal i32 @tz.integer.radix(ptr %end, i64 %high, i64 %low, i32 %shift, i1 %upper) noinline nounwind {
entry:
  %shift.wide = zext i32 %shift to i64
  %back = sub i64 64, %shift.wide
  %bit = shl i64 1, %shift.wide
  %mask = sub i64 %bit, 1
  %letter = select i1 %upper, i8 55, i8 87
  br label %loop
loop:
  %h = phi i64 [ %high, %entry ], [ %h.next, %loop ]
  %l = phi i64 [ %low, %entry ], [ %l.next, %loop ]
  %at = phi ptr [ %end, %entry ], [ %slot, %loop ]
  %count = phi i32 [ 0, %entry ], [ %count.next, %loop ]
  %digit = and i64 %l, %mask
  %small = trunc i64 %digit to i8
  %is.letter = icmp ugt i8 %small, 9
  %origin = select i1 %is.letter, i8 %letter, i8 48
  %char = add i8 %small, %origin
  %slot = getelementptr inbounds i8, ptr %at, i32 -1
  store i8 %char, ptr %slot
  %l.shifted = lshr i64 %l, %shift.wide
  %h.carried = shl i64 %h, %back
  %l.next = or i64 %l.shifted, %h.carried
  %h.next = lshr i64 %h, %shift.wide
  %count.next = add i32 %count, 1
  %rest = or i64 %l.next, %h.next
  %more = icmp ne i64 %rest, 0
  br i1 %more, label %loop, label %exit
exit:
  ret i32 %count.next
}

; Puts the sign before `%count` bytes of text that end at `%end`: "-" for a negative number, else "+"
; when `%plus` asks for it. Returns the new length.
define internal i32 @tz.integer.sign(ptr %end, i32 %count, i1 %negative, i1 %plus) nounwind {
entry:
  %needed = or i1 %negative, %plus
  br i1 %needed, label %write, label %exit
write:
  %char = select i1 %negative, i8 45, i8 43
  %back = sub i32 0, %count
  %at = getelementptr inbounds i8, ptr %end, i32 %back
  %slot = getelementptr inbounds i8, ptr %at, i32 -1
  store i8 %char, ptr %slot
  %signed.count = add i32 %count, 1
  ret i32 %signed.count
exit:
  ret i32 %count
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
  %length = call i32 @tz.integer.sign(ptr %end, i32 %count, i1 %negative, i1 false)
  ret i32 %length
}

; The text of an i128 or u128.
define internal i32 @tz.integer.format128(ptr %end, i128 %value, i1 %signed) nounwind {
entry:
  %below = icmp slt i128 %value, 0
  %negative = and i1 %signed, %below
  %negated = sub i128 0, %value
  %magnitude = select i1 %negative, i128 %negated, i128 %value
  %count = call i32 @tz.integer.magnitude128(ptr %end, i128 %magnitude)
  %length = call i32 @tz.integer.sign(ptr %end, i32 %count, i1 %negative, i1 false)
  ret i32 %length
}

; The text of an integer of 64 bits or less for a format spec ({n:+}, {n:x}, {n:X}, {n:o}, {n:b}).
; `%flags` is the one of the numeric runtime's format spec: bit 0 asks for a "+" before a number
; that is not negative, and bits 4 to 7 pick the form: 3 hexadecimal, 4 uppercase hexadecimal,
; 5 octal, 6 binary, and anything else (the shortest text, fixed, exponent) decimal, which has no
; fraction for an integer. A negative number in a base is "-" and the digits of its magnitude.
define internal i32 @tz.integer.spec(ptr %end, i64 %value, i1 %signed, i32 %flags) nounwind {
entry:
  %below = icmp slt i64 %value, 0
  %negative = and i1 %signed, %below
  %negated = sub i64 0, %value
  %magnitude = select i1 %negative, i64 %negated, i64 %value
  %form.shifted = lshr i32 %flags, 4
  %form = and i32 %form.shifted, 15
  switch i32 %form, label %decimal [
    i32 3, label %hexadecimal
    i32 4, label %capitals
    i32 5, label %octal
    i32 6, label %binary
  ]
hexadecimal:
  br label %radix
capitals:
  br label %radix
octal:
  br label %radix
binary:
  br label %radix
radix:
  %shift = phi i32 [ 4, %hexadecimal ], [ 4, %capitals ], [ 3, %octal ], [ 1, %binary ]
  %upper = phi i1 [ false, %hexadecimal ], [ true, %capitals ], [ false, %octal ], [ false, %binary ]
  %radix.count = call i32 @tz.integer.radix(ptr %end, i64 0, i64 %magnitude, i32 %shift, i1 %upper)
  br label %sign
decimal:
  %decimal.count = call i32 @tz.integer.digits(ptr %end, i64 %magnitude, i32 1)
  br label %sign
sign:
  %count = phi i32 [ %radix.count, %radix ], [ %decimal.count, %decimal ]
  %plus.bit = and i32 %flags, 1
  %plus = icmp ne i32 %plus.bit, 0
  %length = call i32 @tz.integer.sign(ptr %end, i32 %count, i1 %negative, i1 %plus)
  ret i32 %length
}

; The same for an i128 or u128.
define internal i32 @tz.integer.spec128(ptr %end, i128 %value, i1 %signed, i32 %flags) nounwind {
entry:
  %below = icmp slt i128 %value, 0
  %negative = and i1 %signed, %below
  %negated = sub i128 0, %value
  %magnitude = select i1 %negative, i128 %negated, i128 %value
  %form.shifted = lshr i32 %flags, 4
  %form = and i32 %form.shifted, 15
  switch i32 %form, label %decimal [
    i32 3, label %hexadecimal
    i32 4, label %capitals
    i32 5, label %octal
    i32 6, label %binary
  ]
hexadecimal:
  br label %radix
capitals:
  br label %radix
octal:
  br label %radix
binary:
  br label %radix
radix:
  %shift = phi i32 [ 4, %hexadecimal ], [ 4, %capitals ], [ 3, %octal ], [ 1, %binary ]
  %upper = phi i1 [ false, %hexadecimal ], [ true, %capitals ], [ false, %octal ], [ false, %binary ]
  %high.wide = lshr i128 %magnitude, 64
  %high = trunc i128 %high.wide to i64
  %low = trunc i128 %magnitude to i64
  %radix.count = call i32 @tz.integer.radix(ptr %end, i64 %high, i64 %low, i32 %shift, i1 %upper)
  br label %sign
decimal:
  %decimal.count = call i32 @tz.integer.magnitude128(ptr %end, i128 %magnitude)
  br label %sign
sign:
  %count = phi i32 [ %radix.count, %radix ], [ %decimal.count, %decimal ]
  %plus.bit = and i32 %flags, 1
  %plus = icmp ne i32 %plus.bit, 0
  %length = call i32 @tz.integer.sign(ptr %end, i32 %count, i1 %negative, i1 %plus)
  ret i32 %length
}

; The value of a UTF-16 code unit that is an ASCII digit of base 16 or less (letters of either
; case), or 255 for any other unit. Setting bit 5 folds "A" to "F" onto "a" to "f" and nothing
; with a bit above the eighth set, so a unit beyond ASCII is never a digit.
define internal i64 @tz.integer.digit(i16 %unit) nounwind {
entry:
  %decimal = sub i16 %unit, 48
  %is.decimal = icmp ult i16 %decimal, 10
  %folded = or i16 %unit, 32
  %letter = sub i16 %folded, 97
  %is.letter = icmp ult i16 %letter, 6
  %letter.value = add i16 %letter, 10
  %other = select i1 %is.letter, i16 %letter.value, i16 255
  %value = select i1 %is.decimal, i16 %decimal, i16 %other
  %wide = zext i16 %value to i64
  ret i64 %wide
}

; Reads the sign and the base prefix of the text of an integer, a string of `%length` UTF-16 code
; units of 1 to 4096: an optional "+" or "-" (a "-" needs a signed type), then "0x" (base 16) or
; "0b" (base 2) with at least one more unit, or no prefix (base 10) and at least one unit.
; Returns -1 when the text cannot be an integer, else the index of the first digit in bits 0 to
; 31, the base in bits 32 to 39, and 1 in bit 40 for a "-".
define internal i64 @tz.integer.scan(ptr %input, i64 %length, i1 %signed) noinline nounwind {
entry:
  %empty = icmp eq i64 %length, 0
  %long = icmp ugt i64 %length, 4096
  %unusable = or i1 %empty, %long
  br i1 %unusable, label %fail, label %sign
sign:
  %first = load i16, ptr %input
  %minus = icmp eq i16 %first, 45
  %plus = icmp eq i16 %first, 43
  %marked = or i1 %minus, %plus
  %after.sign = zext i1 %marked to i64
  %only.sign = icmp eq i64 %after.sign, %length
  %unsigned = xor i1 %signed, true
  %impossible = and i1 %minus, %unsigned
  %rejected = or i1 %only.sign, %impossible
  br i1 %rejected, label %fail, label %prefix
prefix:
  %next = add i64 %after.sign, 1
  %room = icmp ult i64 %next, %length
  br i1 %room, label %letters, label %plain
letters:
  %zero.at = getelementptr inbounds i16, ptr %input, i64 %after.sign
  %zero = load i16, ptr %zero.at
  %letter.at = getelementptr inbounds i16, ptr %input, i64 %next
  %letter = load i16, ptr %letter.at
  %is.zero = icmp eq i16 %zero, 48
  %is.x = icmp eq i16 %letter, 120
  %is.b = icmp eq i16 %letter, 98
  %hexadecimal = and i1 %is.zero, %is.x
  %binary = and i1 %is.zero, %is.b
  %prefixed = or i1 %hexadecimal, %binary
  %after.prefix = add i64 %after.sign, 2
  %start = select i1 %prefixed, i64 %after.prefix, i64 %after.sign
  %other = select i1 %binary, i64 2, i64 10
  %base = select i1 %hexadecimal, i64 16, i64 %other
  %bare = icmp eq i64 %start, %length
  br i1 %bare, label %fail, label %pack
plain:
  br label %pack
pack:
  %index = phi i64 [ %start, %letters ], [ %after.sign, %plain ]
  %radix = phi i64 [ %base, %letters ], [ 10, %plain ]
  %radix.shifted = shl i64 %radix, 32
  %minus.wide = zext i1 %minus to i64
  %minus.shifted = shl i64 %minus.wide, 40
  %with.radix = or i64 %index, %radix.shifted
  %packed = or i64 %with.radix, %minus.shifted
  ret i64 %packed
fail:
  ret i64 -1
}

; Parses the text of an integer of 64 bits or less (`%bits` wide, signed or not), a string of
; `%length` UTF-16 code units, into the i64 at `%out` (the value's low `%bits` bits are the number,
; the rest repeat its sign or are zero), and says whether the text was an integer in range: the
; syntax of `scan`, digits of the base, a single "_" between two digits, no "_" first or last, and
; no value beyond the type. A unit beyond ASCII is none of these, so it fails the text.
define internal i1 @tz.integer.parse(ptr %input, i64 %length, i32 %bits, i1 %signed, ptr %out) nounwind {
entry:
  %meta = call i64 @tz.integer.scan(ptr %input, i64 %length, i1 %signed)
  %rejected = icmp eq i64 %meta, -1
  br i1 %rejected, label %fail, label %start
start:
  %first = and i64 %meta, 4294967295
  %base.shifted = lshr i64 %meta, 32
  %base = and i64 %base.shifted, 255
  %negative.shifted = lshr i64 %meta, 40
  %negative = trunc i64 %negative.shifted to i1
  %sign.bit = zext i1 %signed to i32
  %width = sub i32 %bits, %sign.bit
  %spare = sub i32 64, %width
  %spare.wide = zext i32 %spare to i64
  %ceiling = lshr i64 -1, %spare.wide
  %extra = and i1 %signed, %negative
  %extra.wide = zext i1 %extra to i64
  %limit = add i64 %ceiling, %extra.wide
  %decimal = icmp eq i64 %base, 10
  %reach = add i64 %first, 19
  %short = icmp ult i64 %reach, %length
  %lead.end = select i1 %short, i64 %reach, i64 %length
  br i1 %decimal, label %lead, label %prepare
; At most 19 decimal digits cannot wrap a u64, so the leading run of digits, up to 19 units, is added
; without a range check and the sum is compared with the limit once. The checked loop below takes
; over from the first unit that is not a digit (a "_" or an error) or the 20th, and reads the texts
; with a prefix from the start.
lead:
  %lead.index = phi i64 [ %first, %start ], [ %lead.next, %lead.digit ]
  %lead.value = phi i64 [ 0, %start ], [ %lead.grown, %lead.digit ]
  %lead.more = icmp ult i64 %lead.index, %lead.end
  br i1 %lead.more, label %lead.body, label %lead.exit
lead.body:
  %lead.at = getelementptr inbounds i16, ptr %input, i64 %lead.index
  %lead.char = load i16, ptr %lead.at
  %lead.offset = sub i16 %lead.char, 48
  %lead.is = icmp ult i16 %lead.offset, 10
  br i1 %lead.is, label %lead.digit, label %lead.exit
lead.digit:
  %lead.digit.wide = zext i16 %lead.offset to i64
  %lead.scaled = mul i64 %lead.value, 10
  %lead.grown = add i64 %lead.scaled, %lead.digit.wide
  %lead.next = add i64 %lead.index, 1
  br label %lead
lead.exit:
  %lead.over = icmp ugt i64 %lead.value, %limit
  br i1 %lead.over, label %fail, label %lead.check
lead.check:
  %lead.complete = icmp eq i64 %lead.index, %length
  %lead.entered = icmp ne i64 %lead.index, %first
  br i1 %lead.complete, label %finish, label %prepare
prepare:
  %slow.index = phi i64 [ %first, %start ], [ %lead.index, %lead.check ]
  %slow.value = phi i64 [ 0, %start ], [ %lead.value, %lead.check ]
  %slow.previous = phi i1 [ false, %start ], [ %lead.entered, %lead.check ]
  %cutoff = udiv i64 %limit, %base
  %cutoff.times = mul i64 %cutoff, %base
  %last = sub i64 %limit, %cutoff.times
  br label %loop
loop:
  %index = phi i64 [ %slow.index, %prepare ], [ %after, %separator ], [ %next, %accumulate ]
  %value = phi i64 [ %slow.value, %prepare ], [ %value, %separator ], [ %grown, %accumulate ]
  %previous = phi i1 [ %slow.previous, %prepare ], [ false, %separator ], [ true, %accumulate ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %finish, label %body
body:
  %at = getelementptr inbounds i16, ptr %input, i64 %index
  %char = load i16, ptr %at
  %underscore = icmp eq i16 %char, 95
  br i1 %underscore, label %separator, label %digit
separator:
  %after = add i64 %index, 1
  %trailing = icmp eq i64 %after, %length
  %lonely = xor i1 %previous, true
  %misplaced = or i1 %trailing, %lonely
  br i1 %misplaced, label %fail, label %loop
digit:
  %value.of = call i64 @tz.integer.digit(i16 %char)
  %valid = icmp ult i64 %value.of, %base
  %beyond = icmp ugt i64 %value, %cutoff
  %at.cutoff = icmp eq i64 %value, %cutoff
  %past.last = icmp ugt i64 %value.of, %last
  %edge = and i1 %at.cutoff, %past.last
  %overflow = or i1 %beyond, %edge
  %in.range = xor i1 %overflow, true
  %accepted = and i1 %valid, %in.range
  br i1 %accepted, label %accumulate, label %fail
accumulate:
  %scaled = mul i64 %value, %base
  %grown = add i64 %scaled, %value.of
  %next = add i64 %index, 1
  br label %loop
finish:
  %final = phi i64 [ %value, %loop ], [ %lead.value, %lead.check ]
  %negated = sub i64 0, %final
  %result = select i1 %negative, i64 %negated, i64 %final
  store i64 %result, ptr %out
  ret i1 true
fail:
  ret i1 false
}

; The same for an i128 or u128.
define internal i1 @tz.integer.parse128(ptr %input, i64 %length, i1 %signed, ptr %out) nounwind {
entry:
  %meta = call i64 @tz.integer.scan(ptr %input, i64 %length, i1 %signed)
  %rejected = icmp eq i64 %meta, -1
  br i1 %rejected, label %fail, label %start
start:
  %first = and i64 %meta, 4294967295
  %base.shifted = lshr i64 %meta, 32
  %base.small = and i64 %base.shifted, 255
  %base = zext i64 %base.small to i128
  %negative.shifted = lshr i64 %meta, 40
  %negative = trunc i64 %negative.shifted to i1
  %ceiling = select i1 %signed, i128 170141183460469231731687303715884105727, i128 340282366920938463463374607431768211455
  %extra = and i1 %signed, %negative
  %extra.wide = zext i1 %extra to i128
  %limit = add i128 %ceiling, %extra.wide
  %cutoff = udiv i128 %limit, %base
  %cutoff.times = mul i128 %cutoff, %base
  %last = sub i128 %limit, %cutoff.times
  br label %loop
loop:
  %index = phi i64 [ %first, %start ], [ %after, %separator ], [ %next, %accumulate ]
  %value = phi i128 [ 0, %start ], [ %value, %separator ], [ %grown, %accumulate ]
  %previous = phi i1 [ false, %start ], [ false, %separator ], [ true, %accumulate ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %finish, label %body
body:
  %at = getelementptr inbounds i16, ptr %input, i64 %index
  %char = load i16, ptr %at
  %underscore = icmp eq i16 %char, 95
  br i1 %underscore, label %separator, label %digit
separator:
  %after = add i64 %index, 1
  %trailing = icmp eq i64 %after, %length
  %lonely = xor i1 %previous, true
  %misplaced = or i1 %trailing, %lonely
  br i1 %misplaced, label %fail, label %loop
digit:
  %value.small = call i64 @tz.integer.digit(i16 %char)
  %value.of = zext i64 %value.small to i128
  %valid = icmp ult i128 %value.of, %base
  %beyond = icmp ugt i128 %value, %cutoff
  %at.cutoff = icmp eq i128 %value, %cutoff
  %past.last = icmp ugt i128 %value.of, %last
  %edge = and i1 %at.cutoff, %past.last
  %overflow = or i1 %beyond, %edge
  %in.range = xor i1 %overflow, true
  %accepted = and i1 %valid, %in.range
  br i1 %accepted, label %accumulate, label %fail
accumulate:
  %scaled = mul i128 %value, %base
  %grown = add i128 %scaled, %value.of
  %next = add i64 %index, 1
  br label %loop
finish:
  %negated = sub i128 0, %value
  %result = select i1 %negative, i128 %negated, i128 %value
  store i128 %result, ptr %out
  ret i1 true
fail:
  ret i1 false
}
