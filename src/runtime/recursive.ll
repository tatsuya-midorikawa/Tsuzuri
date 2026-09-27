%tz.rec.header = type { ptr, ptr, ptr }

define internal void @tz.rec.enqueue(ptr %node, ptr %pending) nounwind {
entry:
  %empty = icmp eq ptr %node, null
  br i1 %empty, label %done, label %push
push:
  %head = load ptr, ptr %pending
  store ptr %head, ptr %node
  store ptr %node, ptr %pending
  br label %done
done:
  ret void
}

define internal void @tz.rec.drop(ptr %value) nounwind {
entry:
  %pending = alloca ptr
  store ptr null, ptr %pending
  call void @tz.rec.enqueue(ptr %value, ptr %pending)
  br label %loop
loop:
  %node = load ptr, ptr %pending
  %empty = icmp eq ptr %node, null
  br i1 %empty, label %done, label %visit
visit:
  %next = load ptr, ptr %node
  store ptr %next, ptr %pending
  %slot = getelementptr inbounds %tz.rec.header, ptr %node, i32 0, i32 1
  %action = load ptr, ptr %slot
  call void %action(ptr %node, ptr %pending)
  br label %loop
done:
  ret void
}

define internal ptr @tz.rec.clone.enqueue(ptr %source, ptr %pending) nounwind {
entry:
  %empty = icmp eq ptr %source, null
  br i1 %empty, label %none, label %copy
none:
  ret ptr null
copy:
  %slot = getelementptr inbounds %tz.rec.header, ptr %source, i32 0, i32 2
  %action = load ptr, ptr %slot
  %value = call ptr %action(ptr %source, ptr %pending)
  ret ptr %value
}

define internal ptr @tz.rec.clone(ptr %source) nounwind {
entry:
  %pending = alloca ptr
  store ptr null, ptr %pending
  %root = call ptr @tz.rec.clone.enqueue(ptr %source, ptr %pending)
  br label %loop
loop:
  %node = load ptr, ptr %pending
  %empty = icmp eq ptr %node, null
  br i1 %empty, label %done, label %visit
visit:
  %next = load ptr, ptr %node
  store ptr %next, ptr %pending
  %action_slot = getelementptr inbounds %tz.rec.header, ptr %node, i32 0, i32 1
  %source_slot = getelementptr inbounds %tz.rec.header, ptr %node, i32 0, i32 2
  %action = load ptr, ptr %action_slot
  %original = load ptr, ptr %source_slot
  call void %action(ptr %node, ptr %original, ptr %pending)
  br label %loop
done:
  ret ptr %root
}