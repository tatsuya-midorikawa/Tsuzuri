@__heap_base = external global i8
@tz.heap.end = internal global i32 ptrtoint (ptr @__heap_base to i32)
@tz.heap.free = internal global i32 0
@tz.heap.small = internal global [32 x i32] zeroinitializer
@tz.heap.smallmask = internal global i32 0
@tz.heap.fast = internal global [32 x i32] zeroinitializer
@tz.heap.fastmask = internal global i32 0
@tz.heap.largest = internal global i32 0
declare i32 @llvm.wasm.memory.size.i32(i32)
declare i32 @llvm.wasm.memory.grow.i32(i32, i32)
declare i32 @llvm.cttz.i32(i32, i1)
declare void @llvm.memcpy.p0.p0.i32(ptr, ptr, i32, i1)

; A block is a 16-byte header and a payload, both 16-byte aligned, with a capacity that is a multiple
; of 16 and at least 32 (the 16 bytes of the header count in it: a request of n bytes needs
; max(32, n + 31 rounded down to 16)). The heap starts at __heap_base, which the linker aligns to 16,
; and ends at @tz.heap.end. Header word 0 holds the capacity with two flags in its low bits: FREE (1)
; and PREV_FREE (2: the block before this one is free). A free block also holds the next (+4) and
; previous (+8) block of its list and, in its last four bytes, its capacity, which the block after it
; reads to find it. Free blocks of capacity 32 to 528 are in the list of their capacity,
; @tz.heap.small[capacity / 16 - 2] (last in, first out); @tz.heap.smallmask has a bit for every list
; that is not empty, so a request finds the smallest list that serves it with one count of trailing
; zeros. Larger free blocks are in one list in address order, @tz.heap.free, and a request takes the
; first that fits, except that a block from which it would cut a rest of 64 KiB or more, and at least four
; times its own size, is passed over: a hole that has been cut up is no longer the hole of the next large
; request. A block of more than twice the largest recent request is not passed over (@tz.heap.largest is
; the largest capacity asked of the lists of large blocks, which loses 1/64 with every such request): no
; recent request has come near it. When every block that fits is passed over, the end of the heap serves
; the request while the end can grow, and otherwise the first block that was passed over is cut.
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
define internal void @tz.heap.insert(i32 %block, i32 %size) nounwind {
entry:
  %p = inttoptr i32 %block to ptr
  %flags = or i32 %size, 1
  store i32 %flags, ptr %p
  %limit = add i32 %block, %size
  %footer = sub i32 %limit, 4
  %footerp = inttoptr i32 %footer to ptr
  store i32 %size, ptr %footerp
  %successor = inttoptr i32 %limit to ptr
  %sflags = load i32, ptr %successor
  %marked = or i32 %sflags, 2
  store i32 %marked, ptr %successor
  %binned = icmp ule i32 %size, 528
  br i1 %binned, label %bin, label %ordered
bin:
  %class = lshr i32 %size, 4
  %index = sub i32 %class, 2
  %slot = getelementptr [32 x i32], ptr @tz.heap.small, i32 0, i32 %index
  %first = load i32, ptr %slot
  %bnextp = getelementptr i8, ptr %p, i32 4
  store i32 %first, ptr %bnextp
  %bprevp = getelementptr i8, ptr %p, i32 8
  store i32 0, ptr %bprevp
  store i32 %block, ptr %slot
  %occupied = icmp ne i32 %first, 0
  br i1 %occupied, label %chain, label %publish
chain:
  %fp = inttoptr i32 %first to ptr
  %fprev = getelementptr i8, ptr %fp, i32 8
  store i32 %block, ptr %fprev
  br label %exit
publish:
  %mask = load i32, ptr @tz.heap.smallmask
  %bit = shl i32 1, %index
  %marked_mask = or i32 %mask, %bit
  store i32 %marked_mask, ptr @tz.heap.smallmask
  br label %exit
ordered:
  %head = load i32, ptr @tz.heap.free
  br label %search
search:
  %current = phi i32 [ %head, %ordered ], [ %following, %advance ]
  %previous = phi i32 [ 0, %ordered ], [ %current, %advance ]
  %last = icmp eq i32 %current, 0
  %later = icmp ugt i32 %current, %block
  %place = or i1 %last, %later
  br i1 %place, label %link, label %advance
advance:
  %cp = inttoptr i32 %current to ptr
  %np = getelementptr i8, ptr %cp, i32 4
  %following = load i32, ptr %np
  br label %search
link:
  %nextp = getelementptr i8, ptr %p, i32 4
  store i32 %current, ptr %nextp
  %prevp = getelementptr i8, ptr %p, i32 8
  store i32 %previous, ptr %prevp
  %inner = icmp ne i32 %previous, 0
  br i1 %inner, label %afterprevious, label %athead
athead:
  store i32 %block, ptr @tz.heap.free
  br label %fixnext
afterprevious:
  %pp = inttoptr i32 %previous to ptr
  %ppn = getelementptr i8, ptr %pp, i32 4
  store i32 %block, ptr %ppn
  br label %fixnext
fixnext:
  %hasnext = icmp ne i32 %current, 0
  br i1 %hasnext, label %backlink, label %exit
backlink:
  %cp2 = inttoptr i32 %current to ptr
  %cprev = getelementptr i8, ptr %cp2, i32 8
  store i32 %block, ptr %cprev
  br label %exit
exit:
  ret void
}

