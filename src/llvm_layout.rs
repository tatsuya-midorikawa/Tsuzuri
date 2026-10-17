//! Value layout (PM01): how a union stores its tag and payload (`UnionLayout`, with the niches),
//! in what order a record keeps its fields (`record_slots`), the models of how many bytes a type
//! takes, and the emitter methods that read and write a union's tag. Every reader of a tag goes
//! through `tag_value` or `load_tag`, every writer through `construct_value` or, in text IR,
//! `tag_type`, so the width of the tag is decided in one place; every reader and writer of a
//! record's field goes through `field_slot`. A layout that depends on the target reads it from
//! `target_scope`, which `emit_program` opens, together with the memo of the layouts it works out.
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// The encoding of an option-like union that stores no tag (PM01 D5): one value of the payload
/// type that the payload never has stands for the nullary case.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Niche {
    /// A string or array `{ ptr, i64 }`: a negative length is the nullary case, as in
    /// `{ ptr null, i64 -1 }`. No string or array has one.
    NegativeLength,
    /// A reference to one value, a single `ptr`: the null pointer is the nullary case. No
    /// reference is null, but linear memory starts at address 0, where `--stack-first` can put a
    /// valid object, so WebAssembly has no such niche (see `webassembly`).
    NullPointer,
}

impl Niche {
    /// The constant that stands for the nullary case.
    fn nullary(self) -> &'static str {
        match self {
            Niche::NegativeLength => "{ ptr null, i64 -1 }",
            Niche::NullPointer => "null",
        }
    }
}

thread_local! {
    /// Whether the program that this thread is emitting targets WebAssembly. Layouts that depend
    /// on the target (`Niche::NullPointer`) read it, because `union_layout` and `llvm_type` are
    /// called with a module alone. Outside `target_scope` the target is native.
    static WEBASSEMBLY: Cell<bool> = const { Cell::new(false) };
    /// The layouts that the program being emitted has worked out so far (see `Memo`).
    static MEMO: RefCell<Option<Memo>> = const { RefCell::new(None) };
}

/// The layouts of one kind of type, found by its id and then by its type arguments.
type Table<T> = HashMap<usize, Vec<(Box<[Type]>, T)>>;

/// The layouts of the unions and records of one module, for one target. A layout is a pure
/// function of the module, the type and the target, and the emitter asks for it at every use of a
/// type, so it is worked out once. The memo lives as long as its `target_scope`, which is shorter
/// than the module it belongs to.
#[derive(Default)]
struct Memo {
    /// The address of the module that the entries belong to.
    module: usize,
    unions: Table<UnionLayout>,
    records: Table<Option<Rc<[usize]>>>,
}

/// The result of `compute` for the type `id` at `arguments`, from the table that `table` picks
/// when the current scope has seen it. Nothing is borrowed while `compute` runs, because it asks
/// for the layouts of other types.
fn memoized<T: Clone>(
    module: &CheckedModule,
    id: usize,
    arguments: &[Type],
    table: fn(&mut Memo) -> &mut Table<T>,
    compute: impl FnOnce() -> T,
) -> T {
    let address = std::ptr::from_ref(module) as usize;
    let seen = MEMO.with_borrow_mut(|memo| {
        let memo = memo.as_mut()?;
        if memo.module != address {
            *memo = Memo {
                module: address,
                ..Memo::default()
            };
        }
        table(memo)
            .get(&id)?
            .iter()
            .find(|(key, _)| **key == *arguments)
            .map(|(_, value)| value.clone())
    });
    if let Some(value) = seen {
        return value;
    }
    let value = compute();
    MEMO.with_borrow_mut(|memo| {
        if let Some(memo) = memo.as_mut().filter(|memo| memo.module == address) {
            table(memo)
                .entry(id)
                .or_default()
                .push((arguments.into(), value.clone()));
        }
    });
    value
}

/// Marks the emission of a program for WebAssembly (`wasm`) or a native target until the guard is
/// dropped; the previous target and layouts come back then.
pub(crate) fn target_scope(wasm: bool) -> TargetScope {
    TargetScope {
        wasm: WEBASSEMBLY.replace(wasm),
        memo: MEMO.replace(Some(Memo::default())),
    }
}

pub(crate) struct TargetScope {
    wasm: bool,
    memo: Option<Memo>,
}

impl Drop for TargetScope {
    fn drop(&mut self) {
        WEBASSEMBLY.set(self.wasm);
        MEMO.set(self.memo.take());
    }
}

/// Whether the program being emitted targets WebAssembly.
fn webassembly() -> bool {
    WEBASSEMBLY.get()
}

/// How a union instance stores its tag and payload.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UnionLayout {
    /// Every case is nullary, so the value is its tag.
    Enum,
    /// Every payload has the LLVM type of this one: `{ iT, T }`.
    Common(Type),
    /// Payloads of different LLVM types share an area of `units` integers of `unit` bytes,
    /// `{ iT, [units x i{8*unit}] }`. `unit` is the largest alignment of a payload, at most 16 (the
    /// one an `i128` has), so every payload stores at the area's own alignment, and `units`
    /// make the area hold the largest payload.
    General { unit: usize, units: usize },
    /// Two cases, case `none` without a payload and case `some` with one: the value is the payload
    /// itself, with no tag, and `niche` tells the nullary case from every payload. The LLVM type
    /// of the union is the payload's, and the payload lives in the union's own storage.
    Niche {
        niche: Niche,
        payload: Type,
        none: usize,
        some: usize,
    },
}

