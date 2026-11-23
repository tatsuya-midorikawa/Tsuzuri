@tz.debug.empty_wide = private unnamed_addr constant [1 x i16] zeroinitializer

define internal void @tz.debug.quiet(ptr %expr, ptr %func, ptr %file, i32 %line, i64 %reserved) nounwind {
entry:
  ret void
}

define internal void @tz.debug.write(%tz.utf8string %text) nounwind {
entry:
  %data = extractvalue %tz.utf8string %text, 0
  %length = extractvalue %tz.utf8string %text, 1
  %prev = call ptr @_set_thread_local_invalid_parameter_handler(ptr @tz.debug.quiet)
  br label %body
body:
  %offset = phi i64 [ 0, %entry ], [ %next, %advance ]
  %done = icmp eq i64 %offset, %length
  br i1 %done, label %newline, label %bytes
bytes:
  %pointer = getelementptr inbounds i8, ptr %data, i64 %offset
  %remaining = sub i64 %length, %offset
  %written = call i64 @write(i32 2, ptr %pointer, i64 %remaining)
  %progress = icmp sgt i64 %written, 0
  br i1 %progress, label %advance, label %write_failed
advance:
  %next = add i64 %offset, %written
  br label %body
newline:
  %line = call i64 @write(i32 2, ptr @tz.debug.newline, i64 1)
  %line_ok = icmp eq i64 %line, 1
  br i1 %line_ok, label %finish, label %write_failed
write_failed:
  %first_byte = icmp eq i64 %offset, 0
  br i1 %first_byte, label %check_absent, label %restore_bad
check_absent:
  %err_ptr = call ptr @_errno()
  store i32 0, ptr %err_ptr, align 4
  %osfh = call i64 @_get_osfhandle(i32 2)
  %no_console = icmp eq i64 %osfh, -2
  %neg_one = icmp eq i64 %osfh, -1
  %err_val = load i32, ptr %err_ptr, align 4
  %is_ebadf = icmp eq i32 %err_val, 9
  %closed_fd = and i1 %neg_one, %is_ebadf
  %absent = or i1 %no_console, %closed_fd
  br i1 %absent, label %fallback, label %restore_bad
fallback:
  %restored_ok = call ptr @_set_thread_local_invalid_parameter_handler(ptr %prev)
  %empty_msg = icmp eq i64 %length, 0
  br i1 %empty_msg, label %fallback_empty, label %fallback_nonempty
fallback_empty:
  call void @OutputDebugStringW(ptr @tz.debug.empty_wide)
  br label %free_only
fallback_nonempty:
  call void @tz.debugger.write(ptr %data, i64 %length)
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
