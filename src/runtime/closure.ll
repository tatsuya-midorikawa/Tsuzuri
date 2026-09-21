define internal %tz.closure @tz.closure.clone(%tz.closure %value) nounwind {
entry:
  %env = extractvalue %tz.closure %value, 1
  %empty = icmp eq ptr %env, null
  br i1 %empty, label %static, label %copy
static:
  ret %tz.closure %value
copy:
  %clone = extractvalue %tz.closure %value, 2
  %new = call ptr %clone(ptr %env)
  %result = insertvalue %tz.closure %value, ptr %new, 1
  ret %tz.closure %result
}

define internal void @tz.closure.drop(%tz.closure %value) nounwind {
entry:
  %env = extractvalue %tz.closure %value, 1
  %empty = icmp eq ptr %env, null
  br i1 %empty, label %exit, label %drop
drop:
  %destroy = extractvalue %tz.closure %value, 3
  call void %destroy(ptr %env)
  br label %exit
exit:
  ret void
}
