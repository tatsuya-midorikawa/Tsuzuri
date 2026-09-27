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

define internal i32 @tz.utf8string.compare(%tz.utf8string %left, %tz.utf8string %right) nounwind {
entry:
  %left_data = extractvalue %tz.utf8string %left, 0
  %left_length = extractvalue %tz.utf8string %left, 1
  %right_data = extractvalue %tz.utf8string %right, 0
  %right_length = extractvalue %tz.utf8string %right, 1
  %shorter = icmp ult i64 %left_length, %right_length
  %length = select i1 %shorter, i64 %left_length, i64 %right_length
  br label %test
test:
  %index = phi i64 [ 0, %entry ], [ %wide_next, %wide_equal ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %prefix, label %width
width:
  %remaining = sub i64 %length, %index
  %wide = icmp uge i64 %remaining, 8
  br i1 %wide, label %compare_wide, label %scalar
compare_wide:
  %left_pointer = getelementptr inbounds i8, ptr %left_data, i64 %index
  %right_pointer = getelementptr inbounds i8, ptr %right_data, i64 %index
  %left_word = load i64, ptr %left_pointer, align 1
  %right_word = load i64, ptr %right_pointer, align 1
  %words_equal = icmp eq i64 %left_word, %right_word
  br i1 %words_equal, label %wide_equal, label %scalar
wide_equal:
  %wide_next = add i64 %index, 8
  br label %test
scalar:
  %offset = phi i64 [ %index, %width ], [ %index, %compare_wide ], [ %next, %equal ]
  %end = icmp eq i64 %offset, %length
  br i1 %end, label %prefix, label %compare
compare:
  %left_at = getelementptr inbounds i8, ptr %left_data, i64 %offset
  %right_at = getelementptr inbounds i8, ptr %right_data, i64 %offset
  %left_byte = load i8, ptr %left_at
  %right_byte = load i8, ptr %right_at
  %same = icmp eq i8 %left_byte, %right_byte
  br i1 %same, label %equal, label %different
equal:
  %next = add i64 %offset, 1
  br label %scalar
different:
  %less = icmp ult i8 %left_byte, %right_byte
  %order = select i1 %less, i32 -1, i32 1
  ret i32 %order
prefix:
  %same_length = icmp eq i64 %left_length, %right_length
  %length_order = select i1 %shorter, i32 -1, i32 1
  %result = select i1 %same_length, i32 0, i32 %length_order
  ret i32 %result
}
