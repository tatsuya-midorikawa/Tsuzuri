; W42: Win64 C and LLVM libcall wrappers for the 128-bit integer helpers of src/runtime/wasm.ll.
; On x86-64 Windows (`x86_64-pc-windows-msvc` and `-gnu`), LLVM lowers 128-bit division/remainder
; libcalls and Clang lowers C `__int128` parameters and results by passing each 128-bit argument by
; pointer (16-byte aligned temporary) and returning the 128-bit result in XMM0 (`<2 x i64>`).
; `windows_i128_helpers` turns the weak `@__<name>` definitions of wasm.ll into internal
; `@tz.runtime.<name>` helpers and appends these weak wrappers.

define weak hidden <2 x i64> @__multi3(ptr %a, ptr %b) nounwind {
entry:
  %x = load i128, ptr %a, align 16
  %y = load i128, ptr %b, align 16
  %r = call i128 @tz.runtime.multi3(i128 %x, i128 %y)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__udivti3(ptr %a, ptr %b) nounwind {
entry:
  %x = load i128, ptr %a, align 16
  %y = load i128, ptr %b, align 16
  %r = call i128 @tz.runtime.udivti3(i128 %x, i128 %y)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__umodti3(ptr %a, ptr %b) nounwind {
entry:
  %x = load i128, ptr %a, align 16
  %y = load i128, ptr %b, align 16
  %r = call i128 @tz.runtime.umodti3(i128 %x, i128 %y)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__udivmodti4(ptr %a, ptr %b, ptr %remainder) nounwind {
entry:
  %x = load i128, ptr %a, align 16
  %y = load i128, ptr %b, align 16
  %r = call i128 @tz.runtime.udivmodti4(i128 %x, i128 %y, ptr %remainder)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__divti3(ptr %a, ptr %b) nounwind {
entry:
  %x = load i128, ptr %a, align 16
  %y = load i128, ptr %b, align 16
  %r = call i128 @tz.runtime.divti3(i128 %x, i128 %y)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__modti3(ptr %a, ptr %b) nounwind {
entry:
  %x = load i128, ptr %a, align 16
  %y = load i128, ptr %b, align 16
  %r = call i128 @tz.runtime.modti3(i128 %x, i128 %y)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__ashlti3(ptr %value, i32 %count) nounwind {
entry:
  %x = load i128, ptr %value, align 16
  %r = call i128 @tz.runtime.ashlti3(i128 %x, i32 %count)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__lshrti3(ptr %value, i32 %count) nounwind {
entry:
  %x = load i128, ptr %value, align 16
  %r = call i128 @tz.runtime.lshrti3(i128 %x, i32 %count)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}

define weak hidden <2 x i64> @__ashrti3(ptr %value, i32 %count) nounwind {
entry:
  %x = load i128, ptr %value, align 16
  %r = call i128 @tz.runtime.ashrti3(i128 %x, i32 %count)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}