impl UnionLayout {
    /// Whether a value of this layout is an LLVM aggregate that holds a tag and a payload
    /// (`Common`, `General`). An enum is its tag alone and a niche union is its payload alone, so
    /// neither is one. Passes that only need the coarse shape of a union value, such as a return
    /// slot for aggregates, ask this.
    pub(crate) fn is_aggregate(&self) -> bool {
        matches!(self, UnionLayout::Common(_) | UnionLayout::General { .. })
    }

    /// The LLVM type of the payload area of a `General` layout.
    fn area(unit: usize, units: usize) -> String {
        format!("[{units} x i{}]", 8 * unit)
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

/// The size and alignment of a structure that stores `fields`, each a `(size, alignment)`, in this
/// order.
fn aggregate(fields: &mut dyn Iterator<Item = (usize, usize)>) -> (usize, usize) {
    let (size, align) = fields.fold((0usize, 1usize), |(size, align), (field, field_align)| {
        (
            size.next_multiple_of(field_align) + field,
            align.max(field_align),
        )
    });
    (size.next_multiple_of(align), align)
}

/// Portable lower bounds for reference guarantees, or the existing allocation upper bounds.
/// Explicit slots and heap blocks are aligned to 16, even for 256-bit SIMD values.
pub(crate) fn layout_bound(ty: &Type, module: &CheckedModule, lower: bool) -> (usize, usize) {
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
        Type::Record(..) => aggregate(
            &mut physical_fields(ty, module)
                .iter()
                .map(|(_, field)| layout_bound(field, module, lower))
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
                // The area aligns like its integers: exactly `unit` up to 8 bytes, and 16 for an
                // `i128`, which older x86-64 LLVM aligns to 8.
                UnionLayout::General { unit, units } => aggregate(
                    &mut [
                        tag,
                        (unit * units, if lower && unit == 16 { 8 } else { unit }),
                    ]
                    .into_iter()
                    .chain(drop_flag(ty, module).map(|_| (1, 1))),
                ),
                UnionLayout::Niche { payload, .. } => layout_bound(&payload, module, lower),
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
    memoized(
        module,
        id,
        arguments,
        |memo| &mut memo.unions,
        || work_out_union_layout(id, arguments, module),
    )
}

fn work_out_union_layout(id: usize, arguments: &[Type], module: &CheckedModule) -> UnionLayout {
    let cases = module.types().union_payloads(id, arguments);
    if let Some(niche) = niche_layout(id, arguments, &cases, module) {
        return niche;
    }
    let payloads: Vec<_> = cases.into_iter().flatten().collect();
    let Some(first) = payloads.first() else {
        // A Drop union keeps its live flag after the tag, so it is never a bare tag.
        return if module.unions[id].user_drop {
            UnionLayout::General { unit: 1, units: 0 }
        } else {
            UnionLayout::Enum
        };
    };
    let llvm = llvm_type(first, module);
    if payloads.iter().all(|ty| llvm_type(ty, module) == llvm) {
        return UnionLayout::Common(first.clone());
    }
    let (bytes, alignment) = payloads.iter().map(|ty| storage_layout(ty, module)).fold(
        (0, 1),
        |(bytes, alignment): (usize, usize), (size, align)| (bytes.max(size), alignment.max(align)),
    );
    let unit = alignment.min(16);
    UnionLayout::General {
        unit,
        units: bytes.div_ceil(unit),
    }
}

/// The niche layout of union `id` at `arguments`, if it has one: exactly two cases, one nullary and
/// one whose payload is a string, `utf8string`, array, or (outside WebAssembly) reference to one
/// value, in a union that is neither recursive (its value is a node pointer) nor a Drop union (it
/// keeps a live flag). `cases` are the payloads of its cases.
fn niche_layout(
    id: usize,
    arguments: &[Type],
    cases: &[Option<Type>],
    module: &CheckedModule,
) -> Option<UnionLayout> {
    let (none, some, payload) = match cases {
        [None, Some(payload)] => (0, 1, payload),
        [Some(payload), None] => (1, 0, payload),
        _ => return None,
    };
    let niche = match payload {
        Type::String | Type::Utf8String | Type::Array(_) => Niche::NegativeLength,
        // A reference to a slice is a pointer and a length, which is not this niche.
        Type::Reference(..) if payload.slice_element().is_none() && !webassembly() => {
            Niche::NullPointer
        }
        _ => return None,
    };
    if module.unions[id].user_drop || module.types().recursive(&Type::Union(id, arguments.into())) {
        return None;
    }
    Some(UnionLayout::Niche {
        niche,
        payload: payload.clone(),
        none,
        some,
    })
}

/// The body of the LLVM type definition of the non-recursive union `ty`.
pub(super) fn union_body(ty: &Type, module: &CheckedModule) -> String {
    let Type::Union(id, arguments) = ty else {
        unreachable!("only union types are defined here")
    };
    let tag = tag_type(ty, module);
    let flag = if drop_flag(ty, module).is_some() {
        ", i8"
    } else {
        ""
    };
    match union_layout(*id, arguments, module) {
        UnionLayout::Enum => tag.into(),
        UnionLayout::Common(payload) => {
            format!("{{ {tag}, {}{flag} }}", llvm_type(&payload, module))
        }
        UnionLayout::General { unit, units } => {
            format!("{{ {tag}, {}{flag} }}", UnionLayout::area(unit, units))
        }
        UnionLayout::Niche { .. } => {
            unreachable!("a niche union is its payload and has no definition")
        }
    }
}

/// The type of the payload storage that ends the node of the recursive union `ty`.
pub(super) fn node_region(ty: &Type, module: &CheckedModule) -> String {
    let Type::Union(id, arguments) = ty else {
        unreachable!("only union types have nodes")
    };
    match union_layout(*id, arguments, module) {
        UnionLayout::Enum => "i8".into(),
        UnionLayout::Common(payload) => llvm_type(&payload, module),
        UnionLayout::General { unit, units } => UnionLayout::area(unit, units),
        UnionLayout::Niche { .. } => unreachable!("a recursive union has no niche"),
    }
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

/// The position in the LLVM type of each logical field of the record `id` at `arguments`:
/// `slots[index]` is where field `index` lives (PM01 D7, D8). `None` keeps the declared order.
/// Every reader and writer of a field goes through `field_slot`, so the layout of a record is
/// decided here and nowhere else, and everything the program can observe (the order of
/// evaluation, drops and clones, the derived comparison, hash and display, the debug info and the
/// C struct of a public record) stays in the declared order.
///
/// A record of the user's program stores its fields by decreasing alignment when that makes the
/// model of its size (`layout_bound`) strictly smaller; fields of one alignment keep their declared
/// order, and the live flag of a Drop record stays last. A record of the standard library keeps its
/// order: the runtime and hand-written IR index it (`Seq`, `Mutex`, `Channel`, `Matrix`, ...).
pub(crate) fn record_slots(
    id: usize,
    arguments: &[Type],
    module: &CheckedModule,
) -> Option<Rc<[usize]>> {
    let record = &module.records[id];
    if record.origin != ModuleOrigin::User || record.fields.len() < 2 {
        return None;
    }
    memoized(
        module,
        id,
        arguments,
        |memo| &mut memo.records,
        || {
            let fields: Vec<_> = module
                .types()
                .record_fields(id, arguments)
                .iter()
                .map(|ty| layout_bound(ty, module, false))
                .collect();
            place_fields(&fields, record.user_drop)
        },
    )
}

/// The slots that store `fields`, each the `(size, alignment)` model of a field in declared order,
/// by decreasing alignment, or `None` when that takes no fewer bytes than the declared order.
/// `flag` adds the live flag of a Drop record after the fields.
fn place_fields(fields: &[(usize, usize)], flag: bool) -> Option<Rc<[usize]>> {
    let size = |order: &mut dyn Iterator<Item = usize>| {
        aggregate(
            &mut order
                .map(|index| fields[index])
                .chain(flag.then_some((1, 1))),
        )
        .0
    };
    let mut order: Vec<usize> = (0..fields.len()).collect();
    // The sort is stable.
    order.sort_by_key(|&index| std::cmp::Reverse(fields[index].1));
    if size(&mut order.iter().copied()) >= size(&mut (0..fields.len())) {
        return None;
    }
    let mut slots = vec![0; fields.len()];
    for (slot, index) in order.into_iter().enumerate() {
        slots[index] = slot;
    }
    Some(slots.into())
}

/// The position in the LLVM type of the field `index` of the record `id` at `arguments`.
pub(crate) fn field_slot(
    id: usize,
    arguments: &[Type],
    module: &CheckedModule,
    index: usize,
) -> usize {
    record_slots(id, arguments, module).map_or(index, |slots| slots[index])
}

/// The fields of the record type `ty` as `(logical index, type)`, in the order of its LLVM type.
pub(super) fn physical_fields(ty: &Type, module: &CheckedModule) -> Vec<(usize, Type)> {
    let Type::Record(id, arguments) = ty else {
        unreachable!("only a record has fields in its type")
    };
    let mut fields: Vec<_> = module
        .types()
        .record_fields(*id, arguments)
        .into_iter()
        .enumerate()
        .collect();
    if let Some(slots) = record_slots(*id, arguments, module) {
        fields.sort_by_key(|&(index, _)| slots[index]);
    }
    fields
}

impl FunctionEmitter<'_, '_> {
    /// The position in the LLVM type of `ty` of its field `index`: a record may keep its fields
    /// out of order, while a tuple and every other aggregate keeps them as declared.
    pub(super) fn field_slot(&self, ty: &Type, index: usize) -> usize {
        match ty {
            Type::Record(id, arguments) => field_slot(*id, arguments, self.module, index),
            _ => index,
        }
    }

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
        // A niche union is its payload: both live at the union's own address.
        if matches!(self.union_layout(ty), UnionLayout::Niche { .. }) {
            return slot.to_owned();
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
        if let UnionLayout::Niche { niche, .. } = &layout {
            // A niche union is its payload, or the constant that no payload equals.
            return match payload {
                Some((value, _)) => value,
                None => niche.nullary().to_owned(),
            };
        }
        if !layout.is_aggregate() {
            // What is left is an enumeration, whose value is its tag.
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

    /// The value of the nullary case `name` of the union `ty`, built by its layout: a layout may
    /// encode the case in bytes other than zero, so the all-zero value is not `None`.
    pub(super) fn nullary_value(&mut self, ty: &Type, name: &str) -> String {
        let Type::Union(id, _) = ty else {
            unreachable!("cases belong to union types")
        };
        let case = self.module.unions[*id]
            .cases
            .iter()
            .position(|(case, payload)| case == name && payload.is_none())
            .unwrap_or_else(|| panic!("{name} is a nullary case of the standard union"));
        self.construct_value(ty, case, None)
    }

    /// The tag of the SSA union value `union` of type `ty`, as `i32`.
    pub(super) fn tag_value(&mut self, ty: &Type, union: &str) -> String {
        if self.module.types().recursive(ty) {
            return self.recursive_tag(ty, union);
        }
        let layout = self.union_layout(ty);
        if let UnionLayout::Niche {
            niche, none, some, ..
        } = layout
        {
            let field = match niche {
                Niche::NegativeLength => {
                    self.value(format!("extractvalue {} {union}, 1", self.ty(ty)))
                }
                // The union is its pointer.
                Niche::NullPointer => union.to_owned(),
            };
            return self.niche_tag(niche, &field, none, some);
        }
        let tag = if matches!(layout, UnionLayout::Enum) {
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
        if let UnionLayout::Niche {
            niche, none, some, ..
        } = self.union_layout(ty)
        {
            let field = match niche {
                Niche::NegativeLength => {
                    let pointer = self.value(format!(
                        "getelementptr inbounds {}, ptr {slot}, i32 0, i32 1",
                        self.ty(ty)
                    ));
                    self.value(format!("load i64, ptr {pointer}"))
                }
                Niche::NullPointer => self.value(format!("load ptr, ptr {slot}")),
            };
            return self.niche_tag(niche, &field, none, some);
        }
        // The tag is the first field, so it is also the value of an enum-like union.
        let tag = self.value(format!("load {}, ptr {slot}", tag_type(ty, self.module)));
        self.widen_tag(ty, tag)
    }

    /// The tag of a niche union, as `i32`, from the niche field `field` that it stores (the length
    /// or the pointer): `none` when the field holds the value of the nullary case, `some`
    /// otherwise.
    fn niche_tag(&mut self, niche: Niche, field: &str, none: usize, some: usize) -> String {
        let absent = match niche {
            Niche::NegativeLength => self.value(format!("icmp slt i64 {field}, 0")),
            Niche::NullPointer => self.value(format!("icmp eq ptr {field}, null")),
        };
        self.value(format!("select i1 {absent}, i32 {none}, i32 {some}"))
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
            UnionLayout::General { .. } => {
                let slot = self.spill(ty, union);
                let pointer = self.payload_pointer(ty, &slot);
                self.value(format!("load {}, ptr {pointer}", self.ty(payload)))
            }
            // A niche union is its payload.
            UnionLayout::Niche { .. } => union.to_owned(),
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
    use super::*;
    use std::collections::BTreeMap;

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

    /// A data layout reduced to what changes the size of the types that Tsuzuri emits: the size of
    /// a pointer and the alignment of `i128`. It is written down here, apart from the model
    /// under test.
    #[derive(Clone, Copy)]
    struct Machine {
        pointer: usize,
        wide: usize,
    }

    const MACHINES: [(&str, Machine); 5] = [
        (
            "aarch64",
            Machine {
                pointer: 8,
                wide: 16,
            },
        ),
        (
            "x86-64, LLVM 18 and later",
            Machine {
                pointer: 8,
                wide: 16,
            },
        ),
        (
            "x86-64, LLVM 17 and earlier",
            Machine {
                pointer: 8,
                wide: 8,
            },
        ),
        (
            "wasm32",
            Machine {
                pointer: 4,
                wide: 16,
            },
        ),
        (
            "wasm32, LLVM 17 and earlier",
            Machine {
                pointer: 4,
                wide: 8,
            },
        ),
    ];

    /// The fields of an aggregate body, split at the commas outside nested brackets and quotes.
    fn split_fields(body: &str) -> Vec<&str> {
        let (mut depth, mut quoted, mut start) = (0usize, false, 0);
        let mut fields = Vec::new();
        for (index, c) in body.char_indices() {
            match c {
                '"' => quoted = !quoted,
                '{' | '[' | '<' if !quoted => depth += 1,
                '}' | ']' | '>' if !quoted => depth -= 1,
                ',' if !quoted && depth == 0 => {
                    fields.push(body[start..index].trim());
                    start = index + 1;
                }
                _ => {}
            }
        }
        let last = body[start..].trim();
        if !last.is_empty() {
            fields.push(last);
        }
        fields
    }

    /// The size and alignment of the LLVM type `text` on `machine`, by the rules of a data layout.
    fn measure(text: &str, named: &BTreeMap<String, String>, machine: Machine) -> (usize, usize) {
        let text = text.trim();
        let between = |open, close| {
            text.strip_prefix(open)
                .and_then(|rest| rest.strip_suffix(close))
        };
        if let Some(body) = between('{', '}') {
            let (size, align) = split_fields(body)
                .into_iter()
                .map(|field| measure(field, named, machine))
                .fold((0usize, 1usize), |(size, align), (field, field_align)| {
                    (
                        size.next_multiple_of(field_align) + field,
                        align.max(field_align),
                    )
                });
            return (size.next_multiple_of(align), align);
        }
        if let Some(body) = between('[', ']') {
            let (count, element) = body.split_once(" x ").unwrap();
            let (size, align) = measure(element, named, machine);
            return (count.parse::<usize>().unwrap() * size, align);
        }
        if let Some(body) = between('<', '>') {
            let (count, element) = body.split_once(" x ").unwrap();
            let bytes = (count.parse::<usize>().unwrap() * measure(element, named, machine).0)
                .next_power_of_two();
            return (bytes, bytes);
        }
        if let Some(name) = text.strip_prefix('%') {
            let body = named
                .get(name.trim_matches('"'))
                .unwrap_or_else(|| panic!("no definition of {text}"));
            return measure(body, named, machine);
        }
        match text {
            "i1" | "i8" => (1, 1),
            "i16" | "half" => (2, 2),
            "i32" | "float" => (4, 4),
            "i64" | "double" => (8, 8),
            "i128" => (16, machine.wide),
            "fp128" => (16, 16),
            "ptr" => (machine.pointer, machine.pointer),
            other => panic!("the test does not model the LLVM type {other}"),
        }
    }

    /// The named types that `ir` defines, by name without the sigil and quotes.
    fn definitions(ir: &str) -> BTreeMap<String, String> {
        ir.lines()
            .filter_map(|line| {
                let (name, body) = line.strip_prefix('%')?.split_once(" = type ")?;
                (body != "opaque").then(|| (name.trim_matches('"').to_owned(), body.to_owned()))
            })
            .collect()
    }

    fn many(cases: usize) -> String {
        let cases: Vec<_> = (0..cases)
            .map(|index| match index {
                0 => "M0 of i64".to_owned(),
                1 => "M1 of string".to_owned(),
                _ => format!("M{index}"),
            })
            .collect();
        format!("union Many = {}\n", cases.join(" | "))
    }

    fn source() -> String {
        format!(
            "extern def drop_log :: i64 -> unit
record Flags {{ a: bool, b: bool }}
record Mixed {{ a: bool, b: i64, c: bool, d: i64 }}
record Spread {{ a: i8, b: i64, c: i8, d: i32, e: bool }}
record Pair8 {{ small: i8, wide: i64 }}
record Framed {{ a: bool, inner: Mixed, b: bool }}
record Cargo<'a> {{ first: bool, value: 'a, last: bool }}
record Overlay {{ a: bool, b: i128, c: i8 }}
record Vectors {{ a: i8, v: f32x8, b: i8 }}
record Guarded {{ a: bool, b: i64, c: i8 }}
instance Drop<Guarded> {{ fn drop value = drop_log value.b }}
record Wide128 {{ a: i128, b: bool }}
record Lanes4 {{ v: f32x4, tag: i8 }}
record Lanes8 {{ v: f32x8, tag: i8 }}
record Resource {{ id: i64, name: string }}
instance Drop<Resource> {{ fn drop value = drop_log value.id }}
union Color = Red | Green | Blue
union Small = Flag of bool | Code of i8 | Nothing
union Shape = Circle of f64 | Rect of f64 * f64 | Empty
union Rich = Name of string | Count of i64 | Pair of i64 * bool | Nada
union Huge = Big of i128 | Little of i8 | Missing
union Lane4 = Four of f32x4 | Scalar of i32 | Absent4
union Lane8 = Eight of f32x8 | Byte of i8 | Absent8
union Nested = Inner of Shape | Outer of Small | Void
union Holder = Held of Mixed | Wide of Wide128 | Lanes of Lanes4 | Idle
union Slot = Free | Full of i64
instance Drop<Slot> {{ fn drop _value = drop_log 1 }}
union Latch = Open | Closed
instance Drop<Latch> {{ fn drop _value = drop_log 2 }}
union Cell = Number of i64 | Text of string | Vacant
instance Drop<Cell> {{ fn drop _value = drop_log 3 }}
union Alias = NoAlias | Spelled of string
union Label = Named of string | Anonymous
union Twin = Left of string | Right of string
union Trio = Uno | Dos of string | Tres
union Guard = Dormant | Gripped of string
instance Drop<Guard> {{ fn drop _value = drop_log 4 }}
record Handlers {{ first: i64 -> i64, second: i64 -> i64 }}
union Choice = NoChoice | Handler of (i64 -> i64)
union Callback = Call of (i64 -> i64) | Words of string
{many}export def use :: i64 -> i64
fn use n =
    let flags = Flags {{ a: true, b: false }}
    let mixed = Mixed {{ a: true, b: 1, c: false, d: 2 }}
    let spread = Spread {{ a: 1, b: 2, c: 3, d: 4, e: true }}
    let pair8 = Pair8 {{ small: 1, wide: 2 }}
    let framed = Framed {{ a: true, inner: mixed, b: false }}
    let cargo_wide = Cargo {{ first: true, value: n, last: false }}
    let cargo_text = Cargo {{ first: true, value: \"a\", last: false }}
    let cargo_flag = Cargo {{ first: true, value: true, last: false }}
    let overlay = Overlay {{ a: true, b: 1i128, c: 2 }}
    let vectors = Vectors {{ a: 1, v: Simd.splat 1.0, b: 2 }}
    let guarded = Guarded {{ a: true, b: 1, c: 2 }}
    let wide = Wide128 {{ a: 1i128, b: true }}
    let lanes4 = Lanes4 {{ v: Simd.splat 1.0, tag: 1 }}
    let lanes8 = Lanes8 {{ v: Simd.splat 1.0, tag: 1 }}
    let resource = Resource {{ id: 1, name: \"a\" }}
    let color = Green
    let small = Nothing
    let shape = Empty
    let rich = Nada
    let huge = Missing
    let lane4 = Absent4
    let lane8 = Absent8
    let nested = Void
    let holder = Idle
    let slot = Free
    let latch = Open
    let cell = Vacant
    let alias = NoAlias
    let label = Anonymous
    let twin = Left \"a\"
    let trio = Uno
    let guard = Dormant
    let many = M2
    let a: Maybe<string> = None
    let b: Maybe<bool> = Some true
    let c: Result<i64, string> = Ok n
    let d: Result<Shape, Rich> = Error Nada
    let e: Maybe<ref Mixed> = None
    let f: Maybe<utf8string> = None
    let g: Maybe<[i64]> = None
    let handlers = Handlers {{ first: \\x -> x + n, second: \\x -> x * n }}
    let choice = NoChoice
    let callback = Words \"a\"
    let h: Maybe<i64 -> i64> = None
    let i: Result<i64 -> i64, string> = Error \"x\"
    let job: Task<i64> = task {{ return n }}
    n
",
            many = many(300)
        )
    }

    /// The types of `source` that the tests below measure, with the labels of their rows.
    fn types(module: &CheckedModule) -> Vec<(&'static str, Type)> {
        let union = |name: &str, arguments: &[Type]| {
            let id = module
                .unions
                .iter()
                .position(|union| union.name == name)
                .unwrap_or_else(|| panic!("no union {name}"));
            Type::Union(id, arguments.into())
        };
        let record_of = |name: &str, arguments: &[Type]| {
            let id = module
                .records
                .iter()
                .position(|record| record.name == name)
                .unwrap_or_else(|| panic!("no record {name}"));
            Type::Record(id, arguments.into())
        };
        let record = |name: &str| record_of(name, &[]);
        let shape = union("Main.Shape", &[]);
        let rich = union("Main.Rich", &[]);
        let closure = Type::function(vec![Type::I64], Type::I64);
        vec![
            ("Color", union("Main.Color", &[])),
            ("Small", union("Main.Small", &[])),
            ("Shape", shape.clone()),
            ("Rich", rich.clone()),
            ("Huge", union("Main.Huge", &[])),
            ("Lane4", union("Main.Lane4", &[])),
            ("Lane8", union("Main.Lane8", &[])),
            ("Nested", union("Main.Nested", &[])),
            ("Holder", union("Main.Holder", &[])),
            ("Slot", union("Main.Slot", &[])),
            ("Latch", union("Main.Latch", &[])),
            ("Cell", union("Main.Cell", &[])),
            ("Many", union("Main.Many", &[])),
            ("Maybe<string>", union("Maybe.Maybe", &[Type::String])),
            (
                "Maybe<utf8string>",
                union("Maybe.Maybe", &[Type::Utf8String]),
            ),
            (
                "Maybe<[i64]>",
                union(
                    "Maybe.Maybe",
                    &[Type::Array(Box::new(Type::Integer(64, true)))],
                ),
            ),
            ("Alias", union("Main.Alias", &[])),
            ("Label", union("Main.Label", &[])),
            ("Twin", union("Main.Twin", &[])),
            ("Trio", union("Main.Trio", &[])),
            ("Guard", union("Main.Guard", &[])),
            ("Maybe<bool>", union("Maybe.Maybe", &[Type::Bool])),
            (
                "Result<i64, string>",
                union("Result.Result", &[Type::Integer(64, true), Type::String]),
            ),
            (
                "Result<Shape, Rich>",
                union("Result.Result", &[shape, rich]),
            ),
            (
                "Maybe<ref Mixed>",
                union(
                    "Maybe.Maybe",
                    &[Type::Reference(Box::new(record("Main.Mixed")), false)],
                ),
            ),
            ("Flags", record("Main.Flags")),
            ("Mixed", record("Main.Mixed")),
            ("Spread", record("Main.Spread")),
            ("Pair8", record("Main.Pair8")),
            ("Framed", record("Main.Framed")),
            (
                "Cargo<i64>",
                record_of("Main.Cargo", &[Type::Integer(64, true)]),
            ),
            ("Cargo<string>", record_of("Main.Cargo", &[Type::String])),
            ("Cargo<bool>", record_of("Main.Cargo", &[Type::Bool])),
            ("Overlay", record("Main.Overlay")),
            ("Vectors", record("Main.Vectors")),
            ("Guarded", record("Main.Guarded")),
            ("Wide128", record("Main.Wide128")),
            ("Lanes4", record("Main.Lanes4")),
            ("Lanes8", record("Main.Lanes8")),
            ("Resource", record("Main.Resource")),
            ("Closure", closure.clone()),
            ("Task<i64>", Type::Task(Box::new(Type::I64))),
            ("Handlers", record("Main.Handlers")),
            ("Choice", union("Main.Choice", &[])),
            ("Callback", union("Main.Callback", &[])),
            ("Maybe<fn>", union("Maybe.Maybe", &[closure.clone()])),
            (
                "Result<fn, string>",
                union("Result.Result", &[closure, Type::String]),
            ),
        ]
    }

    /// The model's upper and lower bounds hold the size and alignment that LLVM gives each type
    /// under every data layout that Tsuzuri targets (PM01 D4).
    #[test]
    fn layout_bounds_cover_actual_sizes() {
        let module = crate::analyze(&source())
            .unwrap_or_else(|error| panic!("{}: {}\n{}", error.code, error.message, source()));
        for wasm in [false, true] {
            let ir = crate::llvm::emit_target(&module, crate::llvm::Entry::Library, wasm).unwrap();
            let named = definitions(&ir);
            // The layouts and bounds of the type depend on the target, as they do while emitting.
            let _target = target_scope(wasm);
            for (label, ty) in types(&module) {
                let (upper, lower) = (
                    layout_bound(&ty, &module, false),
                    layout_bound(&ty, &module, true),
                );
                let name = llvm_type(&ty, &module);
                for (machine_name, machine) in MACHINES
                    .iter()
                    .filter(|(_, machine)| (machine.pointer == 4) == wasm)
                {
                    let (size, align) = measure(&name, &named, *machine);
                    let context = format!(
                        "{label} on {machine_name}: {name} is {size}/{align}, bounds {lower:?}..{upper:?}"
                    );
                    assert!(lower.0 <= size && size <= upper.0, "{context}");
                    assert!(lower.1 <= align && align <= upper.1, "{context}");
                    assert_eq!(size % align, 0, "{context}");
                }
            }
        }
    }

    /// The sizes that `docs/architecture.md` states for unions, on a target whose `i128` is
    /// 16-byte aligned: the tag takes its own width and the payload area starts at the alignment
    /// of the largest payload.
    #[test]
    fn union_sizes_follow_the_documented_rule() {
        let module = crate::analyze(&source()).unwrap();
        let ir = crate::llvm::emit_target(&module, crate::llvm::Entry::Library, false).unwrap();
        let named = definitions(&ir);
        let machine = MACHINES[0].1;
        for (label, size, align) in [
            ("Color", 1, 1),
            ("Small", 2, 1),
            ("Shape", 24, 8),
            ("Rich", 24, 8),
            ("Huge", 32, 16),
            ("Lane4", 32, 16),
            ("Lane8", 48, 16),
            ("Nested", 32, 8),
            ("Holder", 48, 16),
            ("Slot", 24, 8),
            ("Latch", 2, 1),
            ("Cell", 32, 8),
            ("Many", 24, 8),
            // A nullary case and a string or array payload need no tag: the union is its payload.
            ("Maybe<string>", 16, 8),
            ("Maybe<utf8string>", 16, 8),
            ("Maybe<[i64]>", 16, 8),
            ("Alias", 16, 8),
            ("Label", 16, 8),
            // Two payload cases, three cases, and a Drop union keep their tag.
            ("Twin", 24, 8),
            ("Trio", 24, 8),
            ("Guard", 32, 8),
            ("Maybe<bool>", 2, 1),
            ("Result<i64, string>", 24, 8),
            ("Result<Shape, Rich>", 32, 8),
            // A reference is a pointer, so an option of one is the pointer on a native target.
            ("Maybe<ref Mixed>", 8, 8),
            // A function value is `CLOSURE_POINTERS` pointers; a union holds it like any payload
            // (the bounds below allow for the narrower pointers of wasm32).
            ("Choice", 40, 8),
            ("Callback", 40, 8),
            ("Maybe<fn>", 40, 8),
            ("Result<fn, string>", 40, 8),
        ] {
            let ty = types(&module)
                .into_iter()
                .find_map(|(name, ty)| (name == label).then_some(ty))
                .unwrap();
            let name = llvm_type(&ty, &module);
            assert_eq!(
                measure(&name, &named, machine),
                (size, align),
                "{label}: {name}"
            );
            // Up to 8-byte units the layout is the same everywhere, so the bounds are exact. A
            // pointer is 4 bytes in the lower bound, which covers wasm32.
            let narrow = !matches!(
                label,
                "Huge"
                    | "Lane4"
                    | "Lane8"
                    | "Holder"
                    | "Choice"
                    | "Callback"
                    | "Maybe<fn>"
                    | "Result<fn, string>"
            );
            if narrow && !label.starts_with("Maybe<ref") {
                let expected = Some((size, align));
                assert_eq!(Some(layout_bound(&ty, &module, false)), expected, "{label}");
                assert_eq!(Some(layout_bound(&ty, &module, true)), expected, "{label}");
            }
        }
        // On WebAssembly an option of a reference keeps its tag: address 0 can hold an object.
        let wasm_ir = crate::llvm::emit_target(&module, crate::llvm::Entry::Library, true).unwrap();
        let wasm_named = definitions(&wasm_ir);
        let _target = target_scope(true);
        let ty = types(&module)
            .into_iter()
            .find_map(|(name, ty)| (name == "Maybe<ref Mixed>").then_some(ty))
            .unwrap();
        let name = llvm_type(&ty, &module);
        assert_eq!(
            wasm_named[name.trim_start_matches('%').trim_matches('"')],
            "{ i8, ptr }"
        );
        assert_eq!(measure(&name, &wasm_named, MACHINES[3].1), (8, 4));
    }

    /// The number of pointers in a function value and in a `Task` (PM02): the code, the
    /// environment, and what clone and drop need. Every model of its size follows this number.
    const CLOSURE_POINTERS: usize = 4;

    /// A function value takes the same number of pointers on every machine, so its size is that
    /// number of pointers wide, and the model's bounds hold it: the upper bound for 64-bit
    /// pointers and the lower bound for the 4-byte pointers of wasm32. A union or a record keeps
    /// it inline.
    #[test]
    fn function_values_are_as_wide_as_their_pointers() {
        let module = crate::analyze(&source()).unwrap();
        let find = |label: &str| {
            types(&module)
                .into_iter()
                .find_map(|(name, ty)| (name == label).then_some(ty))
                .unwrap()
        };
        for wasm in [false, true] {
            let ir = crate::llvm::emit_target(&module, crate::llvm::Entry::Library, wasm).unwrap();
            let named = definitions(&ir);
            let _target = target_scope(wasm);
            for label in ["Closure", "Task<i64>"] {
                let ty = find(label);
                assert_eq!(llvm_type(&ty, &module), "%tz.closure", "{label}");
                assert_eq!(
                    layout_bound(&ty, &module, false),
                    (CLOSURE_POINTERS * 8, 8),
                    "{label}"
                );
                assert_eq!(
                    layout_bound(&ty, &module, true),
                    (CLOSURE_POINTERS * 4, 4),
                    "{label}"
                );
                for (machine_name, machine) in MACHINES
                    .iter()
                    .filter(|(_, machine)| (machine.pointer == 4) == wasm)
                {
                    assert_eq!(
                        measure("%tz.closure", &named, *machine),
                        (CLOSURE_POINTERS * machine.pointer, machine.pointer),
                        "{label} on {machine_name}"
                    );
                }
            }
            // Two function values in a record, and one in a tagged union beside the tag.
            let handlers = llvm_type(&find("Handlers"), &module);
            let maybe = llvm_type(&find("Maybe<fn>"), &module);
            for (machine_name, machine) in MACHINES
                .iter()
                .filter(|(_, machine)| (machine.pointer == 4) == wasm)
            {
                let word = machine.pointer;
                assert_eq!(
                    measure(&handlers, &named, *machine),
                    (2 * CLOSURE_POINTERS * word, word),
                    "Handlers on {machine_name}"
                );
                assert_eq!(
                    measure(&maybe, &named, *machine),
                    (word + CLOSURE_POINTERS * word, word),
                    "Maybe<fn> on {machine_name}"
                );
            }
        }
    }

    /// The sizes that `docs/architecture.md` states for records, on a target whose `i128` is
    /// 16-byte aligned: a record of the user's program stores its fields by decreasing alignment
    /// when that makes it smaller (PM01 D7, D8).
    #[test]
    fn record_sizes_follow_the_documented_rule() {
        let module = crate::analyze(&source()).unwrap();
        let ir = crate::llvm::emit_target(&module, crate::llvm::Entry::Library, false).unwrap();
        let named = definitions(&ir);
        let machine = MACHINES[0].1;
        for (label, body, size, align) in [
            // Declared order needs 32 bytes: a bool, 7 bytes of padding, an i64, a bool, 7 more.
            ("Mixed", "{ i64, i64, i1, i1 }", 24, 8),
            ("Spread", "{ i64, i32, i8, i8, i1 }", 16, 8),
            // The same size either way keeps the declared order.
            ("Pair8", "{ i8, i64 }", 16, 8),
            ("Flags", "{ i1, i1 }", 2, 1),
            ("Framed", "{ %tz.record.Main.Mixed, i1, i1 }", 32, 8),
            ("Cargo<i64>", "{ i64, i1, i1 }", 16, 8),
            ("Cargo<string>", "{ %tz.string, i1, i1 }", 24, 8),
            ("Cargo<bool>", "{ i1, i1, i1 }", 3, 1),
            ("Overlay", "{ i128, i1, i8 }", 32, 16),
            ("Vectors", "{ <8 x float>, i8, i8 }", 64, 32),
            // The live flag of a Drop record stays last.
            ("Guarded", "{ i64, i1, i8, i8 }", 16, 8),
        ] {
            let ty = types(&module)
                .into_iter()
                .find_map(|(name, ty)| (name == label).then_some(ty))
                .unwrap();
            let name = llvm_type(&ty, &module);
            assert_eq!(
                named[name.trim_start_matches('%').trim_matches('"')],
                body,
                "{label}: {name}"
            );
            assert_eq!(
                measure(&name, &named, machine),
                (size, align),
                "{label}: {name}"
            );
            // The model sees the same order, so its upper bound is no smaller than the size.
            assert!(layout_bound(&ty, &module, false).0 >= size, "{label}");
        }
    }

    /// A nested `target_scope` has its own target and its own memo: a layout that depends on the
    /// target is never taken from the scope around it, and the outer answers come back at its end.
    #[test]
    fn layouts_follow_the_innermost_target_scope() {
        let module = crate::analyze(&source()).unwrap();
        let (id, arguments) = types(&module)
            .into_iter()
            .find_map(|(label, ty)| match ty {
                Type::Union(id, arguments) if label == "Maybe<ref Mixed>" => Some((id, arguments)),
                _ => None,
            })
            .unwrap();
        let niche = || {
            matches!(
                union_layout(id, &arguments, &module),
                UnionLayout::Niche { .. }
            )
        };
        // Outside any scope the target is native, and nothing is remembered.
        assert!(niche());
        {
            let _wasm = target_scope(true);
            assert!(!niche());
            assert!(!niche());
            {
                let _native = target_scope(false);
                assert!(niche());
            }
            assert!(!niche());
        }
        assert!(niche());
    }

    /// The placement is a stable sort by decreasing alignment that is kept only when it takes
    /// fewer bytes, with the model's `(size, alignment)` of each field as input.
    #[test]
    fn fields_are_placed_by_decreasing_alignment_only_when_that_saves_bytes() {
        let place =
            |fields: &[(usize, usize)], flag| place_fields(fields, flag).map(|s| s.to_vec());
        // bool, i64, bool, i64: the i64s first, each group in declared order.
        let mixed = [(1, 1), (8, 8), (1, 1), (8, 8)];
        assert_eq!(place(&mixed, false), Some(vec![2, 0, 3, 1]));
        // No byte saved: equal alignments, a small field before a large one that fills the gap
        // anyway, a zero-sized field, and a single field.
        assert_eq!(place(&[(8, 8), (8, 8)], false), None);
        assert_eq!(place(&[(1, 1), (8, 8)], false), None);
        assert_eq!(place(&[(0, 1), (8, 8)], false), None);
        assert_eq!(place(&[(8, 8)], false), None);
        // The flag of a Drop record takes the last byte; it can tip the choice.
        let guarded = [(1, 1), (8, 8), (1, 1)];
        assert_eq!(place(&guarded, false), Some(vec![1, 0, 2]));
        assert_eq!(place(&guarded, true), Some(vec![1, 0, 2]));
        // Alignments of 16 and 8 come in front of the bytes.
        assert_eq!(
            place(&[(1, 1), (16, 16), (8, 8)], false),
            Some(vec![2, 0, 1])
        );
        // Every result is a permutation.
        for fields in [
            &mixed[..],
            &guarded[..],
            &[(2, 2), (4, 4), (1, 1), (8, 8)][..],
        ] {
            if let Some(slots) = place(fields, false) {
                let mut sorted = slots.clone();
                sorted.sort_unstable();
                assert_eq!(sorted, (0..fields.len()).collect::<Vec<_>>());
            }
        }
    }
}