; Takes a free block out of its list.
define internal void @tz.heap.unlink(i32 %block) nounwind {
entry:
  %p = inttoptr i32 %block to ptr
  %flags = load i32, ptr %p
  %size = and i32 %flags, -16
  %nextp = getelementptr i8, ptr %p, i32 4
  %next = load i32, ptr %nextp
  %prevp = getelementptr i8, ptr %p, i32 8
  %prev = load i32, ptr %prevp
  %inner = icmp ne i32 %prev, 0
  br i1 %inner, label %middle, label %first
first:
  %binned = icmp ule i32 %size, 528
  br i1 %binned, label %binhead, label %largehead
largehead:
  store i32 %next, ptr @tz.heap.free
  br label %fixnext
binhead:
  %class = lshr i32 %size, 4
  %index = sub i32 %class, 2
  %slot = getelementptr [32 x i32], ptr @tz.heap.small, i32 0, i32 %index
  store i32 %next, ptr %slot
  %emptied = icmp eq i32 %next, 0
  br i1 %emptied, label %clear, label %fixnext
clear:
  %mask = load i32, ptr @tz.heap.smallmask
  %bit = shl i32 1, %index
  %inverse = xor i32 %bit, -1
  %cleared = and i32 %mask, %inverse
  store i32 %cleared, ptr @tz.heap.smallmask
  br label %fixnext
middle:
  %pp = inttoptr i32 %prev to ptr
  %ppn = getelementptr i8, ptr %pp, i32 4
  store i32 %next, ptr %ppn
  br label %fixnext
fixnext:
  %hasnext = icmp ne i32 %next, 0
  br i1 %hasnext, label %backlink, label %exit
backlink:
  %np = inttoptr i32 %next to ptr
  %npp = getelementptr i8, ptr %np, i32 8
  store i32 %prev, ptr %npp
  br label %exit
exit:
  ret void
}

; Uses an unlinked free block for a request of the given capacity: the rest, if it is at least 32,
; becomes a free block. Its predecessor is not free, so the block keeps PREV_FREE clear.
define internal i32 @tz.heap.take(i32 %block, i32 %needed) nounwind {
entry:
  %p = inttoptr i32 %block to ptr
  %flags = load i32, ptr %p
  %capacity = and i32 %flags, -16
  %remaining = sub i32 %capacity, %needed
  %split = icmp uge i32 %remaining, 32
  br i1 %split, label %splitblock, label %whole
splitblock:
  store i32 %needed, ptr %p
  %rest = add i32 %block, %needed
  call void @tz.heap.insert(i32 %rest, i32 %remaining)
  ret i32 %block
whole:
  store i32 %capacity, ptr %p
  %after = add i32 %block, %capacity
  %ap = inttoptr i32 %after to ptr
  %aflags = load i32, ptr %ap
  %cleared = and i32 %aflags, -3
  store i32 %cleared, ptr %ap
  ret i32 %block
}

