define internal %tz.closure @tz.closure.clone(%tz.closure %value) nounwind {
entry:
  %env = extractvalue %tz.closure %value, 1
  %empty = icmp eq ptr %env, null
  br i1 %empty, label %static, label %inspect
inspect:
  %desc = extractvalue %tz.closure %value, 0
  %destroy.slot = getelementptr inbounds %tz.closure.desc, ptr %desc, i32 0, i32 2
  %destroy = load ptr, ptr %destroy.slot
  %immediate = icmp eq ptr %destroy, null
  br i1 %immediate, label %static, label %copy
static:
  ret %tz.closure %value
copy:
  %clone.slot = getelementptr inbounds %tz.closure.desc, ptr %desc, i32 0, i32 1
  %clone = load ptr, ptr %clone.slot
  %new = call ptr %clone(ptr %env)
  %result = insertvalue %tz.closure %value, ptr %new, 1
  ret %tz.closure %result
}

define internal void @tz.closure.drop(%tz.closure %value) nounwind {
entry:
  %env = extractvalue %tz.closure %value, 1
  %empty = icmp eq ptr %env, null
  br i1 %empty, label %exit, label %inspect
inspect:
  %desc = extractvalue %tz.closure %value, 0
  %slot = getelementptr inbounds %tz.closure.desc, ptr %desc, i32 0, i32 2
  %destroy = load ptr, ptr %slot
  %immediate = icmp eq ptr %destroy, null
  br i1 %immediate, label %exit, label %drop
drop:
  call void %destroy(ptr %env)
  br label %exit
exit:
  ret void
}
