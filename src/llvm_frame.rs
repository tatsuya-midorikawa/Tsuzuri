//! Stack storage for collection and string literals that are not created with `new`.
//!
//! A literal bound in this function (or used as a temporary operand) keeps its elements in
//! entry-block allocas; string literals keep their code units in the private constant. Every frame
//! pointer lives only at its static position inside the value that received the literal, or in
//! arguments lent to a borrowing worker during one call, so a run-time address test identifies it
//! exactly even after moves, reassignment, or branch merges.
//! Moving such a value out of its slot copies the stack parts to the heap first.

use super::*;
use crate::check::MAX_VALUE_BYTES;

#[derive(Clone, Debug)]
pub(super) enum Frame {
    /// Array elements stored in the alloca `buffer`; one candidate list per element.
    Array {
        buffer: String,
        elements: Vec<Vec<Frame>>,
    },
    /// List nodes stored in the alloca `nodes`, linked in order.
    List {
        nodes: String,
        elements: Vec<Vec<Frame>>,
    },
    /// String code units that stay in the literal constant `data`.
    String { data: String, length: usize },
    /// Record or tuple fields with stack parts, by field index.
    Aggregate(BTreeMap<usize, Vec<Frame>>),
}

/// Conservative stack bytes of one value, matching the documented value-layout rule.
pub(super) fn stack_size(ty: &Type, module: &CheckedModule) -> usize {
    let fields = |types: &mut dyn Iterator<Item = &Type>| {
        types.fold(0usize, |total, ty| {
            total.saturating_add(stack_size(ty, module).next_multiple_of(16))
        })
    };
    match ty {
        Type::Record(id, arguments) => {
            fields(&mut module.types().record_fields(*id, arguments).iter())
        }
        Type::Tuple(elements) => fields(&mut elements.iter()),
        Type::Union(id, arguments) => module
            .types()
            .union_payloads(*id, arguments)
            .iter()
            .flatten()
            .map(|ty| stack_size(ty, module))
            .max()
            .map_or(8, |payload| {
                16usize.saturating_add(payload.next_multiple_of(16))
            }),
        Type::Integer(128, _)
        | Type::Binary(128)
        | Type::Decimal(128)
        | Type::String
        | Type::Utf8String
        | Type::Array(_)
        | Type::List(_) => 16,
        Type::Function(..) | Type::Task(_) => 32,
        _ => 8,
    }
}

/// Stack bytes that `frame_value` would reserve for the literal tree of `expression`.
fn frame_bytes(expression: &TypedExpr, module: &CheckedModule) -> usize {
    let sum = |values: &mut dyn Iterator<Item = &TypedExpr>| {
        values.fold(0usize, |total, value| {
            total.saturating_add(frame_bytes(value, module))
        })
    };
    match (&expression.kind, &expression.ty) {
        (TypedExprKind::Array(elements), Type::Array(element)) => elements
            .len()
            .saturating_mul(stack_size(element, module))
            .saturating_add(sum(&mut elements.iter())),
        (TypedExprKind::List(elements), Type::List(element)) => elements
            .len()
            .saturating_mul(16 + stack_size(element, module).next_multiple_of(16))
            .saturating_add(sum(&mut elements.iter())),
        (TypedExprKind::Record(fields), _) => sum(&mut fields.iter().map(|(_, value)| value)),
        (TypedExprKind::Tuple(elements), _) => sum(&mut elements.iter()),
        (
            TypedExprKind::If {
                then_branch,
                else_branch,
                ..
            },
            _,
        ) => sum(&mut [then_branch.as_ref(), else_branch.as_ref()].into_iter()),
        (TypedExprKind::Block { result, .. }, _) => frame_bytes(result, module),
        (TypedExprKind::Match { arms, .. }, _) => sum(&mut arms.iter().map(|arm| &arm.body)),
        _ => 0,
    }
}

impl FunctionEmitter<'_, '_> {
    /// Evaluates a value that stays in this frame: a binding's initializer or a temporary operand.
    /// Literal trees above the value-layout limit keep the heap path to bound stack use.
    pub(super) fn frame_value(&mut self, expression: &TypedExpr) -> (String, Vec<Frame>) {
        if frame_bytes(expression, self.module) > MAX_VALUE_BYTES {
            return (self.expression(expression), Vec::new());
        }
        self.frame_inner(expression)
    }