; Moves the end of the heap up by `needed` bytes from `begin`, if that stays within the limit and the
; memory has the pages or can grow by them; the caller sets the end.
define internal i1 @tz.heap.extend(i32 %begin, i32 %needed) nounwind {
entry:
  %end = add i32 %begin, %needed
  %within = icmp ule i32 %end, 16777216
  br i1 %within, label %capacitycheck, label %refused
capacitycheck:
  %pages = call i32 @llvm.wasm.memory.size.i32(i32 0)
  %ceil = add i32 %end, 65535
  %required = lshr i32 %ceil, 16
  %grow = icmp ugt i32 %required, %pages
  br i1 %grow, label %expand, label %granted
expand:
  %delta = sub i32 %required, %pages
  %oldpages = call i32 @llvm.wasm.memory.grow.i32(i32 0, i32 %delta)
  %failed = icmp eq i32 %oldpages, -1
  br i1 %failed, label %refused, label %granted
granted:
  ret i1 true
refused:
  ret i1 false
}

; Gives a block that is in use back to the heap: it is merged with a free block on either side, given
; back to the end of the heap when it touches it, and otherwise linked in its list.
define internal void @tz.heap.release(i32 %block) noinline nounwind {
entry:
  %p = inttoptr i32 %block to ptr
  %flags = load i32, ptr %p
  %size = and i32 %flags, -16
  %behind = and i32 %flags, 2
  %mergeback = icmp ne i32 %behind, 0
  br i1 %mergeback, label %back, label %ahead
back:
  %footer = sub i32 %block, 4
  %footerp = inttoptr i32 %footer to ptr
  %before = load i32, ptr %footerp
  %earlier = sub i32 %block, %before
  call void @tz.heap.unlink(i32 %earlier)
  %joined = add i32 %size, %before
  br label %ahead
ahead:
  %first = phi i32 [ %block, %entry ], [ %earlier, %back ]
  %total = phi i32 [ %size, %entry ], [ %joined, %back ]
  %following = add i32 %first, %total
  %end = load i32, ptr @tz.heap.end
  %attop = icmp eq i32 %following, %end
  br i1 %attop, label %trim, label %inspect
trim:
  store i32 %first, ptr @tz.heap.end
  br label %exit
inspect:
  %fp = inttoptr i32 %following to ptr
  %fflags = load i32, ptr %fp
  %freebit = and i32 %fflags, 1
  %mergeahead = icmp ne i32 %freebit, 0
  br i1 %mergeahead, label %forward, label %link
forward:
  call void @tz.heap.unlink(i32 %following)
  %fsize = and i32 %fflags, -16
  %more = add i32 %total, %fsize
  br label %link
link:
  %whole = phi i32 [ %total, %inspect ], [ %more, %forward ]
  call void @tz.heap.insert(i32 %first, i32 %whole)
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
  %mask = load i32, ptr @tz.heap.fastmask
  %done = icmp eq i32 %mask, 0
  br i1 %done, label %exit, label %step
step:
  %index = call i32 @llvm.cttz.i32(i32 %mask, i1 true)
  %bit = shl i32 1, %index
  %inverse = xor i32 %bit, -1
  %cleared = and i32 %mask, %inverse
  store i32 %cleared, ptr @tz.heap.fastmask
  %slot = getelementptr [32 x i32], ptr @tz.heap.fast, i32 0, i32 %index
  %head = load i32, ptr %slot
  store i32 0, ptr %slot
  br label %drain
drain:
  %block = phi i32 [ %head, %step ], [ %following, %run ]
  %empty = icmp eq i32 %block, 0
  br i1 %empty, label %next, label %run
run:
  %bp = inttoptr i32 %block to ptr
  %linkp = getelementptr i8, ptr %bp, i32 16
  %following = load i32, ptr %linkp
  call void @tz.heap.release(i32 %block)
  br label %drain
exit:
  ret void
}

