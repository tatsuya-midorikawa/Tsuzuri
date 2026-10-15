use super::{Constructor, Type, TypeContext};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TypeId(u32);

enum Storage<'a> {
    Borrowed(&'a Type),
    Owned(Box<Type>),
    Child(TypeId, usize),
}

struct Node<'a> {
    storage: Storage<'a>,
    visited: bool,
}

#[derive(Default)]
pub(super) struct TypeArena<'a> {
    nodes: Vec<Node<'a>>,
    buckets: HashMap<u64, Vec<TypeId>>,
}

impl<'a> TypeArena<'a> {
    pub(super) fn borrowed(&mut self, ty: &'a Type) -> TypeId {
        self.intern(Storage::Borrowed(ty))
    }

    fn intern(&mut self, storage: Storage<'a>) -> TypeId {
        let (hash, existing) = self.find(self.resolve(&storage));
        existing.unwrap_or_else(|| self.insert(storage, hash))
    }

    fn owned(&mut self, ty: Type) -> TypeId {
        let (hash, existing) = self.find(&ty);
        existing.unwrap_or_else(|| self.insert(Storage::Owned(Box::new(ty)), hash))
    }

    fn find(&self, ty: &Type) -> (u64, Option<TypeId>) {
        let hash = self.buckets.hasher().hash_one(Fingerprint(ty));
        let existing = self
            .buckets
            .get(&hash)
            .and_then(|ids| ids.iter().find(|&&id| self.ty(id) == ty).copied());
        (hash, existing)
    }

    fn insert(&mut self, storage: Storage<'a>, hash: u64) -> TypeId {
        let id = TypeId(u32::try_from(self.nodes.len()).expect("compiler limits bound type nodes"));
        self.nodes.push(Node {
            storage,
            visited: false,
        });
        self.buckets.entry(hash).or_default().push(id);
        id
    }

    fn resolve<'b>(&'b self, storage: &'b Storage<'a>) -> &'b Type {
        match storage {
            Storage::Borrowed(ty) => ty,
            Storage::Owned(ty) => ty,
            Storage::Child(parent, index) => child(self.ty(*parent), *index),
        }
    }

    pub(super) fn ty(&self, id: TypeId) -> &Type {
        self.resolve(&self.nodes[id.0 as usize].storage)
    }

    pub(super) fn visit(&mut self, id: TypeId) -> bool {
        !std::mem::replace(&mut self.nodes[id.0 as usize].visited, true)
    }

    /// Keeps generated subtrees in one owner; child projections never clone a Type.
    pub(super) fn children(
        &mut self,
        id: TypeId,
        types: TypeContext<'a>,
        references: bool,
        pending: &mut Vec<TypeId>,
    ) {
        match self.ty(id) {
            Type::Record(record, arguments) if arguments.is_empty() => {
                let fields = &types.records[*record].fields;
                for (_, ty) in fields {
                    pending.push(self.borrowed(ty));
                }
                return;
            }
            Type::Union(union, arguments) if arguments.is_empty() => {
                let cases = &types.unions[*union].cases;
                for (_, ty) in cases {
                    if let Some(ty) = ty {
                        pending.push(self.borrowed(ty));
                    }
                }
                return;
            }
            Type::Record(record, arguments) => {
                let fields = types.record_fields(*record, arguments);
                for ty in fields {
                    pending.push(self.owned(ty));
                }
                return;
            }
            Type::Union(union, arguments) => {
                let payloads = types.union_payloads(*union, arguments);
                for ty in payloads.into_iter().flatten() {
                    pending.push(self.owned(ty));
                }

                return;
            }
            _ => {}
        }
        let count = match self.ty(id) {
            Type::Reference(_, _) if references => 1,
            Type::Array(_)
            | Type::List(_)
            | Type::Vec(_)
            | Type::FixedArray(..)
            | Type::Shared(..) => 1,
            Type::Tuple(elements) => elements.len(),
            _ => 0,
        };
        for index in 0..count {
            let storage = match self.nodes[id.0 as usize].storage {
                Storage::Borrowed(ty) => Storage::Borrowed(child(ty, index)),
                _ => Storage::Child(id, index),
            };
            pending.push(self.intern(storage));
        }
    }
}

fn child(ty: &Type, index: usize) -> &Type {
    match ty {
        Type::Reference(inner, _)
        | Type::Array(inner)
        | Type::List(inner)
        | Type::Vec(inner)
        | Type::FixedArray(inner, _)
        | Type::Shared(inner, _) => inner,
        Type::Tuple(elements) => &elements[index],
        _ => unreachable!("only stored type children have projections"),
    }
}

struct Fingerprint<'a>(&'a Type);

