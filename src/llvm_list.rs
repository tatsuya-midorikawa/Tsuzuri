//! The representation of a list `[|T|]`: the emitters that create, walk, index, skip, grow, shrink,
//! copy and free its storage live here, and the rest of the emitter calls them without knowing the
//! layout.
//!
//! A list is `%tz.list = { ptr, i64 }`: the first chunk of its storage and its length. The empty
//! list is `zeroinitializer`. A chunk of `T` is `{ ptr next, i64 rest, i64 capacity, [0 x T] slots }`
//! in one allocation; the header is 24 bytes on every target and the slots follow it, aligned like
//! `T`. For a list `(c, len)`, the list order is the address order inside a chunk and the chunks
//! follow each other in list order:
//!
//! - I1: `len == 0` exactly when `c` is null.
//! - I2: `c.rest < len <= c.rest + c.capacity`; the first chunk holds `live = len - c.rest` elements.
//! - I3: they are the last `live` slots of `c`: the first element is slot `c.capacity - live`.
//!   The slots before it are free; `List.cons` fills them downwards.
//! - I4: every other chunk is full, so its first element is slot 0 (`d = c.next` has
//!   `d.rest + d.capacity == c.rest`).
//! - `next` and `rest` never change after the chunk is made, and `capacity` only while a literal
//!   inside a loop is built (see `list_prefix`).
//!
//! A suffix of a list is the same chunks with a shorter length, so skipping elements costs no
//! more than finding the chunk. A list that is built at once is one chunk of exactly its length;
//! `List.cons` adds chunks that grow by half up to `LIST_CHUNK_MAX_BYTES`.
use super::*;

/// The bytes of a chunk header: the next pointer, `rest` and `capacity`.
const LIST_HEADER_BYTES: usize = 24;
/// The most bytes, header included, of a chunk that `List.cons` adds. A chunk that is built at
/// once has no limit but the allocation size.
const LIST_CHUNK_MAX_BYTES: usize = 32_768;
/// The bytes, header included, that the first chunk of a list built by `List.cons` fits.
const LIST_FIRST_CHUNK_BYTES: usize = 32;

/// A list under construction. `list_builder` allocates its only chunk, `list_append` or
/// `list_prepend` store the elements, and `list_finish` yields the list.
pub(super) struct ListBuilder {
    chunk: String,
    prefixed: bool,
}

/// The element that a list loop is at: its address, the labels that leave the loop and that go on
/// to the next element, and the two numbers that make up its position in the list.
struct ListStep {
    pointer: String,
    position: String,
    offset: String,
    exit: String,
    advance: String,
}

impl ListStep {
    /// The position of the element in the list, a value that a body asks for only if it uses it.
    fn index(&self, emitter: &mut FunctionEmitter<'_, '_>) -> String {
        emitter.value(format!("add i64 {}, {}", self.position, self.offset))
    }
}

impl FunctionEmitter<'_, '_> {
    /// A node of the list that this representation replaced: its size bounds the length of a
    /// list, which `list_guard_length` checks before anything is allocated.
    fn list_node_type(&self, element: &Type) -> String {
        format!("{{ ptr, {} }}", self.ty(element))
    }

    fn list_chunk_type(&self, element: &Type) -> String {
        format!("{{ ptr, i64, i64, [0 x {}] }}", self.ty(element))
    }

    /// The capacity of the first chunk of a list built by `List.cons`, and the largest capacity of
    /// a chunk that it adds, in slots of `element` on the largest layout of its type.
    fn list_capacities(&self, element: &Type) -> (usize, usize) {
        let (size, align) = storage_layout(element, self.module);
        let offset = LIST_HEADER_BYTES.next_multiple_of(align.max(1));
        let stride = size.max(1);
        let fit = |bytes: usize| (bytes.saturating_sub(offset) / stride).max(1);
        (fit(LIST_FIRST_CHUNK_BYTES), fit(LIST_CHUNK_MAX_BYTES))
    }

