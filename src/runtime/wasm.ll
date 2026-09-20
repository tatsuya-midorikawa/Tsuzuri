; LLVM's loop optimizer can introduce i128 arithmetic for i64 source programs.
; Weak, hidden helpers are deduplicated across objects and GC'd when unused.

; LLVM 23 recognizes the limb formula as i128 multiplication, creating a recursive
; libcall back to __multi3. Keep this already-lowered helper out of InstCombine.
define weak hidden i128 @__multi3(i128 %a, i128 %b) nounwind noinline optnone {
entry:
  %al = trunc i128 %a to i64
  %as = lshr i128 %a, 64
  %ah = trunc i128 %as to i64
  %bl = trunc i128 %b to i64
  %bs = lshr i128 %b, 64
  %bh = trunc i128 %bs to i64
  %a0 = and i64 %al, 4294967295
  %a1 = lshr i64 %al, 32
  %b0 = and i64 %bl, 4294967295
  %b1 = lshr i64 %bl, 32
  %w0 = mul i64 %a0, %b0
  %carry0 = lshr i64 %w0, 32
  %p10 = mul i64 %a1, %b0
  %t = add i64 %p10, %carry0
  %w1 = and i64 %t, 4294967295
  %w2 = lshr i64 %t, 32
  %p01 = mul i64 %a0, %b1
  %middle = add i64 %w1, %p01
  %carry1 = lshr i64 %middle, 32
  %p11 = mul i64 %a1, %b1
  %h0 = add i64 %p11, %w2
  %h1 = add i64 %h0, %carry1
  %cross0 = mul i64 %al, %bh
  %cross1 = mul i64 %ah, %bl
  %h2 = add i64 %h1, %cross0
  %high = add i64 %h2, %cross1
  %low = mul i64 %al, %bl
  %low128 = zext i64 %low to i128
  %high128 = zext i64 %high to i128
  %shifted = shl i128 %high128, 64
  %result = or i128 %low128, %shifted
  ret i128 %result
}

; Restoring division uses only fixed shifts, comparison, and subtraction.
; No wide division or multiplication is allowed here: it would recurse via libcalls.
define internal { i128, i128 } @tz.runtime.udivmod(i128 %numerator, i128 %denominator) nounwind {
entry:
  %zero = icmp eq i128 %denominator, 0
  br i1 %zero, label %fail, label %loop
fail:
  call void @llvm.trap()
  unreachable
loop:
  %n = phi i128 [ %numerator, %entry ], [ %nextn, %loop ]
  %q = phi i128 [ 0, %entry ], [ %nextq, %loop ]
  %r = phi i128 [ 0, %entry ], [ %nextr, %loop ]
  %count = phi i32 [ 128, %entry ], [ %remaining, %loop ]
  %bit = lshr i128 %n, 127
  %nextn = shl i128 %n, 1
  %shiftedr = shl i128 %r, 1
  %candidate = or i128 %shiftedr, %bit
  %fits = icmp uge i128 %candidate, %denominator
  %subtracted = sub i128 %candidate, %denominator
  %nextr = select i1 %fits, i128 %subtracted, i128 %candidate
  %shiftedq = shl i128 %q, 1
  %quotientbit = zext i1 %fits to i128
  %nextq = or i128 %shiftedq, %quotientbit
  %remaining = sub i32 %count, 1
  %done = icmp eq i32 %remaining, 0
  br i1 %done, label %exit, label %loop
exit:
  %pair0 = insertvalue { i128, i128 } zeroinitializer, i128 %nextq, 0
  %pair1 = insertvalue { i128, i128 } %pair0, i128 %nextr, 1
  ret { i128, i128 } %pair1
}

define weak hidden i128 @__udivti3(i128 %a, i128 %b) nounwind {
entry:
  %pair = call { i128, i128 } @tz.runtime.udivmod(i128 %a, i128 %b)
  %result = extractvalue { i128, i128 } %pair, 0
  ret i128 %result
}

define weak hidden i128 @__umodti3(i128 %a, i128 %b) nounwind {
entry:
  %pair = call { i128, i128 } @tz.runtime.udivmod(i128 %a, i128 %b)
  %result = extractvalue { i128, i128 } %pair, 1
  ret i128 %result
}

define weak hidden i128 @__udivmodti4(i128 %a, i128 %b, ptr %remainder) nounwind {
entry:
  %pair = call { i128, i128 } @tz.runtime.udivmod(i128 %a, i128 %b)
  %present = icmp ne ptr %remainder, null
  br i1 %present, label %store, label %exit
store:
  %r = extractvalue { i128, i128 } %pair, 1
  store i128 %r, ptr %remainder, align 1
  br label %exit
exit:
  %q = extractvalue { i128, i128 } %pair, 0
  ret i128 %q
}

