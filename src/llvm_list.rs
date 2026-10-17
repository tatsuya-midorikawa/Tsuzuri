//! The representation of a list `[|T|]`: the emitters that create, walk, index, skip, grow, shrink,
//! copy and free its storage live here, and the rest of the emitter calls them without knowing the
//! layout. A list is `%tz.list = { ptr, i64 }`, the storage of its first element and its length;
//! the empty list is `zeroinitializer`.
//!
//! The storage is a chain of nodes `{ ptr next, T value }`, one allocation per element.
use super::*;

/// A list under construction. `list_builder` starts it, `list_append` or `list_prepend` store the
/// elements in list order or in reverse order, and `list_finish` yields the list.
pub(super) struct ListBuilder {
    head: String,
    tail: String,
}

impl FunctionEmitter<'_, '_> {
    fn list_node_type(&self, element: &Type) -> String {
        format!("{{ ptr, {} }}", self.ty(element))
    }

    fn list_element_pointer(&mut self, element: &Type, node: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {node}, i32 0, i32 1",
            self.list_node_type(element)
        ))
    }

    /// Traps unless a list of `length` elements has an allocation size that the program can
    /// address, before the elements are computed.
    pub(super) fn list_guard_length(&mut self, element: &Type, length: &str) {
        self.allocation_size(&self.list_node_type(element), length);
    }

    /// Starts a list of `length` elements.
    pub(super) fn list_builder(&mut self, _element: &Type, _length: &str) -> ListBuilder {
        let pointer = Type::Reference(Box::new(Type::Unit), false);
        let head = self.slot(&pointer);
        let tail = self.slot(&pointer);
        self.instruction(format!("store ptr null, ptr {head}"));
        self.instruction(format!("store ptr {head}, ptr {tail}"));
        ListBuilder { head, tail }
    }

    /// Stores `value` as the element at position `index`; positions come in increasing order.
    pub(super) fn list_append(
        &mut self,
        element: &Type,
        builder: &ListBuilder,
        _index: &str,
        value: &str,
    ) {
        let node = self.value(format!(
            "call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64))",
            self.list_node_type(element)
        ));
        self.instruction(format!("store ptr null, ptr {node}"));
        let pointer = self.list_element_pointer(element, &node);
        self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
        let previous = self.value(format!("load ptr, ptr {}", builder.tail));
        self.instruction(format!("store ptr {node}, ptr {previous}"));
        // The first field of each node is the next pointer, also used as the builder's tail slot.
        self.instruction(format!("store ptr {node}, ptr {}", builder.tail));
    }

    /// Stores `value` as the element at position `length - 1 - index` of a list of `length`
    /// elements; the `index` of the calls come in increasing order.
    pub(super) fn list_prepend(
        &mut self,
        element: &Type,
        builder: &ListBuilder,
        _index: &str,
        _length: &str,
        value: &str,
    ) {
        let previous = self.value(format!("load ptr, ptr {}", builder.head));
        let node_type = self.list_node_type(element);
        let next = self.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({node_type}, ptr null, i32 1) to i64))"));
        self.instruction(format!("store ptr {previous}, ptr {next}"));
        let slot = self.list_element_pointer(element, &next);
        self.instruction(format!("store {} {value}, ptr {slot}", self.ty(element)));
        self.instruction(format!("store ptr {next}, ptr {}", builder.head));
    }

    /// The list of the first `count` elements stored so far, for the cleanup of a literal that an
    /// early exit abandons; the elements after them are not stored yet.
    pub(super) fn list_prefix(
        &mut self,
        element: &Type,
        builder: &ListBuilder,
        count: usize,
    ) -> String {
        self.list_finish(element, builder, &count.to_string())
    }

    pub(super) fn list_finish(
        &mut self,
        _element: &Type,
        builder: &ListBuilder,
        length: &str,
    ) -> String {
        let head = self.value(format!("load ptr, ptr {}", builder.head));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {head}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    /// Runs `body` once per element in order with the element's address and its position. The
    /// element may be freed, and the list is not read again.
    pub(super) fn list_loop(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str, &str),
    ) {
        self.list_loop_control(element, head, length, |emitter, pointer, index, _, _| {
            body(emitter, pointer, index);
        });
    }

    /// `list_loop` whose body also gets the labels that leave the loop and that start the next
    /// iteration, and must jump to the second at the end of each iteration.
    pub(super) fn list_loop_control(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str, &str, &str, &str),
    ) {
        self.list_node_loop(head, length, |emitter, node, index, exit, advance| {
            let pointer = emitter.list_element_pointer(element, node);
            body(emitter, &pointer, index, exit, advance);
        });
    }

    /// The loop over the first `length` nodes from `head`; returns the node that follows them.
    fn list_node_loop(
        &mut self,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str, &str, &str, &str),
    ) -> String {
        let entry = self.block.clone();
        let condition = self.label();
        let element = self.label();
        let advance = self.label();
        let step = self.increment_block(&advance);
        let exit = self.label();
        let index = self.fresh();
        let next_index = self.fresh();
        let node = self.fresh();
        let next_node = self.fresh();
        self.jump(&condition);
        self.begin(&condition);
        self.instruction(format!(
            "{index} = phi i64 [ 0, %{entry} ], [ {next_index}, %{step} ]"
        ));
        self.instruction(format!(
            "{node} = phi ptr [ {head}, %{entry} ], [ {next_node}, %{step} ]"
        ));
        let more = self.value(format!("icmp ult i64 {index}, {length}"));
        self.branch(&more, &element, &exit);
        self.begin(&element);
        // Read the link before the callback, which may free this node.
        self.instruction(format!("{next_node} = load ptr, ptr {node}"));
        body(self, &node, &index, &exit, &advance);
        self.jump(&advance);
        self.begin(&advance);
        // `index < length` as unsigned numbers: the step cannot wrap (R4).
        self.audit_increment(&step, 64, &index, "1", Flags::UNSIGNED);
        let words = self.flag_text(Flags::UNSIGNED);
        self.instruction(format!("{next_index} = add {words}i64 {index}, 1"));
        self.jump(&condition);
        self.begin(&exit);
        node
    }

    /// The address of the element at `index`, which is below the length.
    pub(super) fn list_locate(
        &mut self,
        element: &Type,
        head: &str,
        _length: &str,
        index: &str,
    ) -> String {
        let node = self.list_node_loop(head, index, |_, _, _, _, _| {});
        self.list_element_pointer(element, &node)
    }

    /// The list without its first `count` elements, a view of the same storage; `length` is at
    /// least `count`.
    pub(super) fn list_skip(
        &mut self,
        _element: &Type,
        head: &str,
        length: &str,
        count: usize,
    ) -> String {
        let tail = self.list_node_loop(head, &count.to_string(), |_, _, _, _, _| {});
        let length = self.value(format!("sub i64 {length}, {count}"));
        let descriptor = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {tail}, 0"
        ));
        self.value(format!(
            "insertvalue %tz.list {descriptor}, i64 {length}, 1"
        ))
    }

    /// `List.cons`: the list `value :: list`, which takes the storage of `list`; `length` is below
    /// the largest length.
    pub(super) fn list_push_front(
        &mut self,
        element: &Type,
        head: &str,
        length: &str,
        value: &str,
    ) -> String {
        let node_type = self.list_node_type(element);
        let node = self.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({node_type}, ptr null, i32 1) to i64))"));
        self.instruction(format!("store ptr {head}, ptr {node}"));
        let slot = self.list_element_pointer(element, &node);
        self.instruction(format!("store {} {value}, ptr {slot}", self.ty(element)));
        let length = self.value(format!("add i64 {length}, 1"));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {node}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    /// `List.tail`: the list without its first element, which is dropped; `length` is above zero.
    pub(super) fn list_pop_front(&mut self, element: &Type, head: &str, length: &str) -> String {
        let next = self.value(format!("load ptr, ptr {head}"));
        let slot = self.list_element_pointer(element, head);
        let previous = self.value(format!("load {}, ptr {slot}", self.ty(element)));
        self.drop_value(element, &previous);
        self.instruction(format!("call void @tz.free(ptr {head})"));
        let length = self.value(format!("sub i64 {length}, 1"));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {next}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    /// Drops every element of the list `value` in order and frees its storage.
    pub(super) fn list_drop(&mut self, element: &Type, value: &str) {
        let head = self.value(format!("extractvalue %tz.list {value}, 0"));
        let length = self.value(format!("extractvalue %tz.list {value}, 1"));
        self.list_node_loop(&head, &length, |emitter, node, _, _, _| {
            if element.needs_drop(&emitter.module.types()) {
                let pointer = emitter.list_element_pointer(element, node);
                let value = emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                emitter.drop_value(element, &value);
            }
            emitter.instruction(format!("call void @tz.free(ptr {node})"));
        });
    }

    /// A new list whose elements are the clones of those of the list `value`.
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
    /// outlives the expression, and returns the address that the list value holds.
    pub(super) fn list_stack_storage(&mut self, element: &Type, count: usize) -> String {
        let node = self.list_node_type(element);
        let storage = self.fresh();
        self.allocas
            .push(format!("{storage} = alloca [{count} x {node}], align 16"));
        storage
    }

    fn list_stack_node(&mut self, element: &Type, storage: &str, index: usize) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {storage}, i64 {index}",
            self.list_node_type(element)
        ))
    }

    /// The address of element `index` of the stack storage of a list literal.
    pub(super) fn list_stack_slot(
        &mut self,
        element: &Type,
        storage: &str,
        index: usize,
    ) -> String {
        let node = self.list_stack_node(element, storage, index);
        self.list_element_pointer(element, &node)
    }

    /// Stores `value` as element `index` of the stack storage of a list literal of `count`
    /// elements, the elements in increasing order.
    pub(super) fn list_stack_store(
        &mut self,
        element: &Type,
        storage: &str,
        index: usize,
        count: usize,
        value: &str,
    ) {
        let current = self.list_stack_node(element, storage, index);
        let next = if index + 1 == count {
            "null".to_owned()
        } else {
            self.list_stack_node(element, storage, index + 1)
        };
        // The first field of each node is its link, matching heap-allocated nodes.
        self.instruction(format!("store ptr {next}, ptr {current}"));
        let pointer = self.list_element_pointer(element, &current);
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
