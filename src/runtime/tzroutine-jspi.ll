; The stack switch of the tzroutines of --wasm-feature jspi (TZ09), appended to the IR of a wasm32
; module that spawns, after the stack checks (src/llvm.rs with_stack_checks, whose per-G bounds
; @tsuzuri_stack_base and @tsuzuri_stack_top it sets). JSPI suspends a G's WebAssembly stack, the
; engine's frames and their locals, but not the stack in linear memory, whose pointer is the one
; global @__stack_pointer: whoever runs next moves it. So every point where a G can be suspended
; keeps the G's stack pointer and bounds in locals, which the suspension keeps, puts the scheduler's
; stack in place while the G is away, and writes the G's values back when it is resumed. Every
; access is a volatile access of the global: llvm.stackrestore does not write @__stack_pointer in a
; function without a frame of its own, and InstCombine drops a stackrestore before a `ret`. None of
; these functions has an alloca, so none has a frame of its own in linear memory.
;
; While no G is on the stack, the stack pointer is the top of @tz.jspi.stack, the scheduler's stack,
; on which the glue's synchronous calls run (tsuzuri_tz_fire, and the allocations of a socket's
; results that arrive while a G is suspended); between two export calls it is the module's own.

@__stack_pointer = external addrspace(1) global ptr
@tz.jspi.stack = internal global [16384 x i8] zeroinitializer, align 16

declare void @tz.jspi.import.suspend(ptr) "wasm-import-module"="tsuzuri_tz" "wasm-import-name"="suspend"
declare void @tzr_jspi_run(ptr)
declare void @tzr_jspi_fire()

; Puts the scheduler's stack in place.
define internal void @tz.jspi.idle() noinline nounwind {
entry:
  store volatile i32 ptrtoint (ptr @tz.jspi.stack to i32), ptr addrspace(1) @tsuzuri_stack_base
  store volatile i32 add (i32 ptrtoint (ptr @tz.jspi.stack to i32), i32 16384), ptr addrspace(1) @tsuzuri_stack_top
  store volatile ptr getelementptr inbounds (i8, ptr @tz.jspi.stack, i32 16384), ptr addrspace(1) @__stack_pointer
  ret void
}

; Puts the scheduler's stack in place unless `sp` is on it already (a drop that tsuzuri_tz_fire runs
; may close a socket).
define internal void @tz.jspi.leave(ptr %sp) noinline nounwind {
entry:
  %above = icmp uge ptr %sp, @tz.jspi.stack
  %below = icmp ule ptr %sp, getelementptr inbounds (i8, ptr @tz.jspi.stack, i32 16384)
  %on = and i1 %above, %below
  br i1 %on, label %done, label %switch
switch:
  call void @tz.jspi.idle()
  br label %done
done:
  ret void
}

; Suspends the G until the glue resumes it (tzroutine-jspi.h: tzr_park, tsuzuri_tz_yield).
define hidden void @tzr_jspi_suspend(ptr %g) noinline nounwind {
entry:
  %sp = load volatile ptr, ptr addrspace(1) @__stack_pointer
  %base = load volatile i32, ptr addrspace(1) @tsuzuri_stack_base
  %top = load volatile i32, ptr addrspace(1) @tsuzuri_stack_top
  call void @tz.jspi.idle()
  call void @tz.jspi.import.suspend(ptr %g)
  store volatile i32 %base, ptr addrspace(1) @tsuzuri_stack_base
  store volatile i32 %top, ptr addrspace(1) @tsuzuri_stack_top
  store volatile ptr %sp, ptr addrspace(1) @__stack_pointer
  ret void
}

; The first run of a G, which the glue calls through WebAssembly.promising: the G's stack and its
; bounds (offsets 0 and 4 of its record), the G's code and its end (tzr_jspi_run), then the
; scheduler's stack again.
define void @tsuzuri_tz_entry(i32 %g) nounwind {
entry:
  %record = inttoptr i32 %g to ptr
  %base = load i32, ptr %record, align 4
  %field = getelementptr inbounds i8, ptr %record, i32 4
  %top = load i32, ptr %field, align 4
  %sp = inttoptr i32 %top to ptr
  store volatile i32 %base, ptr addrspace(1) @tsuzuri_stack_base
  store volatile i32 %top, ptr addrspace(1) @tsuzuri_stack_top
  store volatile ptr %sp, ptr addrspace(1) @__stack_pointer
  call void @tzr_jspi_run(ptr %record)
  call void @tz.jspi.idle()
  ret void
}

; Fires the timers that are due, on the scheduler's stack (the glue calls it when its setTimeout comes).
define void @tsuzuri_tz_fire() nounwind {
entry:
  %sp = load volatile ptr, ptr addrspace(1) @__stack_pointer
  %base = load volatile i32, ptr addrspace(1) @tsuzuri_stack_base
  %top = load volatile i32, ptr addrspace(1) @tsuzuri_stack_top
  call void @tz.jspi.leave(ptr %sp)
  call void @tzr_jspi_fire()
  store volatile i32 %base, ptr addrspace(1) @tsuzuri_stack_base
  store volatile i32 %top, ptr addrspace(1) @tsuzuri_stack_top
  store volatile ptr %sp, ptr addrspace(1) @__stack_pointer
  ret void
}
