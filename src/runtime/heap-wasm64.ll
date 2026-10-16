@__heap_base = external global i8
@tz.heap.end = internal global i64 ptrtoint (ptr @__heap_base to i64)
@tz.heap.free = internal global i64 0
@tz.heap.small = internal global [32 x i64] zeroinitializer
@tz.heap.smallmask = internal global i64 0
@tz.heap.fast = internal global [32 x i64] zeroinitializer
@tz.heap.fastmask = internal global i64 0
declare i64 @llvm.wasm.memory.size.i64(i32)
declare i64 @llvm.wasm.memory.grow.i64(i32, i64)
declare i64 @llvm.cttz.i64(i64, i1)
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)

; heap-wasm.ll for 64-bit memory, with words of 8 bytes. A block is a 16-byte header and a payload, both
; 16-byte aligned, with a capacity that is a multiple of 16 and at least 32 (the 16 bytes of the header
; count in it: a request of n bytes needs max(32, n + 31 rounded down to 16)). The heap starts at
; __heap_base, which the linker aligns to 16, and ends at @tz.heap.end. Header word 0 holds the capacity
; with two flags in its low bits: FREE (1) and PREV_FREE (2: the block before this one is free). A free
; block also holds the next (+8) and previous (+16) block of its list and, in its last eight bytes, its
; capacity, which the block after it reads to find it. Free blocks of capacity 32 to 528 are in the list
; of their capacity, @tz.heap.small[capacity / 16 - 2] (last in, first out); @tz.heap.smallmask has a bit
; for every list that is not empty, so a request finds the smallest list that serves it with one count of
; trailing zeros. Larger free blocks are in one list in address order, @tz.heap.free, and a request takes
; the first that fits, except that a request that would cut a rest of 64 KiB or more, and at least four
; times its own size, from that block is served from the end of the heap while the end can grow: a hole
; that has been cut up is no longer the hole of the next large request.
; A block of capacity 32 to 528 that is released is not merged at once: it is pushed on the list of its
; capacity in @tz.heap.fast (next in the payload word at +16) and stays an ordinary block that is in
; use for every other block, so a request of that capacity pops it back in a few instructions, which
; is all that @tz.alloc and @tz.free do for such a block (they are small enough to be inlined; the
; rest is out of line). Only when no other list can serve a request and the heap would have to grow
; are all of those blocks released for real (@tz.heap.flush), which merges them with their
; neighbours. @tz.heap.fastmask has a bit for every list of @tz.heap.fast that is not empty and
; maybe for some that are: a pop does not clear the bit, a flush does.
; A block at the end of the heap grows in place (@tz.realloc), and one that touches the end when it is
; released is given back by lowering the end, so a heap that is emptied again is empty. A block that
; has to move is copied with llvm.memcpy, which is memory.copy: every WASM build enables bulk memory.
; Invariants: two free blocks are never adjacent (a release merges both neighbours), no free block
; touches @tz.heap.end, PREV_FREE is set exactly when the block before is free (a block in
; @tz.heap.fast is not free), and every free block is in the list of its capacity once.

