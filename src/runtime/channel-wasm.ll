; Channels on the default WASM target, which runs one thread (F10 Phase 2). The block is laid out as
; src/llvm_sync.rs does: the capacity, the size of an item, the index of the oldest item, the number
; of items, the live senders, the live receivers, the live handles of either kind, two words that the
; native runtime uses for its waiting lists, and then the ring of items at offset 80. No other thread
; can change a channel, so a wait that cannot end at once can never end: the status asks the caller
; to trap (2), which is what the other targets do when every task is waiting.

define internal void @tsuzuri_channel_copy(ptr %to, ptr %from, i64 %size) nounwind {
entry:
  br label %loop
loop:
  %position = phi i64 [ 0, %entry ], [ %next, %body ]
  %more = icmp ult i64 %position, %size
  br i1 %more, label %body, label %done
body:
  %source = getelementptr inbounds i8, ptr %from, i64 %position
  %byte = load i8, ptr %source
  %target = getelementptr inbounds i8, ptr %to, i64 %position
  store i8 %byte, ptr %target
  %next = add i64 %position, 1
  br label %loop
done:
  ret void
}

; The address of the ring slot that holds the item at `%offset` after the oldest one.
define internal ptr @tsuzuri_channel_slot(ptr %block, i64 %offset) nounwind {
entry:
  %head_at = getelementptr inbounds i8, ptr %block, i64 16
  %head = load i64, ptr %head_at
  %capacity = load i64, ptr %block
  %size_at = getelementptr inbounds i8, ptr %block, i64 8
  %size = load i64, ptr %size_at
  %zero = icmp eq i64 %size, 0
  %stride = select i1 %zero, i64 1, i64 %size
  %position = add i64 %head, %offset
  %index = urem i64 %position, %capacity
  %at = mul i64 %index, %stride
  %items = getelementptr inbounds i8, ptr %block, i64 80
  %slot = getelementptr inbounds i8, ptr %items, i64 %at
  ret ptr %slot
}

; 0: sent; 1: no receiver is left; 2: the channel is full and nobody could make room.
define internal i32 @tsuzuri_channel_send(ptr %block, ptr %item) nounwind {
entry:
  %receivers_at = getelementptr inbounds i8, ptr %block, i64 40
  %receivers = load i64, ptr %receivers_at
  %gone = icmp eq i64 %receivers, 0
  br i1 %gone, label %closed, label %open
closed:
  ret i32 1
open:
  %count_at = getelementptr inbounds i8, ptr %block, i64 24
  %count = load i64, ptr %count_at
  %capacity = load i64, ptr %block
  %full = icmp uge i64 %count, %capacity
  br i1 %full, label %stuck, label %room
stuck:
  ret i32 2
room:
  %size_at = getelementptr inbounds i8, ptr %block, i64 8
  %size = load i64, ptr %size_at
  %slot = call ptr @tsuzuri_channel_slot(ptr %block, i64 %count)
  call void @tsuzuri_channel_copy(ptr %slot, ptr %item, i64 %size)
  %next = add i64 %count, 1
  store i64 %next, ptr %count_at
  ret i32 0
}

; 0: received; 1: empty and no sender is left; 2: empty and nobody could send.
define internal i32 @tsuzuri_channel_recv(ptr %block, ptr %item) nounwind {
entry:
  %count_at = getelementptr inbounds i8, ptr %block, i64 24
  %count = load i64, ptr %count_at
  %empty = icmp eq i64 %count, 0
  br i1 %empty, label %none, label %some
none:
  %senders_at = getelementptr inbounds i8, ptr %block, i64 32
  %senders = load i64, ptr %senders_at
  %closed = icmp eq i64 %senders, 0
  %status = select i1 %closed, i32 1, i32 2
  ret i32 %status
some:
  %size_at = getelementptr inbounds i8, ptr %block, i64 8
  %size = load i64, ptr %size_at
  %slot = call ptr @tsuzuri_channel_slot(ptr %block, i64 0)
  call void @tsuzuri_channel_copy(ptr %item, ptr %slot, i64 %size)
  %head_at = getelementptr inbounds i8, ptr %block, i64 16
  %head = load i64, ptr %head_at
  %capacity = load i64, ptr %block
  %after = add i64 %head, 1
  %wrapped = urem i64 %after, %capacity
  store i64 %wrapped, ptr %head_at
  %left = sub i64 %count, 1
  store i64 %left, ptr %count_at
  ret i32 0
}

define internal void @tsuzuri_channel_clone_sender(ptr %block) nounwind {
entry:
  %senders_at = getelementptr inbounds i8, ptr %block, i64 32
  %senders = load i64, ptr %senders_at
  %more = add i64 %senders, 1
  store i64 %more, ptr %senders_at
  %owners_at = getelementptr inbounds i8, ptr %block, i64 48
  %owners = load i64, ptr %owners_at
  %handles = add i64 %owners, 1
  store i64 %handles, ptr %owners_at
  ret void
}

; Releases a sender (kind 0) or a receiver (kind 1). The result is 1 for the owner of the last
; handle, which drops the items that are left and frees the block.
define internal i32 @tsuzuri_channel_close(ptr %block, i32 %kind) nounwind {
entry:
  %receiver = icmp ne i32 %kind, 0
  %offset = select i1 %receiver, i64 40, i64 32
  %side_at = getelementptr inbounds i8, ptr %block, i64 %offset
  %side = load i64, ptr %side_at
  %fewer = sub i64 %side, 1
  store i64 %fewer, ptr %side_at
  %owners_at = getelementptr inbounds i8, ptr %block, i64 48
  %owners = load i64, ptr %owners_at
  %handles = sub i64 %owners, 1
  store i64 %handles, ptr %owners_at
  %last = icmp eq i64 %handles, 0
  %status = zext i1 %last to i32
  ret i32 %status
}