    pub(super) fn frame_inner(&mut self, expression: &TypedExpr) -> (String, Vec<Frame>) {
        match &expression.kind {
            TypedExprKind::Array(elements) if !elements.is_empty() => {
                self.frame_array(expression, elements)
            }
            TypedExprKind::List(elements) if !elements.is_empty() => {
                self.frame_list(expression, elements)
            }
            TypedExprKind::String(text) if !text.is_empty() => {
                let data = self.string_constant(text);
                let ty = self.ty(&expression.ty);
                let value = self.value(format!("insertvalue {ty} zeroinitializer, ptr {data}, 0"));
                let value = self.value(format!("insertvalue {ty} {value}, i64 {}, 1", text.len()));
                let length = text.len();
                (value, vec![Frame::String { data, length }])
            }
            TypedExprKind::Record(fields) => {
                let fields: Vec<_> = fields
                    .iter()
                    .map(|(index, value)| (*index, value))
                    .collect();
                self.frame_aggregate(&expression.ty, &fields)
            }
            TypedExprKind::Tuple(elements) => {
                let elements: Vec<_> = elements.iter().enumerate().collect();
                self.frame_aggregate(&expression.ty, &elements)
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.expression(condition);
                let yes = self.label();
                let no = self.label();
                let merge = self.label();
                self.branch(&condition, &yes, &no);
                self.begin(&yes);
                let (then_value, mut frames) = self.frame_inner(then_branch);
                let then_end = self.block.clone();
                self.jump(&merge);
                self.begin(&no);
                let (else_value, else_frames) = self.frame_inner(else_branch);
                let else_end = self.block.clone();
                self.jump(&merge);
                self.begin(&merge);
                frames.extend(else_frames);
                let value = self.value(format!(
                    "phi {} [ {then_value}, %{then_end} ], [ {else_value}, %{else_end} ]",
                    self.ty(&expression.ty),
                ));
                (value, frames)
            }
            TypedExprKind::Block { bindings, result } => {
                self.scopes.push(Vec::new());
                self.bind(bindings);
                let (value, frames) = self.frame_inner(result);
                let scope = self.scopes.pop().unwrap();
                self.drop_scope(&scope);
                (value, frames)
            }
            TypedExprKind::Match { local, value, arms } => {
                let mut frames = Vec::new();
                let value = self.match_expression(
                    local,
                    value,
                    arms,
                    &expression.ty,
                    false,
                    Some(&mut frames),
                );
                (value, frames)
            }
            _ => (self.expression(expression), Vec::new()),
        }
    }

    fn frame_array(
        &mut self,
        expression: &TypedExpr,
        elements: &[TypedExpr],
    ) -> (String, Vec<Frame>) {
        let Type::Array(element) = &expression.ty else {
            unreachable!()
        };
        let ty = self.ty(element);
        let buffer = self.fresh();
        self.allocas.push(format!(
            "{buffer} = alloca [{} x {ty}], align 16",
            elements.len()
        ));
        let mut frames = Vec::with_capacity(elements.len());
        for (index, value) in elements.iter().enumerate() {
            let (value, element_frames) = self.frame_inner(value);
            let pointer = self.element_pointer(element, &buffer, &index.to_string());
            self.instruction(format!("store {ty} {value}, ptr {pointer}"));
            frames.push(element_frames);
        }
        let array = self.value(format!(
            "insertvalue %tz.array zeroinitializer, ptr {buffer}, 0"
        ));
        let array = self.value(format!(
            "insertvalue %tz.array {array}, i64 {}, 1",
            elements.len()
        ));
        (
            array,
            vec![Frame::Array {
                buffer,
                elements: frames,
            }],
        )
    }