; Makes the block a free block of the given size, whose neighbours are already merged, and links it in.
define internal void @tz.heap.insert(i64 %block, i64 %size) nounwind {
entry:
  %p = inttoptr i64 %block to ptr
  %flags = or i64 %size, 1
  store i64 %flags, ptr %p
  %limit = add i64 %block, %size
  %footer = sub i64 %limit, 8
  %footerp = inttoptr i64 %footer to ptr
  store i64 %size, ptr %footerp
  %successor = inttoptr i64 %limit to ptr
  %sflags = load i64, ptr %successor
  %marked = or i64 %sflags, 2
  store i64 %marked, ptr %successor
  %binned = icmp ule i64 %size, 528
  br i1 %binned, label %bin, label %ordered
bin:
  %class = lshr i64 %size, 4
  %index = sub i64 %class, 2
  %slot = getelementptr [32 x i64], ptr @tz.heap.small, i64 0, i64 %index
  %first = load i64, ptr %slot
  %bnextp = getelementptr i8, ptr %p, i64 8
  store i64 %first, ptr %bnextp
  %bprevp = getelementptr i8, ptr %p, i64 16
  store i64 0, ptr %bprevp
  store i64 %block, ptr %slot
  %occupied = icmp ne i64 %first, 0
  br i1 %occupied, label %chain, label %publish
chain:
  %fp = inttoptr i64 %first to ptr
  %fprev = getelementptr i8, ptr %fp, i64 16
  store i64 %block, ptr %fprev
  br label %exit
publish:
  %mask = load i64, ptr @tz.heap.smallmask
  %bit = shl i64 1, %index
  %marked_mask = or i64 %mask, %bit
  store i64 %marked_mask, ptr @tz.heap.smallmask
  br label %exit
ordered:
  %head = load i64, ptr @tz.heap.free
  br label %search
search:
  %current = phi i64 [ %head, %ordered ], [ %following, %advance ]
  %previous = phi i64 [ 0, %ordered ], [ %current, %advance ]
  %last = icmp eq i64 %current, 0
  %later = icmp ugt i64 %current, %block
  %place = or i1 %last, %later
  br i1 %place, label %link, label %advance
advance:
  %cp = inttoptr i64 %current to ptr
  %np = getelementptr i8, ptr %cp, i64 8
  %following = load i64, ptr %np
  br label %search
link:
  %nextp = getelementptr i8, ptr %p, i64 8
  store i64 %current, ptr %nextp
  %prevp = getelementptr i8, ptr %p, i64 16
  store i64 %previous, ptr %prevp
  %inner = icmp ne i64 %previous, 0
  br i1 %inner, label %afterprevious, label %athead
athead:
  store i64 %block, ptr @tz.heap.free
  br label %fixnext
afterprevious:
  %pp = inttoptr i64 %previous to ptr
  %ppn = getelementptr i8, ptr %pp, i64 8
  store i64 %block, ptr %ppn
  br label %fixnext
fixnext:
  %hasnext = icmp ne i64 %current, 0
  br i1 %hasnext, label %backlink, label %exit
backlink:
  %cp2 = inttoptr i64 %current to ptr
  %cprev = getelementptr i8, ptr %cp2, i64 16
  store i64 %block, ptr %cprev
  br label %exit
exit:
  ret void
}

; Takes a free block out of its list.
define internal void @tz.heap.unlink(i64 %block) nounwind {
entry:
  %p = inttoptr i64 %block to ptr
  %flags = load i64, ptr %p
  %size = and i64 %flags, -16
  %nextp = getelementptr i8, ptr %p, i64 8
  %next = load i64, ptr %nextp
  %prevp = getelementptr i8, ptr %p, i64 16
  %prev = load i64, ptr %prevp
  %inner = icmp ne i64 %prev, 0
  br i1 %inner, label %middle, label %first
first:
  %binned = icmp ule i64 %size, 528
  br i1 %binned, label %binhead, label %largehead
largehead:
  store i64 %next, ptr @tz.heap.free
  br label %fixnext
binhead:
  %class = lshr i64 %size, 4
  %index = sub i64 %class, 2
  %slot = getelementptr [32 x i64], ptr @tz.heap.small, i64 0, i64 %index
  store i64 %next, ptr %slot
  %emptied = icmp eq i64 %next, 0
  br i1 %emptied, label %clear, label %fixnext
clear:
  %mask = load i64, ptr @tz.heap.smallmask
  %bit = shl i64 1, %index
  %inverse = xor i64 %bit, -1
  %cleared = and i64 %mask, %inverse
  store i64 %cleared, ptr @tz.heap.smallmask
  br label %fixnext
middle:
  %pp = inttoptr i64 %prev to ptr
  %ppn = getelementptr i8, ptr %pp, i64 8
  store i64 %next, ptr %ppn
  br label %fixnext
fixnext:
  %hasnext = icmp ne i64 %next, 0
  br i1 %hasnext, label %backlink, label %exit
backlink:
  %np = inttoptr i64 %next to ptr
  %npp = getelementptr i8, ptr %np, i64 16
  store i64 %prev, ptr %npp
  br label %exit
exit:
  ret void
}