    /// The address of header field `field` of `chunk`: 1 is `rest` and 2 is `capacity`.
    fn list_field(&mut self, element: &Type, chunk: &str, field: usize) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {chunk}, i32 0, i32 {field}",
            self.list_chunk_type(element)
        ))
    }

    fn list_load_field(&mut self, element: &Type, chunk: &str, field: usize) -> String {
        let pointer = self.list_field(element, chunk, field);
        self.value(format!("load i64, ptr {pointer}"))
    }

    fn list_store_field(&mut self, element: &Type, chunk: &str, field: usize, value: &str) {
        let pointer = self.list_field(element, chunk, field);
        self.instruction(format!("store i64 {value}, ptr {pointer}"));
    }

    /// The address of slot `slot` of `chunk`.
    fn list_slot(&mut self, element: &Type, chunk: &str, slot: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {chunk}, i32 0, i32 3, i64 {slot}",
            self.list_chunk_type(element)
        ))
    }

    /// The size in bytes of a chunk of `capacity` slots, an integer literal or a value.
    fn list_chunk_bytes(&mut self, element: &Type, capacity: &str) -> String {
        let ty = self.ty(element);
        if capacity.bytes().all(|byte| byte.is_ascii_digit()) {
            return format!(
                "ptrtoint (ptr getelementptr ({{ ptr, i64, i64, [{capacity} x {ty}] }}, ptr null, i32 1) to i64)"
            );
        }
        let slots = self.value(format!(
            "mul i64 {capacity}, ptrtoint (ptr getelementptr ({ty}, ptr null, i32 1) to i64)"
        ));
        self.value(format!(
            "add i64 {slots}, ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64)",
            self.list_chunk_type(element)
        ))
    }

    /// Allocates a chunk of `capacity` slots with its header written.
    fn list_alloc_chunk(
        &mut self,
        element: &Type,
        capacity: &str,
        next: &str,
        rest: &str,
    ) -> String {
        let bytes = self.list_chunk_bytes(element, capacity);
        let chunk = self.value(format!("call ptr @tz.alloc(i64 {bytes})"));
        self.list_write_header(element, &chunk, next, rest, capacity);
        chunk
    }

    fn list_write_header(
        &mut self,
        element: &Type,
        chunk: &str,
        next: &str,
        rest: &str,
        capacity: &str,
    ) {
        self.instruction(format!("store ptr {next}, ptr {chunk}"));
        self.list_store_field(element, chunk, 1, rest);
        self.list_store_field(element, chunk, 2, capacity);
    }

    /// The slot of the first element of the list `(chunk, length)`, whose length is above zero.
    fn list_first_slot(&mut self, element: &Type, chunk: &str, length: &str) -> String {
        let rest = self.list_load_field(element, chunk, 1);
        let capacity = self.list_load_field(element, chunk, 2);
        let live = self.value(format!("sub i64 {length}, {rest}"));
        self.value(format!("sub i64 {capacity}, {live}"))
    }

    /// Traps unless a list of `length` elements has an allocation size that the program can
    /// address, before the elements are computed.
    pub(super) fn list_guard_length(&mut self, element: &Type, length: &str) {
        self.allocation_size(&self.list_node_type(element), length);
    }

    /// Allocates the only chunk of a list of `length` elements, no chunk if `length` is zero.
    pub(super) fn list_builder(&mut self, element: &Type, length: &str) -> ListBuilder {
        if length.bytes().all(|byte| byte.is_ascii_digit()) && length != "0" {
            let chunk = self.list_alloc_chunk(element, length, "null", "0");
            return ListBuilder {
                chunk,
                prefixed: false,
            };
        }
        let entry = self.block.clone();
        let allocate = self.label();
        let join = self.label();
        let empty = self.value(format!("icmp eq i64 {length}, 0"));
        self.branch(&empty, &join, &allocate);
        self.begin(&allocate);
        let chunk = self.list_alloc_chunk(element, length, "null", "0");
        self.jump(&join);
        self.begin(&join);
        let chunk = self.value(format!(
            "phi ptr [ null, %{entry} ], [ {chunk}, %{allocate} ]"
        ));
        ListBuilder {
            chunk,
            prefixed: false,
        }
    }

    /// Stores `value` as the element at position `index`.
    pub(super) fn list_append(
        &mut self,
        element: &Type,
        builder: &ListBuilder,
        index: &str,
        value: &str,
    ) {
        let pointer = self.list_slot(element, &builder.chunk, index);
        self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
    }

    /// Stores `value` as the element at position `length - 1 - index` of a list of `length`
    /// elements.
    pub(super) fn list_prepend(
        &mut self,
        element: &Type,
        builder: &ListBuilder,
        index: &str,
        length: &str,
        value: &str,
    ) {
        let last = self.value(format!("sub i64 {length}, 1"));
        let position = self.value(format!("sub i64 {last}, {index}"));
        self.list_append(element, builder, &position, value);
    }

    /// The list of the first `count` elements stored so far, for the cleanup of a literal that an
    /// early exit abandons while it builds element `count`. The capacity shrinks to the elements,
    /// so that the live slots start at slot 0, and `list_finish` restores it.
    pub(super) fn list_prefix(
        &mut self,
        element: &Type,
        builder: &mut ListBuilder,
        count: usize,
    ) -> String {
        builder.prefixed = true;
        self.list_store_field(element, &builder.chunk, 2, &count.to_string());
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {}, 0",
            builder.chunk
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {count}, 1"))
    }

    pub(super) fn list_finish(
        &mut self,
        element: &Type,
        builder: &ListBuilder,
        length: &str,
    ) -> String {
        if builder.prefixed {
            self.list_store_field(element, &builder.chunk, 2, length);
        }
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {}, 0",
            builder.chunk
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    /// Runs `body` once per element in order with the element's address and its position.
    pub(super) fn list_loop(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str, &str),
    ) {
        self.list_walk(element, head, length, |emitter, step| {
            let index = step.index(emitter);
            body(emitter, &step.pointer, &index);
        });
    }

    /// `list_loop` for a body that does not need the position of the element.
    pub(super) fn list_each(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str),
    ) {
        self.list_walk(element, head, length, |emitter, step| {
            body(emitter, &step.pointer);
        });
    }

    /// `list_each` whose body also gets the labels that leave the loop and that start the next
    /// iteration, and must jump to the second at the end of each iteration.
    pub(super) fn list_loop_control(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str, &str, &str),
    ) {
        self.list_walk(element, head, length, |emitter, step| {
            body(emitter, &step.pointer, &step.exit, &step.advance);
        });
    }

    /// The loop of the three above. It is two loops. The outer one goes from chunk to chunk, and
    /// the inner one is a counted loop over the `run` live slots of the chunk from a base address:
    /// the shape of a loop over an array, which LLVM unrolls and vectorizes. A cursor that leaves
    /// a chunk by a branch inside a single loop keeps the slot, the end of the chunk and the
    /// chunk in the chain from an element to the next.
    fn list_walk(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &ListStep),
    ) {
        let first = self.label();
        let chunks = self.label();
        let slots = self.label();
        let current = self.label();
        let advance = self.label();
        let step = self.increment_block(&advance);
        let after = self.label();
        let hop = self.label();
        let exit = self.label();
        let chunk = self.fresh();
        let next_chunk = self.fresh();
        let position = self.fresh();
        let next_position = self.fresh();
        let start = self.fresh();
        let offset = self.fresh();
        let next_offset = self.fresh();
        let nonempty = self.value(format!("icmp ne i64 {length}, 0"));
        self.branch(&nonempty, &first, &exit);
        self.begin(&first);
        let first_slot = self.list_first_slot(element, head, length);
        self.jump(&chunks);
        self.begin(&chunks);
        self.instruction(format!(
            "{chunk} = phi ptr [ {head}, %{first} ], [ {next_chunk}, %{hop} ]"
        ));
        self.instruction(format!(
            "{position} = phi i64 [ 0, %{first} ], [ {next_position}, %{hop} ]"
        ));
        self.instruction(format!(
            "{start} = phi i64 [ {first_slot}, %{first} ], [ 0, %{hop} ]"
        ));
        let capacity = self.list_load_field(element, &chunk, 2);
        let run = self.value(format!("sub i64 {capacity}, {start}"));
        let base = self.list_slot(element, &chunk, &start);
        self.jump(&slots);
        self.begin(&slots);
        self.instruction(format!(
            "{offset} = phi i64 [ 0, %{chunks} ], [ {next_offset}, %{step} ]"
        ));
        let more = self.value(format!("icmp ult i64 {offset}, {run}"));
        self.branch(&more, &current, &after);
        self.begin(&current);
        let pointer = self.value(format!(
            "getelementptr inbounds {}, ptr {base}, i64 {offset}",
            self.ty(element)
        ));
        body(
            self,
            &ListStep {
                pointer,
                position: position.clone(),
                offset: offset.clone(),
                exit: exit.clone(),
                advance: advance.clone(),
            },
        );
        self.jump(&advance);
        self.begin(&advance);
        // `offset < run` as unsigned numbers: the step cannot wrap (R4).
        self.audit_increment(&step, 64, &offset, "1", Flags::UNSIGNED);
        let words = self.flag_text(Flags::UNSIGNED);
        self.instruction(format!("{next_offset} = add {words}i64 {offset}, 1"));
        self.jump(&slots);
        self.begin(&after);
        self.instruction(format!("{next_position} = add i64 {position}, {run}"));
        let remaining = self.value(format!("icmp ult i64 {next_position}, {length}"));
        self.branch(&remaining, &hop, &exit);
        self.begin(&hop);
        self.instruction(format!("{next_chunk} = load ptr, ptr {chunk}"));
        self.jump(&chunks);
        self.begin(&exit);
    }

    /// Runs `body` once per pair of elements of two lists, each a head and a length, in order, as
    /// many pairs as `length`, the smaller of the lengths. The body gets the addresses of a pair
    /// and ends in the block that goes on with the next pair.
    ///
    /// A run is the pairs that lie in one chunk of each list: as many as the smaller of the slots
    /// that the two current chunks have left, and of the pairs that `length` has left. The outer
    /// loop goes from run to run, and the inner loop is a counted loop over a run, as in
    /// `list_loop_control`. Only a cursor whose chunk the run used up moves on to the next chunk.
    pub(super) fn list_lockstep_loop(
        &mut self,
        element: &Type,
        left: (&str, &str),
        right: (&str, &str),
        length: &str,
        body: impl FnOnce(&mut Self, &str, &str),
    ) {
        let first = self.label();
        let runs = self.label();
        let slots = self.label();
        let current = self.label();
        let advance = self.label();
        let step = self.increment_block(&advance);
        let after = self.label();
        let moving = self.label();
        let left_hop = self.label();
        let left_join = self.label();
        let right_hop = self.label();
        let join = self.label();
        let exit = self.label();
        let left_chunk = self.fresh();
        let left_next_chunk = self.fresh();
        let left_slot = self.fresh();
        let left_next_slot = self.fresh();
        let right_chunk = self.fresh();
        let right_next_chunk = self.fresh();
        let right_slot = self.fresh();
        let right_next_slot = self.fresh();
        let compared = self.fresh();
        let next_compared = self.fresh();
        let offset = self.fresh();
        let next_offset = self.fresh();
        let nonempty = self.value(format!("icmp ne i64 {length}, 0"));
        self.branch(&nonempty, &first, &exit);
        self.begin(&first);
        let left_start = self.list_first_slot(element, left.0, left.1);
        let right_start = self.list_first_slot(element, right.0, right.1);
        self.jump(&runs);
        self.begin(&runs);
        self.instruction(format!(
            "{left_chunk} = phi ptr [ {}, %{first} ], [ {left_next_chunk}, %{join} ]",
            left.0
        ));
        self.instruction(format!(
            "{left_slot} = phi i64 [ {left_start}, %{first} ], [ {left_next_slot}, %{join} ]"
        ));
        self.instruction(format!(
            "{right_chunk} = phi ptr [ {}, %{first} ], [ {right_next_chunk}, %{join} ]",
            right.0
        ));
        self.instruction(format!(
            "{right_slot} = phi i64 [ {right_start}, %{first} ], [ {right_next_slot}, %{join} ]"
        ));
        self.instruction(format!(
            "{compared} = phi i64 [ 0, %{first} ], [ {next_compared}, %{join} ]"
        ));
        let left_capacity = self.list_load_field(element, &left_chunk, 2);
        let right_capacity = self.list_load_field(element, &right_chunk, 2);
        let left_room = self.value(format!("sub i64 {left_capacity}, {left_slot}"));
        let right_room = self.value(format!("sub i64 {right_capacity}, {right_slot}"));
        let pending = self.value(format!("sub i64 {length}, {compared}"));
        let left_less = self.value(format!("icmp ult i64 {left_room}, {right_room}"));
        let room = self.value(format!(
            "select i1 {left_less}, i64 {left_room}, i64 {right_room}"
        ));
        let room_less = self.value(format!("icmp ult i64 {room}, {pending}"));
        let run = self.value(format!("select i1 {room_less}, i64 {room}, i64 {pending}"));
        let left_base = self.list_slot(element, &left_chunk, &left_slot);
        let right_base = self.list_slot(element, &right_chunk, &right_slot);
        self.jump(&slots);
        self.begin(&slots);
        self.instruction(format!(
            "{offset} = phi i64 [ 0, %{runs} ], [ {next_offset}, %{step} ]"
        ));
        let more = self.value(format!("icmp ult i64 {offset}, {run}"));
        self.branch(&more, &current, &after);
        self.begin(&current);
        let ty = self.ty(element);
        let left_pointer = self.value(format!(
            "getelementptr inbounds {ty}, ptr {left_base}, i64 {offset}"
        ));
        let right_pointer = self.value(format!(
            "getelementptr inbounds {ty}, ptr {right_base}, i64 {offset}"
        ));
        body(self, &left_pointer, &right_pointer);
        self.jump(&advance);
        self.begin(&advance);
        // `offset < run` as unsigned numbers: the step cannot wrap (R5).
        self.audit_increment(&step, 64, &offset, "1", Flags::UNSIGNED);
        let words = self.flag_text(Flags::UNSIGNED);
        self.instruction(format!("{next_offset} = add {words}i64 {offset}, 1"));
        self.jump(&slots);
        self.begin(&after);
        self.instruction(format!("{next_compared} = add i64 {compared}, {run}"));
        let remaining = self.value(format!("icmp ult i64 {next_compared}, {length}"));
        self.branch(&remaining, &moving, &exit);
        self.begin(&moving);
        let left_moved = self.value(format!("add i64 {left_slot}, {run}"));
        let left_full = self.value(format!("icmp eq i64 {left_moved}, {left_capacity}"));
        self.branch(&left_full, &left_hop, &left_join);
        self.begin(&left_hop);
        let left_following = self.value(format!("load ptr, ptr {left_chunk}"));
        self.jump(&left_join);
        self.begin(&left_join);
        self.instruction(format!(
            "{left_next_chunk} = phi ptr [ {left_following}, %{left_hop} ], [ {left_chunk}, %{moving} ]"
        ));
        self.instruction(format!(
            "{left_next_slot} = phi i64 [ 0, %{left_hop} ], [ {left_moved}, %{moving} ]"
        ));
        let right_moved = self.value(format!("add i64 {right_slot}, {run}"));
        let right_full = self.value(format!("icmp eq i64 {right_moved}, {right_capacity}"));
        self.branch(&right_full, &right_hop, &join);
        self.begin(&right_hop);
        let right_following = self.value(format!("load ptr, ptr {right_chunk}"));
        self.jump(&join);
        self.begin(&join);
        self.instruction(format!(
            "{right_next_chunk} = phi ptr [ {right_following}, %{right_hop} ], [ {right_chunk}, %{left_join} ]"
        ));
        self.instruction(format!(
            "{right_next_slot} = phi i64 [ 0, %{right_hop} ], [ {right_moved}, %{left_join} ]"
        ));
        self.jump(&runs);
        self.begin(&exit);
    }

    /// The address of the element at `index`, which is below the length: the loop runs once per
    /// chunk that precedes it.
    pub(super) fn list_locate(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        index: &str,
    ) -> String {
        let entry = self.block.clone();
        let condition = self.label();
        let outside = self.label();
        let found = self.label();
        let chunk = self.fresh();
        let next_chunk = self.fresh();
        let remaining = self.fresh();
        let rest = self.fresh();
        let target = self.fresh();
        let next_target = self.fresh();
        self.jump(&condition);
        self.begin(&condition);
        self.instruction(format!(
            "{chunk} = phi ptr [ {head}, %{entry} ], [ {next_chunk}, %{outside} ]"
        ));
        self.instruction(format!(
            "{remaining} = phi i64 [ {length}, %{entry} ], [ {rest}, %{outside} ]"
        ));
        self.instruction(format!(
            "{target} = phi i64 [ {index}, %{entry} ], [ {next_target}, %{outside} ]"
        ));
        let pointer = self.list_field(element, &chunk, 1);
        self.instruction(format!("{rest} = load i64, ptr {pointer}"));
        let live = self.value(format!("sub i64 {remaining}, {rest}"));
        let inside = self.value(format!("icmp ult i64 {target}, {live}"));
        self.branch(&inside, &found, &outside);
        self.begin(&outside);
        self.instruction(format!("{next_target} = sub i64 {target}, {live}"));
        self.instruction(format!("{next_chunk} = load ptr, ptr {chunk}"));
        self.jump(&condition);
        self.begin(&found);
        let capacity = self.list_load_field(element, &chunk, 2);
        let first = self.value(format!("sub i64 {capacity}, {live}"));
        let slot = self.value(format!("add i64 {first}, {target}"));
        self.list_slot(element, &chunk, &slot)
    }

    /// The list without its first `count` elements, a view of the same chunks; `length` is at
    /// least `count`. A chunk whose elements are all skipped is passed over.
    pub(super) fn list_skip(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        count: usize,
    ) -> String {
        let entry = self.block.clone();
        let condition = self.label();
        let outside = self.label();
        let done = self.label();
        let chunk = self.fresh();
        let next_chunk = self.fresh();
        let remaining = self.value(format!("sub i64 {length}, {count}"));
        let empty = self.value(format!("icmp eq i64 {remaining}, 0"));
        self.branch(&empty, &done, &condition);
        self.begin(&condition);
        self.instruction(format!(
            "{chunk} = phi ptr [ {head}, %{entry} ], [ {next_chunk}, %{outside} ]"
        ));
        let pointer = self.list_field(element, &chunk, 1);
        let rest = self.value(format!("load i64, ptr {pointer}"));
        let starts_here = self.value(format!("icmp ugt i64 {remaining}, {rest}"));
        self.branch(&starts_here, &done, &outside);
        self.begin(&outside);
        self.instruction(format!("{next_chunk} = load ptr, ptr {chunk}"));
        self.jump(&condition);
        self.begin(&done);
        let view = self.value(format!(
            "phi ptr [ null, %{entry} ], [ {chunk}, %{condition} ]"
        ));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {view}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {remaining}, 1"))
    }

    /// `List.cons`: the list `value :: list`, which takes the storage of `list`; `length` is below
    /// the largest length. The element goes into the slot before the first one if the first chunk
    /// has one, else into the last slot of a new first chunk.
    pub(super) fn list_push_front(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        value: &str,
    ) -> String {
        let (first, largest) = self.list_capacities(element);
        let entry = self.block.clone();
        let check = self.label();
        let fit = self.label();
        let grow = self.label();
        let done = self.label();
        let empty = self.value(format!("icmp eq i64 {length}, 0"));
        self.branch(&empty, &grow, &check);
        self.begin(&check);
        let rest = self.list_load_field(element, head, 1);
        let capacity = self.list_load_field(element, head, 2);
        let live = self.value(format!("sub i64 {length}, {rest}"));
        let free = self.value(format!("sub i64 {capacity}, {live}"));
        let room = self.value(format!("icmp ne i64 {free}, 0"));
        self.branch(&room, &fit, &grow);
        self.begin(&fit);
        let slot = self.value(format!("sub i64 {free}, 1"));
        let pointer = self.list_slot(element, head, &slot);
        self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
        self.jump(&done);
        self.begin(&grow);
        let previous = self.value(format!("phi i64 [ 0, %{entry} ], [ {capacity}, %{check} ]"));
        let half = self.value(format!("lshr i64 {previous}, 1"));
        let grown = self.value(format!("add i64 {previous}, {half}"));
        let grown = self.value(format!("add i64 {grown}, 1"));
        let over = self.value(format!("icmp ugt i64 {grown}, {largest}"));
        let capped = self.value(format!("select i1 {over}, i64 {largest}, i64 {grown}"));
        let size = self.value(format!("select i1 {empty}, i64 {first}, i64 {capped}"));
        let chunk = self.list_alloc_chunk(element, &size, head, length);
        let last = self.value(format!("sub i64 {size}, 1"));
        let pointer = self.list_slot(element, &chunk, &last);
        self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
        self.jump(&done);
        self.begin(&done);
        let new_head = self.value(format!("phi ptr [ {head}, %{fit} ], [ {chunk}, %{grow} ]"));
        let length = self.value(format!("add i64 {length}, 1"));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {new_head}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    /// `List.tail`: the list without its first element, which is dropped; `length` is above zero.
    /// The first chunk is freed with its last element.
    pub(super) fn list_pop_front(&mut self, element: &Type, head: &str, length: &str) -> String {
        let rest = self.list_load_field(element, head, 1);
        let live = self.value(format!("sub i64 {length}, {rest}"));
        if element.needs_drop(&self.module.types()) {
            let capacity = self.list_load_field(element, head, 2);
            let slot = self.value(format!("sub i64 {capacity}, {live}"));
            let pointer = self.list_slot(element, head, &slot);
            let previous = self.value(format!("load {}, ptr {pointer}", self.ty(element)));
            self.drop_value(element, &previous);
        }
        let entry = self.block.clone();
        let release = self.label();
        let done = self.label();
        let last = self.value(format!("icmp eq i64 {live}, 1"));
        self.branch(&last, &release, &done);
        self.begin(&release);
        let next = self.value(format!("load ptr, ptr {head}"));
        self.instruction(format!("call void @tz.free(ptr {head})"));
        self.jump(&done);
        self.begin(&done);
        let new_head = self.value(format!(
            "phi ptr [ {head}, %{entry} ], [ {next}, %{release} ]"
        ));
        let length = self.value(format!("sub i64 {length}, 1"));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {new_head}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    /// Drops every element of the list `value` in order and frees its chunks: an outer loop over
    /// the chunks and an inner one over their live slots. A chunk's link is read before it is
    /// freed.
    pub(super) fn list_drop(&mut self, element: &Type, value: &str) {
        let head = self.value(format!("extractvalue %tz.list {value}, 0"));
        let length = self.value(format!("extractvalue %tz.list {value}, 1"));
        let drops = element.needs_drop(&self.module.types());
        let entry = self.block.clone();
        let condition = self.label();
        let release = self.label();
        let latch = self.label();
        let exit = self.label();
        let chunk = self.fresh();
        let next_chunk = self.fresh();
        let remaining = self.fresh();
        let rest = self.fresh();
        self.jump(&condition);
        self.begin(&condition);
        self.instruction(format!(
            "{chunk} = phi ptr [ {head}, %{entry} ], [ {next_chunk}, %{latch} ]"
        ));
        if drops {
            self.instruction(format!(
                "{remaining} = phi i64 [ {length}, %{entry} ], [ {rest}, %{latch} ]"
            ));
        }
        let present = self.value(format!("icmp ne ptr {chunk}, null"));
        self.branch(&present, &release, &exit);
        self.begin(&release);
        self.instruction(format!("{next_chunk} = load ptr, ptr {chunk}"));
        let pointer = self.list_field(element, &chunk, 1);
        self.instruction(format!("{rest} = load i64, ptr {pointer}"));
        if drops {
            let capacity = self.list_load_field(element, &chunk, 2);
            let live = self.value(format!("sub i64 {remaining}, {rest}"));
            let first = self.value(format!("sub i64 {capacity}, {live}"));
            self.array_loop(&live, |emitter, index| {
                let slot = emitter.value(format!("add i64 {first}, {index}"));
                let pointer = emitter.list_slot(element, &chunk, &slot);
                let value = emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                emitter.drop_value(element, &value);
            });
        }
        self.instruction(format!("call void @tz.free(ptr {chunk})"));
        self.jump(&latch);
        self.begin(&latch);
        self.jump(&condition);
        self.begin(&exit);
    }

    /// A new list whose elements are the clones of those of the list `value`, in one chunk.
    pub(super) fn list_clone(&mut self, element: &Type, value: &str) -> String {
        let source = self.value(format!("extractvalue %tz.list {value}, 0"));
        let length = self.value(format!("extractvalue %tz.list {value}, 1"));
        let builder = self.list_builder(element, &length);
        self.list_loop(element, &source, &length, |emitter, pointer, index| {
            let value = emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
            let copy = emitter.clone_value(element, &value);
            emitter.list_append(element, &builder, index, &copy);
        });
        self.list_finish(element, &builder, &length)
    }

    /// Reserves in the entry block the storage of a list literal of `count` elements, which
    /// outlives the expression, and returns the address that the list value holds. The literal is
    /// one full chunk, whose header is written here.
    pub(super) fn list_stack_storage(&mut self, element: &Type, count: usize) -> String {
        let storage = self.fresh();
        self.allocas.push(format!(
            "{storage} = alloca {{ ptr, i64, i64, [{count} x {}] }}, align 16",
            self.ty(element)
        ));
        self.list_write_header(element, &storage, "null", "0", &count.to_string());
        storage
    }

    /// The address of element `index` of the stack storage of a list literal.
    pub(super) fn list_stack_slot(
        &mut self,
        element: &Type,
        storage: &str,
        index: usize,
    ) -> String {
        self.list_slot(element, storage, &index.to_string())
    }

    /// Stores `value` as element `index` of the stack storage of a list literal.
    pub(super) fn list_stack_store(
        &mut self,
        element: &Type,
        storage: &str,
        index: usize,
        value: &str,
    ) {
        let pointer = self.list_stack_slot(element, storage, index);
        self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
    }

    /// The list value of the stack storage of a list literal of `count` elements.
    pub(super) fn list_stack_descriptor(&mut self, storage: &str, count: usize) -> String {
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {storage}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {count}, 1"))
    }
}
