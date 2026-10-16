@__heap_base = external global i8
@tz.heap.end = internal global i32 0
@tz.heap.free = internal global i32 0
declare i32 @llvm.wasm.memory.size.i32(i32)
declare i32 @llvm.wasm.memory.grow.i32(i32, i32)

; A block is a 16-byte header and a payload, both 16-byte aligned, with a capacity that is a multiple
; of 16 and at least 32. Header word 0 holds the capacity with two flags in its low bits: FREE (1) and
; PREV_FREE (2: the block before this one is free). A free block also holds the next (+4) and previous
; (+8) block of its list, in address order, and in its last four bytes its capacity, which the block
; after it reads to find it.
; Invariants: two free blocks are never adjacent (a release merges both neighbours), no free block
; touches @tz.heap.end (such a block is given back by lowering the end), PREV_FREE is set exactly when
; the block before is free, and every free block is in the list once.

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
  %head = load i32, ptr @tz.heap.free
  br label %search
search:
  %current = phi i32 [ %head, %entry ], [ %following, %advance ]
  %previous = phi i32 [ 0, %entry ], [ %current, %advance ]
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
  %nextp = getelementptr i8, ptr %p, i32 4
  %next = load i32, ptr %nextp
  %prevp = getelementptr i8, ptr %p, i32 8
  %prev = load i32, ptr %prevp
  %inner = icmp ne i32 %prev, 0
  br i1 %inner, label %middle, label %first
first:
  store i32 %next, ptr @tz.heap.free
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
  %head = load i32, ptr @tz.heap.free
  br label %search
search:
  %block = phi i32 [ %head, %start ], [ %next, %advance ]
  %empty = icmp eq i32 %block, 0
  br i1 %empty, label %bump, label %inspect
inspect:
  %p = inttoptr i32 %block to ptr
  %flags = load i32, ptr %p
  %capacity = and i32 %flags, -16
  %nextp = getelementptr i8, ptr %p, i32 4
  %next = load i32, ptr %nextp
  %enough = icmp uge i32 %capacity, %needed
  br i1 %enough, label %reuse, label %advance
advance:
  br label %search
reuse:
  call void @tz.heap.unlink(i32 %block)
  %taken = call i32 @tz.heap.take(i32 %block, i32 %needed)
  %takenp = inttoptr i32 %taken to ptr
  %data = getelementptr i8, ptr %takenp, i32 16
  ret ptr %data
bump:
  %oldend = load i32, ptr @tz.heap.end
  %base = ptrtoint ptr @__heap_base to i32
  %unaligned = add i32 %base, 15
  %aligned = and i32 %unaligned, -16
  %initial = icmp eq i32 %oldend, 0
  %begin = select i1 %initial, i32 %aligned, i32 %oldend
  %end = add i32 %begin, %needed
  %within = icmp ule i32 %end, 16777216
  br i1 %within, label %capacitycheck, label %fail
capacitycheck:
  %pages = call i32 @llvm.wasm.memory.size.i32(i32 0)
  %ceil = add i32 %end, 65535
  %required = lshr i32 %ceil, 16
  %grow = icmp ugt i32 %required, %pages
  br i1 %grow, label %expand, label %allocated
expand:
  %delta = sub i32 %required, %pages
  %oldpages = call i32 @llvm.wasm.memory.grow.i32(i32 0, i32 %delta)
  %failed = icmp eq i32 %oldpages, -1
  br i1 %failed, label %fail, label %allocated
allocated:
  store i32 %end, ptr @tz.heap.end
  %header = inttoptr i32 %begin to ptr
  store i32 %needed, ptr %header
  %payload = getelementptr i8, ptr %header, i32 16
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
  %data = ptrtoint ptr %pointer to i32
  %block = sub i32 %data, 16
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
  %first = phi i32 [ %block, %start ], [ %earlier, %back ]
  %total = phi i32 [ %size, %start ], [ %joined, %back ]
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
  %flags = load i32, ptr %header
  %capacity = and i32 %flags, -16
  %enough = icmp ule i32 %needed, %capacity
  br i1 %enough, label %reused, label %adjacent
adjacent:
  %following = add i32 %block, %capacity
  %end = load i32, ptr @tz.heap.end
  %attop = icmp eq i32 %following, %end
  br i1 %attop, label %fallback, label %neighbor
neighbor:
  %np = inttoptr i32 %following to ptr
  %nflags = load i32, ptr %np
  %nfree = and i32 %nflags, 1
  %isfree = icmp ne i32 %nfree, 0
  br i1 %isfree, label %combine, label %fallback
combine:
  %nsize = and i32 %nflags, -16
  %combined = add i32 %capacity, %nsize
  %available = icmp uge i32 %combined, %needed
  br i1 %available, label %absorb, label %fallback
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
  br label %copy_test
copy_test:
  %index = phi i32 [ 0, %fallback ], [ %next_index, %copy ]
  %more = icmp ult i32 %index, %count
  br i1 %more, label %copy, label %copied
copy:
  %source = getelementptr i8, ptr %old, i32 %index
  %target = getelementptr i8, ptr %replacement, i32 %index
  %byte = load i8, ptr %source
  store i8 %byte, ptr %target
  %next_index = add i32 %index, 1
  br label %copy_test
copied:
  call void @tz.free(ptr %old)
  ret ptr %replacement
fail:
  call void @llvm.trap()
  unreachable
}