; Uses an unlinked free block for a request of the given capacity: the rest, if it is at least 32,
; becomes a free block. Its predecessor is not free, so the block keeps PREV_FREE clear.
define internal i64 @tz.heap.take(i64 %block, i64 %needed) nounwind {
entry:
  %p = inttoptr i64 %block to ptr
  %flags = load i64, ptr %p
  %capacity = and i64 %flags, -16
  %remaining = sub i64 %capacity, %needed
  %split = icmp uge i64 %remaining, 32
  br i1 %split, label %splitblock, label %whole
splitblock:
  store i64 %needed, ptr %p
  %rest = add i64 %block, %needed
  call void @tz.heap.insert(i64 %rest, i64 %remaining)
  ret i64 %block
whole:
  store i64 %capacity, ptr %p
  %after = add i64 %block, %capacity
  %ap = inttoptr i64 %after to ptr
  %aflags = load i64, ptr %ap
  %cleared = and i64 %aflags, -3
  store i64 %cleared, ptr %ap
  ret i64 %block
}

; Moves the end of the heap up by `needed` bytes from `begin`, if that stays within the limit and the
; memory has the pages or can grow by them; the caller sets the end.
define internal i1 @tz.heap.extend(i64 %begin, i64 %needed) nounwind {
entry:
  %end = add i64 %begin, %needed
  %within = icmp ule i64 %end, 16777216
  br i1 %within, label %capacitycheck, label %refused
capacitycheck:
  %pages = call i64 @llvm.wasm.memory.size.i64(i32 0)
  %ceil = add i64 %end, 65535
  %required = lshr i64 %ceil, 16
  %grow = icmp ugt i64 %required, %pages
  br i1 %grow, label %expand, label %granted
expand:
  %delta = sub i64 %required, %pages
  %oldpages = call i64 @llvm.wasm.memory.grow.i64(i32 0, i64 %delta)
  %failed = icmp eq i64 %oldpages, -1
  br i1 %failed, label %refused, label %granted
granted:
  ret i1 true
refused:
  ret i1 false
}

; Gives a block that is in use back to the heap: it is merged with a free block on either side, given
; back to the end of the heap when it touches it, and otherwise linked in its list.
define internal void @tz.heap.release(i64 %block) noinline nounwind {
entry:
  %p = inttoptr i64 %block to ptr
  %flags = load i64, ptr %p
  %size = and i64 %flags, -16
  %behind = and i64 %flags, 2
  %mergeback = icmp ne i64 %behind, 0
  br i1 %mergeback, label %back, label %ahead
back:
  %footer = sub i64 %block, 8
  %footerp = inttoptr i64 %footer to ptr
  %before = load i64, ptr %footerp
  %earlier = sub i64 %block, %before
  call void @tz.heap.unlink(i64 %earlier)
  %joined = add i64 %size, %before
  br label %ahead
ahead:
  %first = phi i64 [ %block, %entry ], [ %earlier, %back ]
  %total = phi i64 [ %size, %entry ], [ %joined, %back ]
  %following = add i64 %first, %total
  %end = load i64, ptr @tz.heap.end
  %attop = icmp eq i64 %following, %end
  br i1 %attop, label %trim, label %inspect
trim:
  store i64 %first, ptr @tz.heap.end
  br label %exit
inspect:
  %fp = inttoptr i64 %following to ptr
  %fflags = load i64, ptr %fp
  %freebit = and i64 %fflags, 1
  %mergeahead = icmp ne i64 %freebit, 0
  br i1 %mergeahead, label %forward, label %link
forward:
  call void @tz.heap.unlink(i64 %following)
  %fsize = and i64 %fflags, -16
  %more = add i64 %total, %fsize
  br label %link
link:
  %whole = phi i64 [ %total, %inspect ], [ %more, %forward ]
  call void @tz.heap.insert(i64 %first, i64 %whole)
  br label %exit
exit:
  ret void
}

; Releases every block of @tz.heap.fast for real. A list may be empty while its bit is set: a request
; that takes the last block of a list leaves the bit, and the next release of that capacity sets it again.
define internal void @tz.heap.flush() noinline nounwind {
entry:
  br label %next
next:
  %mask = load i64, ptr @tz.heap.fastmask
  %done = icmp eq i64 %mask, 0
  br i1 %done, label %exit, label %step
step:
  %index = call i64 @llvm.cttz.i64(i64 %mask, i1 true)
  %bit = shl i64 1, %index
  %inverse = xor i64 %bit, -1
  %cleared = and i64 %mask, %inverse
  store i64 %cleared, ptr @tz.heap.fastmask
  %slot = getelementptr [32 x i64], ptr @tz.heap.fast, i64 0, i64 %index
  %head = load i64, ptr %slot
  store i64 0, ptr %slot
  br label %drain
drain:
  %block = phi i64 [ %head, %step ], [ %following, %run ]
  %empty = icmp eq i64 %block, 0
  br i1 %empty, label %next, label %run
run:
  %bp = inttoptr i64 %block to ptr
  %linkp = getelementptr i8, ptr %bp, i64 16
  %following = load i64, ptr %linkp
  call void @tz.heap.release(i64 %block)
  br label %drain
exit:
  ret void
}