    fn frame_list(
        &mut self,
        expression: &TypedExpr,
        elements: &[TypedExpr],
    ) -> (String, Vec<Frame>) {
        let Type::List(element) = &expression.ty else {
            unreachable!()
        };
        let node = self.list_node_type(element);
        let nodes = self.fresh();
        self.allocas.push(format!(
            "{nodes} = alloca [{} x {node}], align 16",
            elements.len()
        ));
        let mut frames = Vec::with_capacity(elements.len());
        for (index, value) in elements.iter().enumerate() {
            let (value, element_frames) = self.frame_inner(value);
            let current = self.frame_node(element, &nodes, index);
            let next = if index + 1 == elements.len() {
                "null".to_owned()
            } else {
                self.frame_node(element, &nodes, index + 1)
            };
            // The first field of each node is its link, matching heap-allocated nodes.
            self.instruction(format!("store ptr {next}, ptr {current}"));
            let pointer = self.list_element_pointer(element, &current);
            self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
            frames.push(element_frames);
        }
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {nodes}, 0"
        ));
        let list = self.value(format!(
            "insertvalue %tz.list {list}, i64 {}, 1",
            elements.len()
        ));
        (
            list,
            vec![Frame::List {
                nodes,
                elements: frames,
            }],
        )
    }

    fn frame_node(&mut self, element: &Type, nodes: &str, index: usize) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {nodes}, i64 {index}",
            self.list_node_type(element)
        ))
    }

    fn frame_aggregate(
        &mut self,
        ty: &Type,
        fields: &[(usize, &TypedExpr)],
    ) -> (String, Vec<Frame>) {
        let llvm = self.ty(ty);
        let mut aggregate = "zeroinitializer".to_owned();
        let mut frames = BTreeMap::new();
        for (index, field) in fields {
            let (value, field_frames) = self.frame_inner(field);
            aggregate = self.value(format!(
                "insertvalue {llvm} {aggregate}, {} {value}, {index}",
                self.ty(&field.ty)
            ));
            if !field_frames.is_empty() {
                frames.insert(*index, field_frames);
            }
        }
        if frames.is_empty() {
            (aggregate, Vec::new())
        } else {
            (aggregate, vec![Frame::Aggregate(frames)])
        }
    }

    /// The stack parts that may be stored at `expression`, a place rooted at a local.
    pub(super) fn frame_of_place(&self, expression: &TypedExpr) -> Vec<Frame> {
        match &expression.kind {
            TypedExprKind::Local(id) => self.frame_locals.get(id).cloned().unwrap_or_default(),
            TypedExprKind::Field(value, index) => self
                .frame_of_place(value)
                .into_iter()
                .flat_map(|frame| match frame {
                    Frame::Aggregate(mut fields) => fields.remove(index).unwrap_or_default(),
                    _ => Vec::new(),
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Records that the value bound to `local` may contain the given stack parts.
    pub(super) fn bind_frames(&mut self, local: &Local, frames: Vec<Frame>) {
        if !frames.is_empty() {
            let slot = self.locals[&local.id].clone();
            self.frame_slots.insert(slot, frames.clone());
            self.frame_locals.insert(local.id, frames);
        }
    }

    /// Reads an operand that is only inspected; temporaries may use stack storage.
    pub(super) fn read_operand(&mut self, expression: &TypedExpr) -> (String, Vec<Frame>) {
        if Self::is_place(expression) {
            (self.read_place(expression, false, false), Vec::new())
        } else {
            self.frame_value(expression)
        }
    }

    /// Takes an operand that is destroyed, or only lent out, before this frame's storage is reused.
    pub(super) fn take_operand(&mut self, expression: &TypedExpr) -> (String, Vec<Frame>) {
        if Self::is_place(expression) {
            let frames = if self.clones_on_take(expression) {
                Vec::new()
            } else {
                self.frame_of_place(expression)
            };
            (self.read_place(expression, true, false), frames)
        } else {
            self.frame_value(expression)
        }
    }

    /// Releases an operand from `read_operand`; places stay with their owner.
    pub(super) fn release_operand(
        &mut self,
        expression: &TypedExpr,
        value: &str,
        frames: &[Frame],
    ) {
        if !Self::is_place(expression) {
            self.drop_framed(&expression.ty, value, frames);
        }
    }

    fn field_types(&self, ty: &Type) -> Option<Vec<Type>> {
        match ty {
            Type::Record(id, arguments) => Some(self.module.types().record_fields(*id, arguments)),
            Type::Tuple(elements) => Some(elements.clone()),
            _ => None,
        }
    }

    /// Candidate stack parts of each field, merged over the aggregate's candidates.
    fn field_frames(frames: &[Frame]) -> BTreeMap<usize, Vec<Frame>> {
        let mut fields: BTreeMap<usize, Vec<Frame>> = BTreeMap::new();
        for frame in frames {
            let Frame::Aggregate(map) = frame else {
                unreachable!("record and tuple values only have aggregate frames")
            };
            for (index, candidates) in map {
                fields
                    .entry(*index)
                    .or_default()
                    .extend(candidates.iter().cloned());
            }
        }
        fields
    }

    /// True when `value` still holds exactly this frame's storage.
    fn frame_test(&mut self, ty: &Type, value: &str, frame: &Frame) -> String {
        let (address, length) = match frame {
            Frame::Array { buffer, elements } => (buffer.as_str(), elements.len()),
            Frame::List { nodes, elements } => (nodes.as_str(), elements.len()),
            Frame::String { data, length } => (data.as_str(), *length),
            Frame::Aggregate(_) => unreachable!("aggregates are tested field by field"),
        };
        let llvm = self.ty(ty);
        let pointer = self.value(format!("extractvalue {llvm} {value}, 0"));
        let same = self.value(format!("icmp eq ptr {pointer}, {address}"));
        // A moved-out slot is zero; the length keeps it distinct even from an alloca at address 0.
        let actual = self.value(format!("extractvalue {llvm} {value}, 1"));
        let whole = self.value(format!("icmp eq i64 {actual}, {length}"));
        self.value(format!("and i1 {same}, {whole}"))
    }

    /// Copies the stack parts of `value` to the heap so that it can leave this frame.
    pub(super) fn relocate(&mut self, ty: &Type, value: &str, frames: &[Frame]) -> String {
        if frames.is_empty() {
            return value.to_owned();
        }
        if let Some(types) = self.field_types(ty) {
            let llvm = self.ty(ty);
            let mut result = value.to_owned();
            for (index, candidates) in Self::field_frames(frames) {
                let field = self.value(format!("extractvalue {llvm} {value}, {index}"));
                let moved = self.relocate(&types[index], &field, &candidates);
                result = self.value(format!(
                    "insertvalue {llvm} {result}, {} {moved}, {index}",
                    self.ty(&types[index])
                ));
            }
            return result;
        }
        let done = self.label();
        let mut incoming = Vec::new();
        for frame in frames {
            let test = self.frame_test(ty, value, frame);
            let copy = self.label();
            let next = self.label();
            self.branch(&test, &copy, &next);
            self.begin(&copy);
            let moved = self.heap_copy(ty, frame);
            incoming.push(format!("[ {moved}, %{} ]", self.block));
            self.jump(&done);
            self.begin(&next);
        }
        incoming.push(format!("[ {value}, %{} ]", self.block));
        self.jump(&done);
        self.begin(&done);
        self.value(format!("phi {} {}", self.ty(ty), incoming.join(", ")))
    }

    /// Moves a frame's elements into new heap storage; element ownership moves along bitwise.
    fn heap_copy(&mut self, ty: &Type, frame: &Frame) -> String {
        match (ty, frame) {
            (Type::String | Type::Utf8String, Frame::String { data, length }) => {
                let ty = self.ty(ty);
                self.value(format!(
                    "call {ty} @{}.new(ptr {data}, i64 {length})",
                    &ty[1..]
                ))
            }
            (Type::Array(element), Frame::Array { buffer, elements }) => {
                let length = elements.len().to_string();
                let (array, data) = self.allocate_array(element, &length);
                let llvm = self.ty(element);
                self.array_loop(&length, |emitter, index| {
                    let source = emitter.element_pointer(element, buffer, index);
                    let value = emitter.value(format!("load {llvm}, ptr {source}"));
                    let target = emitter.element_pointer(element, &data, index);
                    emitter.instruction(format!("store {llvm} {value}, ptr {target}"));
                });
                for (index, candidates) in elements.iter().enumerate() {
                    if candidates.is_empty() {
                        continue;
                    }
                    let pointer = self.element_pointer(element, &data, &index.to_string());
                    let value = self.value(format!("load {llvm}, ptr {pointer}"));
                    let moved = self.relocate(element, &value, candidates);
                    self.instruction(format!("store {llvm} {moved}, ptr {pointer}"));
                }
                array
            }
            (Type::List(element), Frame::List { nodes, elements }) => {
                let length = elements.len().to_string();
                let llvm = self.ty(element);
                let (head, tail) = self.list_builder();
                if elements.iter().all(Vec::is_empty) {
                    self.list_loop(nodes, &length, |emitter, node| {
                        let pointer = emitter.list_element_pointer(element, node);
                        let value = emitter.value(format!("load {llvm}, ptr {pointer}"));
                        emitter.append_list(element, &tail, &value);
                    });
                } else {
                    for (index, candidates) in elements.iter().enumerate() {
                        let node = self.frame_node(element, nodes, index);
                        let pointer = self.list_element_pointer(element, &node);
                        let value = self.value(format!("load {llvm}, ptr {pointer}"));
                        let moved = self.relocate(element, &value, candidates);
                        self.append_list(element, &tail, &moved);
                    }
                }
                self.finish_list(&head, &length)
            }
            _ => unreachable!("frame kinds follow their value types"),
        }
    }

    /// Drops a value that may still use the given stack parts; stack storage is never freed.
    pub(super) fn drop_framed(&mut self, ty: &Type, value: &str, frames: &[Frame]) {
        if frames.is_empty() {
            return self.drop_value(ty, value);
        }
        if let Some(types) = self.field_types(ty) {
            let llvm = self.ty(ty);
            let fields = Self::field_frames(frames);
            for (index, field) in types.iter().enumerate() {
                if !field.needs_drop(&self.module.types()) {
                    continue;
                }
                let extracted = self.value(format!("extractvalue {llvm} {value}, {index}"));
                match fields.get(&index) {
                    Some(candidates) => self.drop_framed(field, &extracted, candidates),
                    None => self.drop_value(field, &extracted),
                }
            }
            return;
        }
        let done = self.label();
        for frame in frames {
            let test = self.frame_test(ty, value, frame);
            let stack = self.label();
            let next = self.label();
            self.branch(&test, &stack, &next);
            self.begin(&stack);
            self.drop_frame_contents(ty, frame);
            self.jump(&done);
            self.begin(&next);
        }
        self.drop_value(ty, value);
        self.jump(&done);
        self.begin(&done);
    }

    fn drop_frame_contents(&mut self, ty: &Type, frame: &Frame) {
        match (ty, frame) {
            (Type::String | Type::Utf8String, Frame::String { .. }) => {}
            (Type::Array(element), Frame::Array { buffer, elements }) => {
                if !element.needs_drop(&self.module.types()) {
                    return;
                }
                let llvm = self.ty(element);
                if elements.iter().all(Vec::is_empty) {
                    self.array_loop(&elements.len().to_string(), |emitter, index| {
                        let pointer = emitter.element_pointer(element, buffer, index);
                        let value = emitter.value(format!("load {llvm}, ptr {pointer}"));
                        emitter.drop_value(element, &value);
                    });
                } else {
                    for (index, candidates) in elements.iter().enumerate() {
                        let pointer = self.element_pointer(element, buffer, &index.to_string());
                        let value = self.value(format!("load {llvm}, ptr {pointer}"));
                        self.drop_framed(element, &value, candidates);
                    }
                }
            }
            (Type::List(element), Frame::List { nodes, elements }) => {
                if !element.needs_drop(&self.module.types()) {
                    return;
                }
                let llvm = self.ty(element);
                if elements.iter().all(Vec::is_empty) {
                    self.list_loop(nodes, &elements.len().to_string(), |emitter, node| {
                        let pointer = emitter.list_element_pointer(element, node);
                        let value = emitter.value(format!("load {llvm}, ptr {pointer}"));
                        emitter.drop_value(element, &value);
                    });
                } else {
                    for (index, candidates) in elements.iter().enumerate() {
                        let node = self.frame_node(element, nodes, index);
                        let pointer = self.list_element_pointer(element, &node);
                        let value = self.value(format!("load {llvm}, ptr {pointer}"));
                        self.drop_framed(element, &value, candidates);
                    }
                }
            }
            _ => unreachable!("frame kinds follow their value types"),
        }
    }
}