; Uses a block of the large list for a request of the given capacity and returns it, in use. A rest above
; 528 takes the place of the block in the list; any other rest goes to its own list.
define internal i32 @tz.heap.carve(i32 %block, i32 %needed) nounwind {
entry:
  %p = inttoptr i32 %block to ptr
  %flags = load i32, ptr %p
  %capacity = and i32 %flags, -16
  %remaining = sub i32 %capacity, %needed
  %big = icmp ugt i32 %remaining, 528
  br i1 %big, label %replace, label %generic
replace:
  %nextp = getelementptr i8, ptr %p, i32 4
  %next = load i32, ptr %nextp
  %prevp = getelementptr i8, ptr %p, i32 8
  %prev = load i32, ptr %prevp
  %rest = add i32 %block, %needed
  %rp = inttoptr i32 %rest to ptr
  %rflags = or i32 %remaining, 1
  store i32 %rflags, ptr %rp
  %rnextp = getelementptr i8, ptr %rp, i32 4
  store i32 %next, ptr %rnextp
  %rprevp = getelementptr i8, ptr %rp, i32 8
  store i32 %prev, ptr %rprevp
  %limit = add i32 %block, %capacity
  %footer = sub i32 %limit, 4
  %footerp = inttoptr i32 %footer to ptr
  store i32 %remaining, ptr %footerp
  store i32 %needed, ptr %p
  %inner = icmp ne i32 %prev, 0
  br i1 %inner, label %afterprevious, label %athead
athead:
  store i32 %rest, ptr @tz.heap.free
  br label %fixnext
afterprevious:
  %pp = inttoptr i32 %prev to ptr
  %ppn = getelementptr i8, ptr %pp, i32 4
  store i32 %rest, ptr %ppn
  br label %fixnext
fixnext:
  %hasnext = icmp ne i32 %next, 0
  br i1 %hasnext, label %backlink, label %done
backlink:
  %np = inttoptr i32 %next to ptr
  %npp = getelementptr i8, ptr %np, i32 8
  store i32 %rest, ptr %npp
  br label %done
done:
  ret i32 %block
generic:
  call void @tz.heap.unlink(i32 %block)
  %taken = call i32 @tz.heap.take(i32 %block, i32 %needed)
  ret i32 %taken
}