; Uses a block of the large list for a request of the given capacity and returns it, in use. A rest above
; 528 takes the place of the block in the list; any other rest goes to its own list.
define internal i64 @tz.heap.carve(i64 %block, i64 %needed) nounwind {
entry:
  %p = inttoptr i64 %block to ptr
  %flags = load i64, ptr %p
  %capacity = and i64 %flags, -16
  %remaining = sub i64 %capacity, %needed
  %big = icmp ugt i64 %remaining, 528
  br i1 %big, label %replace, label %generic
replace:
  %nextp = getelementptr i8, ptr %p, i64 8
  %next = load i64, ptr %nextp
  %prevp = getelementptr i8, ptr %p, i64 16
  %prev = load i64, ptr %prevp
  %rest = add i64 %block, %needed
  %rp = inttoptr i64 %rest to ptr
  %rflags = or i64 %remaining, 1
  store i64 %rflags, ptr %rp
  %rnextp = getelementptr i8, ptr %rp, i64 8
  store i64 %next, ptr %rnextp
  %rprevp = getelementptr i8, ptr %rp, i64 16
  store i64 %prev, ptr %rprevp
  %limit = add i64 %block, %capacity
  %footer = sub i64 %limit, 8
  %footerp = inttoptr i64 %footer to ptr
  store i64 %remaining, ptr %footerp
  store i64 %needed, ptr %p
  %inner = icmp ne i64 %prev, 0
  br i1 %inner, label %afterprevious, label %athead
athead:
  store i64 %rest, ptr @tz.heap.free
  br label %fixnext
afterprevious:
  %pp = inttoptr i64 %prev to ptr
  %ppn = getelementptr i8, ptr %pp, i64 8
  store i64 %rest, ptr %ppn
  br label %fixnext
fixnext:
  %hasnext = icmp ne i64 %next, 0
  br i1 %hasnext, label %backlink, label %done
backlink:
  %np = inttoptr i64 %next to ptr
  %npp = getelementptr i8, ptr %np, i64 16
  store i64 %rest, ptr %npp
  br label %done
done:
  ret i64 %block
generic:
  call void @tz.heap.unlink(i64 %block)
  %taken = call i64 @tz.heap.take(i64 %block, i64 %needed)
  ret i64 %taken
}

