@__heap_base = external global i8
@tz.heap.end = internal global i32 0
@tz.heap.free = internal global i32 0
declare i32 @llvm.wasm.memory.size.i32(i32)
declare i32 @llvm.wasm.memory.grow.i32(i32, i32)

; Address-ordered free blocks have a 16-byte header and coalesce on release.
define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %fits = icmp ule i64 %size, 16777184
  br i1 %fits, label %start, label %fail
start:
  %small = trunc i64 %size to i32
  %rounded = add i32 %small, 31
  %needed = and i32 %rounded, -16
  %head = load i32, ptr @tz.heap.free
  br label %search
search:
  %block = phi i32 [ %head, %start ], [ %next, %advance ]
  %previous = phi ptr [ @tz.heap.free, %start ], [ %nextp, %advance ]
  %empty = icmp eq i32 %block, 0
  br i1 %empty, label %bump, label %inspect
inspect:
  %p = inttoptr i32 %block to ptr
  %capacity = load i32, ptr %p
  %nextp = getelementptr i8, ptr %p, i32 4
  %next = load i32, ptr %nextp
  %enough = icmp uge i32 %capacity, %needed
  br i1 %enough, label %reuse, label %advance
advance:
  br label %search
reuse:
  %remaining = sub i32 %capacity, %needed
  %split = icmp uge i32 %remaining, 32
  br i1 %split, label %splitblock, label %whole
splitblock:
  %rest = add i32 %block, %needed
  %restp = inttoptr i32 %rest to ptr
  store i32 %remaining, ptr %restp
  %restnext = getelementptr i8, ptr %restp, i32 4
  store i32 %next, ptr %restnext
  store i32 %rest, ptr %previous
  store i32 %needed, ptr %p
  br label %reused
whole:
  store i32 %next, ptr %previous
  br label %reused
reused:
  %data = getelementptr i8, ptr %p, i32 16
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
  %size = load i32, ptr %p
  %link = getelementptr i8, ptr %p, i32 4
  %head = load i32, ptr @tz.heap.free
  br label %search
search:
  %next = phi i32 [ %head, %start ], [ %after, %advance ]
  %previous = phi i32 [ 0, %start ], [ %next, %advance ]
  %previouslink = phi ptr [ @tz.heap.free, %start ], [ %afterp, %advance ]
  %last = icmp eq i32 %next, 0
  %later = icmp ugt i32 %next, %block
  %insert = or i1 %last, %later
  br i1 %insert, label %inserting, label %advance
advance:
  %nextp = inttoptr i32 %next to ptr
  %afterp = getelementptr i8, ptr %nextp, i32 4
  %after = load i32, ptr %afterp
  br label %search
inserting:
  store i32 %next, ptr %link
  store i32 %block, ptr %previouslink
  %end = add i32 %block, %size
  %adjacent = icmp eq i32 %end, %next
  br i1 %adjacent, label %joinnext, label %checkprevious
joinnext:
  %np = inttoptr i32 %next to ptr
  %ns = load i32, ptr %np
  %nl = getelementptr i8, ptr %np, i32 4
  %nn = load i32, ptr %nl
  %combined = add i32 %size, %ns
  store i32 %combined, ptr %p
  store i32 %nn, ptr %link
  br label %checkprevious
checkprevious:
  %hasprevious = icmp ne i32 %previous, 0
  br i1 %hasprevious, label %inspectprevious, label %exit
inspectprevious:
  %pp = inttoptr i32 %previous to ptr
  %ps = load i32, ptr %pp
  %pe = add i32 %previous, %ps
  %touches = icmp eq i32 %pe, %block
  br i1 %touches, label %joinprevious, label %exit
joinprevious:
  %current = load i32, ptr %p
  %following = load i32, ptr %link
  %total = add i32 %ps, %current
  store i32 %total, ptr %pp
  store i32 %following, ptr %previouslink
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
  %needed = and i32 %rounded, -16
  %payload = ptrtoint ptr %old to i32
  %block = sub i32 %payload, 16
  %header = inttoptr i32 %block to ptr
  %capacity = load i32, ptr %header
  %enough = icmp ule i32 %needed, %capacity
  br i1 %enough, label %reused, label %adjacent
adjacent:
  %wanted = add i32 %block, %capacity
  %head = load i32, ptr @tz.heap.free
  br label %search
search:
  %current = phi i32 [ %head, %adjacent ], [ %following, %advance ]
  %previous = phi ptr [ @tz.heap.free, %adjacent ], [ %next_link, %advance ]
  %found = icmp eq i32 %current, %wanted
  br i1 %found, label %combine, label %before
before:
  %empty = icmp eq i32 %current, 0
  %later = icmp ugt i32 %current, %wanted
  %missing = or i1 %empty, %later
  br i1 %missing, label %fallback, label %advance
advance:
  %current_header = inttoptr i32 %current to ptr
  %next_link = getelementptr i8, ptr %current_header, i32 4
  %following = load i32, ptr %next_link
  br label %search
combine:
  %neighbor = inttoptr i32 %current to ptr
  %neighbor_size = load i32, ptr %neighbor
  %combined = add i32 %capacity, %neighbor_size
  %available = icmp uge i32 %combined, %needed
  br i1 %available, label %unlink, label %fallback
unlink:
  %neighbor_link = getelementptr i8, ptr %neighbor, i32 4
  %neighbor_next = load i32, ptr %neighbor_link
  %remaining = sub i32 %combined, %needed
  %split = icmp uge i32 %remaining, 32
  br i1 %split, label %split_block, label %whole
split_block:
  %rest = add i32 %block, %needed
  %rest_header = inttoptr i32 %rest to ptr
  store i32 %remaining, ptr %rest_header
  %rest_link = getelementptr i8, ptr %rest_header, i32 4
  store i32 %neighbor_next, ptr %rest_link
  store i32 %rest, ptr %previous
  store i32 %needed, ptr %header
  br label %reused
whole:
  store i32 %neighbor_next, ptr %previous
  store i32 %combined, ptr %header
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
