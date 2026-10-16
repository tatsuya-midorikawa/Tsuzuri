//! Value layout of unions (PM01): how a union stores its tag and payload, the models of how many
//! bytes a type takes, and the emitter methods that read and write a union's tag. Every reader of
//! a tag goes through `tag_value` or `load_tag`, every writer through `construct_value` or, in
//! text IR, `tag_type`, so the width of the tag is decided in one place.
use super::*;

/// How a union instance stores its tag and payload.
pub(crate) enum UnionLayout {
    /// Every case is nullary, so the value is its `i32` tag.
    Enum,
    /// Every payload has the LLVM type of this one: `{ i32, T }`.
    Common(Type),
    /// Payloads of different LLVM types share `{ i32, [K x i128] }` storage
    /// that holds the largest payload.
    General(usize),
}

impl UnionLayout {
    /// Whether a value of this layout is an LLVM aggregate that holds a tag and a payload
    /// (`Common`, `General`). The others are not: an enum is its tag alone. Passes that only need
    /// the coarse shape of a union value, such as a return slot for aggregates, ask this.
    pub(crate) fn is_aggregate(&self) -> bool {
        !matches!(self, UnionLayout::Enum)
    }
}

/// The bytes of the stored tag of a union with `cases` cases: the smallest of 1, 2 and 4 that
/// holds every tag (PM01 D1).
pub(crate) fn tag_bytes(cases: usize) -> usize {
    match cases {
        0..=0x100 => 1,
        0x101..=0x1_0000 => 2,
        _ => 4,
    }
}

/// The bytes of the stored tag of the union `ty`. The node of a recursive union keeps an `i32` tag
/// next to its header pointers.
pub(crate) fn tag_width(ty: &Type, module: &CheckedModule) -> usize {
    let Type::Union(id, _) = ty else {
        unreachable!("tags belong to union types")
    };
    if module.types().recursive(ty) {
        4
    } else {
        tag_bytes(module.unions[*id].cases.len())
    }
}

/// The LLVM type of the stored tag of the union `ty`.
pub(super) fn tag_type(ty: &Type, module: &CheckedModule) -> &'static str {
    match tag_width(ty, module) {
        1 => "i8",
        2 => "i16",
        _ => "i32",
    }
}

/// Size and alignment in bytes of a type's LLVM representation on 64-bit
/// targets with 16-byte `i128`, which bound those of wasm32 and older layouts.
pub(crate) fn storage_layout(ty: &Type, module: &CheckedModule) -> (usize, usize) {
    layout_bound(ty, module, false)
}

/// Portable lower bounds for reference guarantees, or the existing allocation upper bounds.
/// Explicit slots and heap blocks are aligned to 16, even for 256-bit SIMD values.
pub(crate) fn layout_bound(ty: &Type, module: &CheckedModule, lower: bool) -> (usize, usize) {
    let aggregate = |fields: &mut dyn Iterator<Item = (usize, usize)>| {
        let (size, align) = fields.fold((0usize, 1usize), |(size, align), (field, field_align)| {
            (
                size.next_multiple_of(field_align) + field,
                align.max(field_align),
            )
        });
        (size.next_multiple_of(align), align)
    };
    match ty {
        Type::Simd(vector) => (
            vector.bytes(),
            if lower {
                vector.bytes().min(8)
            } else {
                vector.bytes()
            },
        ),
        Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => {
            let bytes = usize::from(*bits).div_ceil(8);
            (bytes, if lower { bytes.min(8) } else { bytes })
        }
        Type::Bool | Type::Unit => (1, 1),
        Type::Char => (2, 2),
        Type::Utf8Char => (4, 4),
        Type::String | Type::Utf8String | Type::Array(_) | Type::List(_) => (16, 8),
        Type::Function(..) | Type::Task(_) => {
            if lower {
                (16, 4)
            } else {
                (32, 8)
            }
        }
        Type::Vec(_) => (24, 8),
        Type::Dyn(_) => {
            if lower {
                (8, 4)
            } else {
                (16, 8)
            }
        }
        Type::Reference(..) if ty.slice_element().is_some() => (16, 8),
        Type::Reference(..) | Type::Handle(_) | Type::Shared(..) => {
            if lower {
                (4, 4)
            } else {
                (8, 8)
            }
        }
        Type::ArrayView(_) => unreachable!("an exclusive slice target is not a value"),
        Type::Length(_) => unreachable!("a length is not a value"),
        // The stride of `[N x T]` is the element's size, already a multiple of its alignment.
        Type::FixedArray(element, _) => {
            let (size, align) = layout_bound(element, module, lower);
            (size * fixed_length(ty), align)
        }
        Type::Tuple(elements) => {
            aggregate(&mut elements.iter().map(|ty| layout_bound(ty, module, lower)))
        }
        Type::Record(id, arguments) => aggregate(
            &mut module
                .types()
                .record_fields(*id, arguments)
                .iter()
                .map(|ty| layout_bound(ty, module, lower))
                .chain(drop_flag(ty, module).map(|_| (1, 1))),
        ),
        Type::Union(..) if module.types().recursive(ty) => {
            if lower {
                (4, 4)
            } else {
                (8, 8)
            }
        }
        Type::Union(id, arguments) => {
            let tag = (tag_width(ty, module), tag_width(ty, module));
            match union_layout(*id, arguments, module) {
                UnionLayout::Enum => tag,
                UnionLayout::Common(payload) => aggregate(
                    &mut [tag, layout_bound(&payload, module, lower)]
                        .into_iter()
                        .chain(drop_flag(ty, module).map(|_| (1, 1))),
                ),
                UnionLayout::General(count) if drop_flag(ty, module).is_some() => aggregate(
                    &mut [tag, (16 * count, if lower { 8 } else { 16 }), (1, 1)].into_iter(),
                ),
                UnionLayout::General(count) => {
                    if lower {
                        (8 + 16 * count, 8)
                    } else {
                        (16 + 16 * count, 16)
                    }
                }
            }
        }
        Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => {
            unreachable!("erroneous and polymorphic types cannot reach LLVM")
        }
    }
}