; The slow path of @tz.alloc for a request of the given capacity: the lists by capacity, the large list,
; releasing the blocks of @tz.heap.fast for real, and the end of the heap. Returns null when the heap
; cannot grow. It stays out of line, so that @tz.alloc is small enough to be inlined.
define internal ptr @tz.heap.alloc_slow(i64 %needed) noinline nounwind {
entry:
  %binned = icmp ule i64 %needed, 528
  br i1 %binned, label %bins, label %large
bins:
  %class = lshr i64 %needed, 4
  %index = sub i64 %class, 2
  %mask = load i64, ptr @tz.heap.smallmask
  %above = shl i64 -1, %index
  %eligible = and i64 %mask, %above
  %any = icmp ne i64 %eligible, 0
  br i1 %any, label %frombin, label %large
frombin:
  %lowest = call i64 @llvm.cttz.i64(i64 %eligible, i1 true)
  %slot = getelementptr [32 x i64], ptr @tz.heap.small, i64 0, i64 %lowest
  %binhead = load i64, ptr %slot
  call void @tz.heap.unlink(i64 %binhead)
  %binblock = call i64 @tz.heap.take(i64 %binhead, i64 %needed)
  %binp = inttoptr i64 %binblock to ptr
  %bindata = getelementptr i8, ptr %binp, i64 16
  ret ptr %bindata
large:
  %head = load i64, ptr @tz.heap.free
  br label %search
search:
  %block = phi i64 [ %head, %large ], [ %next, %advance ]
  %empty = icmp eq i64 %block, 0
  br i1 %empty, label %miss, label %inspect
inspect:
  %p = inttoptr i64 %block to ptr
  %flags = load i64, ptr %p
  %capacity = and i64 %flags, -16
  %nextp = getelementptr i8, ptr %p, i64 8
  %next = load i64, ptr %nextp
  %enough = icmp uge i64 %capacity, %needed
  br i1 %enough, label %candidate, label %advance
advance:
  br label %search
candidate:
  ; A request that would cut a rest of 64 KiB or more, and at least four times its own size, from a block
  ; leaves the block to the large requests while the heap can grow: a hole that is cut into pieces is no longer
  ; the hole of the next large request.
  %rest = sub i64 %capacity, %needed
  %restlarge = icmp uge i64 %rest, 65536
  %quarter = lshr i64 %rest, 2
  %wasteful = icmp uge i64 %quarter, %needed
  %keep = and i1 %restlarge, %wasteful
  br i1 %keep, label %prefertop, label %reuse
prefertop:
  %topbegin = load i64, ptr @tz.heap.end
  %topgrown = call i1 @tz.heap.extend(i64 %topbegin, i64 %needed)
  br i1 %topgrown, label %topallocated, label %reuse
topallocated:
  %topend = add i64 %topbegin, %needed
  store i64 %topend, ptr @tz.heap.end
  %topheader = inttoptr i64 %topbegin to ptr
  store i64 %needed, ptr %topheader
  %toppayload = getelementptr i8, ptr %topheader, i64 16
  ret ptr %toppayload
reuse:
  %taken = call i64 @tz.heap.carve(i64 %block, i64 %needed)
  %takenp = inttoptr i64 %taken to ptr
  %data = getelementptr i8, ptr %takenp, i64 16
  ret ptr %data
miss:
  %pending = load i64, ptr @tz.heap.fastmask
  %stocked = icmp ne i64 %pending, 0
  br i1 %stocked, label %flushing, label %bump
flushing:
  call void @tz.heap.flush()
  br i1 %binned, label %bins, label %large
bump:
  %begin = load i64, ptr @tz.heap.end
  %grown = call i1 @tz.heap.extend(i64 %begin, i64 %needed)
  br i1 %grown, label %allocated, label %refused
allocated:
  %end = add i64 %begin, %needed
  store i64 %end, ptr @tz.heap.end
  %header = inttoptr i64 %begin to ptr
  store i64 %needed, ptr %header
  %payload = getelementptr i8, ptr %header, i64 16
  ret ptr %payload
refused:
  ret ptr null
}

define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %fits = icmp ule i64 %size, 16777184
  br i1 %fits, label %start, label %fail
start:
  %rounded = add i64 %size, 31
  %masked = and i64 %rounded, -16
  %short = icmp ult i64 %masked, 32
  %needed = select i1 %short, i64 32, i64 %masked
  %binned = icmp ule i64 %needed, 528
  br i1 %binned, label %cached, label %slow
cached:
  %fclass = lshr i64 %needed, 4
  %findex = sub i64 %fclass, 2
  %fslot = getelementptr [32 x i64], ptr @tz.heap.fast, i64 0, i64 %findex
  %fhead = load i64, ptr %fslot
  %hit = icmp ne i64 %fhead, 0
  br i1 %hit, label %pop, label %slow
pop:
  %fp = inttoptr i64 %fhead to ptr
  %fdata = getelementptr i8, ptr %fp, i64 16
  %fnext = load i64, ptr %fdata
  store i64 %fnext, ptr %fslot
  ret ptr %fdata
slow:
  %slowdata = call ptr @tz.heap.alloc_slow(i64 %needed)
  %missing = icmp eq ptr %slowdata, null
  br i1 %missing, label %fail, label %done
done:
  ret ptr %slowdata
fail:
  call void @llvm.trap()
  unreachable
}

define internal void @tz.free(ptr %pointer) nounwind {
entry:
  %null = icmp eq ptr %pointer, null
  br i1 %null, label %exit, label %start
start:
  %header = getelementptr i8, ptr %pointer, i64 -16
  %flags = load i64, ptr %header
  %size = and i64 %flags, -16
  %binned = icmp ule i64 %size, 528
  br i1 %binned, label %cache, label %release
cache:
  %class = lshr i64 %size, 4
  %index = sub i64 %class, 2
  %slot = getelementptr [32 x i64], ptr @tz.heap.fast, i64 0, i64 %index
  %head = load i64, ptr %slot
  store i64 %head, ptr %pointer
  %block = ptrtoint ptr %header to i64
  store i64 %block, ptr %slot
  %fresh = icmp eq i64 %head, 0
  br i1 %fresh, label %mark, label %exit
mark:
  %mask = load i64, ptr @tz.heap.fastmask
  %bit = shl i64 1, %index
  %marked = or i64 %mask, %bit
  store i64 %marked, ptr @tz.heap.fastmask
  br label %exit
release:
  %address = ptrtoint ptr %header to i64
  call void @tz.heap.release(i64 %address)
  br label %exit
exit:
  ret void
}

