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

define internal i1 @tz.utf8string.equal(%tz.utf8string %left, %tz.utf8string %right) nounwind {
entry:
  %a = extractvalue %tz.utf8string %left, 0
  %an = extractvalue %tz.utf8string %left, 1
  %b = extractvalue %tz.utf8string %right, 0
  %bn = extractvalue %tz.utf8string %right, 1
  %same = icmp eq i64 %an, %bn
  br i1 %same, label %loop, label %different
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %advance ], [ %wide_next, %wide_advance ]
  %done = icmp eq i64 %i, %an
  br i1 %done, label %equal, label %probe
probe:
  %remaining = sub i64 %an, %i
  %enough = icmp uge i64 %remaining, 8
  br i1 %enough, label %wide, label %compare
wide:
  %wide_ap = getelementptr inbounds i8, ptr %a, i64 %i
  %wide_bp = getelementptr inbounds i8, ptr %b, i64 %i
  %wide_av = load i64, ptr %wide_ap, align 1
  %wide_bv = load i64, ptr %wide_bp, align 1
  %wide_match = icmp eq i64 %wide_av, %wide_bv
  br i1 %wide_match, label %wide_advance, label %different
wide_advance:
  %wide_next = add i64 %i, 8
  br label %loop
compare:
  %ap = getelementptr inbounds i8, ptr %a, i64 %i
  %bp = getelementptr inbounds i8, ptr %b, i64 %i
  %av = load i8, ptr %ap
  %bv = load i8, ptr %bp
  %match = icmp eq i8 %av, %bv
  br i1 %match, label %advance, label %different
advance:
  %next = add i64 %i, 1
  br label %loop
equal:
  ret i1 true
different:
  ret i1 false
}
