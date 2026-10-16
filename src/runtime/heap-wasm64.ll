@__heap_base = external global i8
@tz.heap.end = internal global i64 0
@tz.heap.free = internal global i64 0
@tz.heap.small = internal global [32 x i64] zeroinitializer
@tz.heap.smallmask = internal global i64 0
declare i64 @llvm.wasm.memory.size.i64(i32)
declare i64 @llvm.wasm.memory.grow.i64(i32, i64)
declare i64 @llvm.cttz.i64(i64, i1)

; heap-wasm.ll for 64-bit memory, with words of 8 bytes. A block is a 16-byte header and a payload, both
; 16-byte aligned, with a capacity that is a multiple of 16 and at least 32. Header word 0 holds the
; capacity with two flags in its low bits: FREE (1) and PREV_FREE (2: the block before this one is
; free). A free block also holds the next (+8) and previous (+16) block of its list and, in its last
; eight bytes, its capacity, which the block after it reads to find it. Free blocks of capacity 32 to
; 528 are in the list of their capacity, @tz.heap.small[capacity / 16 - 2] (last in, first out);
; @tz.heap.smallmask has a bit for every list that is not empty, so a request finds the smallest list
; that serves it with one count of trailing zeros. Larger free blocks are in one list in address order,
; @tz.heap.free, and a request takes the first that fits.
; Invariants: two free blocks are never adjacent (a release merges both neighbours), no free block
; touches @tz.heap.end (such a block is given back by lowering the end), PREV_FREE is set exactly when
; the block before is free, and every free block is in the list of its capacity once.

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
  %occupied = icmp ne i64 %first, 0
  br i1 %occupied, label %chain, label %publish
chain:
  %fp = inttoptr i64 %first to ptr
  %fprev = getelementptr i8, ptr %fp, i64 16
  store i64 %block, ptr %fprev
  br label %publish
publish:
  store i64 %block, ptr %slot
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
  br i1 %empty, label %bump, label %inspect
inspect:
  %p = inttoptr i64 %block to ptr
  %flags = load i64, ptr %p
  %capacity = and i64 %flags, -16
  %nextp = getelementptr i8, ptr %p, i64 8
  %next = load i64, ptr %nextp
  %enough = icmp uge i64 %capacity, %needed
  br i1 %enough, label %reuse, label %advance
advance:
  br label %search
reuse:
  call void @tz.heap.unlink(i64 %block)
  %taken = call i64 @tz.heap.take(i64 %block, i64 %needed)
  %takenp = inttoptr i64 %taken to ptr
  %data = getelementptr i8, ptr %takenp, i64 16
  ret ptr %data
bump:
  %oldend = load i64, ptr @tz.heap.end
  %base = ptrtoint ptr @__heap_base to i64
  %unaligned = add i64 %base, 15
  %aligned = and i64 %unaligned, -16
  %initial = icmp eq i64 %oldend, 0
  %begin = select i1 %initial, i64 %aligned, i64 %oldend
  %end = add i64 %begin, %needed
  %within = icmp ule i64 %end, 16777216
  br i1 %within, label %capacitycheck, label %fail
capacitycheck:
  %pages = call i64 @llvm.wasm.memory.size.i64(i32 0)
  %ceil = add i64 %end, 65535
  %required = lshr i64 %ceil, 16
  %grow = icmp ugt i64 %required, %pages
  br i1 %grow, label %expand, label %allocated
expand:
  %delta = sub i64 %required, %pages
  %oldpages = call i64 @llvm.wasm.memory.grow.i64(i32 0, i64 %delta)
  %failed = icmp eq i64 %oldpages, -1
  br i1 %failed, label %fail, label %allocated
allocated:
  store i64 %end, ptr @tz.heap.end
  %header = inttoptr i64 %begin to ptr
  store i64 %needed, ptr %header
  %payload = getelementptr i8, ptr %header, i64 16
  ret ptr %payload
fail:
  call void @llvm.trap()
  unreachable
}

define internal void @tz.free(ptr %pointer) nounwind {
entry:
  %null = icmp eq ptr %pointer, null
  br i1 %null, label %exit, label %start
start:
  %data = ptrtoint ptr %pointer to i64
  %block = sub i64 %data, 16
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
  %first = phi i64 [ %block, %start ], [ %earlier, %back ]
  %total = phi i64 [ %size, %start ], [ %joined, %back ]
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
  %flags = load i64, ptr %header
  %capacity = and i64 %flags, -16
  %enough = icmp ule i64 %needed, %capacity
  br i1 %enough, label %reused, label %adjacent
adjacent:
  %following = add i64 %block, %capacity
  %end = load i64, ptr @tz.heap.end
  %attop = icmp eq i64 %following, %end
  br i1 %attop, label %fallback, label %neighbor
neighbor:
  %np = inttoptr i64 %following to ptr
  %nflags = load i64, ptr %np
  %nfree = and i64 %nflags, 1
  %isfree = icmp ne i64 %nfree, 0
  br i1 %isfree, label %combine, label %fallback
combine:
  %nsize = and i64 %nflags, -16
  %combined = add i64 %capacity, %nsize
  %available = icmp uge i64 %combined, %needed
  br i1 %available, label %absorb, label %fallback
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
  br label %copy_test
copy_test:
  %index = phi i64 [ 0, %fallback ], [ %next_index, %copy ]
  %more = icmp ult i64 %index, %bytes
  br i1 %more, label %copy, label %copied
copy:
  %source = getelementptr i8, ptr %old, i64 %index
  %target = getelementptr i8, ptr %replacement, i64 %index
  %byte = load i8, ptr %source
  store i8 %byte, ptr %target
  %next_index = add i64 %index, 1
  br label %copy_test
copied:
  call void @tz.free(ptr %old)
  ret ptr %replacement
fail:
  call void @llvm.trap()
  unreachable
}