define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size) nounwind {
entry:
  %zero = icmp eq i64 %new_size, 0
  br i1 %zero, label %release, label %check
release:
  call void @tz.free(ptr %old)
  ret ptr null
check:
  %fits = icmp ule i64 %new_size, 16777184
  br i1 %fits, label %nonnull, label %fail
nonnull:
  %null = icmp eq ptr %old, null
  br i1 %null, label %allocate, label %inspect
allocate:
  %fresh = call ptr @tz.alloc(i64 %new_size)
  ret ptr %fresh
inspect:
  %rounded = add i64 %new_size, 31
  %masked = and i64 %rounded, -16
  %short = icmp ult i64 %masked, 32
  %needed = select i1 %short, i64 32, i64 %masked
  %payload = ptrtoint ptr %old to i64
  %block = sub i64 %payload, 16
  %header = inttoptr i64 %block to ptr
  br label %reread
reread:
  %flags = load i64, ptr %header
  %capacity = and i64 %flags, -16
  %enough = icmp ule i64 %needed, %capacity
  br i1 %enough, label %reused, label %adjacent
adjacent:
  %following = add i64 %block, %capacity
  %end = load i64, ptr @tz.heap.end
  %attop = icmp eq i64 %following, %end
  br i1 %attop, label %topgrow, label %neighbor
topgrow:
  %extra = sub i64 %needed, %capacity
  %extended = call i1 @tz.heap.extend(i64 %end, i64 %extra)
  br i1 %extended, label %topped, label %fallback
topped:
  %newend = add i64 %end, %extra
  store i64 %newend, ptr @tz.heap.end
  %topkept = and i64 %flags, 2
  %topsized = or i64 %needed, %topkept
  store i64 %topsized, ptr %header
  br label %reused
neighbor:
  %np = inttoptr i64 %following to ptr
  %nflags = load i64, ptr %np
  %nfree = and i64 %nflags, 1
  %isfree = icmp ne i64 %nfree, 0
  br i1 %isfree, label %combine, label %busy
combine:
  %nsize = and i64 %nflags, -16
  %combined = add i64 %capacity, %nsize
  %available = icmp uge i64 %combined, %needed
  br i1 %available, label %absorb, label %busy
busy:
  %pending = load i64, ptr @tz.heap.fastmask
  %stocked = icmp ne i64 %pending, 0
  br i1 %stocked, label %flushing, label %fallback
flushing:
  call void @tz.heap.flush()
  br label %reread
absorb:
  call void @tz.heap.unlink(i64 %following)
  %kept = and i64 %flags, 2
  %remaining = sub i64 %combined, %needed
  %split = icmp uge i64 %remaining, 32
  br i1 %split, label %split_block, label %whole
split_block:
  %shorter = or i64 %needed, %kept
  store i64 %shorter, ptr %header
  %rest = add i64 %block, %needed
  call void @tz.heap.insert(i64 %rest, i64 %remaining)
  br label %reused
whole:
  %longer = or i64 %combined, %kept
  store i64 %longer, ptr %header
  %after = add i64 %block, %combined
  %ap = inttoptr i64 %after to ptr
  %aflags = load i64, ptr %ap
  %cleared = and i64 %aflags, -3
  store i64 %cleared, ptr %ap
  br label %reused
reused:
  ret ptr %old
fallback:
  %replacement = call ptr @tz.alloc(i64 %new_size)
  %smaller = icmp ult i64 %old_size, %new_size
  %bytes = select i1 %smaller, i64 %old_size, i64 %new_size
  call void @llvm.memcpy.p0.p0.i64(ptr %replacement, ptr %old, i64 %bytes, i1 false)
  call void @tz.free(ptr %old)
  ret ptr %replacement
fail:
  call void @llvm.trap()
  unreachable
}
