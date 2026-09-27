define internal %tz.string @tz.display.join(%tz.array %parts, i32 %kind) nounwind {
entry:
  %data = extractvalue %tz.array %parts, 0
  %length = extractvalue %tz.array %parts, 1
  %tuple = icmp eq i32 %kind, 0
  %linked = icmp eq i32 %kind, 2
  %open = select i1 %tuple, i16 40, i16 91
  %close = select i1 %tuple, i16 41, i16 93
  %overhead = select i1 %linked, i64 4, i64 2
  br label %size_test
size_test:
  %size_index = phi i64 [ 0, %entry ], [ %size_next, %size_advance ]
  %total = phi i64 [ %overhead, %entry ], [ %total_next, %size_advance ]
  %size_done = icmp eq i64 %size_index, %length
  br i1 %size_done, label %allocate, label %size_body
size_body:
  %size_pointer = getelementptr inbounds %tz.string, ptr %data, i64 %size_index
  %size_part = load %tz.string, ptr %size_pointer
  %size_length = extractvalue %tz.string %size_part, 1
  %first = icmp eq i64 %size_index, 0
  %separator = select i1 %first, i64 0, i64 2
  %available = sub i64 9007199254740991, %total
  %with_separator = icmp uge i64 %available, %separator
  %space = sub i64 %available, %separator
  %fits_part = icmp ule i64 %size_length, %space
  %fits = and i1 %with_separator, %fits_part
  br i1 %fits, label %size_advance, label %fail
size_advance:
  %separated = add i64 %total, %separator
  %total_next = add i64 %separated, %size_length
  %size_next = add i64 %size_index, 1
  br label %size_test
fail:
  call void @llvm.trap()
  unreachable
allocate:
  %result = call %tz.string @tz.string.allocate(i64 %total)
  %output = extractvalue %tz.string %result, 0
  store i16 %open, ptr %output
  br i1 %linked, label %open_bar, label %ready
open_bar:
  %bar_pointer = getelementptr inbounds i16, ptr %output, i64 1
  store i16 124, ptr %bar_pointer
  br label %ready
ready:
  %start = select i1 %linked, i64 2, i64 1
  br label %write_test
write_test:
  %index = phi i64 [ 0, %ready ], [ %next, %copy ]
  %position = phi i64 [ %start, %ready ], [ %next_position, %copy ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %finish, label %write_body
write_body:
  %part_pointer = getelementptr inbounds %tz.string, ptr %data, i64 %index
  %part = load %tz.string, ptr %part_pointer
  %part_data = extractvalue %tz.string %part, 0
  %part_length = extractvalue %tz.string %part, 1
  %write_first = icmp eq i64 %index, 0
  br i1 %write_first, label %copy, label %separate
separate:
  %comma_pointer = getelementptr inbounds i16, ptr %output, i64 %position
  store i16 44, ptr %comma_pointer
  %space_position = add i64 %position, 1
  %space_pointer = getelementptr inbounds i16, ptr %output, i64 %space_position
  store i16 32, ptr %space_pointer
  %after_separator = add i64 %position, 2
  br label %copy
copy:
  %part_start = phi i64 [ %position, %write_body ], [ %after_separator, %separate ]
  %target = getelementptr inbounds i16, ptr %output, i64 %part_start
  call void @tz.string.copy(ptr %target, ptr %part_data, i64 %part_length)
  call void @tz.free(ptr %part_data)
  %next_position = add i64 %part_start, %part_length
  %next = add i64 %index, 1
  br label %write_test
finish:
  br i1 %linked, label %close_bar, label %closing
close_bar:
  %last_bar = getelementptr inbounds i16, ptr %output, i64 %position
  store i16 124, ptr %last_bar
  br label %closing
closing:
  %bar_width = select i1 %linked, i64 1, i64 0
  %close_position = add i64 %position, %bar_width
  %close_pointer = getelementptr inbounds i16, ptr %output, i64 %close_position
  store i16 %close, ptr %close_pointer
  call void @tz.free(ptr %data)
  ret %tz.string %result
}

define internal i32 @tz.display.escape(ptr %data, i64 %length, i64 %index, i16 %unit, i16 %quote) nounwind {
entry:
  %quoted = icmp eq i16 %unit, %quote
  %slash = icmp eq i16 %unit, 92
  %literal_escape = or i1 %quoted, %slash
  br i1 %literal_escape, label %literal, label %control
literal:
  %escaped = zext i16 %unit to i32
  ret i32 %escaped
control:
  switch i16 %unit, label %other [ i16 0, label %zero i16 9, label %tab i16 10, label %newline i16 13, label %return ]
zero:
  ret i32 48
tab:
  ret i32 116
newline:
  ret i32 110
return:
  ret i32 114
other:
  %small = icmp ult i16 %unit, 32
  %delete = icmp eq i16 %unit, 127
  %control_unit = or i1 %small, %delete
  br i1 %control_unit, label %unicode, label %surrogate
surrogate:
  %offset = sub i16 %unit, 55296
  %surrogate_unit = icmp ult i16 %offset, 2048
  br i1 %surrogate_unit, label %pair, label %plain
pair:
  %high = icmp ult i16 %offset, 1024
  br i1 %high, label %next_check, label %previous_check
next_check:
  %next = add i64 %index, 1
  %has_next = icmp ult i64 %next, %length
  br i1 %has_next, label %next_read, label %unicode
next_read:
  %next_pointer = getelementptr inbounds i16, ptr %data, i64 %next
  %next_unit = load i16, ptr %next_pointer
  %next_offset = sub i16 %next_unit, 56320
  %next_low = icmp ult i16 %next_offset, 1024
  br i1 %next_low, label %plain, label %unicode
previous_check:
  %has_previous = icmp ne i64 %index, 0
  br i1 %has_previous, label %previous_read, label %unicode
previous_read:
  %previous = sub i64 %index, 1
  %previous_pointer = getelementptr inbounds i16, ptr %data, i64 %previous
  %previous_unit = load i16, ptr %previous_pointer
  %previous_offset = sub i16 %previous_unit, 55296
  %previous_high = icmp ult i16 %previous_offset, 1024
  br i1 %previous_high, label %plain, label %unicode
plain:
  ret i32 0
unicode:
  ret i32 256
}

define internal %tz.string @tz.display.quote(ptr %input, i64 %length, i32 %kind) nounwind {
entry:
  %prefix_bit = and i32 %kind, 1
  %prefixed = icmp ne i32 %prefix_bit, 0
  %character = icmp uge i32 %kind, 2
  %quote = select i1 %character, i16 39, i16 34
  %prefix = select i1 %prefixed, i64 2, i64 0
  %overhead = add i64 %prefix, 2
  %unicode_length = select i1 %prefixed, i64 8, i64 6
  br label %size_test
size_test:
  %size_index = phi i64 [ 0, %entry ], [ %size_next, %size_advance ]
  %total = phi i64 [ %overhead, %entry ], [ %total_next, %size_advance ]
  %size_done = icmp eq i64 %size_index, %length
  br i1 %size_done, label %allocate, label %size_body
size_body:
  %size_pointer = getelementptr inbounds i16, ptr %input, i64 %size_index
  %size_unit = load i16, ptr %size_pointer
  %size_escape = call i32 @tz.display.escape(ptr %input, i64 %length, i64 %size_index, i16 %size_unit, i16 %quote)
  %size_plain = icmp eq i32 %size_escape, 0
  %size_unicode = icmp eq i32 %size_escape, 256
  %size_escaped = select i1 %size_unicode, i64 %unicode_length, i64 2
  %size_count = select i1 %size_plain, i64 1, i64 %size_escaped
  %available = sub i64 9007199254740991, %total
  %fits = icmp ule i64 %size_count, %available
  br i1 %fits, label %size_advance, label %fail
size_advance:
  %total_next = add i64 %total, %size_count
  %size_next = add i64 %size_index, 1
  br label %size_test
fail:
  call void @llvm.trap()
  unreachable
allocate:
  %result = call %tz.string @tz.string.allocate(i64 %total)
  %output = extractvalue %tz.string %result, 0
  br i1 %prefixed, label %write_prefix, label %open
write_prefix:
  store i16 117, ptr %output
  %eight_pointer = getelementptr inbounds i16, ptr %output, i64 1
  store i16 56, ptr %eight_pointer
  br label %open
open:
  %quote_pointer = getelementptr inbounds i16, ptr %output, i64 %prefix
  store i16 %quote, ptr %quote_pointer
  %start = add i64 %prefix, 1
  br label %write_test
write_test:
  %index = phi i64 [ 0, %open ], [ %next_index, %advance ]
  %position = phi i64 [ %start, %open ], [ %next_position, %advance ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %close, label %write_body
write_body:
  %source = getelementptr inbounds i16, ptr %input, i64 %index
  %unit = load i16, ptr %source
  %escape = call i32 @tz.display.escape(ptr %input, i64 %length, i64 %index, i16 %unit, i16 %quote)
  %target = getelementptr inbounds i16, ptr %output, i64 %position
  %plain = icmp eq i32 %escape, 0
  br i1 %plain, label %write_plain, label %write_escape
write_plain:
  store i16 %unit, ptr %target
  br label %advance
write_escape:
  store i16 92, ptr %target
  %letter_position = add i64 %position, 1
  %letter_pointer = getelementptr inbounds i16, ptr %output, i64 %letter_position
  %unicode = icmp eq i32 %escape, 256
  br i1 %unicode, label %hex_start, label %write_short
write_short:
  %letter = trunc i32 %escape to i16
  store i16 %letter, ptr %letter_pointer
  br label %advance
hex_start:
  store i16 117, ptr %letter_pointer
  %after_letter = add i64 %position, 2
  br i1 %prefixed, label %hex_open, label %hex_ready
hex_open:
  %brace_pointer = getelementptr inbounds i16, ptr %output, i64 %after_letter
  store i16 123, ptr %brace_pointer
  br label %hex_ready
hex_ready:
  %hex_offset = select i1 %prefixed, i64 3, i64 2
  %hex_base = add i64 %position, %hex_offset
  br label %hex_test
hex_test:
  %digit_index = phi i16 [ 0, %hex_ready ], [ %next_digit, %hex_body ]
  %hex_finished = icmp eq i16 %digit_index, 4
  br i1 %hex_finished, label %hex_finish, label %hex_body
hex_body:
  %shifted_index = mul i16 %digit_index, 4
  %shift = sub i16 12, %shifted_index
  %shifted = lshr i16 %unit, %shift
  %digit = and i16 %shifted, 15
  %number = icmp ult i16 %digit, 10
  %numeric = add i16 %digit, 48
  %alphabetic = add i16 %digit, 55
  %hex_character = select i1 %number, i16 %numeric, i16 %alphabetic
  %digit_wide = zext i16 %digit_index to i64
  %digit_position = add i64 %hex_base, %digit_wide
  %digit_pointer = getelementptr inbounds i16, ptr %output, i64 %digit_position
  store i16 %hex_character, ptr %digit_pointer
  %next_digit = add i16 %digit_index, 1
  br label %hex_test
hex_finish:
  br i1 %prefixed, label %hex_close, label %hex_done
hex_close:
  %close_position = add i64 %position, 7
  %close_pointer = getelementptr inbounds i16, ptr %output, i64 %close_position
  store i16 125, ptr %close_pointer
  br label %hex_done
hex_done:
  br label %advance
advance:
  %count = phi i64 [ 1, %write_plain ], [ 2, %write_short ], [ %unicode_length, %hex_done ]
  %next_index = add i64 %index, 1
  %next_position = add i64 %position, %count
  br label %write_test
close:
  %end_pointer = getelementptr inbounds i16, ptr %output, i64 %position
  store i16 %quote, ptr %end_pointer
  ret %tz.string %result
}