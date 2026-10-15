define internal void @tz.utf8string.copy(ptr %to, ptr %from, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %copy ]
  %done = icmp eq i64 %i, %length
  br i1 %done, label %exit, label %copy
copy:
  %source = getelementptr inbounds i8, ptr %from, i64 %i
  %destination = getelementptr inbounds i8, ptr %to, i64 %i
  %byte = load i8, ptr %source
  store i8 %byte, ptr %destination
  %next = add i64 %i, 1
  br label %loop
exit:
  ret void
}

define internal %tz.utf8string @tz.utf8string.allocate(i64 %length) nounwind {
entry:
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %zero, label %allocate
zero:
  ret %tz.utf8string zeroinitializer
allocate:
  %data = call ptr @tz.alloc(i64 %length)
  %a = insertvalue %tz.utf8string zeroinitializer, ptr %data, 0
  %b = insertvalue %tz.utf8string %a, i64 %length, 1
  ret %tz.utf8string %b
}

define internal %tz.utf8string @tz.utf8string.new(ptr %source, i64 %length) nounwind {
entry:
  %result = call %tz.utf8string @tz.utf8string.allocate(i64 %length)
  %data = extractvalue %tz.utf8string %result, 0
  call void @tz.utf8string.copy(ptr %data, ptr %source, i64 %length)
  ret %tz.utf8string %result
}
define internal %tz.utf8string @tz.utf8string.concat(%tz.utf8string %left, %tz.utf8string %right) nounwind {
entry:
  %a = extractvalue %tz.utf8string %left, 0
  %an = extractvalue %tz.utf8string %left, 1
  %b = extractvalue %tz.utf8string %right, 0
  %bn = extractvalue %tz.utf8string %right, 1
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %allocate
fail:
  call void @llvm.trap()
  unreachable
allocate:
  %result = call %tz.utf8string @tz.utf8string.allocate(i64 %length)
  %data = extractvalue %tz.utf8string %result, 0
  call void @tz.utf8string.copy(ptr %data, ptr %a, i64 %an)
  %second = getelementptr i8, ptr %data, i64 %an
  call void @tz.utf8string.copy(ptr %second, ptr %b, i64 %bn)
  ret %tz.utf8string %result
}

define internal %tz.utf8string @tz.utf8string.append(%tz.utf8string %left, %tz.utf8string %right) nounwind {
entry:
  %a = extractvalue %tz.utf8string %left, 0
  %an = extractvalue %tz.utf8string %left, 1
  %b = extractvalue %tz.utf8string %right, 0
  %bn = extractvalue %tz.utf8string %right, 1
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %grow
fail:
  call void @llvm.trap()
  unreachable
grow:
  %data = call ptr @tz.realloc(ptr %a, i64 %an, i64 %length)
  %second = getelementptr i8, ptr %data, i64 %an
  call void @tz.utf8string.copy(ptr %second, ptr %b, i64 %bn)
  %result0 = insertvalue %tz.utf8string zeroinitializer, ptr %data, 0
  %result = insertvalue %tz.utf8string %result0, i64 %length, 1
  ret %tz.utf8string %result
}

define internal %tz.utf8string @tz.utf8string.concat_into(ptr %buffer, i64 %capacity, %tz.utf8string %left, %tz.utf8string %right) nounwind {
entry:
  %a = extractvalue %tz.utf8string %left, 0
  %an = extractvalue %tz.utf8string %left, 1
  %b = extractvalue %tz.utf8string %right, 0
  %bn = extractvalue %tz.utf8string %right, 1
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  br i1 %overflow, label %fail, label %size
fail:
  call void @llvm.trap()
  unreachable
size:
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %zero, label %choose
zero:
  ret %tz.utf8string zeroinitializer
choose:
  %fits = icmp ule i64 %length, %capacity
  br i1 %fits, label %stack, label %heap
stack:
  br label %copy
heap:
  %allocated = call %tz.utf8string @tz.utf8string.allocate(i64 %length)
  %heapdata = extractvalue %tz.utf8string %allocated, 0
  br label %copy
copy:
  %data = phi ptr [ %buffer, %stack ], [ %heapdata, %heap ]
  call void @tz.utf8string.copy(ptr %data, ptr %a, i64 %an)
  %second = getelementptr i8, ptr %data, i64 %an
  call void @tz.utf8string.copy(ptr %second, ptr %b, i64 %bn)
  %result0 = insertvalue %tz.utf8string zeroinitializer, ptr %data, 0
  %result = insertvalue %tz.utf8string %result0, i64 %length, 1
  ret %tz.utf8string %result
}