pub(crate) fn union_layout(id: usize, arguments: &[Type], module: &CheckedModule) -> UnionLayout {
    let payloads: Vec<_> = module
        .types()
        .union_payloads(id, arguments)
        .into_iter()
        .flatten()
        .collect();
    let Some(first) = payloads.first() else {
        // A Drop union keeps its live flag after the tag, so it is never a bare tag.
        return if module.unions[id].user_drop {
            UnionLayout::General(0)
        } else {
            UnionLayout::Enum
        };
    };
    let llvm = llvm_type(first, module);
    if payloads.iter().all(|ty| llvm_type(ty, module) == llvm) {
        return UnionLayout::Common(first.clone());
    }
    let bytes = payloads
        .iter()
        .map(|ty| storage_layout(ty, module).0)
        .max()
        .unwrap_or(0);
    UnionLayout::General(bytes.div_ceil(16))
}

/// The index of the live flag (`i8 1`) that follows the fields of a Drop record, or the tag and
/// payload of a non-recursive Drop union. Moved-out storage is zero, which clears the flag, so
/// the drop glue skips the user drop there (B07). Recursive Drop unions use a null node instead.
pub(crate) fn drop_flag(ty: &Type, module: &CheckedModule) -> Option<usize> {
    match ty {
        Type::Record(id, _) if module.records[*id].user_drop => {
            Some(module.records[*id].fields.len())
        }
        Type::Union(id, _) if module.unions[*id].user_drop && !module.types().recursive(ty) => {
            Some(2)
        }
        _ => None,
    }
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn union_layout(&self, ty: &Type) -> UnionLayout {
        let Type::Union(id, arguments) = ty else {
            unreachable!("union layouts belong to union types")
        };
        union_layout(*id, arguments, self.module)
    }

    /// The payload storage of the union stored at `slot`, typed by each case's load or store.
    pub(super) fn payload_pointer(&mut self, ty: &Type, slot: &str) -> String {
        if self.module.types().recursive(ty) {
            let node = self.value(format!("load ptr, ptr {slot}"));
            return self.recursive_payload(ty, &node);
        }
        self.value(format!(
            "getelementptr inbounds {}, ptr {slot}, i32 0, i32 1",
            self.ty(ty)
        ))
    }

    pub(super) fn construct(
        &mut self,
        ty: &Type,
        case_id: usize,
        payload: Option<&TypedExpr>,
    ) -> String {
        let payload = payload.map(|payload| (self.expression(payload), self.ty(&payload.ty)));
        self.construct_value(ty, case_id, payload)
    }

    /// The union value of case `case_id` with the payload `(value, llvm type)`: the one place that
    /// writes a tag.
    pub(super) fn construct_value(
        &mut self,
        ty: &Type,
        case_id: usize,
        payload: Option<(String, String)>,
    ) -> String {
        if self.module.types().recursive(ty) {
            return self.recursive_construct(ty, case_id, payload);
        }
        let layout = self.union_layout(ty);
        if !layout.is_aggregate() {
            return case_id.to_string();
        }
        let llvm = self.ty(ty);
        let tag = tag_type(ty, self.module);
        let tagged = self.value(format!(
            "insertvalue {llvm} zeroinitializer, {tag} {case_id}, 0"
        ));
        let tagged = self.mark_live(ty, tagged);
        let Some((value, payload_type)) = payload else {
            return tagged;
        };
        if let UnionLayout::Common(_) = layout {
            return self.value(format!(
                "insertvalue {llvm} {tagged}, {payload_type} {value}, 1"
            ));
        }
        let slot = self.spill(ty, &tagged);
        let pointer = self.payload_pointer(ty, &slot);
        self.instruction(format!("store {payload_type} {value}, ptr {pointer}"));
        self.value(format!("load {llvm}, ptr {slot}"))
    }

    /// The tag of the SSA union value `union` of type `ty`, as `i32`.
    pub(super) fn tag_value(&mut self, ty: &Type, union: &str) -> String {
        if self.module.types().recursive(ty) {
            return self.recursive_tag(ty, union);
        }
        let tag = if matches!(self.union_layout(ty), UnionLayout::Enum) {
            union.to_owned()
        } else {
            self.value(format!("extractvalue {} {union}, 0", self.ty(ty)))
        };
        self.widen_tag(ty, tag)
    }

    /// The tag of the union of type `ty` stored at `slot`, as `i32`. A recursive union's slot holds
    /// the pointer to its node.
    pub(super) fn load_tag(&mut self, ty: &Type, slot: &str) -> String {
        if self.module.types().recursive(ty) {
            let node = self.value(format!("load ptr, ptr {slot}"));
            return self.recursive_tag(ty, &node);
        }
        // The tag is the first field, so it is also the value of an enum-like union.
        let tag = self.value(format!("load {}, ptr {slot}", tag_type(ty, self.module)));
        self.widen_tag(ty, tag)
    }

    /// `tag`, stored in the width of `ty`'s tag, as `i32`.
    fn widen_tag(&mut self, ty: &Type, tag: String) -> String {
        match tag_type(ty, self.module) {
            "i32" => tag,
            narrow => self.value(format!("zext {narrow} {tag} to i32")),
        }
    }

    pub(super) fn union_tag(&mut self, value: &TypedExpr) -> String {
        if self.module.types().recursive(&value.ty) {
            let (union, frames) = self.read_operand(value);
            let tag = self.recursive_tag(&value.ty, &union);
            self.release_operand(value, &union, &frames);
            return tag;
        }
        if Self::is_place(value) {
            let slot = self.place(value);
            return self.load_tag(&value.ty, &slot);
        }
        let (union, frames) = self.read_operand(value);
        let tag = self.tag_value(&value.ty, &union);
        self.release_operand(value, &union, &frames);
        tag
    }

    pub(super) fn payload_value(&mut self, ty: &Type, union: &str, payload: &Type) -> String {
        if self.module.types().recursive(ty) {
            let pointer = self.recursive_payload(ty, union);
            return self.value(format!("load {}, ptr {pointer}", self.ty(payload)));
        }
        match self.union_layout(ty) {
            UnionLayout::Enum => unreachable!("nullary cases have no payload"),
            UnionLayout::Common(_) => {
                self.value(format!("extractvalue {} {union}, 1", self.ty(ty)))
            }
            UnionLayout::General(_) => {
                let slot = self.spill(ty, union);
                let pointer = self.payload_pointer(ty, &slot);
                self.value(format!("load {}, ptr {pointer}", self.ty(payload)))
            }
        }
    }

    /// Cases whose payload owns resources, by tag.
    pub(super) fn owning_cases(&self, ty: &Type) -> Vec<(usize, Type)> {
        let Type::Union(id, arguments) = ty else {
            unreachable!("union cases belong to union types")
        };
        let types = self.module.types();
        types
            .union_payloads(*id, arguments)
            .into_iter()
            .enumerate()
            .filter_map(|(case, payload)| Some((case, payload.filter(|ty| ty.needs_drop(&types))?)))
            .collect()
    }

    /// Branches on the tag of `value` to one new block per case; other tags reach the returned
    /// join block.
    pub(super) fn case_switch(
        &mut self,
        ty: &Type,
        value: &str,
        cases: &[(usize, Type)],
    ) -> (Vec<String>, String) {
        let tag = self.tag_value(ty, value);
        let labels: Vec<_> = cases.iter().map(|_| self.label()).collect();
        let done = self.label();
        self.instruction(format!("switch i32 {tag}, label %{done} ["));
        for ((case, _), label) in cases.iter().zip(&labels) {
            self.instruction(format!("  i32 {case}, label %{label}"));
        }
        self.instruction("]");
        (labels, done)
    }

    /// The bytes that `tz.alloc` gets for one heap cell of `ty`: its LLVM size, which is exact on
    /// every target, but at least 1.
    pub(super) fn cell_size(&mut self, ty: &Type) -> String {
        let size = format!(
            "ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64)",
            self.ty(ty)
        );
        let empty = self.value(format!("icmp eq i64 {size}, 0"));
        self.value(format!("select i1 {empty}, i64 1, i64 {size}"))
    }
}

#[cfg(test)]
mod tests {
    use super::tag_bytes;

    #[test]
    fn tag_bytes_follow_case_count() {
        for (cases, bytes) in [
            (0, 1),
            (1, 1),
            (2, 1),
            (255, 1),
            (256, 1),
            (257, 2),
            (65_535, 2),
            (65_536, 2),
            (65_537, 4),
            (1 << 24, 4),
        ] {
            assert_eq!(tag_bytes(cases), bytes, "{cases} cases");
        }
    }
}