impl Hash for Fingerprint<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let ty = self.0;
        std::mem::discriminant(ty).hash(state);
        let sequence = |types: &[Type], state: &mut H| {
            types.len().hash(state);
            for ty in types {
                Fingerprint(ty).hash(state);
            }
        };
        match ty {
            Type::Variable(name) => name.hash(state),
            Type::Infer(id) => id.hash(state),
            Type::Integer(bits, signed) => (bits, signed).hash(state),
            Type::Binary(bits) | Type::Decimal(bits) => bits.hash(state),
            Type::Length(length) => length.hash(state),
            Type::Simd(simd) => {
                (simd.bits, simd.width).hash(state);
                std::mem::discriminant(&simd.kind).hash(state);
            }
            Type::Record(id, arguments) | Type::Union(id, arguments) => {
                id.hash(state);
                sequence(arguments, state);
            }
            Type::Array(inner)
            | Type::ArrayView(inner)
            | Type::List(inner)
            | Type::Vec(inner)
            | Type::Task(inner) => Fingerprint(inner).hash(state),
            Type::Reference(inner, mutable) => {
                mutable.hash(state);
                Fingerprint(inner).hash(state);
            }
            Type::Shared(inner, kind) => {
                std::mem::discriminant(kind).hash(state);
                Fingerprint(inner).hash(state);
            }
            Type::FixedArray(element, length) => {
                Fingerprint(element).hash(state);
                Fingerprint(length).hash(state);
            }
            Type::Tuple(elements) => sequence(elements, state),
            Type::Function(parameters, result) => {
                sequence(parameters, state);
                Fingerprint(result).hash(state);
            }
            Type::Handle(name) => name.hash(state),
            Type::Application(head, arguments) => {
                Fingerprint(head).hash(state);
                sequence(arguments, state);
            }
            Type::Partial(partial) => {
                std::mem::discriminant(&partial.constructor).hash(state);
                if let Constructor::Record(id, arity) | Constructor::Union(id, arity) =
                    partial.constructor
                {
                    (id, arity).hash(state);
                }
                sequence(&partial.trailing, state);
            }
            Type::Dyn(dyn_type) => {
                dyn_type.classes.hash(state);
                (dyn_type.copy, dyn_type.send, dyn_type.borrowed).hash(state);
            }
            Type::Error
            | Type::Bool
            | Type::Unit
            | Type::Char
            | Type::Utf8Char
            | Type::String
            | Type::Utf8String => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interns_borrowed_owned_and_projected_types_without_cloning_children() {
        let types = TypeContext {
            records: &[],
            unions: &[],
        };
        let integer = Type::Integer(64, true);
        let array = Type::Array(Box::new(integer.clone()));
        let mut arena = TypeArena::default();
        let borrowed = arena.borrowed(&array);
        assert_eq!(arena.owned(array.clone()), borrowed);
        let owned = arena.owned(Type::List(Box::new(array.clone())));
        let mut pending = Vec::new();
        arena.children(owned, types, false, &mut pending);
        assert_eq!(pending.pop(), Some(borrowed));
        arena.children(borrowed, types, false, &mut pending);
        let element = pending.pop().unwrap();
        assert!(std::ptr::eq(arena.ty(element), child(&array, 0)));
        assert!(arena.visit(element));
        assert!(!arena.visit(element));
        assert_eq!(
            arena
                .nodes
                .iter()
                .filter(|node| matches!(node.storage, Storage::Owned(_)))
                .count(),
            1
        );
        let hash = arena.buckets.hasher().hash_one(Fingerprint(&array));
        arena.buckets.get_mut(&hash).unwrap().insert(0, owned);
        assert_eq!(
            arena.borrowed(&array),
            borrowed,
            "hash collisions use equality"
        );
    }

    #[test]
    fn generated_deep_types_have_one_owner_and_only_id_projections() {
        let mut ty = Type::Integer(64, true);
        for _ in 0..64 {
            ty = Type::Array(Box::new(ty));
        }
        let mut arena = TypeArena::default();
        let root = arena.owned(ty);
        let types = TypeContext {
            records: &[],
            unions: &[],
        };
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if arena.visit(id) {
                arena.children(id, types, false, &mut pending);
            }
        }
        assert_eq!(arena.nodes.len(), 65);
        assert_eq!(
            arena
                .nodes
                .iter()
                .filter(|node| matches!(node.storage, Storage::Owned(_)))
                .count(),
            1
        );
        assert_eq!(
            arena
                .nodes
                .iter()
                .filter(|node| matches!(node.storage, Storage::Child(..)))
                .count(),
            64
        );
    }

    #[test]
    fn borrowed_deep_graph_removes_reference_walk_type_allocations() {
        let mut ty = Type::Integer(64, true);
        for _ in 0..64 {
            ty = Type::Array(Box::new(ty));
        }
        let mut original = vec![ty.clone()];
        let mut seen = std::collections::BTreeSet::new();
        let mut cloned_boxes = 64;
        while let Some(ty) = original.pop() {
            let mut depth = 0;
            let mut inner = &ty;
            while let Type::Array(element) = inner {
                depth += 1;
                inner = element;
            }
            cloned_boxes += depth;
            if seen.insert(ty.clone())
                && let Type::Array(inner) = ty
            {
                original.push(*inner);
            }
        }
        let mut arena = TypeArena::default();
        let root = arena.borrowed(&ty);
        let types = TypeContext {
            records: &[],
            unions: &[],
        };
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if arena.visit(id) {
                arena.children(id, types, false, &mut pending);
            }
        }
        assert_eq!(cloned_boxes, 2144);
        assert!(
            arena
                .nodes
                .iter()
                .all(|node| matches!(node.storage, Storage::Borrowed(_)))
        );
        println!(
            "Type depth 64: reference clone Box allocations {cloned_boxes}; arena Type allocations 0; {} interned ids",
            arena.nodes.len()
        );
    }
}