; The slow path of @tz.alloc for a request of the given capacity: the lists by capacity, the large list,
; releasing the blocks of @tz.heap.fast for real, and the end of the heap. Returns null when the heap
; cannot grow. It stays out of line, so that @tz.alloc is small enough to be inlined.
define internal ptr @tz.heap.alloc_slow(i32 %needed) noinline nounwind {
entry:
  %binned = icmp ule i32 %needed, 528
  br i1 %binned, label %bins, label %large
bins:
  %class = lshr i32 %needed, 4
  %index = sub i32 %class, 2
  %mask = load i32, ptr @tz.heap.smallmask
  %above = shl i32 -1, %index
  %eligible = and i32 %mask, %above
  %any = icmp ne i32 %eligible, 0
  br i1 %any, label %frombin, label %large
frombin:
  %lowest = call i32 @llvm.cttz.i32(i32 %eligible, i1 true)
  %slot = getelementptr [32 x i32], ptr @tz.heap.small, i32 0, i32 %lowest
  %binhead = load i32, ptr %slot
  call void @tz.heap.unlink(i32 %binhead)
  %binblock = call i32 @tz.heap.take(i32 %binhead, i32 %needed)
  %binp = inttoptr i32 %binblock to ptr
  %bindata = getelementptr i8, ptr %binp, i32 16
  ret ptr %bindata
large:
  %head = load i32, ptr @tz.heap.free
  %seen = load i32, ptr @tz.heap.largest
  %faded = lshr i32 %seen, 6
  %fading = sub i32 %seen, %faded
  %bigger = icmp ugt i32 %needed, %fading
  %largest = select i1 %bigger, i32 %needed, i32 %fading
  store i32 %largest, ptr @tz.heap.largest
  br label %search
search:
  %block = phi i32 [ %head, %large ], [ %next, %advance ]
  %held = phi i32 [ 0, %large ], [ %heldnext, %advance ]
  %empty = icmp eq i32 %block, 0
  br i1 %empty, label %miss, label %inspect
inspect:
  %p = inttoptr i32 %block to ptr
  %flags = load i32, ptr %p
  %capacity = and i32 %flags, -16
  %nextp = getelementptr i8, ptr %p, i32 4
  %next = load i32, ptr %nextp
  %enough = icmp uge i32 %capacity, %needed
  br i1 %enough, label %candidate, label %advance
advance:
  %heldnext = phi i32 [ %held, %inspect ], [ %heldsel, %hold ]
  br label %search
candidate:
  ; A request that would cut a rest of 64 KiB or more, and at least four times its own size, from a block
  ; passes the block over: a hole that is cut into pieces is no longer the hole of the next large request.
  ; That holds only for a block of at most twice the largest recent request: a hole that no recent request
  ; has come near is not kept for one that may never come (a heap of blocks of a few KB to a few hundred KB
  ; would otherwise grow to its limit around the big free blocks that merging leaves). The search goes on,
  ; and the first block that the request does not cut like that serves it.
  %rest = sub i32 %capacity, %needed
  %restlarge = icmp uge i32 %rest, 65536
  %quarter = lshr i32 %rest, 2
  %wasteful = icmp uge i32 %quarter, %needed
  %cut = and i1 %restlarge, %wasteful
  %midpoint = lshr i32 %capacity, 1
  %plausible = icmp ule i32 %midpoint, %largest
  %keep = and i1 %cut, %plausible
  br i1 %keep, label %hold, label %reuse
hold:
  ; The first block that was passed over is remembered: it is cut only when the end of the heap cannot grow.
  %unheld = icmp eq i32 %held, 0
  %heldsel = select i1 %unheld, i32 %block, i32 %held
  br label %advance
prefertop:
  %topbegin = load i32, ptr @tz.heap.end
  %topgrown = call i1 @tz.heap.extend(i32 %topbegin, i32 %needed)
  br i1 %topgrown, label %topallocated, label %reuseheld
topallocated:
  %topend = add i32 %topbegin, %needed
  store i32 %topend, ptr @tz.heap.end
  %topheader = inttoptr i32 %topbegin to ptr
  store i32 %needed, ptr %topheader
  %toppayload = getelementptr i8, ptr %topheader, i32 16
  ret ptr %toppayload
reuse:
  %taken = call i32 @tz.heap.carve(i32 %block, i32 %needed)
  %takenp = inttoptr i32 %taken to ptr
  %data = getelementptr i8, ptr %takenp, i32 16
  ret ptr %data
reuseheld:
  %takenheld = call i32 @tz.heap.carve(i32 %held, i32 %needed)
  %heldp = inttoptr i32 %takenheld to ptr
  %heldata = getelementptr i8, ptr %heldp, i32 16
  ret ptr %heldata
miss:
  %passed = icmp ne i32 %held, 0
  br i1 %passed, label %prefertop, label %nofit
nofit:
  %pending = load i32, ptr @tz.heap.fastmask
  %stocked = icmp ne i32 %pending, 0
  br i1 %stocked, label %flushing, label %bump
flushing:
  call void @tz.heap.flush()
  br i1 %binned, label %bins, label %large
bump:
  %begin = load i32, ptr @tz.heap.end
  %grown = call i1 @tz.heap.extend(i32 %begin, i32 %needed)
  br i1 %grown, label %allocated, label %refused
allocated:
  %end = add i32 %begin, %needed
  store i32 %end, ptr @tz.heap.end
  %header = inttoptr i32 %begin to ptr
  store i32 %needed, ptr %header
  %payload = getelementptr i8, ptr %header, i32 16
  ret ptr %payload
refused:
  ret ptr null
}

define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %fits = icmp ule i64 %size, 16777184
  br i1 %fits, label %start, label %fail
start:
  %small = trunc i64 %size to i32
  %rounded = add i32 %small, 31
  %masked = and i32 %rounded, -16
  %short = icmp ult i32 %masked, 32
  %needed = select i1 %short, i32 32, i32 %masked
  %binned = icmp ule i32 %needed, 528
  br i1 %binned, label %cached, label %slow
cached:
  %fclass = lshr i32 %needed, 4
  %findex = sub i32 %fclass, 2
  %fslot = getelementptr [32 x i32], ptr @tz.heap.fast, i32 0, i32 %findex
  %fhead = load i32, ptr %fslot
  %hit = icmp ne i32 %fhead, 0
  br i1 %hit, label %pop, label %slow
pop:
  %fp = inttoptr i32 %fhead to ptr
  %fdata = getelementptr i8, ptr %fp, i32 16
  %fnext = load i32, ptr %fdata
  store i32 %fnext, ptr %fslot
  ret ptr %fdata
