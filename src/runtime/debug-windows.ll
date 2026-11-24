define internal void @tz.debug.quiet(ptr %expr, ptr %func, ptr %file, i32 %line, i64 %reserved) nounwind {
entry:
  ret void
}

define internal void @tz.debug.write(%tz.utf8string %text) nounwind {
entry:
  %line_buf = alloca [255 x i8], align 1
  %data = extractvalue %tz.utf8string %text, 0
  %length = extractvalue %tz.utf8string %text, 1
  %prev = call ptr @_set_thread_local_invalid_parameter_handler(ptr @tz.debug.quiet)
  %std_err = call ptr @GetStdHandle(i32 -12)
  %std_null = icmp eq ptr %std_err, null
  %std_inv_ptr = inttoptr i64 -1 to ptr
  %std_inv = icmp eq ptr %std_err, %std_inv_ptr
  %std_missing = or i1 %std_null, %std_inv
  %osfh = call i64 @_get_osfhandle(i32 2)
  %crt_missing = icmp slt i64 %osfh, 0
  %absent = or i1 %std_missing, %crt_missing
  br i1 %absent, label %fallback, label %body
body:
  %offset = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %offset, %length
  br i1 %done, label %newline, label %bytes
bytes:
  %pointer = getelementptr inbounds i8, ptr %data, i64 %offset
  %remaining = sub i64 %length, %offset
  %written = call i64 @write(i32 2, ptr %pointer, i64 %remaining)
  %progress = icmp sgt i64 %written, 0
  br i1 %progress, label %advance, label %restore_bad
advance:
  %next = add i64 %offset, %written
  br label %body
newline:
  %line = call i64 @write(i32 2, ptr @tz.debug.newline, i64 1)
  %line_ok = icmp eq i64 %line, 1
  br i1 %line_ok, label %finish, label %restore_bad
fallback:
  %restored_ok = call ptr @_set_thread_local_invalid_parameter_handler(ptr %prev)
  %empty_msg = icmp eq i64 %length, 0
  br i1 %empty_msg, label %fallback_nl_only, label %fallback_check_short
fallback_check_short:
  %short = icmp ule i64 %length, 254
  br i1 %short, label %fallback_short_copy, label %fallback_long
fallback_short_copy:
  br label %copy_loop
copy_loop:
  %idx = phi i64 [ 0, %fallback_short_copy ], [ %idx_next, %copy_body ]
  %copy_done = icmp eq i64 %idx, %length
  br i1 %copy_done, label %copy_finish, label %copy_body
copy_body:
  %src_p = getelementptr inbounds i8, ptr %data, i64 %idx
  %dst_p = getelementptr inbounds i8, ptr %line_buf, i64 %idx
  %ch = load i8, ptr %src_p, align 1
  store i8 %ch, ptr %dst_p, align 1
  %idx_next = add i64 %idx, 1
  br label %copy_loop
copy_finish:
  %nl_p = getelementptr inbounds i8, ptr %line_buf, i64 %length
  store i8 10, ptr %nl_p, align 1
  %total_len = add i64 %length, 1
  call void @tz.debugger.write(ptr %line_buf, i64 %total_len)
  br label %free_only
fallback_long:
  call void @tz.debugger.write(ptr %data, i64 %length)
  br label %fallback_nl_only
fallback_nl_only:
  call void @tz.debugger.write(ptr @tz.debug.newline, i64 1)
  br label %free_only
finish:
  %restored_done = call ptr @_set_thread_local_invalid_parameter_handler(ptr %prev)
  br label %free_only
free_only:
  call void @tz.free(ptr %data)
  ret void
restore_bad:
  %restored_bad = call ptr @_set_thread_local_invalid_parameter_handler(ptr %prev)
  br label %bad
bad:
  call void @llvm.trap()
  unreachable
}