define weak hidden i128 @__divti3(i128 %a, i128 %b) nounwind {
entry:
  %negativea = icmp slt i128 %a, 0
  %negativeb = icmp slt i128 %b, 0
  %negateda = sub i128 0, %a
  %negatedb = sub i128 0, %b
  %absa = select i1 %negativea, i128 %negateda, i128 %a
  %absb = select i1 %negativeb, i128 %negatedb, i128 %b
  %pair = call { i128, i128 } @tz.runtime.udivmod(i128 %absa, i128 %absb)
  %q = extractvalue { i128, i128 } %pair, 0
  %negative = xor i1 %negativea, %negativeb
  %negatedq = sub i128 0, %q
  %result = select i1 %negative, i128 %negatedq, i128 %q
  ret i128 %result
}

define weak hidden i128 @__modti3(i128 %a, i128 %b) nounwind {
entry:
  %negativea = icmp slt i128 %a, 0
  %negativeb = icmp slt i128 %b, 0
  %negateda = sub i128 0, %a
  %negatedb = sub i128 0, %b
  %absa = select i1 %negativea, i128 %negateda, i128 %a
  %absb = select i1 %negativeb, i128 %negatedb, i128 %b
  %pair = call { i128, i128 } @tz.runtime.udivmod(i128 %absa, i128 %absb)
  %r = extractvalue { i128, i128 } %pair, 1
  %negatedr = sub i128 0, %r
  %result = select i1 %negativea, i128 %negatedr, i128 %r
  ret i128 %result
}

; Keep variable wide shifts in limbs: recognizing an i128 shift would recurse.
define internal i128 @tz.runtime.shift(i128 %value, i32 %count, i32 %mode) nounwind noinline optnone {
entry:
  %masked = and i32 %count, 127
  %zero = icmp eq i32 %masked, 0
  br i1 %zero, label %unchanged, label %start
unchanged:
  ret i128 %value
start:
  %low = trunc i128 %value to i64
  %shifted = lshr i128 %value, 64
  %high = trunc i128 %shifted to i64
  %amount32 = and i32 %masked, 63
  %amount = zext i32 %amount32 to i64
  %large = icmp uge i32 %masked, 64
  %left = icmp eq i32 %mode, 0
  br i1 %left, label %leftshift, label %rightshift
leftshift:
  br i1 %large, label %leftlarge, label %leftsmall
leftlarge:
  %llhigh = shl i64 %low, %amount
  br label %join
leftsmall:
  %inverse = sub i64 64, %amount
  %lslow = shl i64 %low, %amount
  %lshigh0 = shl i64 %high, %amount
  %lscarry = lshr i64 %low, %inverse
  %lshigh = or i64 %lshigh0, %lscarry
  br label %join
rightshift:
  %signed = icmp eq i32 %mode, 2
  %logical = lshr i64 %high, %amount
  %arithmetic = ashr i64 %high, %amount
  %upper = select i1 %signed, i64 %arithmetic, i64 %logical
  br i1 %large, label %rightlarge, label %rightsmall
rightlarge:
  %sign = ashr i64 %high, 63
  %rlhigh = select i1 %signed, i64 %sign, i64 0
  br label %join
rightsmall:
  %rinverse = sub i64 64, %amount
  %rslow0 = lshr i64 %low, %amount
  %rscarry = shl i64 %high, %rinverse
  %rslow = or i64 %rslow0, %rscarry
  br label %join
join:
  %outlow = phi i64 [ 0, %leftlarge ], [ %lslow, %leftsmall ], [ %upper, %rightlarge ], [ %rslow, %rightsmall ]
  %outhigh = phi i64 [ %llhigh, %leftlarge ], [ %lshigh, %leftsmall ], [ %rlhigh, %rightlarge ], [ %upper, %rightsmall ]
  %lo128 = zext i64 %outlow to i128
  %hi128 = zext i64 %outhigh to i128
  %positioned = shl i128 %hi128, 64
  %result = or i128 %lo128, %positioned
  ret i128 %result
}

define weak hidden i128 @__ashlti3(i128 %value, i32 %count) nounwind {
entry:
  %result = call i128 @tz.runtime.shift(i128 %value, i32 %count, i32 0)
  ret i128 %result
}

define weak hidden i128 @__lshrti3(i128 %value, i32 %count) nounwind {
entry:
  %result = call i128 @tz.runtime.shift(i128 %value, i32 %count, i32 1)
  ret i128 %result
}

define weak hidden i128 @__ashrti3(i128 %value, i32 %count) nounwind {
entry:
  %result = call i128 @tz.runtime.shift(i128 %value, i32 %count, i32 2)
  ret i128 %result
}