slow:
  %slowdata = call ptr @tz.heap.alloc_slow(i32 %needed)
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
  %header = getelementptr i8, ptr %pointer, i32 -16
  %flags = load i32, ptr %header
  %size = and i32 %flags, -16
  %binned = icmp ule i32 %size, 528
  br i1 %binned, label %cache, label %release
cache:
  %class = lshr i32 %size, 4
  %index = sub i32 %class, 2
  %slot = getelementptr [32 x i32], ptr @tz.heap.fast, i32 0, i32 %index
  %head = load i32, ptr %slot
  store i32 %head, ptr %pointer
  %block = ptrtoint ptr %header to i32
  store i32 %block, ptr %slot
  %fresh = icmp eq i32 %head, 0
  br i1 %fresh, label %mark, label %exit
mark:
  %mask = load i32, ptr @tz.heap.fastmask
  %bit = shl i32 1, %index
  %marked = or i32 %mask, %bit
  store i32 %marked, ptr @tz.heap.fastmask
  br label %exit
release:
  %address = ptrtoint ptr %header to i32
  call void @tz.heap.release(i32 %address)
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
  %small = trunc i64 %new_size to i32
  %rounded = add i32 %small, 31
  %masked = and i32 %rounded, -16
  %short = icmp ult i32 %masked, 32
  %needed = select i1 %short, i32 32, i32 %masked
  %payload = ptrtoint ptr %old to i32
  %block = sub i32 %payload, 16
  %header = inttoptr i32 %block to ptr
  br label %reread
reread:
  %flags = load i32, ptr %header
  %capacity = and i32 %flags, -16
  %enough = icmp ule i32 %needed, %capacity
  br i1 %enough, label %reused, label %adjacent
adjacent:
  %following = add i32 %block, %capacity
  %end = load i32, ptr @tz.heap.end
  %attop = icmp eq i32 %following, %end
  br i1 %attop, label %topgrow, label %neighbor
topgrow:
  %extra = sub i32 %needed, %capacity
  %extended = call i1 @tz.heap.extend(i32 %end, i32 %extra)
  br i1 %extended, label %topped, label %fallback
topped:
  %newend = add i32 %end, %extra
  store i32 %newend, ptr @tz.heap.end
  %topkept = and i32 %flags, 2
  %topsized = or i32 %needed, %topkept
  store i32 %topsized, ptr %header
  br label %reused
neighbor:
  %np = inttoptr i32 %following to ptr
  %nflags = load i32, ptr %np
  %nfree = and i32 %nflags, 1
  %isfree = icmp ne i32 %nfree, 0
  br i1 %isfree, label %combine, label %busy
combine:
  %nsize = and i32 %nflags, -16
  %combined = add i32 %capacity, %nsize
  %available = icmp uge i32 %combined, %needed
  br i1 %available, label %absorb, label %busy
busy:
  %pending = load i32, ptr @tz.heap.fastmask
  %stocked = icmp ne i32 %pending, 0
  br i1 %stocked, label %flushing, label %fallback
flushing:
  call void @tz.heap.flush()
  br label %reread
absorb:
  call void @tz.heap.unlink(i32 %following)
  %kept = and i32 %flags, 2
  %remaining = sub i32 %combined, %needed
  %split = icmp uge i32 %remaining, 32
  br i1 %split, label %split_block, label %whole
split_block:
  %shorter = or i32 %needed, %kept
  store i32 %shorter, ptr %header
  %rest = add i32 %block, %needed
  call void @tz.heap.insert(i32 %rest, i32 %remaining)
  br label %reused
whole:
  %longer = or i32 %combined, %kept
  store i32 %longer, ptr %header
  %after = add i32 %block, %combined
  %ap = inttoptr i32 %after to ptr
  %aflags = load i32, ptr %ap
  %cleared = and i32 %aflags, -3
  store i32 %cleared, ptr %ap
  br label %reused
reused:
  ret ptr %old
fallback:
  %replacement = call ptr @tz.alloc(i64 %new_size)
  %smaller = icmp ult i64 %old_size, %new_size
  %bytes = select i1 %smaller, i64 %old_size, i64 %new_size
  %count = trunc i64 %bytes to i32
  call void @llvm.memcpy.p0.p0.i32(ptr %replacement, ptr %old, i32 %count, i1 false)
  call void @tz.free(ptr %old)
  ret ptr %replacement
fail:
  call void @llvm.trap()
  unreachable
}
