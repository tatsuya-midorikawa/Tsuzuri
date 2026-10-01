@__heap_base = external global i8
@tz.heap.end = internal global i64 0
@tz.heap.free = internal global i64 0
declare i64 @llvm.wasm.memory.size.i64(i32)
declare i64 @llvm.wasm.memory.grow.i64(i32, i64)

; heap-wasm.ll for 64-bit memory: the 16-byte header holds an i64 capacity and an i64 next link.
define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %fits = icmp ule i64 %size, 16777184
  br i1 %fits, label %start, label %fail
start:
  %rounded = add i64 %size, 31
  %needed = and i64 %rounded, -16
  %head = load i64, ptr @tz.heap.free
  br label %search
search:
  %block = phi i64 [ %head, %start ], [ %next, %advance ]
  %previous = phi ptr [ @tz.heap.free, %start ], [ %nextp, %advance ]
  %empty = icmp eq i64 %block, 0
  br i1 %empty, label %bump, label %inspect
inspect:
  %p = inttoptr i64 %block to ptr
  %capacity = load i64, ptr %p
  %nextp = getelementptr i8, ptr %p, i64 8
  %next = load i64, ptr %nextp
  %enough = icmp uge i64 %capacity, %needed
  br i1 %enough, label %reuse, label %advance
advance:
  br label %search
reuse:
  %remaining = sub i64 %capacity, %needed
  %split = icmp uge i64 %remaining, 32
  br i1 %split, label %splitblock, label %whole
splitblock:
  %rest = add i64 %block, %needed
  %restp = inttoptr i64 %rest to ptr
  store i64 %remaining, ptr %restp
  %restnext = getelementptr i8, ptr %restp, i64 8
  store i64 %next, ptr %restnext
  store i64 %rest, ptr %previous
  store i64 %needed, ptr %p
  br label %reused
whole:
  store i64 %next, ptr %previous
  br label %reused
reused:
  %data = getelementptr i8, ptr %p, i64 16
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
  %size = load i64, ptr %p
  %link = getelementptr i8, ptr %p, i64 8
  %head = load i64, ptr @tz.heap.free
  br label %search
search:
  %next = phi i64 [ %head, %start ], [ %after, %advance ]
  %previous = phi i64 [ 0, %start ], [ %next, %advance ]
  %previouslink = phi ptr [ @tz.heap.free, %start ], [ %afterp, %advance ]
  %last = icmp eq i64 %next, 0
  %later = icmp ugt i64 %next, %block
  %insert = or i1 %last, %later
  br i1 %insert, label %inserting, label %advance
advance:
  %nextp = inttoptr i64 %next to ptr
  %afterp = getelementptr i8, ptr %nextp, i64 8
  %after = load i64, ptr %afterp
  br label %search
inserting:
  store i64 %next, ptr %link
  store i64 %block, ptr %previouslink
  %end = add i64 %block, %size
  %adjacent = icmp eq i64 %end, %next
  br i1 %adjacent, label %joinnext, label %checkprevious
joinnext:
  %np = inttoptr i64 %next to ptr
  %ns = load i64, ptr %np
  %nl = getelementptr i8, ptr %np, i64 8
  %nn = load i64, ptr %nl
  %combined = add i64 %size, %ns
  store i64 %combined, ptr %p
  store i64 %nn, ptr %link
  br label %checkprevious
checkprevious:
  %hasprevious = icmp ne i64 %previous, 0
  br i1 %hasprevious, label %inspectprevious, label %exit
inspectprevious:
  %pp = inttoptr i64 %previous to ptr
  %ps = load i64, ptr %pp
  %pe = add i64 %previous, %ps
  %touches = icmp eq i64 %pe, %block
  br i1 %touches, label %joinprevious, label %exit
joinprevious:
  %current = load i64, ptr %p
  %following = load i64, ptr %link
  %total = add i64 %ps, %current
  store i64 %total, ptr %pp
  store i64 %following, ptr %previouslink
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
  %needed = and i64 %rounded, -16
  %payload = ptrtoint ptr %old to i64
  %block = sub i64 %payload, 16
  %header = inttoptr i64 %block to ptr
  %capacity = load i64, ptr %header
  %enough = icmp ule i64 %needed, %capacity
  br i1 %enough, label %reused, label %adjacent
adjacent:
  %wanted = add i64 %block, %capacity
  %head = load i64, ptr @tz.heap.free
  br label %search
search:
  %current = phi i64 [ %head, %adjacent ], [ %following, %advance ]
  %previous = phi ptr [ @tz.heap.free, %adjacent ], [ %next_link, %advance ]
  %found = icmp eq i64 %current, %wanted
  br i1 %found, label %combine, label %before
before:
  %empty = icmp eq i64 %current, 0
  %later = icmp ugt i64 %current, %wanted
  %missing = or i1 %empty, %later
  br i1 %missing, label %fallback, label %advance
advance:
  %current_header = inttoptr i64 %current to ptr
  %next_link = getelementptr i8, ptr %current_header, i64 8
  %following = load i64, ptr %next_link
  br label %search
combine:
  %neighbor = inttoptr i64 %current to ptr
  %neighbor_size = load i64, ptr %neighbor
  %combined = add i64 %capacity, %neighbor_size
  %available = icmp uge i64 %combined, %needed
  br i1 %available, label %unlink, label %fallback
unlink:
  %neighbor_link = getelementptr i8, ptr %neighbor, i64 8
  %neighbor_next = load i64, ptr %neighbor_link
  %remaining = sub i64 %combined, %needed
  %split = icmp uge i64 %remaining, 32
  br i1 %split, label %split_block, label %whole
split_block:
  %rest = add i64 %block, %needed
  %rest_header = inttoptr i64 %rest to ptr
  store i64 %remaining, ptr %rest_header
  %rest_link = getelementptr i8, ptr %rest_header, i64 8
  store i64 %neighbor_next, ptr %rest_link
  store i64 %rest, ptr %previous
  store i64 %needed, ptr %header
  br label %reused
whole:
  store i64 %neighbor_next, ptr %previous
  store i64 %combined, ptr %header
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
